// SPDX-License-Identifier: GPL-3.0-or-later
//! Lightloom engine core.
//!
//! * [`color`]: [`Rgb`](color::Rgb) and the global × layout × device brightness math.
//! * [`frame`]: a BGRA canvas [`Frame`](frame::Frame) (320x200 by default).
//! * [`frame_store`]: [`FrameStore`](frame_store::FrameStore), the in-process
//!   triple buffer with a monotonically increasing sequence number and a
//!   condvar so plugin threads wake once per published frame.
//! * [`layout`]: [`Layout`](layout::Layout) / [`Placement`](layout::Placement)
//!   mirroring `schemas/layout.schema.json`.
//! * [`sampler`]: precomputed per-LED pixel-offset tables so `device.color(x, y)`
//!   never allocates or does trigonometry on the hot path.
//! * [`direct`]: [`DirectLeds`](direct::DirectLeds), per-endpoint colours set
//!   from outside the canvas (the OpenRGB SDK server) that win over sampling.
//! * [`components`]: component descriptors (SignalRGB component JSON), channel
//!   instances and the descriptor library.
//!
//! Registry, scheduler, persistence and the import wizard land here in later
//! milestones.

#![forbid(unsafe_code)]

pub mod color;
pub mod components;
pub mod direct;
pub mod frame;
pub mod frame_store;
pub mod layout;
pub mod sampler;

pub use color::{apply_brightness, effective_brightness, Rgb};
pub use components::{
    parse_channel, Component, ComponentInstance, ComponentLibrary, LibraryEntry, LoadIssue,
};
pub use direct::DirectLeds;
pub use frame::Frame;
pub use frame_store::FrameStore;
pub use layout::{
    CanvasSize, ColorCorrection, DeviceGeometry, Layout, LightingMode, Placement, Point, Scale,
    CANVAS_HEIGHT, CANVAS_WIDTH,
};
pub use sampler::{compute_offsets, OffsetTable, RotationPivot, SamplePoint, SamplerConfig, OFF_CANVAS};
