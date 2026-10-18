// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Hotplug notifications.

use crate::error::HalResult;
use crate::key::DeviceKey;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::Receiver;

/// Which backend the arriving/removed device belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeviceClass {
    /// HID top-level collection.
    Hid,
    /// Raw USB device.
    Usb,
    /// Serial port.
    Serial,
    /// LampArray (Dynamic Lighting) device.
    LampArray,
}

/// A device appeared or disappeared.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceEvent {
    /// A device was connected.
    Arrived {
        /// Device identity.
        key: DeviceKey,
        /// Backend class.
        class: DeviceClass,
        /// Backend path (for `open`).
        path: String,
    },
    /// A device was removed.
    Removed {
        /// Device identity.
        key: DeviceKey,
        /// Backend class.
        class: DeviceClass,
        /// Backend path that is now gone.
        path: String,
    },
}

impl DeviceEvent {
    /// Device identity of the event.
    pub fn key(&self) -> &DeviceKey {
        match self {
            DeviceEvent::Arrived { key, .. } | DeviceEvent::Removed { key, .. } => key,
        }
    }
}

/// Source of [`DeviceEvent`]s. A plug or unplug arrives as a burst of events
/// (one per HID collection); consumers wait for it to settle, then enumerate.
pub trait HotplugSource: Send {
    /// Start watching and return the receiving end of the event channel.
    fn subscribe(&mut self) -> HalResult<Receiver<DeviceEvent>>;
    /// Stop watching; the channel closes.
    fn stop(&mut self);
}
