// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Lightscript (effect HTML) metadata.
//!
//! An effect is one self-contained HTML file whose `<head>` carries the
//! engine-facing metadata as attributes on `<meta>` tags:
//!
//! ```html
//! <title>Name</title>
//! <meta description="…"/>
//! <meta publisher="…"/>
//! <meta property="speed" label="Speed" type="number" min="1" max="10" default="3" tooltip="…"/>
//! <meta meter="health" type="linear" x="0.1" y="0.9" width="0.3" height="0.02" h="0-20" s="60-100" l="30-70">
//!   <resolution size="1920x1080" x="0.1" y="0.88" width="0.3" height="0.03"/>
//! ```
//!
//! [`parse_effect_meta`] turns that into an [`EffectMeta`] matching
//! `schemas/effect-meta.schema.json`; [`ControlMeta::js_initializer`] produces
//! the `var <prop> = <value>;` statement the engine evaluates after the page
//! scripts ran (strings quoted, numbers/booleans raw).

#![forbid(unsafe_code)]

pub mod model;
pub mod parse;

pub use model::{
    ControlMeta, ControlType, ControlValue, DisplaySize, EffectMeta, HslRanges, MeterMeta, MeterType, Range,
    Region, ResolutionOverride, Uses,
};
pub use parse::parse_effect_meta;
