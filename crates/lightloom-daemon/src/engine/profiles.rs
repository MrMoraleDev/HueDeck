// SPDX-License-Identifier: GPL-3.0-or-later
//! Profiles (OpenRGB-style) and live hardware modes.
//!
//! * A **profile** is a named snapshot of the whole lighting setup
//!   ([`lightloom_api::model::Profile`]): the effect with its control values, the
//!   global brightness and canvas pause, the current layout (placements with
//!   their scale, rotation, per-device brightness and enable, and the layout
//!   brightness), and one open settings object per known device (enable, plugin
//!   parameters, channel components, exit mode, hardware mode). The per-device
//!   object keeps keys this build does not know, so later per-device settings
//!   (per-LED colours) travel in the same document without a schema change.
//!   Saving captures the setup now; applying sets it back (the layout is saved
//!   under its name and switched to; devices Lightloom has never seen are
//!   skipped and reported). The default profile (`settings.defaultProfile`) is
//!   applied at start, before the canvas and the devices start.
//! * A **hardware mode** (`HardwareModes()` / `SetHardwareMode`, crate
//!   `lightloom_plugin_host::hwmode`) is kept per device (`hardwareMode.<key>`)
//!   and handed to every new session of the device; the running session gets
//!   it at once. Live only: nothing is stored in the device.

use super::{internal, keys, lock, Engine};
use lightloom_api::model::{
    DeviceHardwareMode, HardwareModeView, Profile, ProfileDevice, ProfileLighting, ProfileView,
    PROFILE_FORMAT, PROFILE_VERSION,
};
use lightloom_api::{ApiError, Request};
use lightloom_plugin_host::hwmode::{HardwareModeChoice, HardwareModeInfo};
use lightloom_plugin_host::save::ShutdownMode;
use lightloom_plugin_host::session::{Control, HardwareModeCall};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::time::Duration;

/// Longest profile name.
const MAX_NAME: usize = 100;
/// How long a hardware-mode request waits for the device's session.
const MODE_WAIT: Duration = Duration::from_secs(10);

/// A hardware-mode choice as the plugin host takes it.
pub(super) fn to_choice(m: &DeviceHardwareMode) -> HardwareModeChoice {
    HardwareModeChoice {
        id: m.id.clone(),
        colors: m.colors.clone(),
        speed: m.speed,
        brightness: m.brightness,
    }
}

/// A hardware-mode choice as clients see it.
pub(super) fn from_choice(c: &HardwareModeChoice) -> DeviceHardwareMode {
    DeviceHardwareMode {
        id: c.id.clone(),
        colors: c.colors.clone(),
        speed: c.speed,
        brightness: c.brightness,
    }
}

/// A plugin's `HardwareModes()` entry as clients see it.
pub(super) fn mode_view(m: &HardwareModeInfo) -> HardwareModeView {
    HardwareModeView {
        id: m.id.clone(),
        name: m.name.clone(),
        colors: m.colors,
        color_names: m.color_names.clone(),
        speed: m.speed,
        brightness: m.brightness,
    }
}

/// `name` trimmed, when it can name a profile.
fn check_name(name: &str) -> Result<String, ApiError> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > MAX_NAME || n.chars().any(char::is_control) {
        return Err(ApiError::bad_request(format!(
            "a profile name is 1-{MAX_NAME} characters, no control characters"
        )));
    }
    Ok(n.to_string())
}

fn no_profile(name: &str) -> ApiError {
    ApiError::not_found(format!("no profile {name:?}"))
}

/// `base`, or `base (2)`, `base (3)`, ... the first one not in `taken`.
fn unique(base: &str, taken: &BTreeSet<String>) -> String {
    let mut name = base.to_string();
    let mut n = 2;
    while taken.contains(&name) {
        name = format!("{base} ({n})");
        n += 1;
    }
    name
}

