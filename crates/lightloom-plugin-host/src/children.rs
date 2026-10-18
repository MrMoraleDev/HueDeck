// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Canvas endpoints a device plugin creates besides the device itself
//! (docs/host-api/components-channels.md §1, §2 and §4):
//!
//! * **subdevices** (`device.createSubdevice(id)`), sampled per cell through
//!   `device.subdeviceColor(id, x, y)`;
//! * **channels** (`device.addChannel(name, limit[, defaultCount])`) and the
//!   component instances assigned to them, sampled per LED into the stream
//!   `channel.getColors()` returns.
//!
//! Every subdevice and component instance has its own placement on the canvas:
//! the caller's one for its uid (`<device key>:<subdevice id>`, or the
//! instance's `ComponentId`), otherwise [`Placement::fit`] (refitted when the
//! plugin resizes a subdevice).
//!
//! A channel takes the components the caller assigned to its name (SignalRGB
//! persists them per device and channel name); a channel nobody configured
//! gets the placeholder `Default Strip - N`, N = the plugin's default count or
//! half the channel limit.

use crate::device::Brightness;
use lightloom_engine::{
    apply_brightness, compute_offsets, effective_brightness, CanvasSize, Component, ComponentInstance,
    DeviceGeometry, Frame, OffsetTable, Placement, SamplerConfig,
};
use std::collections::BTreeMap;

/// Most subdevices one plugin may create.
pub const MAX_SUBDEVICES: usize = 256;
/// Most channels one plugin may add.
pub const MAX_CHANNELS: usize = 64;
/// Most LEDs one subdevice may name or map.
pub const MAX_SUBDEVICE_LEDS: usize = 4096;
/// Longest LED name kept (characters).
pub const MAX_LED_NAME_CHARS: usize = 128;
/// Largest subdevice image (URL or base64) kept, in bytes.
pub const MAX_IMAGE_BYTES: usize = 1 << 20;

/// Where one endpoint samples the canvas.
#[derive(Clone, Debug)]
struct Surface {
    geometry: DeviceGeometry,
    placement: Placement,
    /// Placed by [`Placement::fit`]: refitted when the size changes.
    fitted: bool,
    /// One offset per `geometry.led_positions` entry.
    table: OffsetTable,
    factor: f32,
    /// `LightingMode::Forced` colour of the placement.
    forced: Option<lightloom_engine::Rgb>,
}

impl Surface {
    fn new(
        geometry: DeviceGeometry,
        placement: Option<Placement>,
        cfg: &SamplerConfig,
        b: Brightness,
    ) -> Self {
        let fitted = placement.is_none();
        let placement = placement.unwrap_or_else(|| fit(geometry.size, cfg));
        let table = compute_offsets(&geometry, &placement, cfg);
        let factor = effective_brightness(b.global, b.layout, placement.brightness);
        let forced = placement.forced();
        Self {
            geometry,
            placement,
            fitted,
            table,
            factor,
            forced,
        }
    }

    fn set_geometry(&mut self, geometry: DeviceGeometry, cfg: &SamplerConfig) {
        if self.fitted {
            let brightness = self.placement.brightness;
            self.placement = fit(geometry.size, cfg);
            self.placement.brightness = brightness;
        }
        self.geometry = geometry;
        self.table = compute_offsets(&self.geometry, &self.placement, cfg);
    }

    fn set_brightness(&mut self, b: Brightness) {
        self.factor = effective_brightness(b.global, b.layout, self.placement.brightness);
    }

    /// A placement chosen from outside (layout editor): no longer refitted.
    fn set_placement(&mut self, placement: Placement, cfg: &SamplerConfig, b: Brightness) {
        self.fitted = false;
        self.forced = placement.forced();
        self.factor = effective_brightness(b.global, b.layout, placement.brightness);
        self.placement = placement;
        self.table = compute_offsets(&self.geometry, &self.placement, cfg);
    }

    /// Colour of entry `i` of the table (black when off-canvas or absent), or the forced colour.
    fn sample(&self, frame: &Frame, cfg: &SamplerConfig, i: usize) -> [u8; 3] {
        if let Some(c) = self.forced {
            let c = apply_brightness(c, self.factor);
            return [c.r, c.g, c.b];
        }
        let ok = frame.width() == cfg.canvas_width && frame.height() == cfg.canvas_height;
        match self.table.offsets().get(i) {
            Some(&off) if ok && off >= 0 => {
                let c = apply_brightness(frame.pixel_at_offset(off as usize), self.factor);
                [c.r, c.g, c.b]
            }
            _ => [0, 0, 0],
        }
    }
}

