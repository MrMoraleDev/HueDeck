// SPDX-License-Identifier: GPL-3.0-or-later
//! Native Windows HID backend (no hidapi).
//!
//! hidapi 2.6 enumerates every HID interface, and reads every device's USB
//! strings, whenever a context is created: about 150 ms on the reference
//! machine before the first report can be sent. [`NativeHidBackend`] talks
//! to the HID class driver directly:
//!
//! * enumeration: the interface list from cfgmgr32
//!   ([`probe::hid_interface_paths`]); then, only for collections of
//!   interest, a zero-access handle for `HidD_GetAttributes`, `HIDP_CAPS`,
//!   the declared feature report ids and the USB strings;
//! * I/O on an overlapped `GENERIC_READ | GENERIC_WRITE` handle (a zero-access
//!   one for system mice and keyboards, which Windows opens exclusively: feature
//!   reports still work there, as with hidapi; reads and writes do not): output
//!   reports with `WriteFile`, padded to `OutputReportByteLength` as Windows
//!   requires; input reports through one persistent pending `ReadFile`, so no
//!   report is lost between calls, report id first (`0x00` for unnumbered
//!   reports); `HidD_SetFeature`, padded to
//!   `FeatureReportByteLength`; `IOCTL_HID_GET_FEATURE` /
//!   `IOCTL_HID_GET_INPUT_REPORT` for byte counts; `HidD_FlushQueue`.
//!
//! Report semantics match [`crate::hid::WindowsHidBackend`] (which adapts
//! hidapi to the same contract), so a plugin cannot tell the two apart;
//! GET_REPORT(Input) works only here.
//!
//! Feature report lengths: `HIDP_CAPS` only gives the longest feature report
//! of a collection, so every [`ReportSpec`] carries that upper bound; a
//! GET_REPORT with it returns the report's real length.

use crate::group::{group_endpoints, own_usb_parent, DeviceGroup, ProbedEndpoint};
use crate::path::parse_hid_path;
use crate::probe;
use lightloom_hal::hid::{HidBackend, HidEndpointInfo, HidHandle, ReportLengths, ReportSpec};
use lightloom_hal::{HalError, HalResult};
use std::collections::BTreeSet;
use windows::core::PCWSTR;
use windows::Win32::Devices::HumanInterfaceDevice::{
    HidD_FlushQueue, HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetManufacturerString,
    HidD_GetPreparsedData, HidD_GetProductString, HidD_GetSerialNumberString, HidD_SetFeature,
    HidD_SetNumInputBuffers, HidP_Feature, HidP_GetButtonCaps, HidP_GetCaps, HidP_GetValueCaps,
    HIDD_ATTRIBUTES, HIDP_BUTTON_CAPS, HIDP_CAPS, HIDP_STATUS_SUCCESS, HIDP_VALUE_CAPS, PHIDP_PREPARSED_DATA,
};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_DEVICE_NOT_CONNECTED, ERROR_DEV_NOT_EXIST, ERROR_FILE_NOT_FOUND,
    ERROR_IO_INCOMPLETE, ERROR_IO_PENDING, ERROR_NO_SUCH_DEVICE, GENERIC_READ, GENERIC_WRITE, HANDLE,
    WIN32_ERROR,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_FLAGS_AND_ATTRIBUTES, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Threading::{CreateEventW, ResetEvent, INFINITE};
use windows::Win32::System::IO::{
    CancelIoEx, DeviceIoControl, GetOverlappedResult, GetOverlappedResultEx, OVERLAPPED,
};

/// `HID_OUT_CTL_CODE(100)`: GET_REPORT(Feature) with the returned byte count.
const IOCTL_HID_GET_FEATURE: u32 = 0x000B_0192;
/// `HID_OUT_CTL_CODE(104)`: GET_REPORT(Input).
const IOCTL_HID_GET_INPUT_REPORT: u32 = 0x000B_01A2;
/// Longest a single write or GET_REPORT may take.
const IO_TIMEOUT_MS: u32 = 1000;
/// Input reports the class driver queues for our handle (hidapi uses 64).
const INPUT_BUFFERS: u32 = 64;
/// UTF-16 units of a HID string buffer (USB strings hold at most 126 characters).
const STRING_UNITS: usize = 128;
/// `WAIT_TIMEOUT` as reported by `GetOverlappedResultEx`.
const WAIT_TIMEOUT_ERROR: WIN32_ERROR = WIN32_ERROR(258);

