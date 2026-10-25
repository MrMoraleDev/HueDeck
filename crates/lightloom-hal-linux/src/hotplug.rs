// SPDX-License-Identifier: GPL-3.0-or-later
//! hidraw and raw USB device arrival and removal through the udev netlink
//! monitor, the Linux [`HotplugSource`]. Pure Rust (no libudev): a `NETLINK_KOBJECT_UEVENT`
//! socket bound to the udev multicast group, so events arrive after
//! systemd-udevd has applied the rules (including `uaccess`).
//!
//! `add` and `change` become [`DeviceEvent::Arrived`] (since systemd 258 ACLs
//! are re-applied through `change` events, so an open that failed with
//! `EACCES` is worth retrying); `remove` becomes [`DeviceEvent::Removed`].
//! As on Windows, keys carry VID/PID (from the `DEVPATH`) and the serial only
//! when sysfs still has it; consumers wait for the burst to settle and
//! enumerate again.
//!
//! Inside a Flatpak, netlink multicast needs the host network namespace
//! (`--share=network`); without udevd no event arrives and hotplug is inert.

use crate::sys::{self, Ready};
use crate::uevent;
use lightloom_hal::hotplug::{DeviceEvent, HotplugSource};
use lightloom_hal::{HalError, HalResult};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread::JoinHandle;

/// udev's monitor multicast group (`MONITOR_GROUP_UDEV`).
const UDEV_GROUP: u32 = 2;
/// How often the watcher thread checks for [`HotplugSource::stop`].
const POLL_MS: i32 = 250;

/// hidraw hotplug notifications (see the module docs).
#[derive(Debug, Default)]
pub struct HidrawHotplug {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl HidrawHotplug {
    /// A watcher that is not watching yet.
    pub fn new() -> Self {
        Self::default()
    }
}

impl HotplugSource for HidrawHotplug {
    fn subscribe(&mut self) -> HalResult<Receiver<DeviceEvent>> {
        self.stop();
        let socket =
            sys::uevent_socket(UDEV_GROUP).map_err(|e| HalError::Backend(format!("udev monitor: {e}")))?;
        let (tx, rx) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        self.stop = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("hidraw-hotplug".into())
            .spawn(move || {
                let mut buf = vec![0u8; 8192];
                while !stop.load(Ordering::Acquire) {
                    match sys::poll_in(&socket, POLL_MS) {
                        Ok(Ready::Readable) => {}
                        Ok(Ready::Timeout) => continue,
                        Ok(Ready::Hangup) | Err(_) => break,
                    }
                    loop {
                        let n = match sys::recv(&socket, &mut buf) {
                            Ok(n) => n,
                            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                            Err(_) => break, // WouldBlock: drained
                        };
                        let Some(ev) = uevent::parse(&buf[..n]) else {
                            continue;
                        };
                        if let Some(event) =
                            uevent::device_event(&ev, &crate::sysfs::RealFs, Path::new("/sys"))
                        {
                            tracing::debug!(?event, "hidraw hotplug");
                            if tx.send(event).is_err() {
                                return;
                            }
                        }
                    }
                }
            })
            .map_err(HalError::Io)?;
        self.thread = Some(thread);
        Ok(rx)
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for HidrawHotplug {
    fn drop(&mut self) {
        self.stop();
    }
}