fn fit(size: [u32; 2], cfg: &SamplerConfig) -> Placement {
    Placement::fit(
        size,
        CanvasSize {
            width: cfg.canvas_width,
            height: cfg.canvas_height,
        },
    )
}

/// A subdevice: a separately placed LED grid of the device.
#[derive(Clone, Debug)]
pub struct Subdevice {
    /// The plugin's id.
    pub id: String,
    /// Display name (`setSubdeviceName`; the id until set).
    pub name: String,
    /// `setSubdeviceImageUrl` / `setSubdeviceImage` payload, if any.
    pub image: Option<String>,
    /// `setSubdeviceLeds` names.
    pub led_names: Vec<String>,
    /// `setSubdeviceLeds` positions.
    pub led_positions: Vec<[u32; 2]>,
    /// `setSubdeviceLedMap`.
    pub led_map: Vec<u32>,
    surface: Surface,
}

impl Subdevice {
    /// Grid size in cells.
    pub fn size(&self) -> [u32; 2] {
        self.surface.geometry.size
    }

    /// Where it sits on the canvas.
    pub fn placement(&self) -> &Placement {
        &self.surface.placement
    }
}

/// One component instance on a channel.
#[derive(Clone, Debug)]
pub struct ChannelComponent {
    /// The instance.
    pub instance: ComponentInstance,
    /// Stream slot of each LED (`None`: its mapping points outside the stream).
    slots: Vec<Option<usize>>,
    surface: Surface,
}

impl ChannelComponent {
    /// Where it sits on the canvas.
    pub fn placement(&self) -> &Placement {
        &self.surface.placement
    }

    fn stream(&self, frame: &Frame, cfg: &SamplerConfig, out: &mut Vec<u8>) {
        let start = out.len();
        out.resize(start + self.instance.component.led_count as usize * 3, 0);
        for (i, slot) in self.slots.iter().enumerate() {
            if let Some(s) = slot {
                let c = self.surface.sample(frame, cfg, i);
                out[start + s * 3..start + s * 3 + 3].copy_from_slice(&c);
            }
        }
    }
}

/// A channel of the device (an ARGB header, a fan port).
#[derive(Clone, Debug)]
pub struct Channel {
    /// The plugin's name; the persistence key of its components.
    pub name: String,
    /// LED budget of the channel (0 = none).
    pub led_limit: u32,
    /// Assigned components in chain order.
    pub components: Vec<ChannelComponent>,
}

impl Channel {
    /// Sum of the components' LED counts.
    pub fn raw_led_count(&self) -> usize {
        self.components
            .iter()
            .map(|c| c.instance.component.led_count as usize)
            .sum()
    }

    /// Whether every component is a placeholder strip (true when empty).
    pub fn only_default_components(&self) -> bool {
        self.components
            .iter()
            .all(|c| c.instance.component.is_default_strip())
    }
}

/// What a canvas endpoint is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EndpointKind {
    /// The device itself.
    Device,
    /// A subdevice (`device.createSubdevice`).
    Subdevice,
    /// A component instance on a channel.
    Component,
}

/// A canvas endpoint of a running session, as the outside (layout editor) sees it.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct EndpointView {
    /// Placement key: the device key, `<device key>:<subdevice id>`, or the component's id.
    pub uid: String,
    /// What it is.
    pub kind: EndpointKind,
    /// Display name.
    pub name: String,
    /// The channel of a component.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// LED grid size in cells.
    pub size: [u32; 2],
    /// Number of LEDs.
    pub leds: usize,
    /// Where it sits now.
    pub placement: Placement,
}

/// A channel of a running session (component editor).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInfo {
    /// The plugin's name; the persistence key of its components.
    pub name: String,
    /// LED budget (0 = none).
    pub led_limit: u32,
    /// LEDs the plugin is told about (components, capped at the limit).
    pub led_count: usize,
    /// Component instance ids in chain order.
    pub components: Vec<String>,
}

