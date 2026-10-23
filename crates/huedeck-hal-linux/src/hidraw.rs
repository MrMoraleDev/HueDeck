// SPDX-License-Identifier: GPL-3.0-or-later
//! hidraw HID backend: sysfs enumeration ([`crate::sysfs`]), TLC split
//! ([`crate::tlc`]) and report I/O on `/dev/hidrawN` (ioctls in [`crate::sys`]).
//!
//! Each endpoint is one TLC of a hidraw node (`/dev/hidraw3#tlc1`). hidraw
//! allows any number of opens, so every TLC handle opens the node on its own
//! and keeps only its own input reports (by report id when the interface has
//! several numbered TLCs). Output and feature reports are padded or
//! truncated to the TLC's longest report of that kind, as hidapi does on
//! Windows, so the device sees the same bytes on both OSes.
//!
//! Enumeration reads only world-readable sysfs files; a node without access
//! (no udev rule yet) is still listed and fails at `open` with
//! [`HalError::PermissionDenied`].

use crate::sys::{self, Ready};
use crate::sysfs::{self, HidrawNode};
use crate::tlc::{self, Tlc};
use huedeck_hal::hid::{HidBackend, HidEndpointInfo, HidHandle};
use huedeck_hal::{HalError, HalResult};
use huedeck_hal_windows::DeviceGroup;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// usbhid's control-transfer timeout (`USB_CTRL_SET_TIMEOUT`), reported for `ETIMEDOUT`.
const KERNEL_TIMEOUT_MS: u32 = 5000;

/// Map an I/O error of a hidraw node onto the HAL vocabulary.
pub(crate) fn map_io(e: io::Error, what: &str) -> HalError {
    match e.raw_os_error() {
        Some(libc::ENODEV) | Some(libc::EIO) | Some(libc::ENXIO) | Some(libc::ESHUTDOWN) => {
            HalError::Disconnected
        }
        Some(libc::EACCES) | Some(libc::EPERM) => HalError::PermissionDenied(format!(
            "{what}: {e} (install the udev rules: `huedeck udev generate --out 70-huedeck.rules`, \
             see packaging/udev/README.md)"
        )),
        Some(libc::ENOENT) => HalError::NotFound(what.to_string()),
        Some(libc::EBUSY) => HalError::Busy(what.to_string()),
        Some(libc::ETIMEDOUT) => HalError::Timeout(KERNEL_TIMEOUT_MS),
        _ => HalError::Io(e),
    }
}

/// The Linux hidraw backend.
#[derive(Clone, Debug)]
pub struct LinuxHidBackend {
    class_dir: PathBuf,
}

impl Default for LinuxHidBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxHidBackend {
    /// The backend over `/sys/class/hidraw` (nothing is read until asked).
    pub fn new() -> Self {
        Self {
            class_dir: PathBuf::from("/sys/class/hidraw"),
        }
    }

    /// Every hidraw node (none when the hidraw class is absent).
    pub fn nodes(&self) -> HalResult<Vec<HidrawNode>> {
        match sysfs::enumerate(&self.class_dir) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(HalError::Io(e)),
        }
    }

    /// Enumerate and group endpoints by physical device ([`tlc::group`]).
    pub fn enumerate_grouped(&self) -> HalResult<Vec<DeviceGroup>> {
        Ok(tlc::group(&self.nodes()?))
    }

    /// Devices whose `(vid, pid)` is in `wanted`, or with a TLC whose
    /// `(usage_page, usage)` is in `usages`.
    pub fn enumerate_grouped_matching(
        &self,
        wanted: &BTreeSet<(u16, u16)>,
        usages: &BTreeSet<(u16, u16)>,
    ) -> HalResult<Vec<DeviceGroup>> {
        let mut groups = self.enumerate_grouped()?;
        groups.retain(|g| {
            wanted.contains(&(g.key.vid, g.key.pid))
                || g.endpoints
                    .iter()
                    .any(|e| usages.contains(&(e.info.usage_page, e.info.usage)))
        });
        Ok(groups)
    }

    fn node_of(&self, devnode: &str) -> HalResult<HidrawNode> {
        let name = devnode.trim_start_matches("/dev/");
        let link = self.class_dir.join(name).join("device");
        let hid_dir = std::fs::canonicalize(&link).map_err(|e| map_io(e, devnode))?;
        sysfs::read_node(&sysfs::RealFs, name, &hid_dir)
            .ok_or_else(|| HalError::NotFound(devnode.to_string()))
    }
}

impl HidBackend for LinuxHidBackend {
    fn enumerate(&self) -> HalResult<Vec<HidEndpointInfo>> {
        Ok(self
            .nodes()?
            .iter()
            .flat_map(tlc::endpoints)
            .map(|e| e.info)
            .collect())
    }