/// A kernel handle closed on drop.
struct OwnedHandle(HANDLE);

// SAFETY: a file or event handle is a process-wide kernel handle that may be
// used and closed from any thread.
unsafe impl Send for OwnedHandle {}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful CreateFileW/CreateEventW and is closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn map_win(context: &str, e: &windows::core::Error) -> HalError {
    let code = e.code();
    let text = format!("{context}: {}", e.message());
    if code == ERROR_ACCESS_DENIED.to_hresult() {
        HalError::PermissionDenied(text)
    } else if code == ERROR_DEVICE_NOT_CONNECTED.to_hresult()
        || code == ERROR_NO_SUCH_DEVICE.to_hresult()
        || code == ERROR_DEV_NOT_EXIST.to_hresult()
        || code == ERROR_FILE_NOT_FOUND.to_hresult()
    {
        // The device is gone. ERROR_GEN_FAILURE is not among these: a present
        // device returns it when it stalls a report it does not support.
        HalError::Disconnected
    } else {
        HalError::Backend(text)
    }
}

fn is_timeout(e: &windows::core::Error) -> bool {
    e.code() == WAIT_TIMEOUT_ERROR.to_hresult() || e.code() == ERROR_IO_INCOMPLETE.to_hresult()
}

fn open_path(path: &str, access: u32, overlapped: bool) -> windows::core::Result<OwnedHandle> {
    let w = wide(path);
    let flags = if overlapped {
        FILE_FLAG_OVERLAPPED
    } else {
        FILE_FLAGS_AND_ATTRIBUTES(0)
    };
    // SAFETY: NUL-terminated path that outlives the call; no security attributes or template.
    let h = unsafe {
        CreateFileW(
            PCWSTR(w.as_ptr()),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            flags,
            None,
        )
    }?;
    Ok(OwnedHandle(h))
}

fn new_event() -> HalResult<OwnedHandle> {
    // SAFETY: anonymous manual-reset event, initially non-signalled.
    unsafe { CreateEventW(None, true, false, PCWSTR::null()) }
        .map(OwnedHandle)
        .map_err(|e| map_win("CreateEventW", &e))
}

/// Preparsed data, freed on drop.
struct Preparsed(PHIDP_PREPARSED_DATA);

impl Preparsed {
    fn of(handle: HANDLE) -> Option<Self> {
        let mut pp = PHIDP_PREPARSED_DATA(0);
        // SAFETY: valid handle and out pointer.
        unsafe { HidD_GetPreparsedData(handle, &mut pp) }.then_some(Self(pp))
    }
}

impl Drop for Preparsed {
    fn drop(&mut self) {
        // SAFETY: allocated by HidD_GetPreparsedData, freed exactly once.
        let _ = unsafe { HidD_FreePreparsedData(self.0) };
    }
}

/// What `HIDP_CAPS` and the feature value/button caps say about one collection.
struct CollectionCaps {
    usage_page: u16,
    usage: u16,
    lengths: ReportLengths,
    feature_reports: Vec<ReportSpec>,
}

fn collection_caps(handle: HANDLE) -> Result<CollectionCaps, String> {
    let pp = Preparsed::of(handle).ok_or_else(|| {
        format!(
            "HidD_GetPreparsedData: {}",
            windows::core::Error::from_thread().message()
        )
    })?;
    let mut caps = HIDP_CAPS::default();
    // SAFETY: `pp` is live preparsed data; `caps` is writable.
    let status = unsafe { HidP_GetCaps(pp.0, &mut caps) };
    if status != HIDP_STATUS_SUCCESS {
        return Err(format!("HidP_GetCaps failed (NTSTATUS {:#x})", status.0));
    }
    let mut ids = BTreeSet::new();
    if caps.NumberFeatureValueCaps > 0 {
        let mut n = caps.NumberFeatureValueCaps;
        let mut v = vec![HIDP_VALUE_CAPS::default(); n as usize];
        // SAFETY: `v` holds `n` elements, as the length argument says.
        if unsafe { HidP_GetValueCaps(HidP_Feature, v.as_mut_ptr(), &mut n, pp.0) } == HIDP_STATUS_SUCCESS {
            ids.extend(v.iter().take(n as usize).map(|c| c.ReportID));
        }
    }
    if caps.NumberFeatureButtonCaps > 0 {
        let mut n = caps.NumberFeatureButtonCaps;
        let mut b = vec![HIDP_BUTTON_CAPS::default(); n as usize];
        // SAFETY: `b` holds `n` elements, as the length argument says.
        if unsafe { HidP_GetButtonCaps(HidP_Feature, b.as_mut_ptr(), &mut n, pp.0) } == HIDP_STATUS_SUCCESS {
            ids.extend(b.iter().take(n as usize).map(|c| c.ReportID));
        }
    }
    let feature_len = caps.FeatureReportByteLength as usize;
    Ok(CollectionCaps {
        usage_page: caps.UsagePage,
        usage: caps.Usage,
        lengths: ReportLengths {
            input: caps.InputReportByteLength as usize,
            output: caps.OutputReportByteLength as usize,
            feature: feature_len,
        },
        feature_reports: if feature_len == 0 {
            Vec::new()
        } else {
            ids.into_iter()
                .map(|id| ReportSpec { id, len: feature_len })
                .collect()
        },
    })
}

