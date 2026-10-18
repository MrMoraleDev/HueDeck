// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Windows HAL backends.
//!
//! * [`path`]: pure parser for Windows HID device-interface paths
//!   (`\\?\HID#VID_1532&PID_007C&MI_03&Col02#8&...#{guid}`) → interface
//!   number, collection number (`ColNN` hex-decoded, 1-based; 0 without
//!   `ColNN`), VID/PID, instance id. Compiles everywhere so the fixtures run on
//!   Linux CI too.
//! * [`hid`] (Windows only): [`WindowsHidBackend`], hidapi `windows-native`
//!   backed implementation of `huedeck_hal::HidBackend`, grouping top-level
//!   collections by their parent USB device.
//! * [`group`]: pure grouping of collections into physical devices (USB
//!   parent → serial → instance prefix), SignalRGB-shaped `vid:pid:serial` keys.
//! * [`probe`] (Windows only): read-only `CM_Get_Parent` walk and
//!   zero-access `HIDP_CAPS` report lengths.
//! * [`system`] (Windows only): running process names (conflict reporting) and
//!   this process's working set.
//!
//! Hotplug (`CM_Register_Notification`), raw USB (`nusb`), serial, LampArray and the broker
//! client follow in later milestones.

#![cfg_attr(not(windows), allow(dead_code))]

pub mod group;
pub mod path;

#[cfg(windows)]
pub mod hid;
#[cfg(windows)]
pub mod probe;
#[cfg(windows)]
pub mod system;

#[cfg(windows)]
pub use hid::WindowsHidBackend;

pub use group::{group_endpoints, DeviceGroup, GroupingSource, ProbedEndpoint};
