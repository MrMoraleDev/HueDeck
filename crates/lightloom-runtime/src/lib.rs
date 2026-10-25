// SPDX-License-Identifier: GPL-3.0-or-later
//! The device runtime `lightloom` (CLI runs) and `lightloomd` (the daemon) share:
//!
//! * [`hardware`]: the HID backend of this OS and the present devices a plugin claims;
//! * [`smbus`]: the SMBus buses (NVIDIA GPU I2C, chipset SMBus; Linux `/dev/i2c-*`)
//!   and the devices SMBus plugins find on them;
//! * [`sensors`]: the sensor backends (Linux hwmon);
//! * [`hotplug`]: arrival/removal notifications and how long to let them settle;
//! * [`lamparray`]: the OS LampArray backend (Windows Dynamic Lighting) and which of
//!   its devices Lightloom drives itself (those no plugin owns);
//! * [`conflicts`]: other RGB software holding the same devices (reported, never killed);
//! * [`signalrgb`]: a local SignalRGB installation's setup, read in place (channel
//!   components, placements, effects);
//! * [`notify`]: this OS's system notifications (Windows toasts);
//! * [`dirs`]: Lightloom's data and config directories and the daemon's `api-port` file;
//! * [`pawnio`]: PawnIO / RAM lighting facts (read-only) and the elevated
//!   helper run behind the dashboard's Install / Remove buttons;
//! * [`system`]: machine facts for `@SignalRGB/systeminfo` and the process's memory.
//! * [`user_registry`]: the per-user registry for keys Lightloom writes on request
//!   (the opt-in `lightloom://` URL scheme).

#![forbid(unsafe_code)]

pub mod conflicts;
pub mod dirs;
pub mod effects;
pub mod hardware;
pub mod hotplug;
pub mod lamparray;
pub mod notify;
pub mod pawnio;
pub mod sensors;
pub mod signalrgb;
pub mod smbus;
pub mod system;
pub mod user_registry;
