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
//! rquickjs-backed pieces:
//!
//! * [`runtime`]: one QuickJS-NG runtime + context per plugin on its own OS
//!   thread, 64 MiB heap cap, deadline-driven interrupt handler, the
//!   `@SignalRGB/*` resolver/loader over the embedded `prelude/`, and the
//!   compat host module (`js/host.js`: recording stubs, or device natives).
//! * [`device`]: the native backing of `device.*` for a real device (HID I/O
//!   through huedeck-hal, canvas sampling, I/O statistics, dry-run writes).
//! * [`session`]: one device session = one plugin thread running
//!   Validate → Initialize → Render per frame → Shutdown.
//! * [`catalog`]: plugin discovery over plugin directories (VID:PID index).
//! * [`exports`]: namespace JSON → [`PluginMeta`].
//! * [`dryrun`]: the `huedeck compat dry-run` corpus harness.
// TODO(M2): `bus.*` (SMBus), channels/components, subdevices, hotplug, watchdog fault policy.

#![forbid(unsafe_code)]

pub mod catalog;
pub mod device;
pub mod dryrun;
pub mod exports;
pub mod loader;
pub mod meta;
pub mod resolve;
pub mod runtime;
pub mod session;

pub use loader::{Loader, LoaderError, PluginIndex};
pub use meta::{ControllableParameter, DeviceType, PluginMeta, PluginType, ProductIdExport};
pub use resolve::{ModuleResolver, ResolveError, ResolvedModule};
