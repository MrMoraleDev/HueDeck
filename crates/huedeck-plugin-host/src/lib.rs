// SPDX-License-Identifier: Apache-2.0 OR MIT
//! SignalRGB-compatible plugin host (`compat-v1` profile).
//!
//! This milestone ships only the pieces that need no JavaScript engine:
//!
//! * [`meta`]: the export model — [`PluginMeta`](meta::PluginMeta) and the
//!   *call-if-function* rule for metadata exports.
//! * [`resolve`]: bare-specifier and relative module resolution
//!   (`@SignalRGB/*` → host shims, `./X.js` → inside the plugin root only).
//! * [`loader`]: the [`Loader`](loader::Loader) trait plus the VID:PID
//!   [`PluginIndex`](loader::PluginIndex) with SignalRGB's last-scanned-wins rule.
//!
//! The rquickjs-backed runtime (one runtime per plugin thread, `device.*` /
//! `bus.*` host objects, API_GAP trap, watchdog) follows in M0 spike B / M1.
// TODO(M1): add the `runtime` module on rquickjs 0.14 (QuickJS-NG).

#![forbid(unsafe_code)]

pub mod loader;
pub mod meta;
pub mod resolve;

pub use loader::{Loader, LoaderError, PluginIndex};
pub use meta::{ControllableParameter, DeviceType, PluginMeta, PluginType, ProductIdExport};
pub use resolve::{ModuleResolver, ResolveError, ResolvedModule};
