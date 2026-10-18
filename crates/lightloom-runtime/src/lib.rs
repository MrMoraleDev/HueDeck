// SPDX-License-Identifier: GPL-3.0-or-later
//! The device runtime `lightloom` (CLI runs) and `lightloomd` (the daemon) share:
//!
//! * [`hardware`]: the HID backend of this OS and the present devices a plugin claims;
//! * [`hotplug`]: arrival/removal notifications and how long to let them settle;
//! * [`conflicts`]: other RGB software holding the same devices (reported, never killed);
//! * [`signalrgb`]: a local SignalRGB installation's setup, read in place (channel
//!   components, placements, effects);
//! * [`system`]: machine facts for `@SignalRGB/systeminfo` and the process's memory.

#![forbid(unsafe_code)]

pub mod conflicts;
pub mod effects;
pub mod hardware;
pub mod hotplug;
pub mod signalrgb;
pub mod system;
