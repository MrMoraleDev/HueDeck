// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Per-LED sampling tables.
//!
//! For every LED of a device the sampler precomputes the linear pixel offset
//! (`y * canvas_width + x`) it reads from the canvas, so the per-frame hot
//! path is one table lookup per LED. Tables are recomputed only when the
//! layout or the device geometry changes.
//!
//! Transform of LED cell `(lx, ly)` (plugin `LedPositions()` coordinates):
//! 1. sample point in footprint space: `((lx + 0.5) * scale.x, (ly + 0.5) * scale.y)`
//!    (cell centre; [`SamplePoint::TopLeft`] drops the `0.5`);
//! 2. `flip_x` / `flip_y` mirror the footprint about its own axes;
//! 3. rotation by `rotation` degrees clockwise (screen Y points down) about
//!    the [`RotationPivot`] — top-left by default, as `schemas/layout.schema.json`
//!    states ("rotation about the endpoint's top-left corner, applied before translation");
//! 4. translate by `position`; floor to an integer pixel; off-canvas → [`OFF_CANVAS`].
//!
//! Open questions tracked for M3 runtime verification (see plan): cell-centre
//! vs top-left sample point, point vs area sample, and whether SignalRGB's
//! pivot really is the top-left corner. All three are parameters here so a
//! one-line change follows the measurement.

use crate::color::Rgb;
use crate::frame::Frame;
use crate::layout::{DeviceGeometry, Placement, Scale, CANVAS_HEIGHT, CANVAS_WIDTH};

/// Offset value for LEDs that fall outside the canvas (sampled as black).
pub const OFF_CANVAS: i32 = -1;

/// Point about which `rotation` is applied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RotationPivot {
    /// Top-left corner of the footprint (`position`); the schema's definition.
    #[default]
    TopLeft,
    /// Centre of the scaled footprint (`w*sx/2, h*sy/2`).
    Center,
}

/// Where inside a cell the sample is taken.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SamplePoint {
    /// `(lx + 0.5) * scale`.
    #[default]
    CellCenter,
    /// `lx * scale`.
    TopLeft,
}

/// Sampler parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SamplerConfig {
    /// Canvas width in pixels.
    pub canvas_width: u32,
    /// Canvas height in pixels.
    pub canvas_height: u32,
    /// Rotation pivot.
    pub pivot: RotationPivot,
    /// Sample point inside each cell.
    pub sample_point: SamplePoint,
    /// Scale used when the placement stores none (the plugin's `DefaultScale()`).
    pub default_scale: f32,
}

impl Default for SamplerConfig {
    fn default() -> Self {
        Self {
            canvas_width: CANVAS_WIDTH,
            canvas_height: CANVAS_HEIGHT,
            pivot: RotationPivot::default(),
            sample_point: SamplePoint::default(),
            default_scale: 1.0,
        }
    }
}

/// Precomputed offsets, one per LED in `LedPositions()` order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OffsetTable {
    offsets: Vec<i32>,
    canvas_width: u32,
    canvas_height: u32,
}

impl OffsetTable {
    /// Offsets in LED order (`OFF_CANVAS` for LEDs outside the canvas).
    pub fn offsets(&self) -> &[i32] {
        &self.offsets
    }

    /// Number of LEDs.
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    /// Whether there are no LEDs.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// Canvas `(x, y)` of LED `i`, or `None` when off-canvas.
    pub fn pixel_of(&self, i: usize) -> Option<(u32, u32)> {
        let off = *self.offsets.get(i)?;
        if off < 0 {
            return None;
        }
        let off = off as u32;
        Some((off % self.canvas_width, off / self.canvas_width))
    }

    /// Sample `frame` into `out` (resized to the LED count). Frames of a
    /// different size than the table was built for sample as black.
    pub fn sample_into(&self, frame: &Frame, out: &mut Vec<Rgb>) {
        out.clear();
        out.reserve(self.offsets.len());
        let ok = frame.width() == self.canvas_width && frame.height() == self.canvas_height;
        for &off in &self.offsets {
            if ok && off >= 0 {
                out.push(frame.pixel_at_offset(off as usize));
            } else {
                out.push(Rgb::BLACK);
            }
        }
    }

    /// Allocating convenience around [`OffsetTable::sample_into`].
    pub fn sample(&self, frame: &Frame) -> Vec<Rgb> {
        let mut out = Vec::new();
        self.sample_into(frame, &mut out);
        out
    }
}

/// Exact sine/cosine for right angles so 90° rotations land on exact pixels.
fn sin_cos_degrees(deg: f32) -> (f32, f32) {
    let d = deg.rem_euclid(360.0);
    if d == 0.0 {
        (0.0, 1.0)
    } else if d == 90.0 {
        (1.0, 0.0)
    } else if d == 180.0 {
        (0.0, -1.0)
    } else if d == 270.0 {
        (-1.0, 0.0)
    } else {
        d.to_radians().sin_cos()
    }
}