impl Engine {
    /// The profile list for the snapshot.
    pub(super) fn profile_views(&self) -> Vec<ProfileView> {
        self.store
            .profiles()
            .unwrap_or_default()
            .into_iter()
            .map(|p| ProfileView {
                devices: p.devices.len(),
                effect: p.lighting.effect,
                layout: p.layout.name,
                name: p.name,
                created_at: p.created_at,
                updated_at: p.updated_at,
            })
            .collect()
    }

    /// The current profile and the default one from the database (start, import).
    pub(super) fn load_profiles(&mut self) {
        let name = |k: &str| {
            self.store
                .get::<Option<String>>(k)
                .ok()
                .flatten()
                .flatten()
                .filter(|n| !n.is_empty())
        };
        self.profile = name(keys::PROFILE);
        self.default_profile = name(keys::DEFAULT_PROFILE);
    }

    /// Apply the default profile at start (before the canvas and the devices start).
    pub(super) fn apply_default_profile(&mut self) {
        let Some(name) = self.default_profile.clone() else {
            return;
        };
        match self.store.profile(&name) {
            Ok(Some(p)) => match self.apply_profile(&p, false) {
                Ok(notes) => {
                    self.log(format!("default profile {name} applied"));
                    for n in notes {
                        self.log(format!("profile {name}: {n}"));
                    }
                }
                Err(e) => self
                    .warnings
                    .push(format!("the default profile {name} was not applied: {e}")),
            },
            _ => self
                .warnings
                .push(format!("the default profile {name} does not exist any more")),
        }
    }

    /// Profile requests.
    pub(super) fn handle_profiles(&mut self, request: Request) -> Result<Value, ApiError> {
        match request {
            Request::SaveProfile { name } => {
                let name = check_name(&name)?;
                let p = self.capture_profile(&name)?;
                self.store.save_profile(&p).map_err(internal)?;
                self.set_current_profile(Some(name.clone()))?;
                self.log(format!("profile {name} saved"));
                Ok(json!({ "profile": name }))
            }
            Request::GetProfile { name } => {
                let p = self
                    .store
                    .profile(&name)
                    .map_err(internal)?
                    .ok_or_else(|| no_profile(&name))?;
                serde_json::to_value(p).map_err(internal)
            }
            Request::ApplyProfile { name } => {
                let p = self
                    .store
                    .profile(&name)
                    .map_err(internal)?
                    .ok_or_else(|| no_profile(&name))?;
                let notes = self.apply_profile(&p, true)?;
                self.log(format!("profile {name} applied"));
                Ok(json!({ "profile": name, "notes": notes }))
            }
            Request::DeleteProfile { name } => {
                if !self.store.delete_profile(&name).map_err(internal)? {
                    return Err(no_profile(&name));
                }
                if self.default_profile.as_deref() == Some(name.as_str()) {
                    self.set_default_profile(None)?;
                }
                if self.profile.as_deref() == Some(name.as_str()) {
                    self.set_current_profile(None)?;
                }
                Ok(json!({}))
            }
            Request::RenameProfile { name, to } => {
                let to = check_name(&to)?;
                if to != name && self.store.profile(&to).map_err(internal)?.is_some() {
                    return Err(ApiError::conflict(format!("a profile {to:?} exists already")));
                }
                if !self.store.rename_profile(&name, &to).map_err(internal)? {
                    return Err(no_profile(&name));
                }
                if self.default_profile.as_deref() == Some(name.as_str()) {
                    self.set_default_profile(Some(to.clone()))?;
                }
                if self.profile.as_deref() == Some(name.as_str()) {
                    self.set_current_profile(Some(to.clone()))?;
                }
                Ok(json!({ "profile": to }))
            }
            Request::DuplicateProfile { name, to } => {
                let to = check_name(&to)?;
                let mut p = self
                    .store
                    .profile(&name)
                    .map_err(internal)?
                    .ok_or_else(|| no_profile(&name))?;
                if self.store.profile(&to).map_err(internal)?.is_some() {
                    return Err(ApiError::conflict(format!("a profile {to:?} exists already")));
                }
                p.name = to.clone();
                p.created_at.clear();
                self.store.save_profile(&p).map_err(internal)?;
                Ok(json!({ "profile": to }))
            }
            Request::ImportProfile { profile, name } => {
                let mut p: Profile = serde_json::from_value(profile)
                    .map_err(|e| ApiError::bad_request(format!("not a Lightloom profile: {e}")))?;
                if p.format != PROFILE_FORMAT {
                    return Err(ApiError::bad_request(format!(
                        "not a Lightloom profile (format {:?})",
                        p.format
                    )));
                }
                if p.version > PROFILE_VERSION {
                    return Err(ApiError::bad_request(format!(
                        "profile version {} is newer than this Lightloom ({PROFILE_VERSION})",
                        p.version
                    )));
                }
                let base = check_name(name.as_deref().unwrap_or(&p.name))?;
                let taken: BTreeSet<String> = self
                    .store
                    .profiles()
                    .map_err(internal)?
                    .into_iter()
                    .map(|p| p.name)
                    .collect();
                p.name = unique(&base, &taken);
                p.version = PROFILE_VERSION;
                p.created_at.clear();
                let saved = self.store.save_profile(&p).map_err(internal)?;
                self.log(format!("profile {} imported", saved.name));
                Ok(json!({ "profile": saved.name }))
            }
            Request::SetDefaultProfile { name } => {
                let name = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
                if let Some(n) = &name {
                    if self.store.profile(n).map_err(internal)?.is_none() {
                        return Err(no_profile(n));
                    }
                }
                self.set_default_profile(name)?;
                Ok(json!({ "defaultProfile": self.default_profile }))
            }
            _ => Err(ApiError::internal("not a profile request")),
        }
    }

