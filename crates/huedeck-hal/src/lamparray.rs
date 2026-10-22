// SPDX-License-Identifier: GPL-3.0-or-later
//! LampArray backend (Windows Dynamic Lighting via WinRT, HID LampArray on Linux).
//!
//! HueDeck consumes LampArray devices as a generic backend so DL-certified
//! keyboards light without a plugin. Plugins skip LampArray collections.

use crate::error::HalResult;
use serde::{Deserialize, Serialize};

/// HID usage page of LampArray collections (skipped by plugins).
pub const LAMPARRAY_USAGE_PAGE: u16 = 0x59;
/// HID usage of the LampArray top-level collection.
pub const LAMPARRAY_USAGE: u16 = 0x01;

/// Kind of LampArray (HID LampArrayKind).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LampArrayKind {
    /// Keyboard.
    Keyboard,
    /// Mouse.
    Mouse,
    /// Game controller.
    GameController,
    /// Peripheral.
    Peripheral,
    /// Scene lighting.
    Scene,
    /// Notification.
    Notification,
    /// Chassis.
    Chassis,
    /// Wearable.
    Wearable,
    /// Furniture.
    Furniture,
    /// Art.
    Art,
    /// Unknown / vendor.
    Other(u32),
}

/// One lamp of a LampArray.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LampInfo {
    /// Lamp index.
    pub index: u32,
    /// Position in micrometres relative to the array bounding box.
    pub position_um: [i32; 3],
    /// Whether the lamp is a programmable RGB lamp.
    pub is_rgb: bool,
}

/// Static description of a LampArray device.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LampArrayInfo {
    /// Backend-specific id (device interface path / WinRT DeviceId).
    pub id: String,
    /// Product name.
    pub name: String,
    /// Kind.
    pub kind: LampArrayKind,
    /// Bounding box in micrometres.
    pub bounding_box_um: [u32; 3],
    /// Minimum update interval in microseconds.
    pub min_update_interval_us: u32,
    /// Lamps.
    pub lamps: Vec<LampInfo>,
}

/// An open LampArray.
pub trait LampArrayHandle: Send {
    /// Static information.
    fn info(&self) -> &LampArrayInfo;
    /// Set every lamp colour (RGBA, one entry per lamp index) in one update.
    fn set_colors(&mut self, rgba: &[[u8; 4]]) -> HalResult<()>;
    /// Return control to the OS/firmware (autonomous mode).
    fn release(&mut self) -> HalResult<()>;
}

/// Enumerates and opens LampArray devices.
pub trait LampArrayBackend: Send + Sync {
    /// Enumerate LampArray devices.
    fn enumerate(&self) -> HalResult<Vec<LampArrayInfo>>;
    /// Open by id.
    fn open(&self, id: &str) -> HalResult<Box<dyn LampArrayHandle>>;
}