type StringFn = unsafe fn(HANDLE, *mut core::ffi::c_void, u32) -> bool;

fn hid_string(handle: HANDLE, f: StringFn) -> String {
    let mut buf = [0u16; STRING_UNITS];
    // SAFETY: the buffer length is given in bytes, as the HidD_Get*String functions expect.
    if !unsafe { f(handle, buf.as_mut_ptr().cast(), (buf.len() * 2) as u32) } {
        return String::new();
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// Which collections an enumeration describes: those of the devices in
/// `ids` (every device when `None`), plus, on any device, the collections
/// whose `(usage page, usage)` is in `usages`.
#[derive(Clone, Copy, Default)]
struct Wanted<'a> {
    ids: Option<&'a BTreeSet<(u16, u16)>>,
    usages: Option<&'a BTreeSet<(u16, u16)>>,
}

impl Wanted<'_> {
    fn id(&self, key: (u16, u16)) -> bool {
        self.ids.is_none_or(|w| w.contains(&key))
    }

    fn usage(&self, key: (u16, u16)) -> bool {
        self.usages.is_some_and(|u| u.contains(&key))
    }

    fn any_usage(&self) -> bool {
        self.usages.is_some_and(|u| !u.is_empty())
    }
}

/// Describe the collection at `path` through a zero-access handle (no report
/// I/O). `None` when the collection is not `wanted`.
fn endpoint_info(path: &str, wanted: Wanted<'_>, strings: bool) -> Result<Option<HidEndpointInfo>, String> {
    let handle = open_path(path, 0, false).map_err(|e| format!("CreateFileW(access=0): {}", e.message()))?;
    let mut attrs = HIDD_ATTRIBUTES {
        Size: std::mem::size_of::<HIDD_ATTRIBUTES>() as u32,
        ..Default::default()
    };
    // SAFETY: valid handle; `attrs.Size` is set as the API requires.
    if !unsafe { HidD_GetAttributes(handle.0, &mut attrs) } {
        return Err("HidD_GetAttributes failed".into());
    }
    let id_wanted = wanted.id((attrs.VendorID, attrs.ProductID));
    if !id_wanted && !wanted.any_usage() {
        return Ok(None);
    }
    let caps = collection_caps(handle.0)?;
    if !id_wanted && !wanted.usage((caps.usage_page, caps.usage)) {
        return Ok(None);
    }
    let parsed = parse_hid_path(path);
    // `MI_xx` when the path names one (also for virtual collections a vendor driver
    // re-exposes, e.g. Razer Synapse's RZVIRTUAL nodes); otherwise 0 for a
    // non-composite USB device, as hidapi and SignalRGB report, and -1 for every
    // other transport (docs/host-api/hid-usb.md, "Endpoint tuple semantics").
    let usb = probe::hid_bus(path) == probe::HidBus::Usb;
    let interface_number = match (parsed.interface_number, usb) {
        (mi, _) if mi >= 0 => mi,
        (_, true) => 0,
        (_, false) => HidEndpointInfo::UNKNOWN_INTERFACE,
    };
    let (serial, mut manufacturer, product) = if strings {
        (
            hid_string(handle.0, HidD_GetSerialNumberString),
            hid_string(handle.0, HidD_GetManufacturerString),
            hid_string(handle.0, HidD_GetProductString),
        )
    } else {
        Default::default()
    };
    // Same fallback as hidapi for USB devices: the USB interface's INF manufacturer.
    if strings && usb && manufacturer.is_empty() {
        manufacturer = probe::parent_manufacturer(path).unwrap_or_default();
    }
    Ok(Some(HidEndpointInfo {
        vid: attrs.VendorID,
        pid: attrs.ProductID,
        serial,
        manufacturer,
        product,
        path: path.to_string(),
        interface_number,
        usage_page: caps.usage_page,
        usage: caps.usage,
        collection: parsed.collection,
        report_lengths: caps.lengths,
        feature_reports: caps.feature_reports,
    }))
}

