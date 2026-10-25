// SPDX-License-Identifier: GPL-3.0-or-later
//! Per-LED custom colours: the "custom LEDs" mode of an endpoint and its
//! zones (`lightloom_engine::custom`), the LED painter's requests and their
//! persistence.
//!
//! * An endpoint in custom mode has static colours in the shared
//!   [`DirectLeds`](lightloom_engine::DirectLeds): every session reads them
//!   once per frame, so the plugin's `device.color(x, y)` (and channel and
//!   subdevice colours) answer from the per-LED map instead of the canvas,
//!   with the brightness product applied. Plugins do not change.
//! * Edits go to the direct colours at once (the next frame shows them) and
//!   to the database [`PERSIST_AFTER`] after the last edit of a burst (a
//!   painter's drag sends many), and on shutdown.
//! * Edits need the endpoint's session: the slot count comes from its LEDs.
//!   Modes and colours of endpoints that are away stay in the database and
//!   come back with them.

use super::{internal, Engine};
use crate::store::Store;
use lightloom_api::model::{LedEdit, LedEndpointView, LedMapView};
use lightloom_api::ApiError;
use lightloom_engine::{led_spots, CustomLeds, CustomLedsError, DirectLeds, Rgb, SlotLayout};
use lightloom_plugin_host::session::EndpointKind;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

/// How long after the last edit the colours are written to the database.
pub(super) const PERSIST_AFTER: Duration = Duration::from_millis(750);

/// Every endpoint's custom LEDs, and what is not saved yet.
#[derive(Debug, Default)]
pub(super) struct CustomLedsState {
    /// By endpoint uid: the device key and the state.
    states: BTreeMap<String, (String, CustomLeds)>,
    /// Uids edited since the last save.
    unsaved: BTreeSet<String>,
    last_edit: Option<Instant>,
}

impl CustomLedsState {
    /// The stored state; endpoints in custom mode get their static colours.
    pub(super) fn load(store: &Store, direct: &DirectLeds) -> anyhow::Result<Self> {
        let mut s = Self::default();
        for (uid, row) in store.custom_leds()? {
            if row.state.enabled {
                direct.set_static(&uid, &row.state.colors);
            }
            s.states.insert(uid, (row.device, row.state));
        }
        Ok(s)
    }

    /// The custom LEDs of device `key`'s endpoints, by uid (profiles).
    pub(super) fn of_device(&self, key: &str) -> BTreeMap<String, CustomLeds> {
        self.states
            .iter()
            .filter(|(_, (device, state))| device == key && !state.is_blank())
            .map(|(uid, (_, state))| (uid.clone(), state.clone()))
            .collect()
    }

    /// Whether `uid` shows its custom colours.
    pub(super) fn is_custom(&self, uid: &str) -> bool {
        self.states.get(uid).is_some_and(|(_, s)| s.enabled)
    }

    /// Write the edited states to the database once [`PERSIST_AFTER`] passed
    /// since the last edit (`now`: at once). Failures stay unsaved for the next try.
    pub(super) fn save(&mut self, store: &Store, now: bool) -> anyhow::Result<()> {
        if self.unsaved.is_empty() || !now && self.last_edit.is_some_and(|t| t.elapsed() < PERSIST_AFTER) {
            return Ok(());
        }
        for uid in std::mem::take(&mut self.unsaved) {
            let Some((device, state)) = self.states.get(&uid) else {
                continue;
            };
            if let Err(e) = store.set_custom_leds(&uid, device, state) {
                self.unsaved.insert(uid);
                return Err(e);
            }
        }
        Ok(())
    }

    fn touched(&mut self, uid: &str) {
        self.unsaved.insert(uid.to_string());
        self.last_edit = Some(Instant::now());
    }

    /// Forget everything (an import replaced the database): drop every static
    /// colour, then take the stored state again.
    pub(super) fn reload(&mut self, store: &Store, direct: &DirectLeds) -> anyhow::Result<()> {
        for uid in self.states.keys() {
            direct.clear_static(uid);
        }
        *self = Self::load(store, direct)?;
        Ok(())
    }
}

