// SPDX-License-Identifier: GPL-3.0-or-later
//! Renderer clients.
//!
//! A [`RendererClient`] produces canvas frames into a shared
//! [`FrameStore`]. Two implementations are planned:
//!
//! * [`NullRenderer`] (this crate, M1): solid / rainbow / test-pattern frames
//!   generated in-process at a fixed fps from a [`FrameClock`]. Used headless,
//!   in CI and by `lightloom light|rainbow|bench`.
//! * [`CefRenderer`]: runs `lightloom-renderer --serve` on an effect, publishes
//!   every frame of its shared-memory ring (see `lightloom_protocol::shm`) and
//!   supervises it (restart, keep-alive, 15 fps downshift).

#![forbid(unsafe_code)]

pub mod cef;
pub mod clock;
pub mod null;

pub use cef::{CefRenderer, CefRendererConfig, CefStatus};
pub use clock::FrameClock;
pub use null::{NullRenderer, Pattern};

use lightloom_engine::FrameStore;
use std::sync::Arc;

/// Errors from renderer clients.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// `start` called twice.
    #[error("renderer already running")]
    AlreadyRunning,
    /// The store has a different canvas size than the renderer.
    #[error("canvas size mismatch: renderer {renderer:?}, store {store:?}")]
    SizeMismatch {
        /// Renderer canvas `(w, h)`.
        renderer: (u32, u32),
        /// Store canvas `(w, h)`.
        store: (u32, u32),
    },
    /// Could not spawn the render thread / process.
    #[error("spawn failed: {0}")]
    Spawn(#[from] std::io::Error),
}

/// Something that publishes frames into a [`FrameStore`].
pub trait RendererClient: Send {
    /// Human-readable name for logs/UI.
    fn name(&self) -> &str;
    /// Begin publishing into `store`.
    fn start(&mut self, store: Arc<FrameStore>) -> Result<(), RenderError>;
    /// Stop publishing and release resources. Idempotent.
    fn stop(&mut self);
    /// Whether frames are currently being produced.
    fn is_running(&self) -> bool;
    /// One line about what happened (renderer restarts, republished frames), if anything to say.
    fn status_line(&self) -> Option<String> {
        None
    }
}
