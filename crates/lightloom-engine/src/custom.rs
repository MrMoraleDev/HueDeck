// SPDX-License-Identifier: GPL-3.0-or-later
//! Per-LED custom colours: the "custom LEDs" mode of an endpoint (a device,
//! a subdevice or a channel component), as OpenRGB lets the user colour
//! single LEDs of a keyboard.
//!
//! An endpoint in that mode shows one user-chosen colour per LED instead of
//! the canvas. The colours reach the plugin through the static layer of
//! [`DirectLeds`](crate::DirectLeds), so plugins see them through
//! `device.color(x, y)` and channel colours as they see the canvas, with the
//! brightness product applied.
//!
//! Colours are kept per **slot**, the index [`DirectLeds`](crate::DirectLeds)
//! uses for the endpoint: the LED index (`LedPositions()` / `LedCoordinates`
//! order) for devices and components, and the grid cell (`y * width + x`)
//! for subdevices. [`led_spots`] lists an endpoint's LEDs with their slots.
//!
//! Zones are named sets of slots (OpenRGB zones, kept simple: a saved
//! selection) that the user colours at once.

use crate::color::Rgb;
use crate::direct::MAX_DIRECT_LEDS;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Most zones per endpoint.
pub const MAX_ZONES: usize = 64;
/// Longest zone name, in characters.
pub const MAX_ZONE_NAME: usize = 64;

/// How an endpoint's slots map to its LEDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotLayout {
    /// One slot per LED, in LED order (devices, components).
    Leds,
    /// One slot per grid cell, row-major (subdevices).
    Cells,
}

/// One LED as the LED painter draws it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedSpot {
    /// Index into the endpoint's colours.
    pub slot: u32,
    /// Cell column.
    pub x: u32,
    /// Cell row.
    pub y: u32,
    /// LED name (`LedNames()`; empty when the plugin gave none).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}

/// The LEDs of an endpoint with a `size` grid and LED `positions` / `names`,
/// and its slot count. LEDs outside the grid are left out; for
/// [`SlotLayout::Cells`] an empty `positions` makes every cell an LED.
pub fn led_spots(
    layout: SlotLayout,
    size: [u32; 2],
    positions: &[[u32; 2]],
    names: &[String],
) -> (Vec<LedSpot>, usize) {
    let [w, h] = size;
    let name = |i: usize| names.get(i).cloned().unwrap_or_default();
    match layout {
        SlotLayout::Leds => {
            let n = positions.len().min(MAX_DIRECT_LEDS);
            let spots = positions[..n]
                .iter()
                .enumerate()
                .filter(|(_, p)| p[0] < w && p[1] < h)
                .map(|(i, p)| LedSpot {
                    slot: i as u32,
                    x: p[0],
                    y: p[1],
                    name: name(i),
                })
                .collect();
            (spots, n)
        }
        SlotLayout::Cells => {
            let cells = (w as usize).saturating_mul(h as usize).min(MAX_DIRECT_LEDS);
            let mut spots: Vec<LedSpot> = Vec::new();
            if positions.is_empty() {
                for slot in 0..cells {
                    let slot = slot as u32;
                    spots.push(LedSpot {
                        slot,
                        x: slot % w,
                        y: slot / w,
                        name: String::new(),
                    });
                }
            } else {
                let mut seen = std::collections::BTreeSet::new();
                for (i, p) in positions.iter().enumerate() {
                    let slot = p[1] as usize * w as usize + p[0] as usize;
                    if p[0] < w && p[1] < h && slot < cells && seen.insert(slot) {
                        spots.push(LedSpot {
                            slot: slot as u32,
                            x: p[0],
                            y: p[1],
                            name: name(i),
                        });
                    }
                }
            }
            (spots, cells)
        }
    }
}