fn kind_name(kind: EndpointKind) -> &'static str {
    match kind {
        EndpointKind::Device => "device",
        EndpointKind::Subdevice => "subdevice",
        EndpointKind::Component => "component",
    }
}

fn color(text: &str) -> Result<Rgb, ApiError> {
    Rgb::from_hex(text).ok_or_else(|| ApiError::bad_request(format!("{text:?} is not a #rrggbb colour")))
}

fn edit_error(e: CustomLedsError) -> ApiError {
    match e {
        CustomLedsError::NoZone(_) => ApiError::not_found(e.to_string()),
        _ => ApiError::bad_request(e.to_string()),
    }
}

impl Engine {
    /// The endpoints of device `key`'s session with their LEDs and custom state.
    fn led_targets(&self, key: &str) -> Result<Vec<LedEndpointView>, ApiError> {
        let session = self
            .sessions()
            .find(|h| h.key() == key)
            .ok_or_else(|| ApiError::conflict(format!("device {key} is not running")))?;
        Ok(session
            .endpoints()
            .iter()
            .map(|e| {
                let layout = match e.kind {
                    EndpointKind::Subdevice => SlotLayout::Cells,
                    _ => SlotLayout::Leds,
                };
                let (leds, slots) = led_spots(layout, e.size, &e.led_positions, &e.led_names);
                let mut state = self
                    .custom_leds
                    .states
                    .get(&e.uid)
                    .map(|(_, s)| s.clone())
                    .unwrap_or_default();
                state.fit(slots);
                LedEndpointView {
                    uid: e.uid.clone(),
                    kind: kind_name(e.kind).into(),
                    name: e.name.clone(),
                    channel: e.channel.clone(),
                    size: e.size,
                    slots,
                    leds,
                    custom: state.enabled,
                    colors: state.colors.iter().map(|c| c.to_hex()).collect(),
                    zones: state.zones,
                }
            })
            .collect())
    }

    /// `GET /v1/devices/{key}/leds`.
    pub(super) fn led_map(&self, key: &str) -> Result<Value, ApiError> {
        let endpoints = self.led_targets(key)?;
        let device_type = self
            .shared
            .snapshot
            .load()
            .devices
            .iter()
            .find(|d| d.key == key)
            .map(|d| d.device_type.clone())
            .unwrap_or_default();
        serde_json::to_value(LedMapView {
            key: key.to_string(),
            device_type,
            endpoints,
        })
        .map_err(internal)
    }

    /// The state of endpoint `uid` of device `key`, fitted to its slot count.
    fn led_state(&self, key: &str, uid: &str) -> Result<CustomLeds, ApiError> {
        let target = self
            .led_targets(key)?
            .into_iter()
            .find(|t| t.uid == uid)
            .ok_or_else(|| ApiError::not_found(format!("device {key} has no endpoint {uid}")))?;
        let mut state = self
            .custom_leds
            .states
            .get(uid)
            .map(|(_, s)| s.clone())
            .unwrap_or_default();
        state.fit(target.slots);
        Ok(state)
    }

    /// Keep `state` for `uid` and show it (or the canvas) from the next frame on.
    fn put_led_state(&mut self, key: &str, uid: &str, state: CustomLeds) {
        let was_custom = self.custom_leds.is_custom(uid);
        if state.enabled {
            self.shared.direct.set_static(uid, &state.colors);
        } else if was_custom {
            self.shared.direct.clear_static(uid);
        }
        if was_custom != state.enabled {
            // The device card shows the mode.
            self.dirty = true;
        }
        self.custom_leds
            .states
            .insert(uid.to_string(), (key.to_string(), state));
        self.custom_leds.touched(uid);
    }