/// HID backend over the Windows HID class driver (see the module docs).
#[derive(Debug, Default, Clone, Copy)]
pub struct NativeHidBackend;

impl NativeHidBackend {
    /// The backend (stateless; nothing is enumerated until asked).
    pub fn new() -> Self {
        Self
    }

    fn enumerate_matching(&self, wanted: Wanted<'_>) -> HalResult<Vec<HidEndpointInfo>> {
        let paths = probe::hid_interface_paths().map_err(HalError::Backend)?;
        let mut out = Vec::new();
        for path in paths {
            // Skip without opening when the path already names another device
            // and no collection is wanted by usage.
            if let Some(key) = probe::path_vid_pid(&path) {
                if !wanted.id(key) && !wanted.any_usage() {
                    continue;
                }
            }
            match endpoint_info(&path, wanted, true) {
                Ok(Some(info)) => out.push(info),
                Ok(None) => {}
                Err(e) => tracing::debug!(path, error = %e, "HID collection skipped"),
            }
        }
        Ok(out)
    }

    fn group(endpoints: Vec<HidEndpointInfo>) -> Vec<DeviceGroup> {
        let probed = endpoints
            .into_iter()
            .map(|info| {
                let lengths = info.report_lengths;
                let mut ep = ProbedEndpoint::from_info(info);
                ep.usb_parent = own_usb_parent(probe::usb_parent(&ep.info.path), ep.info.vid, ep.info.pid);
                ep.caps = Ok(lengths);
                ep
            })
            .collect();
        group_endpoints(probed)
    }

    /// Every collection, grouped by physical device (see [`crate::group`]).
    pub fn enumerate_grouped(&self) -> HalResult<Vec<DeviceGroup>> {
        Ok(Self::group(self.enumerate_matching(Wanted::default())?))
    }

    /// Only the collections of devices whose `(vid, pid)` is in `wanted`
    /// (the devices a host has plugins for); others are never opened.
    pub fn enumerate_grouped_for(&self, wanted: &BTreeSet<(u16, u16)>) -> HalResult<Vec<DeviceGroup>> {
        self.enumerate_grouped_matching(wanted, &BTreeSet::new())
    }

    /// The collections of devices whose `(vid, pid)` is in `wanted`, plus the
    /// collections of any device whose `(usage page, usage)` is in `usages`
    /// (generic plugins such as HID LampArray). Collections of other devices
    /// are opened only to read their capabilities.
    pub fn enumerate_grouped_matching(
        &self,
        wanted: &BTreeSet<(u16, u16)>,
        usages: &BTreeSet<(u16, u16)>,
    ) -> HalResult<Vec<DeviceGroup>> {
        let started = std::time::Instant::now();
        let groups = Self::group(self.enumerate_matching(Wanted {
            ids: Some(wanted),
            usages: Some(usages),
        })?);
        tracing::debug!(
            elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
            groups = groups.len(),
            "native filtered HID enumeration"
        );
        Ok(groups)
    }
}

impl HidBackend for NativeHidBackend {
    fn enumerate(&self) -> HalResult<Vec<HidEndpointInfo>> {
        self.enumerate_matching(Wanted::default())
    }

    fn open(&self, path: &str) -> HalResult<Box<dyn HidHandle>> {
        Ok(Box::new(NativeHidHandle::open(path)?))
    }
}

/// The persistent overlapped read of a handle. Boxed so that the OVERLAPPED
/// and the buffer keep their addresses while a read is pending.
struct PendingRead {
    overlapped: OVERLAPPED,
    event: OwnedHandle,
    buf: Vec<u8>,
    pending: bool,
}