/// What the caller decides for a device's subdevices and channels.
#[derive(Clone, Debug, Default)]
pub struct ChildSetup {
    /// Device key (`vid:pid:serial`), the prefix of subdevice uids.
    pub device_key: String,
    /// Components per channel name. A channel absent here gets the placeholder strip.
    pub channel_components: BTreeMap<String, Vec<ComponentInstance>>,
    /// Placements by endpoint uid; endpoints without one are fitted to the canvas.
    pub placements: BTreeMap<String, Placement>,
}

/// Subdevices and channels of one device session.
#[derive(Clone, Debug, Default)]
pub struct Children {
    setup: ChildSetup,
    sampler: SamplerConfig,
    brightness: Brightness,
    subdevices: Vec<Subdevice>,
    channels: Vec<Channel>,
    led_limit: u32,
    subdevice_controller: bool,
}

impl Children {
    /// Replace the caller's decisions (before the plugin runs).
    pub fn set_setup(&mut self, setup: ChildSetup) {
        self.setup = setup;
    }

    /// Sampler parameters shared with the device.
    pub fn set_sampler(&mut self, sampler: SamplerConfig) {
        self.sampler = sampler;
        for s in &mut self.subdevices {
            let g = s.surface.geometry.clone();
            s.surface.set_geometry(g, &self.sampler);
        }
        for c in self.channels.iter_mut().flat_map(|ch| ch.components.iter_mut()) {
            let g = c.surface.geometry.clone();
            c.surface.set_geometry(g, &self.sampler);
        }
    }

    /// Global and layout brightness.
    pub fn set_brightness(&mut self, brightness: Brightness) {
        self.brightness = brightness;
        for s in &mut self.subdevices {
            s.surface.set_brightness(brightness);
        }
        for c in self.channels.iter_mut().flat_map(|ch| ch.components.iter_mut()) {
            c.surface.set_brightness(brightness);
        }
    }

    /// A new placement for the child endpoint `uid` (`<device key>:<subdevice id>` or a
    /// component instance's id), kept for later refits too. `false` when no child has it.
    pub fn set_placement(&mut self, uid: &str, placement: Placement) -> bool {
        self.setup.placements.insert(uid.to_string(), placement.clone());
        let (cfg, b) = (self.sampler, self.brightness);
        let prefix = format!("{}:", self.setup.device_key);
        if let Some(id) = uid.strip_prefix(&prefix) {
            if let Some(s) = self.subdevices.iter_mut().find(|s| s.id == id) {
                s.surface.set_placement(placement, &cfg, b);
                return true;
            }
        }
        if let Some(c) = self
            .channels
            .iter_mut()
            .flat_map(|ch| ch.components.iter_mut())
            .find(|c| c.instance.id == uid)
        {
            c.surface.set_placement(placement, &cfg, b);
            return true;
        }
        false
    }

    /// The channels in creation order.
    pub fn channel_infos(&self) -> Vec<ChannelInfo> {
        self.channels
            .iter()
            .zip(self.led_counts())
            .map(|(ch, led_count)| ChannelInfo {
                name: ch.name.clone(),
                led_limit: ch.led_limit,
                led_count,
                components: ch.components.iter().map(|c| c.instance.id.clone()).collect(),
            })
            .collect()
    }

    /// The subdevices and component instances as endpoints, in creation and chain order.
    pub fn views(&self) -> Vec<EndpointView> {
        let mut out: Vec<EndpointView> = self
            .subdevices
            .iter()
            .map(|s| EndpointView {
                uid: self.uid(&s.id),
                kind: EndpointKind::Subdevice,
                name: s.name.clone(),
                channel: None,
                size: s.surface.geometry.size,
                leds: s.surface.geometry.led_positions.len(),
                placement: s.surface.placement.clone(),
            })
            .collect();
        for ch in &self.channels {
            for c in &ch.components {
                out.push(EndpointView {
                    uid: c.instance.id.clone(),
                    kind: EndpointKind::Component,
                    name: c.instance.component.display_name.clone(),
                    channel: Some(ch.name.clone()),
                    size: c.surface.geometry.size,
                    leds: c.instance.component.led_count as usize,
                    placement: c.surface.placement.clone(),
                });
            }
        }
        out
    }

    /// Subdevices in creation order.
    pub fn subdevices(&self) -> &[Subdevice] {
        &self.subdevices
    }