    fn set_current_profile(&mut self, name: Option<String>) -> Result<(), ApiError> {
        self.store.set(keys::PROFILE, &name).map_err(internal)?;
        self.profile = name;
        self.dirty = true;
        Ok(())
    }

    fn set_default_profile(&mut self, name: Option<String>) -> Result<(), ApiError> {
        self.store.set(keys::DEFAULT_PROFILE, &name).map_err(internal)?;
        self.default_profile = name;
        self.dirty = true;
        Ok(())
    }

    /// The whole lighting setup now, as profile `name`.
    fn capture_profile(&self, name: &str) -> Result<Profile, ApiError> {
        let values = match &self.effect {
            Some(id) => self.store.effect_params(id).map_err(internal)?,
            None => Map::new(),
        };
        let mut layout = self.layout.clone();
        layout.created_at = None;
        layout.updated_at = None;
        let defaults = lock(&self.defaults).clone();
        let mut devices = std::collections::BTreeMap::new();
        for row in self.store.devices().map_err(internal)? {
            let channels = self.store.channels(&row.key).map_err(internal)?;
            devices.insert(
                row.key.clone(),
                ProfileDevice {
                    enabled: Some(row.enabled),
                    parameters: row.parameters,
                    channels,
                    shutdown_mode: defaults
                        .shutdown_overrides
                        .get(&row.key)
                        .map(|m| m.as_str().to_string()),
                    hardware_mode: defaults.hardware_modes.get(&row.key).map(from_choice),
                    extra: Map::new(),
                },
            );
        }
        Ok(Profile {
            format: PROFILE_FORMAT.into(),
            version: PROFILE_VERSION,
            name: name.to_string(),
            created_at: String::new(),
            updated_at: String::new(),
            lighting: ProfileLighting {
                effect: self.effect.clone(),
                values,
                brightness: self.brightness,
                enabled: self.enabled,
            },
            layout,
            devices,
        })
    }