/// Why an edit of [`CustomLeds`] was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CustomLedsError {
    /// A slot past the endpoint's LEDs.
    #[error("LED slot {slot} is out of range (the endpoint has {count})")]
    Slot {
        /// The slot.
        slot: u32,
        /// The endpoint's slot count.
        count: usize,
    },
    /// No zone of that name.
    #[error("no zone {0:?}")]
    NoZone(String),
    /// An empty or overlong zone name.
    #[error("a zone name must have 1 to {MAX_ZONE_NAME} characters")]
    ZoneName,
    /// [`MAX_ZONES`] zones already.
    #[error("at most {MAX_ZONES} zones per endpoint")]
    TooManyZones,
}

/// The custom-LED state of one endpoint (persisted per endpoint uid).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomLeds {
    /// The endpoint shows [`CustomLeds::colors`] instead of the canvas.
    #[serde(default)]
    pub enabled: bool,
    /// One colour per slot (`#rrggbb`); slots past the end are black.
    #[serde(default, with = "hex_colors")]
    pub colors: Vec<Rgb>,
    /// Named sets of slots.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub zones: BTreeMap<String, Vec<u32>>,
}

impl CustomLeds {
    /// Make room for `slots` colours (new slots black; extra ones dropped),
    /// and drop zone slots past the end.
    pub fn fit(&mut self, slots: usize) {
        let slots = slots.min(MAX_DIRECT_LEDS);
        self.colors.resize(slots, Rgb::BLACK);
        for z in self.zones.values_mut() {
            z.retain(|s| (*s as usize) < slots);
        }
    }

    /// Every slot `color`. Returns whether anything changed.
    pub fn fill(&mut self, color: Rgb) -> bool {
        let changed = self.colors.iter().any(|c| *c != color);
        self.colors.iter_mut().for_each(|c| *c = color);
        changed
    }

    /// Set the given slots; refused as a whole when one is out of range.
    /// Returns whether anything changed.
    pub fn set(&mut self, changes: &[(u32, Rgb)]) -> Result<bool, CustomLedsError> {
        let count = self.colors.len();
        if let Some((slot, _)) = changes.iter().find(|(s, _)| *s as usize >= count) {
            return Err(CustomLedsError::Slot { slot: *slot, count });
        }
        let mut changed = false;
        for (slot, color) in changes {
            let c = &mut self.colors[*slot as usize];
            changed |= *c != *color;
            *c = *color;
        }
        Ok(changed)
    }

    /// Colour every slot of zone `name`. Returns whether anything changed.
    pub fn color_zone(&mut self, name: &str, color: Rgb) -> Result<bool, CustomLedsError> {
        let slots = self
            .zones
            .get(name)
            .ok_or_else(|| CustomLedsError::NoZone(name.to_string()))?;
        let changes: Vec<(u32, Rgb)> = slots.iter().map(|s| (*s, color)).collect();
        self.set(&changes)
    }

    /// Create or replace zone `name` (slots sorted, duplicates dropped).
    pub fn save_zone(&mut self, name: &str, slots: &[u32]) -> Result<(), CustomLedsError> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > MAX_ZONE_NAME {
            return Err(CustomLedsError::ZoneName);
        }
        let count = self.colors.len();
        if let Some(slot) = slots.iter().find(|s| **s as usize >= count) {
            return Err(CustomLedsError::Slot { slot: *slot, count });
        }
        if !self.zones.contains_key(name) && self.zones.len() >= MAX_ZONES {
            return Err(CustomLedsError::TooManyZones);
        }
        let mut slots = slots.to_vec();
        slots.sort_unstable();
        slots.dedup();
        self.zones.insert(name.to_string(), slots);
        Ok(())
    }

    /// Delete zone `name`; whether there was one.
    pub fn delete_zone(&mut self, name: &str) -> bool {
        self.zones.remove(name.trim()).is_some()
    }

    /// Nothing worth keeping: off, no colour other than black, no zone.
    pub fn is_blank(&self) -> bool {
        !self.enabled && self.zones.is_empty() && self.colors.iter().all(|c| *c == Rgb::BLACK)
    }
}