    /// Channels in creation order.
    pub fn channels(&self) -> &[Channel] {
        &self.channels
    }

    /// `SubdeviceController()` / `device.SetIsSubdeviceController(flag)`.
    pub fn subdevice_controller(&self) -> bool {
        self.subdevice_controller
    }

    /// Set the subdevice-controller flag.
    pub fn set_subdevice_controller(&mut self, flag: bool) {
        self.subdevice_controller = flag;
    }

    /// Device LED budget (`device.SetLedLimit`; 0 = none).
    pub fn led_limit(&self) -> u32 {
        self.led_limit
    }

    /// `device.SetLedLimit(n)`.
    pub fn set_led_limit(&mut self, n: u32) {
        self.led_limit = n;
    }

    fn uid(&self, id: &str) -> String {
        format!("{}:{id}", self.setup.device_key)
    }

    fn subdevice_mut(&mut self, id: &str) -> Option<&mut Subdevice> {
        self.subdevices.iter_mut().find(|s| s.id == id)
    }

    /// `device.createSubdevice(id)`: create-if-absent. The log line to print, if any.
    pub fn create_subdevice(&mut self, id: &str) -> Result<String, String> {
        if self.subdevices.iter().any(|s| s.id == id) {
            return Ok(String::new());
        }
        if self.subdevices.len() >= MAX_SUBDEVICES {
            return Err(format!(
                "device.createSubdevice({id}): more than {MAX_SUBDEVICES} subdevices"
            ));
        }
        let uid = self.uid(id);
        let placement = self.setup.placements.get(&uid).cloned();
        let surface = Surface::new(
            DeviceGeometry::full_grid(1, 1),
            placement,
            &self.sampler,
            self.brightness,
        );
        self.subdevices.push(Subdevice {
            id: id.to_string(),
            name: id.to_string(),
            image: None,
            led_names: Vec::new(),
            led_positions: Vec::new(),
            led_map: Vec::new(),
            surface,
        });
        Ok(format!("Created Sub device with uid: {uid}"))
    }

    /// `device.removeSubdevice(id)`. The log line to print.
    pub fn remove_subdevice(&mut self, id: &str) -> String {
        match self.subdevices.iter().position(|s| s.id == id) {
            Some(i) => {
                let s = self.subdevices.remove(i);
                format!("Removing Subdevice: {} ({id})", s.name)
            }
            None => format!("Failed to find subdevice with name: {id}"),
        }
    }

    /// `device.getCurrentSubdevices()`.
    pub fn subdevice_ids(&self) -> Vec<String> {
        self.subdevices.iter().map(|s| s.id.clone()).collect()
    }

    /// `device.setSubdeviceName(id, name)`.
    pub fn set_subdevice_name(&mut self, id: &str, name: String) -> Result<(), String> {
        let s = self
            .subdevice_mut(id)
            .ok_or_else(|| missing("setSubdeviceName", id))?;
        s.name = name;
        Ok(())
    }

    /// `device.setSubdeviceSize(id, w, h)` (grid bounds as for `Size()`).
    pub fn set_subdevice_size(&mut self, id: &str, w: i32, h: i32) -> Result<(), String> {
        let size = [u32::try_from(w).unwrap_or(0), u32::try_from(h).unwrap_or(0)];
        let size =
            crate::device::checked_grid(size).map_err(|e| format!("device.setSubdeviceSize({id}): {e}"))?;
        let cfg = self.sampler;
        let s = self
            .subdevice_mut(id)
            .ok_or_else(|| missing("setSubdeviceSize", id))?;
        s.surface
            .set_geometry(DeviceGeometry::full_grid(size[0], size[1]), &cfg);
        Ok(())
    }