    fn open(&self, path: &str) -> HalResult<Box<dyn HidHandle>> {
        let (devnode, index) = tlc::parse_endpoint_path(path)
            .ok_or_else(|| HalError::InvalidArgument(format!("not a hidraw path: {path}")))?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(devnode)
            .map_err(|e| map_io(e, devnode))?;
        // The descriptor through the open node (also proves it is hidraw);
        // sysfs supplies the identity.
        let descriptor = sys::report_descriptor(&file).map_err(|e| map_io(e, devnode))?;
        let mut node = self.node_of(devnode)?;
        node.descriptor = descriptor;
        let info = tlc::endpoints(&node)
            .into_iter()
            .map(|e| e.info)
            .find(|i| i.path == tlc::endpoint_path(devnode, index))
            .ok_or_else(|| HalError::NotFound(path.to_string()))?;
        let tlc = huedeck_hid_desc::parse(&node.descriptor)
            .ok()
            .and_then(|d| tlc::tlcs(&d).into_iter().find(|t| t.index == index));
        tracing::debug!(path, report_lengths = ?info.report_lengths, "opened hidraw endpoint");
        Ok(Box::new(LinuxHidHandle {
            file,
            info,
            tlc,
            scratch: Vec::new(),
        }))
    }
}

/// One open TLC of a hidraw node.
pub struct LinuxHidHandle {
    file: File,
    info: HidEndpointInfo,
    /// `None` when the descriptor did not parse (no demultiplexing, no padding).
    tlc: Option<Tlc>,
    scratch: Vec<u8>,
}

impl std::fmt::Debug for LinuxHidHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinuxHidHandle")
            .field("path", &self.info.path)
            .finish()
    }
}

impl LinuxHidHandle {
    fn numbered(&self) -> bool {
        self.tlc.as_ref().is_none_or(|t| t.numbered)
    }
}

impl HidHandle for LinuxHidHandle {
    fn info(&self) -> &HidEndpointInfo {
        &self.info
    }

    fn write(&mut self, data: &[u8]) -> HalResult<usize> {
        if data.is_empty() {
            return Err(HalError::InvalidArgument("empty output report".into()));
        }
        let report = tlc::fit_report(data, self.info.report_lengths.output);
        self.file.write(&report).map_err(|e| map_io(e, &self.info.path))?;
        Ok(data.len())
    }

    fn read_timeout(&mut self, buf: &mut [u8], timeout_ms: i32) -> HalResult<usize> {
        let deadline = (timeout_ms > 0).then(|| Instant::now() + Duration::from_millis(timeout_ms as u64));
        if self.scratch.is_empty() {
            self.scratch = vec![0u8; sys::HID_MAX_BUFFER_SIZE];
        }
        loop {
            let wait = match deadline {
                None => timeout_ms.min(0),
                Some(d) => d.saturating_duration_since(Instant::now()).as_millis() as i32,
            };
            match sys::poll_in(&self.file, wait).map_err(|e| map_io(e, &self.info.path))? {
                Ready::Timeout => return Ok(0),
                Ready::Hangup => return Err(HalError::Disconnected),
                Ready::Readable => {}
            }
            let n = match self.file.read(&mut self.scratch) {
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::Interrupted => {
                    continue
                }
                Err(e) => return Err(map_io(e, &self.info.path)),
            };
            let report = &self.scratch[..n];
            if self.tlc.as_ref().is_none_or(|t| t.accepts_input(report)) {
                let numbered = self.numbered();
                return Ok(tlc::place_input(numbered, report, buf));
            }
            // Another TLC's report: drop it and keep waiting.
            if timeout_ms == 0 || deadline.is_some_and(|d| Instant::now() >= d) {
                return Ok(0);
            }
        }
    }

    fn send_feature_report(&mut self, data: &[u8]) -> HalResult<usize> {
        if data.is_empty() {
            return Err(HalError::InvalidArgument("empty feature report".into()));
        }
        let mut report = tlc::fit_report(data, self.info.report_lengths.feature).into_owned();
        sys::set_feature(&self.file, &mut report).map_err(|e| map_io(e, &self.info.path))?;
        Ok(data.len())
    }

    fn get_feature_report(&mut self, buf: &mut [u8]) -> HalResult<usize> {
        sys::get_feature(&self.file, buf).map_err(|e| map_io(e, &self.info.path))
    }

    fn get_input_report(&mut self, buf: &mut [u8]) -> HalResult<usize> {
        sys::get_input(&self.file, buf).map_err(|e| map_io(e, &self.info.path))
    }

    fn report_descriptor(&mut self) -> HalResult<Vec<u8>> {
        // The whole interface's descriptor; callers pick the TLC by usage.
        sys::report_descriptor(&self.file).map_err(|e| map_io(e, &self.info.path))
    }
}
