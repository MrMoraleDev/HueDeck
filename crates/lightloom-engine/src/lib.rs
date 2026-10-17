// SPDX-License-Identifier: Apache-2.0 OR MIT
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
//!
//! Registry, scheduler, channels/components, persistence and the import
//! wizard land here in later milestones.

#![forbid(unsafe_code)]

pub mod color;
pub mod frame;
pub mod frame_store;
pub mod layout;
pub mod sampler;

pub use color::{apply_brightness, effective_brightness, Rgb};
pub use frame::Frame;
pub use frame_store::FrameStore;
pub use layout::{
    CanvasSize, ColorCorrection, DeviceGeometry, Layout, LightingMode, Placement, Point, Scale, CANVAS_HEIGHT,
    CANVAS_WIDTH,
};
pub use sampler::{compute_offsets, OffsetTable, RotationPivot, SamplePoint, SamplerConfig, OFF_CANVAS};