    /// Set the setup of profile `p` back; `live`: the canvas and the devices run
    /// (at start they do not yet, and start with it). Answers with what was skipped.
    fn apply_profile(&mut self, p: &Profile, live: bool) -> Result<Vec<String>, ApiError> {
        let mut notes = Vec::new();
        if live {
            self.stop_playlist();
        }

        // Global brightness and the canvas pause.
        self.brightness = p.lighting.brightness.min(100);
        self.store
            .set(keys::BRIGHTNESS, &self.brightness)
            .map_err(internal)?;
        let paused_changed = p.lighting.enabled != self.enabled;
        if paused_changed {
            self.enabled = p.lighting.enabled;
            self.store.set(keys::ENABLED, &self.enabled).map_err(internal)?;
        }

        // The layout, saved under its name and switched to (pushes the brightness too).
        let mut layout = p.layout.clone();
        if layout.name.trim().is_empty() {
            layout.name = p.name.clone();
        }
        layout.created_at = None;
        layout.updated_at = None;
        self.store.save_layout(&layout).map_err(internal)?;
        let saved = self
            .store
            .layout(&layout.name)
            .map_err(internal)?
            .unwrap_or(layout);
        self.switch_layout(saved).map_err(internal)?;

        // Devices.
        let rows = self.store.devices().map_err(internal)?;
        let mut restart = BTreeSet::new();
        let mut reconcile = false;
        for (key, d) in &p.devices {
            let Some(row) = rows.iter().find(|r| &r.key == key) else {
                notes.push(format!("{key}: not seen by Lightloom yet, skipped"));
                continue;
            };
            if let Some(on) = d.enabled {
                if on != row.enabled {
                    self.store.set_device_enabled(key, on).map_err(internal)?;
                    if on {
                        restart.insert(key.clone());
                    }
                    reconcile = true;
                }
            }
            if !d.parameters.is_empty() {
                let mut values = d.parameters.clone();
                self.coerce_parameters(key, &mut values);
                self.store.set_device_parameters(key, &values).map_err(internal)?;
                lock(&self.defaults)
                    .parameters
                    .entry(key.clone())
                    .or_default()
                    .extend(values.clone());
                if let Some(h) = self.sessions().find(|h| h.key() == key) {
                    h.control(Control::Parameters(values));
                }
            }
            let stored = self.store.channels(key).map_err(internal)?;
            for (channel, components) in &d.channels {
                if stored.get(channel) == Some(components) {
                    continue;
                }
                match self.store.set_channel(key, channel, components) {
                    Ok(()) => {
                        restart.insert(key.clone());
                    }
                    Err(e) => notes.push(format!("{key} {channel}: {e}")),
                }
            }
            let mode = match d.shutdown_mode.as_deref() {
                None | Some("" | "default") => None,
                Some(m) => match ShutdownMode::parse(m) {
                    Some(mode) => Some(mode),
                    None => {
                        notes.push(format!("{key}: unknown exit mode {m:?}, skipped"));
                        None
                    }
                },
            };
            self.store
                .set(
                    &format!("{}{key}", keys::DEVICE_SHUTDOWN_MODE),
                    &mode.map_or("", ShutdownMode::as_str),
                )
                .map_err(internal)?;
            let effective = {
                let mut defaults = lock(&self.defaults);
                match mode {
                    Some(m) => defaults.shutdown_overrides.insert(key.clone(), m),
                    None => defaults.shutdown_overrides.remove(key),
                };
                defaults.shutdown_mode_of(key)
            };
            if let Some(h) = self.sessions().find(|h| h.key() == key) {
                h.control(Control::ShutdownMode(effective));
            }
            // A session restarted below gets the mode from the defaults.
            let wait = live && !restart.contains(key);
            if let Err(e) = self.set_hardware_mode(key, d.hardware_mode.clone(), wait) {
                notes.push(format!("{key}: hardware mode: {}", e.message));
            }
        }
        if !restart.is_empty() {
            self.reload_channels();
        }
        if reconcile {
            self.reconcile();
        }
        let present: Vec<String> = self.present.iter().map(|p| p.device.key.clone()).collect();
        for key in present.iter().filter(|k| restart.contains(*k)) {
            self.restart_device(key);
        }

        // The effect, last, so it starts with the profile's values.
        let effect = p.lighting.effect.clone().filter(|id| {
            let ok = self.effects.get(id).is_some();
            if !ok {
                notes.push(format!(
                    "the effect {id} is not in the library, the current one stays"
                ));
            }
            ok
        });
        match effect {
            Some(id) if live => {
                self.show_effect(id, p.lighting.values.clone())?;
            }
            Some(id) => {
                if !p.lighting.values.is_empty() {
                    self.store
                        .set_effect_params(&id, &p.lighting.values)
                        .map_err(internal)?;
                }
                self.store.set(keys::EFFECT, &id).map_err(internal)?;
                self.effect = Some(id);
            }
            None if live && paused_changed => self.start_canvas(),
            None => {}
        }
        self.set_current_profile(Some(p.name.clone()))?;
        self.dirty = true;
        Ok(notes)
    }

