// SPDX-License-Identifier: GPL-3.0-or-later
//! LCD faces: what sits between a face renderer and a plugin's `@SignalRGB/lcd` calls.
//!
//! An LCD device's plugin calls `LCD.initialize({width, height, circular})`, then
//! `LCD.getFrame({format, quality})` in every `Render()` and streams the bytes to the
//! screen. The frames come from a *face*: an HTML page rendered at the LCD's own size and
//! frame rate (docs/host-api/lcd-lightloom.md). This crate holds the engine-independent
//! pieces:
//!
//! * [`spec`]: the `LCD.initialize` settings, validated as SignalRGB does.
//! * [`format`]: the `getFrame` formats and their encoders (JPEG, PNG, BMP, RGBA, RGB,
//!   RGB565); the two NZXT formats have no documented layout and fail with
//!   [`LcdError::UnsupportedFormat`] instead of a guess.
//! * [`geometry`]: `getImageBuffer` crops (scaled to an output size) and `getSlicedFrame`
//!   grids.
//! * [`provider`]: [`LcdProvider`], what a device session asks for the frames of its
//!   face (the daemon starts a renderer per LCD device), and [`MockLcdProvider`], a
//!   renderer-free provider for tests and machines without the CEF renderer.
//!
//! Frames are [`lightloom_engine::Frame`]s (BGRA8, premultiplied as CEF paints them); every
//! encoder composites them over black, so the output is opaque.

#![forbid(unsafe_code)]

pub mod format;
pub mod geometry;
pub mod provider;
pub mod spec;

pub use format::{encode, LcdFormat};
pub use geometry::{crop_scale, slice, ImageRequest};
pub use provider::{LcdAttachment, LcdProvider, MockLcdProvider};
pub use spec::LcdSpec;

/// Why an `@SignalRGB/lcd` call failed. [`LcdError::code`] is what the JS side sees as
/// `error.code`; the messages follow SignalRGB's where those are known.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LcdError {
    /// `getFrame` / `getSlicedFrame` / `getImageBuffer` before `initialize`.
    #[error("LCD.{call}(): LCD not initialized. Call LCD.initialize() first.")]
    NotInitialized {
        /// The member called.
        call: &'static str,
    },
    /// Bad `initialize` settings (SignalRGB's messages).
    #[error("{0}")]
    InvalidSettings(String),
    /// A format name that is not one of the eight.
    #[error("Invalid image format: {0}. Valid formats are: JPEG, PNG, BMP, RGBA, RGB, RGB565, NZXT::RGB565, NZXT::Q565")]
    InvalidFormat(String),
    /// A known format whose byte layout is not documented anywhere Lightloom may use:
    /// failing beats sending a device bytes in a guessed layout.
    #[error("LCD format {format} is not supported by this host: {reason}")]
    UnsupportedFormat {
        /// The format's canonical name.
        format: &'static str,
        /// What is unknown about it.
        reason: &'static str,
    },
    /// A `getSlicedFrame` grid that does not divide the canvas into cells.
    #[error("{0}")]
    InvalidGrid(String),
    /// A `getImageBuffer` region or output size out of range.
    #[error("{0}")]
    OutOfBounds(String),
    /// The encoder failed (should not happen for valid frames).
    #[error("LCD frame encoding failed: {0}")]
    Encode(String),
}

impl LcdError {
    /// Stable code for scripts (`error.code` in JS).
    pub fn code(&self) -> &'static str {
        match self {
            LcdError::NotInitialized { .. } => "NOT_INITIALIZED",
            LcdError::InvalidSettings(_) => "INVALID_SETTINGS",
            LcdError::InvalidFormat(_) => "INVALID_FORMAT",
            LcdError::UnsupportedFormat { .. } => "UNSUPPORTED_FORMAT",
            LcdError::InvalidGrid(_) => "INVALID_GRID",
            LcdError::OutOfBounds(_) => "OUT_OF_BOUNDS",
            LcdError::Encode(_) => "ENCODE_FAILED",
        }
    }
}