/// `Vec<Rgb>` as a list of `#rrggbb` strings (unparsable entries read as black).
mod hex_colors {
    use crate::color::Rgb;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(colors: &[Rgb], s: S) -> Result<S::Ok, S::Error> {
        colors.iter().map(|c| c.to_hex()).collect::<Vec<_>>().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Rgb>, D::Error> {
        let list = Vec::<String>::deserialize(d)?;
        Ok(list
            .iter()
            .map(|s| Rgb::from_hex(s).unwrap_or(Rgb::BLACK))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgb = Rgb::new(255, 0, 0);
    const BLUE: Rgb = Rgb::new(0, 0, 255);

    #[test]
    fn spots_follow_the_slot_layout() {
        let names = vec!["Esc".to_string(), "F1".to_string()];
        let (spots, n) = led_spots(SlotLayout::Leds, [3, 2], &[[0, 0], [2, 1], [5, 5]], &names);
        assert_eq!(n, 3);
        assert_eq!(
            spots,
            vec![
                LedSpot {
                    slot: 0,
                    x: 0,
                    y: 0,
                    name: "Esc".into()
                },
                LedSpot {
                    slot: 1,
                    x: 2,
                    y: 1,
                    name: "F1".into()
                },
            ],
            "the LED outside the grid is left out, its slot kept"
        );
        // Subdevices: cells; named LEDs only when the plugin named them.
        let (spots, n) = led_spots(SlotLayout::Cells, [3, 2], &[[2, 1], [2, 1]], &names);
        assert_eq!((n, spots.len()), (6, 1));
        assert_eq!((spots[0].slot, spots[0].name.as_str()), (5, "Esc"));
        let (spots, n) = led_spots(SlotLayout::Cells, [2, 2], &[], &[]);
        assert_eq!(n, 4);
        assert_eq!(
            spots.iter().map(|s| (s.x, s.y)).collect::<Vec<_>>(),
            [(0, 0), (1, 0), (0, 1), (1, 1)]
        );
    }

    #[test]
    fn edits_and_zones() {
        let mut c = CustomLeds::default();
        c.fit(4);
        assert!(c.is_blank());
        assert!(c.fill(RED));
        assert!(!c.fill(RED), "no change");
        assert!(c.set(&[(1, BLUE)]).unwrap());
        assert_eq!(c.colors, [RED, BLUE, RED, RED]);
        assert_eq!(
            c.set(&[(0, BLUE), (4, BLUE)]).unwrap_err(),
            CustomLedsError::Slot { slot: 4, count: 4 }
        );
        assert_eq!(c.colors[0], RED, "refused as a whole");
        c.save_zone(" WASD ", &[3, 2, 3]).unwrap();
        assert_eq!(c.zones["WASD"], [2, 3]);
        assert!(c.color_zone("WASD", BLUE).unwrap());
        assert_eq!(c.colors, [RED, BLUE, BLUE, BLUE]);
        assert_eq!(
            c.color_zone("nope", RED).unwrap_err(),
            CustomLedsError::NoZone("nope".into())
        );
        assert_eq!(c.save_zone("", &[0]).unwrap_err(), CustomLedsError::ZoneName);
        assert!(c.save_zone("x", &[9]).is_err());
        // Shrinking drops slots past the end, in zones too.
        c.fit(3);
        assert_eq!(c.zones["WASD"], [2]);
        assert!(c.delete_zone("WASD") && !c.delete_zone("WASD"));
    }

    #[test]
    fn json_keeps_colours_as_hex() {
        let mut c = CustomLeds {
            enabled: true,
            colors: vec![RED, BLUE],
            ..CustomLeds::default()
        };
        c.save_zone("Logo", &[1]).unwrap();
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"enabled": true, "colors": ["#ff0000", "#0000ff"], "zones": {"Logo": [1]}})
        );
        let back: CustomLeds = serde_json::from_value(v).unwrap();
        assert_eq!(back, c);
        let odd: CustomLeds = serde_json::from_value(serde_json::json!({"colors": ["bad"]})).unwrap();
        assert_eq!((odd.enabled, odd.colors), (false, vec![Rgb::BLACK]));
    }
}
