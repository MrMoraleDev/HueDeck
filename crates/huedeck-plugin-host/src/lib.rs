// SPDX-License-Identifier: Apache-2.0 OR MIT
//! SignalRGB-compatible plugin host (`compat-v1` profile).
//!
//! Engine-independent pieces:
//!
//! * [`meta`]: the export model — [`PluginMeta`] and the
//!   *call-if-function* rule for metadata exports.
//! * [`resolve`]: bare-specifier and relative module resolution
//!   (`@SignalRGB/*` → host shims, `./X.js` → inside the plugin root only).
//! * [`loader`]: the [`Loader`] trait plus the VID:PID
//!   [`PluginIndex`] with SignalRGB's last-scanned-wins rule.
//!
//! rquickjs-backed pieces (M0 spike B):
//!
//! * [`runtime`]: one QuickJS-NG runtime + context per plugin on its own OS
//!   thread, 64 MiB heap cap, deadline-driven interrupt handler, the
//!   `@SignalRGB/*` resolver/loader over the embedded `prelude/`, and the
//!   recording stub host (`js/stub_host.js`).
//! * [`exports`]: namespace JSON → [`PluginMeta`].
//! * [`dryrun`]: the `huedeck compat dry-run` corpus harness.
// TODO(M1/M2): replace the stub host with real `device.*` / `bus.*` objects
// backed by huedeck-hal, and the dry-run loop with the engine's plugin threads.

#![forbid(unsafe_code)]

pub mod dryrun;
pub mod exports;
pub mod loader;
pub mod meta;
pub mod resolve;
pub mod runtime;

pub use loader::{Loader, LoaderError, PluginIndex};
pub use meta::{ControllableParameter, DeviceType, PluginMeta, PluginType, ProductIdExport};
pub use resolve::{ModuleResolver, ResolveError, ResolvedModule};