/// An open collection.
pub struct NativeHidHandle {
    info: HidEndpointInfo,
    handle: OwnedHandle,
    /// Opened without read/write access (a system mouse or keyboard collection).
    feature_only: bool,
    io_event: OwnedHandle,
    read: Box<PendingRead>,
}

// SAFETY: the kernel handles are process-wide; the OVERLAPPED of the pending
// read is only touched through `&mut self` (and in Drop), never shared.
unsafe impl Send for NativeHidHandle {}

impl NativeHidHandle {
    /// Open the collection at `path` for report I/O.
    pub fn open(path: &str) -> HalResult<Self> {
        let info = endpoint_info(path, Wanted::default(), false)
            .map_err(HalError::Backend)?
            .ok_or_else(|| HalError::NotFound(path.to_string()))?;
        // Windows owns system mice and keyboards exclusively; like hidapi, fall back to a
        // zero-access handle, which still carries feature reports (Razer mice, for one).
        let (handle, feature_only) = match open_path(path, GENERIC_READ.0 | GENERIC_WRITE.0, true) {
            Ok(h) => (h, false),
            Err(e) if e.code() == ERROR_ACCESS_DENIED.to_hresult() => {
                let h = open_path(path, 0, true).map_err(|e| map_win("CreateFileW(access=0)", &e))?;
                (h, true)
            }
            Err(e) => return Err(map_win("CreateFileW", &e)),
        };
        // SAFETY: valid handle; a larger queue only reduces lost input reports.
        let _ = unsafe { HidD_SetNumInputBuffers(handle.0, INPUT_BUFFERS) };
        let io_event = new_event()?;
        let read_event = new_event()?;
        let read = Box::new(PendingRead {
            overlapped: OVERLAPPED {
                hEvent: read_event.0,
                ..Default::default()
            },
            event: read_event,
            buf: vec![0u8; info.report_lengths.input.max(1)],
            pending: false,
        });
        tracing::debug!(path, feature_only, report_lengths = ?info.report_lengths, feature_reports = ?info.feature_reports, "opened HID endpoint (native)");
        Ok(Self {
            info,
            handle,
            feature_only,
            io_event,
            read,
        })
    }

    /// Wait for an overlapped operation just started with `ov`; on timeout
    /// cancel it and wait for the cancellation, so the kernel has released
    /// the buffers before this returns.
    fn finish(&self, ov: &mut OVERLAPPED, started: windows::core::Result<()>, what: &str) -> HalResult<u32> {
        if let Err(e) = started {
            if e.code() != ERROR_IO_PENDING.to_hresult() {
                return Err(map_win(what, &e));
            }
        }
        let mut n = 0u32;
        // SAFETY: `ov` belongs to the operation just started on this handle.
        match unsafe { GetOverlappedResultEx(self.handle.0, ov, &mut n, IO_TIMEOUT_MS, false) } {
            Ok(()) => Ok(n),
            Err(e) if is_timeout(&e) => {
                // SAFETY: cancel exactly this operation, then block until the kernel is done with it.
                unsafe {
                    let _ = CancelIoEx(self.handle.0, Some(ov));
                    let _ = GetOverlappedResult(self.handle.0, ov, &mut n, true);
                }
                Err(HalError::Timeout(IO_TIMEOUT_MS))
            }
            Err(e) => Err(map_win(what, &e)),
        }
    }

    fn io_overlapped(&self) -> OVERLAPPED {
        // SAFETY: manual-reset event owned by this handle.
        let _ = unsafe { ResetEvent(self.io_event.0) };
        OVERLAPPED {
            hEvent: self.io_event.0,
            ..Default::default()
        }
    }

    fn get_report(&mut self, code: u32, buf: &mut [u8], what: &str) -> HalResult<usize> {
        if buf.is_empty() {
            return Err(HalError::InvalidArgument("empty report buffer".into()));
        }
        let mut ov = self.io_overlapped();
        let len = buf.len() as u32;
        let ptr = buf.as_mut_ptr().cast::<core::ffi::c_void>();
        // SAFETY: the report id travels in the input buffer and the report comes back in the same
        // buffer (as hidapi does); `buf` and `ov` outlive the operation because `finish` waits for
        // completion or cancellation.
        let started = unsafe {
            DeviceIoControl(
                self.handle.0,
                code,
                Some(ptr as *const core::ffi::c_void),
                len,
                Some(ptr),
                len,
                None,
                Some(&mut ov),
            )
        };
        let mut n = self.finish(&mut ov, started, what)? as usize;
        // The class driver does not count the report-id byte of unnumbered reports.
        if buf[0] == 0 {
            n += 1;
        }
        Ok(n.min(buf.len()))
    }
}

