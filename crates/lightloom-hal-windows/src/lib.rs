// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Windows HAL backends.
//!
//! * [`path`]: pure parser for Windows HID device-interface paths
//!   (`\\?\HID#VID_1532&PID_007C&MI_03&Col02#8&...#{guid}`) → interface
//!   number, collection number (`ColNN` hex-decoded, 1-based; 0 without
//!   `ColNN`), VID/PID, instance id. Compiles everywhere so the fixtures run on
//!   Linux CI too.
//! * [`hid`] (Windows only): [`WindowsHidBackend`], hidapi `windows-native`
//!   backed implementation of `lightloom_hal::HidBackend`, grouping top-level
//!   collections by their parent USB device.
//! * [`native`] (Windows only): [`NativeHidBackend`], the same contract
//!   straight over the HID class driver (no hidapi context, so no full
//!   enumeration before the first report; GET_REPORT(Input) supported).
//! * [`group`]: pure grouping of collections into physical devices (USB
//!   parent → serial → instance prefix), SignalRGB-shaped `vid:pid:serial` keys.
//! * [`probe`] (Windows only): read-only `CM_Get_Parent` walk and
//!   zero-access `HIDP_CAPS` report lengths.
//! * [`system`] (Windows only): running process names (conflict reporting) and
//!   this process's working set.
//! * [`console`] (Windows only): console control events, with close/log-off/
//!   shutdown held until devices are restored.
//! * [`registry`] (Windows only): read-only `HKEY_CURRENT_USER` string values
//!   (importing SignalRGB's component assignments).
//!
//! * [`hotplug`] (Windows only): HID arrival/removal notifications
//!   (`CM_Register_Notification`).
//!
//! Raw USB (`nusb`), serial, LampArray and the broker client follow in later milestones.

#![cfg_attr(not(windows), allow(dead_code))]

pub mod group;
pub mod path;

#[cfg(windows)]
pub mod console;
#[cfg(windows)]
pub mod hid;
#[cfg(windows)]
pub mod hotplug;
#[cfg(windows)]
pub mod native;
#[cfg(windows)]
pub mod probe;
#[cfg(windows)]
pub mod registry;
#[cfg(windows)]
pub mod system;

#[cfg(windows)]
pub use hid::WindowsHidBackend;
#[cfg(windows)]
pub use native::NativeHidBackend;

pub use group::{group_endpoints, DeviceGroup, GroupingSource, ProbedEndpoint};
