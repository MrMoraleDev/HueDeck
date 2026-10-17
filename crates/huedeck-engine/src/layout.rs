// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Layouts: where each canvas endpoint sits on the 320x200 canvas.
//!
//! The JSON form is pinned by `schemas/layout.schema.json`: camelCase keys
//! (`gridSize`, `flipX`, `flipY`, `lightingMode`, `forcedColor`,
//! `colorCorrection`), placements keyed by endpoint uid. SignalRGB's persisted
//! names (`flipped`, `flippedV`, `ColorCorrection`) are translated by the
//! import wizard, not here.
//!
//! Sampling contract (schema description): canvas = position + rotate(scale *
//! ledPosition), rotation about the endpoint's top-left corner applied before
//! translation; `flipX`/`flipY` mirror the footprint. Effective LED brightness
//! = global x layout.brightness x endpoint.brightness (each 0-100).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Visible canvas width.
pub const CANVAS_WIDTH: u32 = 320;
/// Visible canvas height.
pub const CANVAS_HEIGHT: u32 = 200;
/// Default snap grid (SignalRGB `layouts/gridSize`).
pub const DEFAULT_GRID_SIZE: u32 = 4;
/// Allowed `DefaultScale()` range (exe: "Value must be between 0.5 and 75.0").
pub const SCALE_MIN: f32 = 0.5;
/// Upper bound of the scale range.
pub const SCALE_MAX: f32 = 75.0;
/// Schema default of `forcedColor`.
pub const DEFAULT_FORCED_COLOR: &str = "#009bde";

/// A 2-D point in canvas pixels (schema `vec2`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// X, canvas pixels from the left.
    pub x: f32,
    /// Y, canvas pixels from the top.
    pub y: f32,
}

/// Independent X/Y scale factors (canvas pixels per LED cell).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scale {
    /// Horizontal.
    pub x: f32,
    /// Vertical.
    pub y: f32,
}

impl Scale {
    /// Uniform scale.
    pub fn uniform(s: f32) -> Self {
        Self { x: s, y: s }
    }
}

impl Default for Scale {
    fn default() -> Self {
        Self::uniform(1.0)
    }
}

/// Canvas size stored with a layout (schema `canvas`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CanvasSize {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl Default for CanvasSize {
    fn default() -> Self {
        Self { width: CANVAS_WIDTH, height: CANVAS_HEIGHT }
    }
}

/// How an endpoint gets its colours (schema `lightingMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LightingMode {
    /// Sample the effect canvas.
    #[default]
    Canvas,
    /// Every LED shows `forcedColor`.
    Forced,
}

/// Per-endpoint channel balance in percent plus a brightness floor (schema `colorCorrection`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ColorCorrection {
    /// Red, `0..=100`.
    pub red: u8,
    /// Green, `0..=100`.
    pub green: u8,
    /// Blue, `0..=100`.
    pub blue: u8,
    /// Minimum brightness floor, `0..=100`.
    pub min_brightness: u8,
}

impl Default for ColorCorrection {
    fn default() -> Self {
        Self { red: 100, green: 100, blue: 100, min_brightness: 0 }
    }
}

/// Where one endpoint sits on the canvas (schema `placement`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Placement {
    /// Top-left corner of the (unrotated) footprint.
    pub position: Point,
    /// Canvas pixels per LED cell; `None` = the plugin's `DefaultScale()`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<Scale>,
    /// Degrees `0..360`, clockwise on screen.
    pub rotation: f32,
    /// Mirror horizontally (UI "invert").
    pub flip_x: bool,
    /// Mirror vertically.
    pub flip_y: bool,
    /// Disabled endpoints render black (unless `lighting_mode` is `Forced`).
    pub enabled: bool,
    /// Per-endpoint brightness slider `0..=100`.
    pub brightness: u8,
    /// Canvas sampling or forced colour.
    pub lighting_mode: LightingMode,
    /// `#RRGGBB` used when `lighting_mode` is `Forced`.
    pub forced_color: String,
    /// Optional colour correction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_correction: Option<ColorCorrection>,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            position: Point::default(),
            scale: None,
            rotation: 0.0,
            flip_x: false,
            flip_y: false,
            enabled: true,
            brightness: 100,
            lighting_mode: LightingMode::Canvas,
            forced_color: DEFAULT_FORCED_COLOR.to_string(),
            color_correction: None,
        }
    }
}