    /// Choose device `key`'s hardware mode (`None`: follow the effect): checked
    /// against its plugin's modes when it is connected, kept per device, handed to
    /// the running session (`wait`: and its answer awaited; a refusal undoes the choice).
    pub(super) fn set_hardware_mode(
        &mut self,
        key: &str,
        mode: Option<DeviceHardwareMode>,
        wait: bool,
    ) -> Result<Value, ApiError> {
        let known = self
            .store
            .devices()
            .map_err(internal)?
            .iter()
            .any(|r| r.key == key);
        if !known {
            return Err(ApiError::not_found(format!("no device {key}")));
        }
        let mut choice = mode.as_ref().map(to_choice);
        if let (Some(c), Some(p)) = (&choice, self.present.iter().find(|p| p.device.key == key)) {
            choice = Some(
                c.normalized(&p.plugin.meta.hardware_modes)
                    .map_err(ApiError::bad_request)?,
            );
        }
        let before = lock(&self.defaults).hardware_modes.get(key).cloned();
        if before == choice {
            return Ok(
                json!({ "device": key, "hardwareMode": choice.as_ref().map(from_choice), "applied": true }),
            );
        }
        self.remember_hardware_mode(key, choice.clone())?;
        let Some(h) = self.sessions().find(|h| h.key() == key) else {
            return Ok(
                json!({ "device": key, "hardwareMode": choice.as_ref().map(from_choice), "applied": false }),
            );
        };
        let (reply, rx) = std::sync::mpsc::channel();
        let sent = h.control(Control::HardwareMode(HardwareModeCall {
            choice: choice.clone(),
            reply: wait.then_some(reply),
        }));
        let applied = if wait && sent {
            match rx.recv_timeout(MODE_WAIT) {
                Ok(Ok(())) => true,
                Ok(Err(e)) => {
                    self.remember_hardware_mode(key, before)?;
                    return Err(ApiError::conflict(e));
                }
                Err(_) => false,
            }
        } else {
            sent
        };
        self.log(match &choice {
            Some(c) => format!("{key}: hardware mode {}", c.id),
            None => format!("{key}: follows the effect"),
        });
        self.dirty = true;
        Ok(json!({ "device": key, "hardwareMode": choice.as_ref().map(from_choice), "applied": applied }))
    }

    fn remember_hardware_mode(
        &mut self,
        key: &str,
        choice: Option<HardwareModeChoice>,
    ) -> Result<(), ApiError> {
        self.store
            .set(&format!("{}{key}", keys::DEVICE_HARDWARE_MODE), &choice)
            .map_err(internal)?;
        let mut d = lock(&self.defaults);
        match choice {
            Some(c) => d.hardware_modes.insert(key.to_string(), c),
            None => d.hardware_modes.remove(key),
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_checked_and_made_unique() {
        assert_eq!(check_name("  Night ").unwrap(), "Night");
        assert!(check_name(" ").is_err());
        assert!(check_name("a\nb").is_err());
        assert!(check_name(&"x".repeat(101)).is_err());
        let taken: BTreeSet<String> = ["A".to_string(), "A (2)".to_string()].into();
        assert_eq!(unique("A", &taken), "A (3)");
        assert_eq!(unique("B", &taken), "B");
    }
}