fn feature_only_error() -> HalError {
    HalError::PermissionDenied(
        "Windows holds this mouse/keyboard collection exclusively: only feature reports are available".into(),
    )
}

/// `data`, zero-padded to `len` when shorter.
fn padded(data: &[u8], len: usize) -> std::borrow::Cow<'_, [u8]> {
    if data.len() >= len {
        std::borrow::Cow::Borrowed(data)
    } else {
        let mut v = data.to_vec();
        v.resize(len, 0);
        std::borrow::Cow::Owned(v)
    }
}

impl HidHandle for NativeHidHandle {
    fn info(&self) -> &HidEndpointInfo {
        &self.info
    }

    fn report_descriptor(&mut self) -> HalResult<Vec<u8>> {
        crate::hid::read_report_descriptor(&self.info.path)
    }

    fn write(&mut self, data: &[u8]) -> HalResult<usize> {
        if data.is_empty() {
            return Err(HalError::InvalidArgument("empty output report".into()));
        }
        if self.feature_only {
            return Err(feature_only_error());
        }
        let buf = padded(data, self.info.report_lengths.output);
        let mut ov = self.io_overlapped();
        // SAFETY: `buf` and `ov` outlive the operation: `finish` waits for completion or cancellation.
        let started = unsafe { WriteFile(self.handle.0, Some(&buf), None, Some(&mut ov)) };
        self.finish(&mut ov, started, "WriteFile")?;
        Ok(data.len())
    }

    fn read_timeout(&mut self, buf: &mut [u8], timeout_ms: i32) -> HalResult<usize> {
        if self.feature_only {
            return Err(feature_only_error());
        }
        let handle = self.handle.0;
        let r = &mut *self.read;
        if !r.pending {
            // SAFETY: manual-reset event owned by this read.
            let _ = unsafe { ResetEvent(r.event.0) };
            r.buf.fill(0);
            // SAFETY: `r.buf` and `r.overlapped` live in the Box until the read completes or Drop
            // cancels it and waits.
            match unsafe { ReadFile(handle, Some(&mut r.buf), None, Some(&mut r.overlapped)) } {
                Ok(()) => {}
                Err(e) if e.code() == ERROR_IO_PENDING.to_hresult() => {}
                Err(e) => return Err(map_win("ReadFile", &e)),
            }
            r.pending = true;
        }
        let wait = u32::try_from(timeout_ms).unwrap_or(INFINITE);
        let mut n = 0u32;
        // SAFETY: the OVERLAPPED of the pending read on this handle.
        match unsafe { GetOverlappedResultEx(handle, &r.overlapped, &mut n, wait, false) } {
            Ok(()) => r.pending = false,
            // Still pending: the next call collects the report.
            Err(e) if is_timeout(&e) => return Ok(0),
            Err(e) => {
                r.pending = false;
                return Err(map_win("ReadFile", &e));
            }
        }
        // Windows delivers the report id first (0x00 for unnumbered reports): kept, as the HAL
        // contract and SignalRGB plugins expect.
        let data = &r.buf[..(n as usize).min(r.buf.len())];
        let copy = data.len().min(buf.len());
        buf[..copy].copy_from_slice(&data[..copy]);
        Ok(copy)
    }

    fn send_feature_report(&mut self, data: &[u8]) -> HalResult<usize> {
        if data.is_empty() {
            return Err(HalError::InvalidArgument("empty feature report".into()));
        }
        let buf = padded(data, self.info.report_lengths.feature);
        // SAFETY: valid handle; pointer and length describe `buf`.
        if unsafe { HidD_SetFeature(self.handle.0, buf.as_ptr().cast(), buf.len() as u32) } {
            Ok(data.len())
        } else {
            Err(map_win("HidD_SetFeature", &windows::core::Error::from_thread()))
        }
    }