impl Placement {
    /// Placement from a plugin's `DefaultPosition()` / `DefaultScale()` exports
    /// (scale clamped to `SCALE_MIN..=SCALE_MAX`).
    pub fn from_defaults(default_position: [f32; 2], default_scale: f32) -> Self {
        Self {
            position: Point { x: default_position[0], y: default_position[1] },
            scale: Some(Scale::uniform(default_scale.clamp(SCALE_MIN, SCALE_MAX))),
            ..Default::default()
        }
    }

    /// Rotation normalised into `0..360`.
    pub fn rotation_normalized(&self) -> f32 {
        self.rotation.rem_euclid(360.0)
    }

    /// The scale to sample with: the stored one, or `default_scale` (the
    /// plugin's `DefaultScale()`) when the layout stores none.
    pub fn effective_scale(&self, default_scale: f32) -> Scale {
        self.scale.unwrap_or_else(|| Scale::uniform(default_scale))
    }
}

/// A named layout: per-endpoint placements plus the layout brightness.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Layout {
    /// Profile name (`layouts/currentProfile`); the user-facing key.
    pub name: String,
    /// Optional stable id (UUID).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Layout brightness slider `0..=100`.
    pub brightness: u8,
    /// Snap grid in canvas pixels.
    pub grid_size: u32,
    /// Canvas size the placements refer to.
    pub canvas: CanvasSize,
    /// Placements keyed by endpoint uid (see `huedeck_hal::EndpointUid` grammar).
    pub endpoints: BTreeMap<String, Placement>,
    /// RFC 3339 creation time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// RFC 3339 last-update time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            name: "Default".into(),
            id: None,
            brightness: 100,
            grid_size: DEFAULT_GRID_SIZE,
            canvas: CanvasSize::default(),
            endpoints: BTreeMap::new(),
            created_at: None,
            updated_at: None,
        }
    }
}

impl Layout {
    /// Snap a coordinate to this layout's grid.
    pub fn snap(&self, v: f32) -> f32 {
        if self.grid_size == 0 {
            return v;
        }
        let g = self.grid_size as f32;
        (v / g).round() * g
    }
}

/// What a plugin exports about its LED grid (`Size()`, `LedPositions()`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceGeometry {
    /// `Size()` → `[width, height]` in LED cells, each ≥ 1.
    pub size: [u32; 2],
    /// `LedPositions()` → one `[x, y]` per LED, inside `size`.
    pub led_positions: Vec<[u32; 2]>,
}

impl DeviceGeometry {
    /// Validate what the exe validates: size ≥ 1x1 and every LED inside the grid.
    pub fn validate(&self) -> Result<(), String> {
        if self.size[0] == 0 || self.size[1] == 0 {
            return Err("Invalid Size set: Width and Height must be at least 1".into());
        }
        for (i, p) in self.led_positions.iter().enumerate() {
            if p[0] >= self.size[0] || p[1] >= self.size[1] {
                return Err(format!("LED {i} at [{}, {}] is outside Size [{}, {}]", p[0], p[1], self.size[0], self.size[1]));
            }
        }
        Ok(())
    }

