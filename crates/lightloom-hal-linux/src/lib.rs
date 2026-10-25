// SPDX-License-Identifier: GPL-3.0-or-later
//! Linux HAL backends (M7).
//!
//! * [`sysfs`]: hidraw nodes from `/sys/class/hidraw` (identity, USB parent,
//!   report descriptor) without opening them;
//! * [`tlc`]: the report-descriptor split into top-level collections and the
//!   grouping into physical devices, giving the same
//!   `{interface, usage, usage_page, collection}` tuples as Windows;
//! * [`uevent`]: netlink uevent parsing (udev and kernel shapes);
//! * [`i2c`]: `/dev/i2c-*` adapter discovery by sysfs name and the
//!   [`SmbusBus`](lightloom_hal::SmbusBus) of each (Windows policy; the
//!   device and opener are Linux only);
//! * [`hwmon`]: read-only temperatures and fan speeds;
//! * [`tty`]: serial ports and the USB device behind each from sysfs (nothing
//!   opened); `LinuxSerialBackend` (Linux only) opens them over `serialport`;
//! * `hidraw` (Linux only): [`LinuxHidBackend`], report I/O on `/dev/hidrawN`;
//! * `hotplug` (Linux only): [`HidrawHotplug`], the udev netlink monitor.
//!
//! The pure modules compile on every OS so their tests (and the tuple-parity
//! fixtures shared with the Windows path logic) run in Windows CI too.
//! `unsafe` is confined to the Linux-only `sys` module (ioctls, poll, socket).
//!
//! Planned: `nusb` raw USB.

#![deny(unsafe_code)]

pub mod hwmon;
pub mod i2c;
pub mod sysfs;
pub mod tlc;
pub mod tty;
pub mod uevent;

#[cfg(test)]
mod fixtures;

#[cfg(target_os = "linux")]
pub mod hidraw;
#[cfg(target_os = "linux")]
pub mod hotplug;
#[cfg(target_os = "linux")]
mod sys;

#[cfg(target_os = "linux")]
pub use hidraw::LinuxHidBackend;
#[cfg(target_os = "linux")]
pub use hotplug::HidrawHotplug;
#[cfg(target_os = "linux")]
pub use tty::LinuxSerialBackend;