    fn get_feature_report(&mut self, buf: &mut [u8]) -> HalResult<usize> {
        self.get_report(IOCTL_HID_GET_FEATURE, buf, "IOCTL_HID_GET_FEATURE")
    }

    fn get_input_report(&mut self, buf: &mut [u8]) -> HalResult<usize> {
        self.get_report(IOCTL_HID_GET_INPUT_REPORT, buf, "IOCTL_HID_GET_INPUT_REPORT")
    }

    fn clear_read_buffer(&mut self) -> HalResult<()> {
        // SAFETY: valid handle.
        if unsafe { HidD_FlushQueue(self.handle.0) } {
            Ok(())
        } else {
            Err(map_win("HidD_FlushQueue", &windows::core::Error::from_thread()))
        }
    }
}

impl Drop for NativeHidHandle {
    fn drop(&mut self) {
        if self.read.pending {
            let mut n = 0u32;
            // SAFETY: cancel the outstanding read and block until the kernel has released
            // `read.buf` and `read.overlapped`; the handle is closed only after this returns.
            unsafe {
                let _ = CancelIoEx(self.handle.0, Some(&self.read.overlapped));
                let _ = GetOverlappedResult(self.handle.0, &self.read.overlapped, &mut n, true);
            }
            self.read.pending = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_reports_are_padded_long_ones_borrowed() {
        assert_eq!(&*padded(&[0x52, 1], 4), &[0x52, 1, 0, 0]);
        let long = [1u8, 2, 3];
        assert!(matches!(padded(&long, 2), std::borrow::Cow::Borrowed(_)));
        assert!(matches!(padded(&long, 0), std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn ioctl_codes_match_the_hid_class_definitions() {
        // CTL_CODE(FILE_DEVICE_KEYBOARD = 0x0B, function, METHOD_OUT_DIRECT = 2, FILE_ANY_ACCESS = 0)
        let ctl = |function: u32| (0x0B << 16) | (function << 2) | 2;
        assert_eq!(IOCTL_HID_GET_FEATURE, ctl(100));
        assert_eq!(IOCTL_HID_GET_INPUT_REPORT, ctl(104));
    }

    #[test]
    fn enumeration_is_read_only_and_consistent_with_the_path() {
        // Zero-access handles only; safe while other software owns the devices.
        let backend = NativeHidBackend::new();
        let endpoints = backend.enumerate().expect("enumerate");
        for e in &endpoints {
            if let Some((vid, pid)) = probe::path_vid_pid(&e.path) {
                assert_eq!((vid, pid), (e.vid, e.pid), "{}", e.path);
            }
            assert!(e.interface_number >= -1);
            assert!(e
                .feature_reports
                .iter()
                .all(|r| r.len == e.report_lengths.feature));
        }
        let wanted: BTreeSet<(u16, u16)> = endpoints.iter().take(1).map(|e| (e.vid, e.pid)).collect();
        let groups = backend.enumerate_grouped_for(&wanted).expect("filtered");
        assert!(groups.iter().all(|g| wanted.contains(&(g.key.vid, g.key.pid))));
    }

    #[test]
    fn native_and_hidapi_enumerations_agree() {
        // Both are read-only (zero-access handles); plugins must see the same tuples either way.
        type Row = (String, u16, u16, String, i32, u16, u16, u16, ReportLengths);
        fn rows(groups: Vec<DeviceGroup>) -> Vec<(String, Row)> {
            let mut out: Vec<_> = groups
                .into_iter()
                .flat_map(|g| {
                    let key = g.key.to_string();
                    g.endpoints.into_iter().map(move |e| {
                        let i = e.info;
                        (
                            key.clone(),
                            (
                                i.path.to_ascii_lowercase(),
                                i.vid,
                                i.pid,
                                i.serial,
                                i.interface_number,
                                i.usage_page,
                                i.usage,
                                i.collection,
                                i.report_lengths,
                            ),
                        )
                    })
                })
                .collect();
            out.sort_by(|a, b| a.1 .0.cmp(&b.1 .0));
            out
        }
        let native = rows(NativeHidBackend::new().enumerate_grouped().expect("native"));
        let hidapi = rows(
            crate::WindowsHidBackend::new()
                .expect("hidapi")
                .enumerate_grouped()
                .expect("hidapi"),
        );
        assert_eq!(native, hidapi);
    }
}
