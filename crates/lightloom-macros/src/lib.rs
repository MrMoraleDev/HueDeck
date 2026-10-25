// SPDX-License-Identifier: GPL-3.0-or-later
//! The macro runtime (docs/host-api/macros-system.md, docs/macros.md):
//! SignalRGB-shaped macro scripts, Actions (`export function Action()`,
//! `onExecute`) and Inputs (`export function Input()`, `onApplication`,
//! `onKey`, `onMouse`, `block.trigger()`), each in its own QuickJS runtime on
//! its own thread.
//!
//! * [`script`]: one script's runtime and the host globals (`js/macro_host.js`).
//! * [`options`]: `this.Options` descriptors and their values.
//! * [`keys`]: key and button names, combos, `keyString`.
//! * [`host`]: what the daemon provides ([`host::MacroHost`]) and `signalrgb.*`.
//! * [`manager`]: folders, configuration, workers, the event bus, the hooks.
//!
//! Every effect on the desktop goes through `lightloom_hal::input::Desktop`;
//! tests use `RecordingDesktop`, so they never inject input.

#![forbid(unsafe_code)]

pub mod host;
pub mod keys;
pub mod manager;
pub mod options;
pub mod script;

pub use host::{EngineCall, EngineState, MacroHost, Prompt};
pub use manager::{Kind, MacroConfig, MacroError, MacroManager, MacrosView, ScriptConfig, ScriptDir};