    /// `PATCH /v1/devices/{key}/leds/{uid}`.
    pub(super) fn set_leds(&mut self, key: &str, uid: &str, edit: LedEdit) -> Result<Value, ApiError> {
        let mut state = self.led_state(key, uid)?;
        let before = state.clone();
        if let Some(list) = &edit.colors {
            let colors = list.iter().map(|c| color(c)).collect::<Result<Vec<_>, _>>()?;
            let changes: Vec<(u32, Rgb)> = colors
                .into_iter()
                .enumerate()
                .map(|(i, c)| (i as u32, c))
                .collect();
            state.set(&changes).map_err(edit_error)?;
        }
        if let Some(c) = &edit.fill {
            state.fill(color(c)?);
        }
        if !edit.set.is_empty() {
            let changes = edit
                .set
                .iter()
                .map(|(slot, c)| Ok((*slot, color(c)?)))
                .collect::<Result<Vec<_>, ApiError>>()?;
            state.set(&changes).map_err(edit_error)?;
        }
        for (zone, c) in &edit.zones {
            state.color_zone(zone, color(c)?).map_err(edit_error)?;
        }
        match edit.mode.as_deref() {
            None => {}
            Some("custom") => state.enabled = true,
            Some("effect") => state.enabled = false,
            Some(m) => {
                return Err(ApiError::bad_request(format!(
                    "mode {m:?}: use \"custom\" or \"effect\""
                )))
            }
        }
        let changed = state != before;
        let custom = state.enabled;
        if changed {
            self.put_led_state(key, uid, state);
        }
        Ok(json!({ "uid": uid, "custom": custom, "changed": changed }))
    }

    /// `PUT /v1/devices/{key}/leds/{uid}/zones/{name}`.
    pub(super) fn save_led_zone(
        &mut self,
        key: &str,
        uid: &str,
        name: &str,
        leds: &[u32],
    ) -> Result<Value, ApiError> {
        let mut state = self.led_state(key, uid)?;
        state.save_zone(name, leds).map_err(edit_error)?;
        let zones = state.zones.clone();
        self.put_led_state(key, uid, state);
        Ok(json!({ "uid": uid, "zones": zones }))
    }

    /// `DELETE /v1/devices/{key}/leds/{uid}/zones/{name}`.
    pub(super) fn delete_led_zone(&mut self, key: &str, uid: &str, name: &str) -> Result<Value, ApiError> {
        let mut state = self.led_state(key, uid)?;
        if !state.delete_zone(name) {
            return Err(ApiError::not_found(format!("no zone {name:?}")));
        }
        let zones = state.zones.clone();
        self.put_led_state(key, uid, state);
        Ok(json!({ "uid": uid, "zones": zones }))
    }

    /// Make device `key`'s custom LEDs exactly `wanted` (a profile's): endpoints
    /// it does not name go back to the effect, without colours or zones. Saved at once.
    pub(super) fn restore_custom_leds(&mut self, key: &str, wanted: BTreeMap<String, CustomLeds>) {
        let now: Vec<String> = self.custom_leds.of_device(key).into_keys().collect();
        for uid in now.iter().filter(|u| !wanted.contains_key(*u)) {
            self.put_led_state(key, uid, CustomLeds::default());
        }
        for (uid, state) in wanted {
            self.put_led_state(key, &uid, state);
        }
        self.save_custom_leds(true);
    }

    /// Save edited custom LEDs when their burst is over (`now`: at once).
    pub(super) fn save_custom_leds(&mut self, now: bool) {
        if let Err(e) = self.custom_leds.save(&self.store, now) {
            tracing::warn!("cannot save the custom LED colours: {e:#}");
        }
    }

    /// Take the custom LEDs from the database again (after an import).
    pub(super) fn reload_custom_leds(&mut self) {
        if let Err(e) = self.custom_leds.reload(&self.store, &self.shared.direct) {
            self.warnings
                .push(format!("cannot read the custom LED colours: {e:#}"));
        }
        self.dirty = true;
    }
}
