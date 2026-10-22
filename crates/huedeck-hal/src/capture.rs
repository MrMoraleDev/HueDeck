// SPDX-License-Identifier: GPL-3.0-or-later
//! Screen capture source feeding `engine.zone.*` (28x20 HSL grid + 160x100 RGBA).

use crate::error::HalResult;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A display that can be captured.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayInfo {
    /// Backend-specific id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Native width in pixels.
    pub width: u32,
    /// Native height in pixels.
    pub height: u32,
    /// Whether this is the primary display.
    pub primary: bool,
}

/// A captured (possibly downscaled) frame, BGRA8, row-major, no padding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureFrame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes, BGRA.
    pub bgra: Vec<u8>,
    /// Time since the backend started.
    pub timestamp: Duration,
}

/// Screen capture backend (DXGI/WGC on Windows, PipeWire/X11 on Linux).
pub trait CaptureBackend: Send {
    /// Enumerate displays.
    fn displays(&self) -> HalResult<Vec<DisplayInfo>>;
    /// Capture `display` scaled to `width x height`.
    fn capture(&mut self, display: &str, width: u32, height: u32) -> HalResult<CaptureFrame>;
}