    /// `device.setSubdeviceLeds(id, names, positions)`; `flat` = `[x0, y0, x1, y1, ...]`.
    pub fn set_subdevice_leds(&mut self, id: &str, names: Vec<String>, flat: &[i32]) -> Result<(), String> {
        if names.len() > MAX_SUBDEVICE_LEDS || flat.len() > MAX_SUBDEVICE_LEDS * 2 {
            return Err(format!(
                "device.setSubdeviceLeds({id}): more than {MAX_SUBDEVICE_LEDS} LEDs"
            ));
        }
        let names: Vec<String> = names
            .into_iter()
            .map(|n| crate::device::capped(n, MAX_LED_NAME_CHARS))
            .collect();
        let positions: Vec<[u32; 2]> = flat
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| {
                [
                    u32::try_from(p[0]).unwrap_or(u32::MAX),
                    u32::try_from(p[1]).unwrap_or(u32::MAX),
                ]
            })
            .collect();
        if names.len() != positions.len() {
            return Err(format!(
                "ERROR: Led names and positions vary in length! Names: {}, Positions: {}",
                names.len(),
                positions.len()
            ));
        }
        let s = self
            .subdevice_mut(id)
            .ok_or_else(|| missing("setSubdeviceLeds", id))?;
        s.led_names = names;
        s.led_positions = positions;
        Ok(())
    }

    /// `device.setSubdeviceLedMap(id, map)`.
    pub fn set_subdevice_led_map(&mut self, id: &str, map: &[i32]) -> Result<(), String> {
        let s = self
            .subdevice_mut(id)
            .ok_or_else(|| missing("setSubdeviceLedMap", id))?;
        s.led_map = map
            .iter()
            .take(MAX_SUBDEVICE_LEDS)
            .map(|&v| u32::try_from(v).unwrap_or(0))
            .collect();
        Ok(())
    }

    /// `device.setSubdeviceImageUrl(id, url)` / `setSubdeviceImage(id, base64)`.
    pub fn set_subdevice_image(&mut self, id: &str, image: String) -> Result<(), String> {
        if image.len() > MAX_IMAGE_BYTES {
            return Err(format!(
                "device.setSubdeviceImage({id}): {} bytes is larger than {MAX_IMAGE_BYTES}",
                image.len()
            ));
        }
        if image
            .to_ascii_lowercase()
            .split(['?', '#'])
            .next()
            .is_some_and(|u| u.ends_with(".gif"))
        {
            return Err("Supported image types are 'PNG', 'JPG', and 'SVG'".into());
        }
        let s = self
            .subdevice_mut(id)
            .ok_or_else(|| missing("setSubdeviceImageUrl", id))?;
        s.image = (!image.is_empty()).then_some(image);
        Ok(())
    }

    /// `device.subdeviceColor(id, x, y)`: black (and an error to log once) when
    /// the subdevice or the cell does not exist.
    pub fn subdevice_color(&self, frame: &Frame, id: &str, x: i32, y: i32) -> Result<[u8; 3], String> {
        let s = self
            .subdevices
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| missing("subdeviceColor", id))?;
        let [w, h] = s.surface.geometry.size;
        match (u32::try_from(x), u32::try_from(y)) {
            (Ok(cx), Ok(cy)) if cx < w && cy < h => Ok(s.surface.sample(frame, &self.sampler, (cy * w + cx) as usize)),
            _ => Err(format!(
                "device.subdeviceColor(): Out of bounds error. Coordinate({x}, {y}) is out of range for subdevice {id}"
            )),
        }
    }

    // ------------------------------------------------------------------ channels

    /// `device.addChannel(name, limit[, defaultCount])` (`default_count < 0`:
    /// omitted, half the limit). Adding an existing name updates its limit and
    /// keeps its components. Returns log lines.
    pub fn add_channel(&mut self, name: &str, limit: i32, default_count: i32) -> Result<Vec<String>, String> {
        let limit = u32::try_from(limit).unwrap_or(0);
        if let Some(ch) = self.channels.iter_mut().find(|c| c.name == name) {
            // Plugins re-run their setup on mode changes; only a new limit is news.
            if ch.led_limit == limit {
                return Ok(Vec::new());
            }
            ch.led_limit = limit;
            return Ok(vec![format!(
                "addChannel({name}): channel exists, limit changed to {limit}"
            )]);
        }
        if self.channels.len() >= MAX_CHANNELS {
            return Err(format!(
                "device.addChannel({name}): more than {MAX_CHANNELS} channels"
            ));
        }
        let mut notes = Vec::new();
        let instances = match self.setup.channel_components.get(name) {
            Some(list) => list.clone(),
            None => {
                let n = if default_count >= 0 {
                    default_count as u32
                } else {
                    limit / 2
                };
                let n = n.min(lightloom_engine::components::MAX_COMPONENT_LEDS);
                if n == 0 {
                    Vec::new()
                } else {
                    notes.push(format!(
                        "{name}: no components assigned, using Default Strip - {n}"
                    ));
                    vec![ComponentInstance::new(Component::default_strip(n))]
                }
            }
        };
        let components = instances
            .into_iter()
            .map(|instance| self.place_component(instance))
            .collect();
        self.channels.push(Channel {
            name: name.to_string(),
            led_limit: limit,
            components,
        });
        Ok(notes)
    }

    fn place_component(&self, instance: ComponentInstance) -> ChannelComponent {
        let c = &instance.component;
        let geometry = DeviceGeometry {
            size: [c.width, c.height],
            led_positions: c.led_coordinates.clone(),
        };
        let placement = self.setup.placements.get(&instance.id).cloned();
        let surface = Surface::new(geometry, placement, &self.sampler, self.brightness);
        ChannelComponent {
            slots: c.stream_slots(),
            instance,
            surface,
        }
    }

    /// `device.removeChannel(name)`. The log line to print.
    pub fn remove_channel(&mut self, name: &str) -> String {
        match self.channels.iter().position(|c| c.name == name) {
            Some(i) => {
                self.channels.remove(i);
                format!("Removing Channel: {name}")
            }
            None => format!("Removing Channel: {name} (not found)"),
        }
    }

    /// `device.getChannelNames()`, creation order.
    pub fn channel_names(&self) -> Vec<String> {
        self.channels.iter().map(|c| c.name.clone()).collect()
    }

    /// The channel called `name`.
    pub fn channel(&self, name: &str) -> Option<&Channel> {
        self.channels.iter().find(|c| c.name == name)
    }

    /// `channel.SetLedLimit(n)`.
    pub fn set_channel_limit(&mut self, name: &str, n: i32) -> bool {
        match self.channels.iter_mut().find(|c| c.name == name) {
            Some(ch) => {
                ch.led_limit = u32::try_from(n).unwrap_or(0);
                true
            }
            None => false,
        }
    }

    /// LEDs each channel exposes, in creation order: its components' sum, cut
    /// at the channel limit and at what is left of the device limit (0 = none).
    pub fn led_counts(&self) -> Vec<usize> {
        let mut left = (self.led_limit > 0).then_some(self.led_limit as usize);
        self.channels
            .iter()
            .map(|ch| {
                let mut n = ch.raw_led_count();
                if ch.led_limit > 0 {
                    n = n.min(ch.led_limit as usize);
                }
                if let Some(l) = left.as_mut() {
                    n = n.min(*l);
                    *l -= n;
                }
                n
            })
            .collect()
    }

    /// `channel.ledCount` / `channel.LedCount()`, `None` for an unknown channel.
    pub fn channel_led_count(&self, name: &str) -> Option<usize> {
        let i = self.channels.iter().position(|c| c.name == name)?;
        self.led_counts().get(i).copied()
    }

    /// `device.getLedCount()`: LEDs over all channels.
    pub fn total_led_count(&self) -> usize {
        self.led_counts().iter().sum()
    }

    /// `channel.getColors()` as flat RGB: the components' streams in chain
    /// order, cut to [`Children::channel_led_count`]. Empty for an unknown channel.
    pub fn channel_colors(&self, frame: &Frame, name: &str) -> Vec<u8> {
        let Some(count) = self.channel_led_count(name) else {
            return Vec::new();
        };
        let Some(ch) = self.channel(name) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(ch.raw_led_count() * 3);
        for c in &ch.components {
            if out.len() >= count * 3 {
                break;
            }
            c.stream(frame, &self.sampler, &mut out);
        }
        out.truncate(count * 3);
        out
    }

    /// The component `id` of channel `name`.
    pub fn component(&self, name: &str, id: &str) -> Option<&ChannelComponent> {
        self.channel(name)?
            .components
            .iter()
            .find(|c| c.instance.id == id)
    }

    /// `channel.getComponentColors(id)` as flat RGB (empty when unknown).
    pub fn component_colors(&self, frame: &Frame, name: &str, id: &str) -> Vec<u8> {
        let mut out = Vec::new();
        if let Some(c) = self.component(name, id) {
            c.stream(frame, &self.sampler, &mut out);
        }
        out
    }
}

