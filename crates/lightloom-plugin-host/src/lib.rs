// SPDX-License-Identifier: GPL-3.0-or-later
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
//!   through lightloom-hal, canvas sampling, I/O statistics, dry-run writes).
//! * [`children`]: subdevices and channels (with their component instances),
//!   each placed and sampled on the canvas on its own.
//! * [`session`]: one device session = one plugin thread running
//!   Validate → Initialize → Render per frame → Shutdown.
//! * [`manager`]: one session per present device, following hotplug
//!   (stop on removal, restart behind a stale handle, start on arrival).
//! * [`catalog`]: plugin discovery over plugin directories (VID:PID index).
//! * [`save`]: storing lighting in a device (`SaveToDevice`) and the
//!   shutdown modes (`Shutdown(SystemSuspending, mode)`), Lightloom extensions.
//! * [`stats`]: fixed-memory latency histograms for long-running sessions.
//! * [`exports`]: namespace JSON → [`PluginMeta`].
//! * [`dryrun`]: the `lightloom compat dry-run` corpus harness.
//! * [`smbus`]: the SMBus policy (scan window, allowlist, SPD write deny,
//!   transaction lock, dry-run) every `bus.*` call goes through; [`bus`]: the
//!   natives behind `bus.*`; [`discover`]: `Scan(bus)` of the SMBus plugins
//!   offered each bus, which yields the SMBus devices; [`spd`]: the SPD
//!   check that gates RAM plugins on chipset buses.

#![forbid(unsafe_code)]

pub mod bus;
pub mod catalog;
pub mod children;
pub mod device;
pub mod discover;
pub mod dryrun;
pub mod exports;
pub mod loader;
pub mod manager;
pub mod meta;
pub mod resolve;
pub mod runtime;
pub mod save;
pub mod session;
pub mod smbus;
pub mod spd;
pub mod stats;

pub use loader::{Loader, LoaderError, PluginIndex};
pub use meta::{ControllableParameter, DeviceType, GpuIdentifier, PluginMeta, PluginType, ProductIdExport};
pub use resolve::{ModuleResolver, ResolveError, ResolvedModule};
