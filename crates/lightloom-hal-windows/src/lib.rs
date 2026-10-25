// SPDX-License-Identifier: GPL-3.0-or-later
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
//!   (importing SignalRGB's component assignments) and `HKEY_LOCAL_MACHINE`
//!   ones (whether PawnIO is installed); the `HKEY_CURRENT_USER` writer for
//!   the opt-in `lightloom://` URL scheme.
//! * [`broker`]: client of the privileged `lightloom-iobroker` (chipset SMBus,
//!   RAM lighting) as `lightloom_hal::SmbusBus` objects; the named-pipe
//!   transport is Windows only.
//! * [`pawnio`]: whether the PawnIO driver is installed (its uninstall entry,
//!   never the device), what to tell the user, and the contract with the
//!   elevated helper `lightloom-setup`.
//! * [`elevate`] (Windows only): start a program through the UAC prompt
//!   (`runas`) on an explicit user action, and wait for it.
//! * [`service`] (Windows only): read-only state of the I/O broker service.
//! * [`owner`] (Windows only): the named mutex that lets one Lightloom process
//!   at a time drive SMBus / GPU I2C.
//!
//! * [`hotplug`] (Windows only): HID and USB device arrival/removal
//!   notifications (`CM_Register_Notification`).
//! * [`usb`]: [`WinUsbBackend`] (Windows only), raw USB over WinUSB with
//!   `nusb` for `rawusb`/`hybrid` plugins; it claims only interfaces WinUSB
//!   owns and never installs a driver. The driver bookkeeping is pure.
//! * [`toast`] (Windows only): system notifications as toasts, under a
//!   per-user AppUserModelID registration (`lightloom_hal::Notifier`).
//!
//! * [`lamparray`] (Windows only): Windows Dynamic Lighting LampArrays through
//!   WinRT `Windows.Devices.Lights` (`DeviceWatcher` discovery, availability,
//!   package-identity check).
//! * [`power`] (Windows only): sleep/resume, lock/unlock and display off/on
//!   from a hidden window (`lightloom_hal::PowerSource`).
//!
//! * [`serial`] (Windows and Linux): COM port enumeration with the USB
//!   VID:PID behind each port (Windows) and port I/O over `serialport`
//!   (shared with the Linux backend).

#![cfg_attr(not(windows), allow(dead_code))]

pub mod broker;
pub mod group;
pub mod path;
pub mod pawnio;
pub mod usb;

#[cfg(windows)]
pub mod console;
#[cfg(windows)]
pub mod elevate;
#[cfg(windows)]
pub mod hid;
#[cfg(windows)]
pub mod hotplug;
#[cfg(windows)]
pub mod lamparray;
#[cfg(windows)]
pub mod native;
#[cfg(windows)]
pub mod owner;
#[cfg(windows)]
pub mod power;
#[cfg(windows)]
pub mod probe;
#[cfg(windows)]
pub mod registry;
#[cfg(any(windows, target_os = "linux"))]
pub mod serial;
#[cfg(windows)]
pub mod service;
#[cfg(windows)]
pub mod system;
#[cfg(windows)]
pub mod toast;

#[cfg(windows)]
pub use hid::WindowsHidBackend;
#[cfg(windows)]
pub use native::NativeHidBackend;
pub use serial::WindowsSerialBackend;
#[cfg(windows)]
pub use usb::WinUsbBackend;

pub use group::{group_endpoints, DeviceGroup, GroupingSource, ProbedEndpoint};