fn missing(call: &str, id: &str) -> String {
    format!("device.{call}(): Failed to find subdevice with name: {id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_engine::{Point, Rgb, Scale};

    fn children() -> Children {
        let mut c = Children::default();
        c.set_sampler(SamplerConfig::default());
        c.set_setup(ChildSetup {
            device_key: "1462:7c91:S".into(),
            ..ChildSetup::default()
        });
        c
    }

    fn gradient() -> Frame {
        // Red rises along x, so each LED of a fitted strip gets a distinct colour.
        let mut f = Frame::new(320, 200);
        for y in 0..200 {
            for x in 0..320 {
                f.set_pixel(x, y, Rgb::new((x * 255 / 319) as u8, 0, 10));
            }
        }
        f
    }

    #[test]
    fn subdevices_are_created_once_resized_and_sampled() {
        let mut c = children();
        assert_eq!(
            c.create_subdevice("Header 1").unwrap(),
            "Created Sub device with uid: 1462:7c91:S:Header 1"
        );
        assert_eq!(c.create_subdevice("Header 1").unwrap(), "");
        c.set_subdevice_name("Header 1", "MSI - Header 1".into()).unwrap();
        c.set_subdevice_leds("Header 1", vec!["Led 1".into(), "Led 2".into()], &[0, 0, 1, 0])
            .unwrap();
        assert!(c
            .set_subdevice_leds("Header 1", vec!["a".into()], &[])
            .unwrap_err()
            .contains("vary in length"));
        c.set_subdevice_size("Header 1", 2, 1).unwrap();
        assert!(c.set_subdevice_size("Header 1", 0, 1).is_err());
        assert!(c.set_subdevice_size("nope", 1, 1).is_err());
        let s = &c.subdevices()[0];
        assert_eq!((s.size(), s.name.as_str()), ([2, 1], "MSI - Header 1"));
        // Fitted: 2 cells across 320 px (scale clamps to 75 px per cell), centred.
        assert_eq!(s.placement().scale, Some(Scale::uniform(75.0)));
        let f = gradient();
        let left = c.subdevice_color(&f, "Header 1", 0, 0).unwrap();
        let right = c.subdevice_color(&f, "Header 1", 1, 0).unwrap();
        assert!(left[0] < right[0], "{left:?} {right:?}");
        assert!(c
            .subdevice_color(&f, "Header 1", 2, 0)
            .unwrap_err()
            .contains("Out of bounds"));
        assert!(c.subdevice_color(&f, "nope", 0, 0).is_err());
        assert_eq!(c.subdevice_ids(), vec!["Header 1".to_string()]);
        assert_eq!(
            c.remove_subdevice("Header 1"),
            "Removing Subdevice: MSI - Header 1 (Header 1)"
        );
        assert_eq!(
            c.remove_subdevice("Header 1"),
            "Failed to find subdevice with name: Header 1"
        );
        assert!(c.set_subdevice_image("x", "a.GIF".into()).is_err());
    }

    #[test]
    fn plugin_supplied_sizes_are_bounded() {
        let mut c = children();
        c.create_subdevice("S").unwrap();
        let names = vec!["n".to_string(); MAX_SUBDEVICE_LEDS + 1];
        let flat = vec![0; (MAX_SUBDEVICE_LEDS + 1) * 2];
        assert!(c
            .set_subdevice_leds("S", names, &flat)
            .unwrap_err()
            .contains("more than"));
        let long = "x".repeat(1000);
        c.set_subdevice_leds("S", vec![long], &[0, 0]).unwrap();
        assert_eq!(c.subdevices()[0].led_names[0].chars().count(), MAX_LED_NAME_CHARS);
        assert!(c
            .set_subdevice_image("S", "a".repeat(MAX_IMAGE_BYTES + 1))
            .is_err());
        c.set_subdevice_led_map("S", &vec![1; MAX_SUBDEVICE_LEDS * 2])
            .unwrap();
        assert_eq!(c.subdevices()[0].led_map.len(), MAX_SUBDEVICE_LEDS);
    }

    #[test]
    fn caller_placements_win_over_fitting() {
        let mut c = children();
        c.setup.placements.insert(
            "1462:7c91:S:Fixed".into(),
            Placement {
                position: Point { x: 10.0, y: 20.0 },
                scale: Some(Scale::uniform(4.0)),
                ..Placement::default()
            },
        );
        c.create_subdevice("Fixed").unwrap();
        c.set_subdevice_size("Fixed", 3, 3).unwrap();
        assert_eq!(c.subdevices()[0].placement().position, Point { x: 10.0, y: 20.0 });
    }

    #[test]
    fn unconfigured_channels_get_the_placeholder_strip() {
        let mut c = children();
        let notes = c.add_channel("JRainbow 1", 120, -1).unwrap();
        assert_eq!(
            notes,
            vec!["JRainbow 1: no components assigned, using Default Strip - 60".to_string()]
        );
        c.add_channel("Explicit", 80, 40).unwrap();
        c.add_channel("Empty", 0, -1).unwrap();
        assert_eq!(c.channel_names(), vec!["JRainbow 1", "Explicit", "Empty"]);
        assert_eq!(c.channel_led_count("JRainbow 1"), Some(60));
        assert_eq!(c.channel_led_count("Explicit"), Some(40));
        assert_eq!(c.channel_led_count("Empty"), Some(0));
        assert_eq!(c.channel_led_count("missing"), None);
        assert!(c.channel("JRainbow 1").unwrap().only_default_components());
        assert_eq!(c.total_led_count(), 100);
        let colors = c.channel_colors(&gradient(), "JRainbow 1");
        assert_eq!(colors.len(), 180);
        // The fitted strip spans the canvas: red rises from the first LED to the last.
        assert!(colors[0] < colors[177], "{} {}", colors[0], colors[177]);
        assert_eq!(c.remove_channel("Explicit"), "Removing Channel: Explicit");
        assert_eq!(c.channel_names(), vec!["JRainbow 1", "Empty"]);
    }

    #[test]
    fn assigned_components_follow_mapping_and_limits() {
        let mut c = children();
        // A 3-LED component whose mapping reverses the wire order.
        let mut comp = Component::strip("Rev".into(), 3);
        comp.led_mapping = vec![2, 1, 0];
        let rev = ComponentInstance::with_id(comp, "rev".into());
        let strip = ComponentInstance::with_id(Component::strip("S".into(), 4), "s".into());
        c.setup
            .channel_components
            .insert("Ch".into(), vec![rev.clone(), strip.clone()]);
        c.setup.channel_components.insert("Off".into(), vec![]);
        c.add_channel("Ch", 6, -1).unwrap();
        c.add_channel("Off", 10, 5).unwrap();
        assert_eq!(
            c.channel_led_count("Ch"),
            Some(6),
            "7 LEDs cut at the channel limit"
        );
        assert_eq!(
            c.channel_led_count("Off"),
            Some(0),
            "an explicit empty setup stays empty"
        );
        let f = gradient();
        let rev_stream = c.component_colors(&f, "Ch", "rev");
        assert_eq!(rev_stream.len(), 9);
        // LED 0 (leftmost) lands in slot 2: the stream runs right to left.
        assert!(rev_stream[0] > rev_stream[6], "{rev_stream:?}");
        let all = c.channel_colors(&f, "Ch");
        assert_eq!(all.len(), 18);
        assert_eq!(&all[..9], &rev_stream[..]);
        assert!(c.component("Ch", "s").is_some());
        assert!(c.component_colors(&f, "Ch", "nope").is_empty());
        // Device budget: 4 LEDs in total, taken by the first channel first.
        c.set_led_limit(4);
        assert_eq!(c.led_counts(), vec![4, 0]);
        assert!(c.set_channel_limit("Ch", 2));
        assert_eq!(c.led_counts(), vec![2, 0]);
        assert!(!c.set_channel_limit("nope", 2));
    }

    #[test]
    fn brightness_scales_every_child() {
        let mut c = children();
        c.add_channel("Ch", 2, 1).unwrap();
        c.create_subdevice("Sub").unwrap();
        let f = Frame::solid(320, 200, Rgb::new(200, 100, 50));
        assert_eq!(c.channel_colors(&f, "Ch"), vec![200, 100, 50]);
        c.set_brightness(Brightness {
            global: 50,
            layout: 100,
        });
        assert_eq!(c.channel_colors(&f, "Ch"), vec![100, 50, 25]);
        assert_eq!(c.subdevice_color(&f, "Sub", 0, 0).unwrap(), [100, 50, 25]);
    }
}
