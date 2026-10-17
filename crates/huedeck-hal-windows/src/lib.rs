// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Windows HAL backends.
//!
//! * [`path`]: pure parser for Windows HID device-interface paths
//!   (`\\?\HID#VID_1532&PID_007C&MI_03&Col02#8&...#{guid}`) → interface
//!   number, 0-based collection index, VID/PID, instance id. Compiles everywhere
//!   so the fixtures run on Linux CI too.
//! * [`hid`] (Windows only): [`WindowsHidBackend`], hidapi `windows-native`
//!   backed implementation of `huedeck_hal::HidBackend`, grouping top-level
//!   collections by their parent USB device.
//!
//! Hotplug (`CM_Register_Notification`), exact parent association
//! (`CM_Get_Parent`), raw USB (`nusb`), serial, LampArray and the broker
//! client follow in later milestones.

#![cfg_attr(not(windows), allow(dead_code))]

pub mod path;

#[cfg(windows)]
pub mod hid;

#[cfg(windows)]
pub use hid::{DeviceGroup, WindowsHidBackend};