/// Build the offset table for `geometry` placed by `placement`.
/// A disabled placement yields an all-`OFF_CANVAS` table.
pub fn compute_offsets(geometry: &DeviceGeometry, placement: &Placement, cfg: &SamplerConfig) -> OffsetTable {
    let mut offsets = Vec::with_capacity(geometry.led_positions.len());
    let (cw, ch) = (cfg.canvas_width as f32, cfg.canvas_height as f32);
    let Scale { x: sx, y: sy } = placement.effective_scale(cfg.default_scale);
    let (fw, fh) = (geometry.size[0] as f32 * sx, geometry.size[1] as f32 * sy);
    let (sin, cos) = sin_cos_degrees(placement.rotation);
    let (px, py) = match cfg.pivot {
        RotationPivot::TopLeft => (0.0, 0.0),
        RotationPivot::Center => (fw / 2.0, fh / 2.0),
    };
    let half = match cfg.sample_point {
        SamplePoint::CellCenter => 0.5,
        SamplePoint::TopLeft => 0.0,
    };
    // Small bias so values like 4.9999997 produced by float rotation floor to 5.
    const EPS: f32 = 1e-3;

    for led in &geometry.led_positions {
        if !placement.enabled {
            offsets.push(OFF_CANVAS);
            continue;
        }
        let mut x = (led[0] as f32 + half) * sx;
        let mut y = (led[1] as f32 + half) * sy;
        if placement.flip_x {
            x = fw - x;
        }
        if placement.flip_y {
            y = fh - y;
        }
        let (dx, dy) = (x - px, y - py);
        let rx = dx * cos - dy * sin + px;
        let ry = dx * sin + dy * cos + py;
        let cx = (rx + placement.position.x + EPS).floor();
        let cy = (ry + placement.position.y + EPS).floor();
        if cx < 0.0 || cy < 0.0 || cx >= cw || cy >= ch {
            offsets.push(OFF_CANVAS);
        } else {
            offsets.push(cy as i32 * cfg.canvas_width as i32 + cx as i32);
        }
    }
    OffsetTable {
        offsets,
        canvas_width: cfg.canvas_width,
        canvas_height: cfg.canvas_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Point;

    fn placement(x: f32, y: f32, scale: f32) -> Placement {
        Placement {
            position: Point { x, y },
            scale: Some(Scale::uniform(scale)),
            ..Default::default()
        }
    }

    #[test]
    fn identity_mapping_for_a_full_canvas_device() {
        // Corsair Nexus style: Size [320,200], scale 1, position [0,0] mirrors the canvas.
        let geom = DeviceGeometry::full_grid(320, 200);
        let table = compute_offsets(&geom, &placement(0.0, 0.0, 1.0), &SamplerConfig::default());
        assert_eq!(table.len(), 64_000);
        for (i, off) in table.offsets().iter().enumerate() {
            assert_eq!(*off as usize, i);
        }
        assert_eq!(table.pixel_of(321), Some((1, 1)));
    }

    #[test]
    fn scaled_device_samples_cell_centres() {
        let geom = DeviceGeometry {
            size: [3, 3],
            led_positions: vec![[0, 0], [2, 2], [1, 0]],
        };
        let table = compute_offsets(&geom, &placement(10.0, 100.0, 8.0), &SamplerConfig::default());
        // LED (0,0) centre = (10 + 4, 100 + 4)
        assert_eq!(table.pixel_of(0), Some((14, 104)));
        // LED (2,2) centre = (10 + 20, 100 + 20)
        assert_eq!(table.pixel_of(1), Some((30, 120)));
        assert_eq!(table.pixel_of(2), Some((22, 104)));
        assert_eq!(table.offsets()[0], 104 * 320 + 14);
    }

    #[test]
    fn absent_scale_uses_the_plugin_default() {
        let geom = DeviceGeometry {
            size: [2, 2],
            led_positions: vec![[1, 1]],
        };
        let p = Placement {
            position: Point { x: 0.0, y: 0.0 },
            scale: None,
            ..Default::default()
        };
        let cfg = SamplerConfig {
            default_scale: 8.0,
            ..Default::default()
        };
        assert_eq!(compute_offsets(&geom, &p, &cfg).pixel_of(0), Some((12, 12)));
        assert_eq!(
            compute_offsets(&geom, &p, &SamplerConfig::default()).pixel_of(0),
            Some((1, 1))
        );
    }

    #[test]
    fn top_left_sample_point_option() {
        let geom = DeviceGeometry {
            size: [2, 2],
            led_positions: vec![[1, 1]],
        };
        let cfg = SamplerConfig {
            sample_point: SamplePoint::TopLeft,
            ..Default::default()
        };
        let table = compute_offsets(&geom, &placement(0.0, 0.0, 10.0), &cfg);
        assert_eq!(table.pixel_of(0), Some((10, 10)));
    }

    #[test]
    fn flips_mirror_inside_the_footprint() {
        let geom = DeviceGeometry {
            size: [2, 1],
            led_positions: vec![[0, 0], [1, 0]],
        };
        let p = Placement {
            flip_x: true,
            ..placement(0.0, 0.0, 10.0)
        };
        let table = compute_offsets(&geom, &p, &SamplerConfig::default());
        // footprint 20x10 px, cell-centre sampling: LED 0 centre (5,5) → (15,5), LED 1 (15,5) → (5,5)
        assert_eq!(table.pixel_of(0), Some((15, 5)));
        assert_eq!(table.pixel_of(1), Some((5, 5)));

        let geom = DeviceGeometry {
            size: [1, 2],
            led_positions: vec![[0, 0], [0, 1]],
        };
        let p = Placement {
            flip_y: true,
            ..placement(0.0, 0.0, 10.0)
        };
        let table = compute_offsets(&geom, &p, &SamplerConfig::default());
        assert_eq!(table.pixel_of(0), Some((5, 15)));
        assert_eq!(table.pixel_of(1), Some((5, 5)));
    }

    #[test]
    fn default_pivot_is_the_top_left_corner() {
        // Schema: rotate about the endpoint's top-left corner, then translate.
        let geom = DeviceGeometry {
            size: [2, 1],
            led_positions: vec![[0, 0], [1, 0]],
        };
        let p = Placement {
            rotation: 90.0,
            ..placement(100.0, 100.0, 10.0)
        };
        let table = compute_offsets(&geom, &p, &SamplerConfig::default());
        // centre (5,5) → (-5, 5) → (95, 105); centre (15,5) → (-5, 15) → (95, 115)
        assert_eq!(table.pixel_of(0), Some((95, 105)));
        assert_eq!(table.pixel_of(1), Some((95, 115)));

        let p = Placement {
            rotation: 180.0,
            ..placement(100.0, 100.0, 10.0)
        };
        let table = compute_offsets(&geom, &p, &SamplerConfig::default());
        // (5,5) → (-5,-5) → (95, 95)
        assert_eq!(table.pixel_of(0), Some((95, 95)));
    }

    #[test]
    fn rotation_about_centre_is_exact_for_right_angles() {
        let geom = DeviceGeometry {
            size: [2, 2],
            led_positions: vec![[0, 0], [1, 0], [1, 1], [0, 1]],
        };
        let cfg = SamplerConfig {
            pivot: RotationPivot::Center,
            ..Default::default()
        };
        let p = Placement {
            rotation: 90.0,
            ..placement(100.0, 100.0, 10.0)
        };
        let table = compute_offsets(&geom, &p, &cfg);
        // 90° clockwise about (110,110): (0,0)→where (1,0) was, (1,0)→(1,1), (1,1)→(0,1), (0,1)→(0,0)
        assert_eq!(table.pixel_of(0), Some((115, 105)));
        assert_eq!(table.pixel_of(1), Some((115, 115)));
        assert_eq!(table.pixel_of(2), Some((105, 115)));
        assert_eq!(table.pixel_of(3), Some((105, 105)));

        let p = Placement {
            rotation: 180.0,
            ..placement(100.0, 100.0, 10.0)
        };
        let table = compute_offsets(&geom, &p, &cfg);
        assert_eq!(table.pixel_of(0), Some((115, 115)));
    }

    #[test]
    fn off_canvas_and_disabled() {
        let geom = DeviceGeometry {
            size: [2, 1],
            led_positions: vec![[0, 0], [1, 0]],
        };
        let table = compute_offsets(&geom, &placement(310.0, 0.0, 10.0), &SamplerConfig::default());
        assert_eq!(table.pixel_of(0), Some((315, 5)));
        assert_eq!(table.offsets()[1], OFF_CANVAS);
        assert_eq!(table.pixel_of(1), None);

        let p = Placement {
            enabled: false,
            ..placement(0.0, 0.0, 1.0)
        };
        let table = compute_offsets(&geom, &p, &SamplerConfig::default());
        assert!(table.offsets().iter().all(|o| *o == OFF_CANVAS));

        let table = compute_offsets(&geom, &placement(-30.0, 0.0, 10.0), &SamplerConfig::default());
        assert_eq!(table.offsets(), &[OFF_CANVAS, OFF_CANVAS]);
    }

    #[test]
    fn sampling_reads_the_frame() {
        let mut frame = Frame::new(320, 200);
        frame.set_pixel(14, 104, Rgb::new(9, 8, 7));
        let geom = DeviceGeometry {
            size: [1, 1],
            led_positions: vec![[0, 0]],
        };
        let table = compute_offsets(&geom, &placement(10.0, 100.0, 8.0), &SamplerConfig::default());
        assert_eq!(table.sample(&frame), vec![Rgb::new(9, 8, 7)]);
        let wrong_size = Frame::solid(2, 2, Rgb::WHITE);
        assert_eq!(table.sample(&wrong_size), vec![Rgb::BLACK]);
    }
}