    /// A full grid, row-major, for devices without an explicit map.
    pub fn full_grid(width: u32, height: u32) -> Self {
        let mut led_positions = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                led_positions.push([x, y]);
            }
        }
        Self { size: [width, height], led_positions }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_defaults_and_clamps() {
        let p = Placement::from_defaults([10.0, 100.0], 200.0);
        assert_eq!(p.scale, Some(Scale::uniform(SCALE_MAX)));
        assert!(p.enabled);
        assert_eq!(p.brightness, 100);
        assert_eq!(p.lighting_mode, LightingMode::Canvas);
        assert_eq!(p.forced_color, "#009bde");
        assert_eq!(Placement { rotation: -90.0, ..Default::default() }.rotation_normalized(), 270.0);
        assert_eq!(Placement::default().effective_scale(12.0), Scale::uniform(12.0));
        assert_eq!(p.effective_scale(12.0), Scale::uniform(SCALE_MAX));
    }

    #[test]
    fn layout_snap() {
        let l = Layout::default();
        assert_eq!(l.snap(5.0), 4.0);
        assert_eq!(l.snap(6.0), 8.0);
        assert_eq!(Layout { grid_size: 0, ..Default::default() }.snap(5.5), 5.5);
    }

    #[test]
    fn geometry_validation() {
        assert!(DeviceGeometry::full_grid(3, 2).validate().is_ok());
        assert!(DeviceGeometry { size: [0, 1], led_positions: vec![] }.validate().is_err());
        assert!(DeviceGeometry { size: [2, 2], led_positions: vec![[2, 0]] }.validate().is_err());
    }

    /// The example object from `schemas/layout.schema.json`.
    const SCHEMA_EXAMPLE: &str = r##"{
      "name": "Desk",
      "brightness": 100,
      "gridSize": 4,
      "endpoints": {
        "1462:7c91:": {
          "position": { "x": 120, "y": 80 },
          "scale": { "x": 40, "y": 40 },
          "rotation": 0,
          "flipX": false,
          "flipY": false,
          "enabled": true,
          "brightness": 100,
          "lightingMode": "Canvas",
          "forcedColor": "#009bde"
        },
        "8d3978fc-1b2a-4c7e-9f00-123456789abc": {
          "position": { "x": 0, "y": 180 },
          "scale": { "x": 5, "y": 5 }
        }
      }
    }"##;

    #[test]
    fn json_matches_layout_schema() {
        let layout: Layout = serde_json::from_str(SCHEMA_EXAMPLE).unwrap();
        assert_eq!(layout.name, "Desk");
        assert_eq!(layout.grid_size, 4);
        assert_eq!(layout.canvas, CanvasSize { width: 320, height: 200 });
        let board = &layout.endpoints["1462:7c91:"];
        assert_eq!(board.position, Point { x: 120.0, y: 80.0 });
        assert_eq!(board.scale, Some(Scale::uniform(40.0)));
        assert_eq!(board.lighting_mode, LightingMode::Canvas);
        let strip = &layout.endpoints["8d3978fc-1b2a-4c7e-9f00-123456789abc"];
        assert!(strip.enabled && !strip.flip_x && strip.color_correction.is_none());

        let json = serde_json::to_value(&layout).unwrap();
        assert!(json.get("gridSize").is_some());
        let ep = &json["endpoints"]["1462:7c91:"];
        for key in ["position", "scale", "rotation", "flipX", "flipY", "enabled", "brightness", "lightingMode", "forcedColor"] {
            assert!(ep.get(key).is_some(), "missing {key}");
        }
        assert_eq!(ep["lightingMode"], "Canvas");
        assert!(ep.get("flipped").is_none() && ep.get("colorCorrection").is_none());

        let bare = serde_json::to_value(Placement::default()).unwrap();
        assert!(bare.get("scale").is_none(), "absent scale means DefaultScale()");

        let cc: ColorCorrection = serde_json::from_str(r#"{"red": 90, "minBrightness": 5}"#).unwrap();
        assert_eq!(cc, ColorCorrection { red: 90, green: 100, blue: 100, min_brightness: 5 });
        assert_eq!(serde_json::to_value(LightingMode::Forced).unwrap(), "Forced");
    }
}
