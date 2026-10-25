// SPDX-License-Identifier: GPL-3.0-or-later
//! Persistence: one SQLite database (`state.sqlite`, WAL) per user, per
//! ADR-0008. Only the engine thread holds the connection.
//!
//! Identifiers follow the upstream uid grammar (`vid:pid:serial`,
//! `<key>:<subdevice>`, component GUIDs), channel component arrays keep
//! SignalRGB's JSON shape, and values are JSON text, so the SignalRGB import
//! is a mapping and [`Store::export`] / [`Store::import`] round-trip byte for
//! byte (everything is ordered by key).

use anyhow::{bail, Context};
use lightloom_api::model::{Playlist, Profile};
use lightloom_engine::{Layout, Placement};
use rusqlite::{params, Connection, OptionalExtension};
use rusqlite_migration::{Migrations, M};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::Path;

/// Schema, oldest first. Never edit a released migration; append a new one.
const MIGRATIONS: &[&str] = &[
    r#"
CREATE TABLE kv (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
CREATE TABLE devices (
    key        TEXT PRIMARY KEY NOT NULL,
    vid        INTEGER NOT NULL,
    pid        INTEGER NOT NULL,
    name       TEXT NOT NULL DEFAULT '',
    plugin     TEXT NOT NULL DEFAULT '',
    enabled    INTEGER NOT NULL DEFAULT 1,
    first_seen TEXT NOT NULL,
    last_seen  TEXT NOT NULL
);
CREATE TABLE device_parameters (
    device TEXT NOT NULL,
    name   TEXT NOT NULL,
    value  TEXT NOT NULL,
    PRIMARY KEY (device, name)
);
CREATE TABLE layouts (
    name       TEXT PRIMARY KEY NOT NULL,
    brightness INTEGER NOT NULL DEFAULT 100,
    grid_size  INTEGER NOT NULL DEFAULT 4,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE placements (
    layout    TEXT NOT NULL REFERENCES layouts(name) ON DELETE CASCADE ON UPDATE CASCADE,
    uid       TEXT NOT NULL,
    placement TEXT NOT NULL,
    PRIMARY KEY (layout, uid)
);
CREATE TABLE channel_components (
    device     TEXT NOT NULL,
    channel    TEXT NOT NULL,
    components TEXT NOT NULL,
    PRIMARY KEY (device, channel)
);
CREATE TABLE effect_params (
    effect TEXT NOT NULL,
    name   TEXT NOT NULL,
    value  TEXT NOT NULL,
    PRIMARY KEY (effect, name)
);
CREATE TABLE presets (
    effect   TEXT NOT NULL,
    name     TEXT NOT NULL,
    params   TEXT NOT NULL,
    PRIMARY KEY (effect, name)
);
"#,
    r#"
CREATE TABLE plugin_settings (
    plugin TEXT NOT NULL,
    id     TEXT NOT NULL,
    key    TEXT NOT NULL,
    value  TEXT NOT NULL,
    PRIMARY KEY (plugin, id, key)
);
"#,
    r#"
CREATE TABLE playlists (
    id         TEXT PRIMARY KEY NOT NULL,
    name       TEXT NOT NULL,
    shuffle    INTEGER NOT NULL DEFAULT 0,
    entries    TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
"#,
    // Per-LED custom colours, mode and zones of each endpoint (store/leds.rs).
    r#"
CREATE TABLE custom_leds (
    uid    TEXT PRIMARY KEY NOT NULL,
    device TEXT NOT NULL,
    state  TEXT NOT NULL
);
"#,
    r#"
CREATE TABLE profiles (
    name       TEXT PRIMARY KEY NOT NULL,
    data       TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
"#,
];

mod leds;
pub use leds::CustomLedsRow;

/// `format` of an export document.
pub const EXPORT_FORMAT: &str = "lightloom-state";
/// Export document version this build writes and reads.
pub const EXPORT_VERSION: u32 = 1;

/// A device Lightloom has driven.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceRow {
    /// `vid:pid:serial`.
    pub key: String,
    /// USB vendor id.
    pub vid: u16,
    /// USB product id.
    pub pid: u16,
    /// Plugin-given name.
    pub name: String,
    /// Plugin file it was driven with.
    pub plugin: String,
    /// Driven at all (a disabled device is left alone).
    pub enabled: bool,
    /// RFC 3339.
    pub first_seen: String,
    /// RFC 3339.
    pub last_seen: String,
    /// Plugin parameter values the user set.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub parameters: Map<String, Value>,
}

/// Stored state of one effect.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EffectState {
    /// Control values the user set.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub params: Map<String, Value>,
    /// Named sets of control values.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub presets: BTreeMap<String, Map<String, Value>>,
}

/// Everything persisted, as one JSON document (`lightloom config export`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateExport {
    /// Always [`EXPORT_FORMAT`].
    pub format: String,
    /// [`EXPORT_VERSION`].
    pub version: u32,
    /// Key/value settings.
    pub settings: BTreeMap<String, Value>,
    /// Devices, by key.
    pub devices: Vec<DeviceRow>,
    /// Layouts, by name.
    pub layouts: Vec<Layout>,
    /// Component arrays by device key and channel name.
    pub channels: BTreeMap<String, BTreeMap<String, Value>>,
    /// Effect state by effect id.
    pub effects: BTreeMap<String, EffectState>,
    /// Effect playlists, by id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub playlists: Vec<Playlist>,
    /// Network plugin settings (`service.saveSetting`): plugin -> id -> key -> value.
    /// They may hold pairing keys (Hue user names, Nanoleaf tokens).
    #[serde(default, rename = "pluginSettings")]
    pub plugin_settings: BTreeMap<String, BTreeMap<String, BTreeMap<String, String>>>,
    /// Profiles, by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<Profile>,
    /// Per-LED custom colours, mode and zones, by endpoint uid.
    #[serde(default, rename = "customLeds", skip_serializing_if = "BTreeMap::is_empty")]
    pub custom_leds: BTreeMap<String, CustomLedsRow>,
}

/// The database.
pub struct Store {
    conn: Connection,
}

fn now() -> String {
    humantime::format_rfc3339_seconds(std::time::SystemTime::now()).to_string()
}

fn to_json<T: Serialize>(value: &T) -> anyhow::Result<String> {
    Ok(serde_json::to_string(value)?)
}

impl Store {
    /// Open (creating it if needed) and migrate the database at `path`. Refuses a
    /// database written by a newer Lightloom.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::setup(conn).with_context(|| format!("preparing {}", path.display()))
    }

    /// A database in memory (tests, `--ephemeral`).
    pub fn in_memory() -> anyhow::Result<Self> {
        Self::setup(Connection::open_in_memory()?)
    }

    fn setup(mut conn: Connection) -> anyhow::Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let migrations = Migrations::new(MIGRATIONS.iter().map(|sql| M::up(sql)).collect());
        migrations
            .to_latest(&mut conn)
            .context("the database schema is newer than this Lightloom, or a migration failed")?;
        Ok(Self { conn })
    }

    // ------------------------------------------------------------------ settings

    /// A setting.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> anyhow::Result<Option<T>> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0))
            .optional()?;
        match raw {
            Some(text) => Ok(Some(
                serde_json::from_str(&text).with_context(|| format!("setting {key}"))?,
            )),
            None => Ok(None),
        }
    }

    /// Store a setting.
    pub fn set<T: Serialize>(&self, key: &str, value: &T) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, to_json(value)?],
        )?;
        Ok(())
    }

    // ------------------------------------------------------------------- devices

    /// Note that the device was seen (driven) now; created on first sight.
    pub fn device_seen(&self, key: &str, vid: u16, pid: u16, name: &str, plugin: &str) -> anyhow::Result<()> {
        let t = now();
        self.conn.execute(
            "INSERT INTO devices (key, vid, pid, name, plugin, enabled, first_seen, last_seen)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6)
             ON CONFLICT(key) DO UPDATE SET name = excluded.name, plugin = excluded.plugin,
                 last_seen = excluded.last_seen",
            params![key, vid, pid, name, plugin, t],
        )?;
        Ok(())
    }

    /// Every device, by key, with its parameters.
    pub fn devices(&self) -> anyhow::Result<Vec<DeviceRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT key, vid, pid, name, plugin, enabled, first_seen, last_seen FROM devices ORDER BY key",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(DeviceRow {
                    key: r.get(0)?,
                    vid: r.get(1)?,
                    pid: r.get(2)?,
                    name: r.get(3)?,
                    plugin: r.get(4)?,
                    enabled: r.get::<_, i64>(5)? != 0,
                    first_seen: r.get(6)?,
                    last_seen: r.get(7)?,
                    parameters: Map::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|mut row| {
                row.parameters = self.device_parameters(&row.key)?;
                Ok(row)
            })
            .collect()
    }

    /// Whether the device is driven; unknown devices are.
    pub fn device_enabled(&self, key: &str) -> anyhow::Result<bool> {
        let enabled: Option<i64> = self
            .conn
            .query_row("SELECT enabled FROM devices WHERE key = ?1", [key], |r| r.get(0))
            .optional()?;
        Ok(enabled.is_none_or(|e| e != 0))
    }

    /// Enable or disable a known device.
    pub fn set_device_enabled(&self, key: &str, enabled: bool) -> anyhow::Result<()> {
        let n = self.conn.execute(
            "UPDATE devices SET enabled = ?2 WHERE key = ?1",
            params![key, enabled as i64],
        )?;
        if n == 0 {
            bail!("unknown device {key}");
        }
        Ok(())
    }

    /// The device's stored plugin parameter values.
    pub fn device_parameters(&self, key: &str) -> anyhow::Result<Map<String, Value>> {
        self.json_map(
            "SELECT name, value FROM device_parameters WHERE device = ?1 ORDER BY name",
            key,
        )
    }

    /// Merge plugin parameter values into the device's stored ones.
    pub fn set_device_parameters(&self, key: &str, values: &Map<String, Value>) -> anyhow::Result<()> {
        for (name, value) in values {
            self.conn.execute(
                "INSERT INTO device_parameters (device, name, value) VALUES (?1, ?2, ?3)
                 ON CONFLICT(device, name) DO UPDATE SET value = excluded.value",
                params![key, name, to_json(value)?],
            )?;
        }
        Ok(())
    }

    // ------------------------------------------------------------------- layouts

    /// The names of every layout.
    pub fn layout_names(&self) -> anyhow::Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT name FROM layouts ORDER BY name")?;
        let names = stmt
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(names)
    }

    /// The layout `name` with its placements.
    pub fn layout(&self, name: &str) -> anyhow::Result<Option<Layout>> {
        let row: Option<(u8, u32, String, String)> = self
            .conn
            .query_row(
                "SELECT brightness, grid_size, created_at, updated_at FROM layouts WHERE name = ?1",
                [name],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        let Some((brightness, grid_size, created, updated)) = row else {
            return Ok(None);
        };
        let mut stmt = self
            .conn
            .prepare("SELECT uid, placement FROM placements WHERE layout = ?1 ORDER BY uid")?;
        let mut endpoints = BTreeMap::new();
        for row in stmt.query_map([name], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (uid, text) = row?;
            let placement: Placement =
                serde_json::from_str(&text).with_context(|| format!("placement of {uid} in {name}"))?;
            endpoints.insert(uid, placement);
        }
        Ok(Some(Layout {
            name: name.to_string(),
            brightness,
            grid_size,
            endpoints,
            created_at: Some(created),
            updated_at: Some(updated),
            ..Layout::default()
        }))
    }

    /// Create or replace a layout (all of its placements).
    pub fn save_layout(&mut self, layout: &Layout) -> anyhow::Result<()> {
        let tx = self.conn.transaction()?;
        let t = now();
        let created = layout.created_at.clone().unwrap_or_else(|| t.clone());
        let updated = layout.updated_at.clone().unwrap_or(t);
        tx.execute(
            "INSERT INTO layouts (name, brightness, grid_size, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(name) DO UPDATE SET brightness = excluded.brightness, grid_size = excluded.grid_size,
                 updated_at = excluded.updated_at",
            params![layout.name, layout.brightness, layout.grid_size, created, updated],
        )?;
        tx.execute("DELETE FROM placements WHERE layout = ?1", [&layout.name])?;
        for (uid, placement) in &layout.endpoints {
            tx.execute(
                "INSERT INTO placements (layout, uid, placement) VALUES (?1, ?2, ?3)",
                params![layout.name, uid, to_json(placement)?],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Store one endpoint's placement in an existing layout.
    pub fn set_placement(&self, layout: &str, uid: &str, placement: &Placement) -> anyhow::Result<()> {
        let n = self.conn.execute(
            "UPDATE layouts SET updated_at = ?2 WHERE name = ?1",
            params![layout, now()],
        )?;
        if n == 0 {
            bail!("unknown layout {layout}");
        }
        self.conn.execute(
            "INSERT INTO placements (layout, uid, placement) VALUES (?1, ?2, ?3)
             ON CONFLICT(layout, uid) DO UPDATE SET placement = excluded.placement",
            params![layout, uid, to_json(placement)?],
        )?;
        Ok(())
    }

    /// Set a layout's brightness.
    pub fn set_layout_brightness(&self, layout: &str, brightness: u8) -> anyhow::Result<()> {
        let n = self.conn.execute(
            "UPDATE layouts SET brightness = ?2, updated_at = ?3 WHERE name = ?1",
            params![layout, brightness.min(100), now()],
        )?;
        if n == 0 {
            bail!("unknown layout {layout}");
        }
        Ok(())
    }

    /// Delete a layout; `false` when there was none.
    pub fn delete_layout(&self, name: &str) -> anyhow::Result<bool> {
        Ok(self.conn.execute("DELETE FROM layouts WHERE name = ?1", [name])? > 0)
    }

    // ------------------------------------------------------------------ channels

    /// The device's component arrays by channel name (SignalRGB's JSON shape).
    pub fn channels(&self, device: &str) -> anyhow::Result<BTreeMap<String, Value>> {
        let mut stmt = self.conn.prepare(
            "SELECT channel, components FROM channel_components WHERE device = ?1 ORDER BY channel",
        )?;
        let mut out = BTreeMap::new();
        for row in stmt.query_map([device], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (channel, text) = row?;
            out.insert(
                channel.clone(),
                serde_json::from_str(&text).with_context(|| format!("components of {device} {channel}"))?,
            );
        }
        Ok(out)
    }

    /// Store a channel's component array (must be a JSON array).
    pub fn set_channel(&self, device: &str, channel: &str, components: &Value) -> anyhow::Result<()> {
        if !components.is_array() {
            bail!("the components of {device} {channel} must be an array");
        }
        self.conn.execute(
            "INSERT INTO channel_components (device, channel, components) VALUES (?1, ?2, ?3)
             ON CONFLICT(device, channel) DO UPDATE SET components = excluded.components",
            params![device, channel, to_json(components)?],
        )?;
        Ok(())
    }

    // ------------------------------------------------------------------- effects

    /// The control values stored for an effect.
    pub fn effect_params(&self, effect: &str) -> anyhow::Result<Map<String, Value>> {
        self.json_map(
            "SELECT name, value FROM effect_params WHERE effect = ?1 ORDER BY name",
            effect,
        )
    }

    /// Merge control values into an effect's stored ones.
    pub fn set_effect_params(&self, effect: &str, values: &Map<String, Value>) -> anyhow::Result<()> {
        for (name, value) in values {
            self.conn.execute(
                "INSERT INTO effect_params (effect, name, value) VALUES (?1, ?2, ?3)
                 ON CONFLICT(effect, name) DO UPDATE SET value = excluded.value",
                params![effect, name, to_json(value)?],
            )?;
        }
        Ok(())
    }

    /// An effect's presets by name.
    pub fn presets(&self, effect: &str) -> anyhow::Result<BTreeMap<String, Map<String, Value>>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, params FROM presets WHERE effect = ?1 ORDER BY name")?;
        let mut out = BTreeMap::new();
        for row in stmt.query_map([effect], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (name, text) = row?;
            out.insert(
                name.clone(),
                serde_json::from_str(&text).with_context(|| format!("preset {name} of {effect}"))?,
            );
        }
        Ok(out)
    }

    /// Create or replace a preset.
    pub fn save_preset(&self, effect: &str, name: &str, values: &Map<String, Value>) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO presets (effect, name, params) VALUES (?1, ?2, ?3)
             ON CONFLICT(effect, name) DO UPDATE SET params = excluded.params",
            params![effect, name, to_json(values)?],
        )?;
        Ok(())
    }

    /// The SignalRGB import's effect mapping: each effect's control values are
    /// merged into the stored ones and its presets created or replaced, all in
    /// one transaction (nothing is written when one fails). Returns how many
    /// control values and presets were written.
    pub fn merge_effects(
        &mut self,
        effects: &BTreeMap<String, EffectState>,
    ) -> anyhow::Result<(usize, usize)> {
        let tx = self.conn.transaction()?;
        let (mut values, mut presets) = (0, 0);
        for (effect, state) in effects {
            for (name, value) in &state.params {
                tx.execute(
                    "INSERT INTO effect_params (effect, name, value) VALUES (?1, ?2, ?3)
                     ON CONFLICT(effect, name) DO UPDATE SET value = excluded.value",
                    params![effect, name, to_json(value)?],
                )?;
                values += 1;
            }
            for (name, preset) in &state.presets {
                tx.execute(
                    "INSERT INTO presets (effect, name, params) VALUES (?1, ?2, ?3)
                     ON CONFLICT(effect, name) DO UPDATE SET params = excluded.params",
                    params![effect, name, to_json(preset)?],
                )?;
                presets += 1;
            }
        }
        tx.commit()?;
        Ok((values, presets))
    }

    /// Delete a preset; `false` when there was none.
    pub fn delete_preset(&self, effect: &str, name: &str) -> anyhow::Result<bool> {
        Ok(self.conn.execute(
            "DELETE FROM presets WHERE effect = ?1 AND name = ?2",
            [effect, name],
        )? > 0)
    }

    // ----------------------------------------------------------------- playlists

    /// Every playlist, by id.
    pub fn playlists(&self) -> anyhow::Result<Vec<Playlist>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, shuffle, entries, updated_at FROM playlists ORDER BY id")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, name, shuffle, entries, updated_at)| {
                Ok(Playlist {
                    entries: serde_json::from_str(&entries)
                        .with_context(|| format!("entries of playlist {id}"))?,
                    id,
                    name,
                    shuffle: shuffle != 0,
                    updated_at,
                })
            })
            .collect()
    }

    /// The playlist `id`.
    pub fn playlist(&self, id: &str) -> anyhow::Result<Option<Playlist>> {
        Ok(self.playlists()?.into_iter().find(|p| p.id == id))
    }

    /// Create or replace a playlist (`updated_at` is set now); answers with it.
    pub fn save_playlist(&self, playlist: &Playlist) -> anyhow::Result<Playlist> {
        let mut p = playlist.clone();
        p.updated_at = now();
        Self::insert_playlist(&self.conn, &p)?;
        Ok(p)
    }

    fn insert_playlist(conn: &Connection, p: &Playlist) -> anyhow::Result<()> {
        if p.id.is_empty() {
            bail!("a playlist needs an id");
        }
        conn.execute(
            "INSERT INTO playlists (id, name, shuffle, entries, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, shuffle = excluded.shuffle,
                 entries = excluded.entries, updated_at = excluded.updated_at",
            params![p.id, p.name, p.shuffle as i64, to_json(&p.entries)?, p.updated_at],
        )?;
        Ok(())
    }

    /// Delete a playlist; `false` when there was none.
    pub fn delete_playlist(&self, id: &str) -> anyhow::Result<bool> {
        Ok(self.conn.execute("DELETE FROM playlists WHERE id = ?1", [id])? > 0)
    }

    // ------------------------------------------------------------------ profiles

    /// Every profile, by name.
    pub fn profiles(&self) -> anyhow::Result<Vec<Profile>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, data, created_at, updated_at FROM profiles ORDER BY name")?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(name, data, created_at, updated_at)| {
                let mut p: Profile =
                    serde_json::from_str(&data).with_context(|| format!("profile {name}"))?;
                p.name = name;
                p.created_at = created_at;
                p.updated_at = updated_at;
                Ok(p)
            })
            .collect()
    }

    /// The profile `name`.
    pub fn profile(&self, name: &str) -> anyhow::Result<Option<Profile>> {
        Ok(self.profiles()?.into_iter().find(|p| p.name == name))
    }

    /// Create or replace a profile: `updated_at` is now, `created_at` the existing
    /// profile's (or the given one, or now). Answers with it as stored.
    pub fn save_profile(&self, profile: &Profile) -> anyhow::Result<Profile> {
        if profile.name.trim().is_empty() {
            bail!("a profile needs a name");
        }
        let mut p = profile.clone();
        let t = now();
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT created_at FROM profiles WHERE name = ?1",
                [&p.name],
                |r| r.get(0),
            )
            .optional()?;
        p.created_at = existing
            .or_else(|| (!p.created_at.is_empty()).then(|| p.created_at.clone()))
            .unwrap_or_else(|| t.clone());
        p.updated_at = t;
        Self::insert_profile(&self.conn, &p)?;
        Ok(p)
    }

    fn insert_profile(conn: &Connection, p: &Profile) -> anyhow::Result<()> {
        conn.execute(
            "INSERT INTO profiles (name, data, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(name) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at",
            params![p.name, to_json(p)?, p.created_at, p.updated_at],
        )?;
        Ok(())
    }

    /// Rename a profile; `false` when there was none. The new name must be free.
    pub fn rename_profile(&mut self, from: &str, to: &str) -> anyhow::Result<bool> {
        let Some(mut p) = self.profile(from)? else {
            return Ok(false);
        };
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM profiles WHERE name = ?1", [from])?;
        p.name = to.to_string();
        p.updated_at = now();
        Self::insert_profile(&tx, &p)?;
        tx.commit()?;
        Ok(true)
    }

    /// Delete a profile; `false` when there was none.
    pub fn delete_profile(&self, name: &str) -> anyhow::Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM profiles WHERE name = ?1", [name])?
            > 0)
    }

    // ------------------------------------------------------------ export/import

    /// Everything, as one document ordered by key.
    pub fn export(&self) -> anyhow::Result<StateExport> {
        let mut settings = BTreeMap::new();
        {
            let mut stmt = self.conn.prepare("SELECT key, value FROM kv ORDER BY key")?;
            for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
                let (key, text) = row?;
                settings.insert(key, serde_json::from_str(&text)?);
            }
        }
        let mut layouts = Vec::new();
        for name in self.layout_names()? {
            if let Some(l) = self.layout(&name)? {
                layouts.push(l);
            }
        }
        let mut channels: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT DISTINCT device FROM channel_components ORDER BY device")?;
            let devices = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            for d in devices {
                channels.insert(d.clone(), self.channels(&d)?);
            }
        }
        let mut effects: BTreeMap<String, EffectState> = BTreeMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT effect FROM effect_params UNION SELECT effect FROM presets ORDER BY effect",
            )?;
            let ids = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            for id in ids {
                effects.insert(
                    id.clone(),
                    EffectState {
                        params: self.effect_params(&id)?,
                        presets: self.presets(&id)?,
                    },
                );
            }
        }
        Ok(StateExport {
            format: EXPORT_FORMAT.to_string(),
            version: EXPORT_VERSION,
            settings,
            devices: self.devices()?,
            layouts,
            channels,
            effects,
            playlists: self.playlists()?,
            plugin_settings: {
                let mut all: BTreeMap<String, BTreeMap<String, BTreeMap<String, String>>> = BTreeMap::new();
                for (plugin, id, key, value) in self.plugin_settings()? {
                    all.entry(plugin)
                        .or_default()
                        .entry(id)
                        .or_default()
                        .insert(key, value);
                }
                all
            },
            profiles: self.profiles()?,
            custom_leds: self.custom_leds()?,
        })
    }

    /// Every network plugin setting as `(plugin, id, key, value)`.
    pub fn plugin_settings(&self) -> anyhow::Result<Vec<(String, String, String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT plugin, id, key, value FROM plugin_settings ORDER BY plugin, id, key")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Store (`Some`) or delete (`None`) one network plugin setting.
    pub fn set_plugin_setting(
        &self,
        plugin: &str,
        id: &str,
        key: &str,
        value: Option<&str>,
    ) -> anyhow::Result<()> {
        match value {
            Some(v) => self.conn.execute(
                "INSERT INTO plugin_settings (plugin, id, key, value) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(plugin, id, key) DO UPDATE SET value = excluded.value",
                params![plugin, id, key, v],
            )?,
            None => self.conn.execute(
                "DELETE FROM plugin_settings WHERE plugin = ?1 AND id = ?2 AND key = ?3",
                params![plugin, id, key],
            )?,
        };
        Ok(())
    }

    /// Replace everything with `state` in one transaction.
    pub fn import(&mut self, state: &StateExport) -> anyhow::Result<()> {
        if state.format != EXPORT_FORMAT {
            bail!("not a Lightloom state export (format {:?})", state.format);
        }
        if state.version > EXPORT_VERSION {
            bail!(
                "state export version {} is newer than this Lightloom ({EXPORT_VERSION})",
                state.version
            );
        }
        let tx = self.conn.transaction()?;
        for table in [
            "placements",
            "layouts",
            "device_parameters",
            "devices",
            "channel_components",
            "effect_params",
            "presets",
            "playlists",
            "kv",
            "plugin_settings",
            "profiles",
            "custom_leds",
        ] {
            tx.execute(&format!("DELETE FROM {table}"), [])?;
        }
        for (plugin, ids) in &state.plugin_settings {
            for (id, keys) in ids {
                for (key, value) in keys {
                    tx.execute(
                        "INSERT INTO plugin_settings (plugin, id, key, value) VALUES (?1, ?2, ?3, ?4)",
                        params![plugin, id, key, value],
                    )?;
                }
            }
        }
        for (key, value) in &state.settings {
            tx.execute(
                "INSERT INTO kv (key, value) VALUES (?1, ?2)",
                params![key, to_json(value)?],
            )?;
        }
        for d in &state.devices {
            tx.execute(
                "INSERT INTO devices (key, vid, pid, name, plugin, enabled, first_seen, last_seen)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    d.key,
                    d.vid,
                    d.pid,
                    d.name,
                    d.plugin,
                    d.enabled as i64,
                    d.first_seen,
                    d.last_seen
                ],
            )?;
            for (name, value) in &d.parameters {
                tx.execute(
                    "INSERT INTO device_parameters (device, name, value) VALUES (?1, ?2, ?3)",
                    params![d.key, name, to_json(value)?],
                )?;
            }
        }
        for l in &state.layouts {
            let t = now();
            tx.execute(
                "INSERT INTO layouts (name, brightness, grid_size, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    l.name,
                    l.brightness,
                    l.grid_size,
                    l.created_at.clone().unwrap_or_else(|| t.clone()),
                    l.updated_at.clone().unwrap_or(t)
                ],
            )?;
            for (uid, p) in &l.endpoints {
                tx.execute(
                    "INSERT INTO placements (layout, uid, placement) VALUES (?1, ?2, ?3)",
                    params![l.name, uid, to_json(p)?],
                )?;
            }
        }
        for (device, channels) in &state.channels {
            for (channel, components) in channels {
                if !components.is_array() {
                    bail!("the components of {device} {channel} must be an array");
                }
                tx.execute(
                    "INSERT INTO channel_components (device, channel, components) VALUES (?1, ?2, ?3)",
                    params![device, channel, to_json(components)?],
                )?;
            }
        }
        for (effect, e) in &state.effects {
            for (name, value) in &e.params {
                tx.execute(
                    "INSERT INTO effect_params (effect, name, value) VALUES (?1, ?2, ?3)",
                    params![effect, name, to_json(value)?],
                )?;
            }
            for (name, values) in &e.presets {
                tx.execute(
                    "INSERT INTO presets (effect, name, params) VALUES (?1, ?2, ?3)",
                    params![effect, name, to_json(values)?],
                )?;
            }
        }
        for p in &state.playlists {
            Self::insert_playlist(&tx, p)?;
        }
        for p in &state.profiles {
            if p.name.trim().is_empty() {
                bail!("a profile needs a name");
            }
            Self::insert_profile(&tx, p)?;
        }
        for (uid, row) in &state.custom_leds {
            Self::insert_custom_leds(&tx, uid, &row.device, &row.state)?;
        }
        tx.commit()?;
        Ok(())
    }

    fn json_map(&self, sql: &str, arg: &str) -> anyhow::Result<Map<String, Value>> {
        let mut stmt = self.conn.prepare(sql)?;
        let mut out = Map::new();
        for row in stmt.query_map([arg], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (name, text) = row?;
            out.insert(name, serde_json::from_str(&text)?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_engine::Point;
    use serde_json::json;

    fn sample(store: &mut Store) {
        store.set("lighting.brightness", &80).unwrap();
        store.set("lighting.effect", &"lightloom:plasma").unwrap();
        store
            .device_seen("1462:7c91:A0", 0x1462, 0x7c91, "MSI", "msi.js")
            .unwrap();
        store
            .device_seen("1532:007c:00", 0x1532, 0x007c, "Razer", "razer.js")
            .unwrap();
        store
            .set_device_parameters(
                "1532:007c:00",
                json!({"dpi": 800, "mode": "Canvas"}).as_object().unwrap(),
            )
            .unwrap();
        store.set_device_enabled("1532:007c:00", false).unwrap();
        let mut leds = lightloom_engine::CustomLeds {
            enabled: true,
            colors: vec![lightloom_engine::Rgb::new(255, 0, 0); 3],
            ..Default::default()
        };
        leds.save_zone("Logo", &[2]).unwrap();
        store
            .set_custom_leds("1532:007c:00", "1532:007c:00", &leds)
            .unwrap();
        let mut layout = Layout {
            name: "Desk".into(),
            brightness: 70,
            ..Layout::default()
        };
        layout.endpoints.insert(
            "1462:7c91:A0".into(),
            Placement {
                position: Point { x: 10.0, y: 20.0 },
                rotation: 90.0,
                ..Placement::default()
            },
        );
        store.save_layout(&layout).unwrap();
        store
            .set_placement("Desk", "comp-1", &Placement::default())
            .unwrap();
        store
            .set_channel(
                "1462:7c91:A0",
                "JRainbow 1",
                &json!([{"ProductName": "Strip", "ComponentId": "c1"}]),
            )
            .unwrap();
        store
            .set_effect_params("lightloom:plasma", json!({"speed": 60}).as_object().unwrap())
            .unwrap();
        store
            .save_preset(
                "lightloom:plasma",
                "Calm",
                json!({"speed": 5}).as_object().unwrap(),
            )
            .unwrap();
        store
            .save_preset(
                "signalrgb:Rainbow",
                "Fast",
                json!({"speed": 9}).as_object().unwrap(),
            )
            .unwrap();
        store
            .save_playlist(
                &serde_json::from_value(json!({
                    "id": "evening",
                    "name": "Evening",
                    "shuffle": true,
                    "entries": [
                        {"effect": "lightloom:plasma", "preset": "Calm", "duration": 60},
                        {"effect": "signalrgb:Rainbow", "values": {"speed": 2}}
                    ]
                }))
                .unwrap(),
            )
            .unwrap();
        store.save_profile(&profile("Night")).unwrap();
    }

    fn profile(name: &str) -> Profile {
        serde_json::from_value(json!({
            "format": "lightloom-profile",
            "version": 1,
            "name": name,
            "lighting": {"effect": "lightloom:plasma", "values": {"speed": 3}, "brightness": 40},
            "layout": {"name": "Desk", "brightness": 70, "endpoints": {}},
            "devices": {"1462:7c91:A0": {"enabled": true, "laterSetting": {"0": "#ff0000"}}}
        }))
        .unwrap()
    }

    #[test]
    fn profiles_are_saved_renamed_and_deleted() {
        let mut s = Store::in_memory().unwrap();
        let first = s.save_profile(&profile("Night")).unwrap();
        assert!(!first.created_at.is_empty());
        let again = s.save_profile(&profile("Night")).unwrap();
        assert_eq!(
            again.created_at, first.created_at,
            "a replace keeps its creation time"
        );
        s.save_profile(&profile("Day")).unwrap();
        let names: Vec<String> = s.profiles().unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["Day", "Night"]);
        let night = s.profile("Night").unwrap().unwrap();
        assert_eq!(night.lighting.brightness, 40);
        assert_eq!(
            night.devices["1462:7c91:A0"].extra["laterSetting"]["0"], "#ff0000",
            "unknown device keys are kept"
        );
        assert!(s.rename_profile("Night", "Evening").unwrap());
        assert!(!s.rename_profile("Night", "X").unwrap());
        assert_eq!(s.profile("Evening").unwrap().unwrap().name, "Evening");
        assert!(s.delete_profile("Evening").unwrap());
        assert!(!s.delete_profile("Evening").unwrap());
        assert!(s.save_profile(&profile(" ")).is_err());
    }

    #[test]
    fn plugin_settings_round_trip_and_travel_with_exports() {
        let s = Store::in_memory().unwrap();
        s.set_plugin_setting("Philips Hue", "001788fffe", "username", Some("abc"))
            .unwrap();
        s.set_plugin_setting("Philips Hue", "001788fffe", "username", Some("def"))
            .unwrap();
        s.set_plugin_setting("WLED", "forced", "list", Some("[]"))
            .unwrap();
        s.set_plugin_setting("WLED", "forced", "list", None).unwrap();
        assert_eq!(
            s.plugin_settings().unwrap(),
            vec![(
                "Philips Hue".into(),
                "001788fffe".into(),
                "username".into(),
                "def".into()
            )]
        );
        let export = s.export().unwrap();
        assert_eq!(
            export.plugin_settings["Philips Hue"]["001788fffe"]["username"],
            "def"
        );
        let mut t = Store::in_memory().unwrap();
        t.import(&export).unwrap();
        assert_eq!(t.plugin_settings().unwrap(), s.plugin_settings().unwrap());
    }

    #[test]
    fn values_round_trip_through_their_tables() {
        let mut s = Store::in_memory().unwrap();
        sample(&mut s);
        assert_eq!(s.get::<u8>("lighting.brightness").unwrap(), Some(80));
        assert_eq!(s.get::<u8>("missing").unwrap(), None);
        assert!(!s.device_enabled("1532:007c:00").unwrap());
        assert!(s.device_enabled("never:seen").unwrap());
        assert!(s.set_device_enabled("never:seen", false).is_err());
        let devices = s.devices().unwrap();
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[1].parameters["dpi"], 800);
        let desk = s.layout("Desk").unwrap().unwrap();
        assert_eq!((desk.brightness, desk.endpoints.len()), (70, 2));
        assert_eq!(desk.endpoints["1462:7c91:A0"].rotation, 90.0);
        assert!(s.set_placement("Nope", "x", &Placement::default()).is_err());
        assert_eq!(
            s.channels("1462:7c91:A0").unwrap()["JRainbow 1"][0]["ComponentId"],
            "c1"
        );
        assert!(s.set_channel("d", "c", &json!({"not": "an array"})).is_err());
        assert_eq!(s.effect_params("lightloom:plasma").unwrap()["speed"], 60);
        assert_eq!(s.presets("lightloom:plasma").unwrap()["Calm"]["speed"], 5);
        assert!(s.delete_preset("lightloom:plasma", "Calm").unwrap());
        assert!(!s.delete_preset("lightloom:plasma", "Calm").unwrap());
        let evening = s.playlist("evening").unwrap().unwrap();
        assert_eq!(
            (evening.name.as_str(), evening.shuffle, evening.entries.len()),
            ("Evening", true, 2)
        );
        assert_eq!(evening.entries[0].preset.as_deref(), Some("Calm"));
        assert_eq!(
            (evening.entries[0].duration, evening.entries[1].duration),
            (Some(60), None)
        );
        assert_eq!(evening.entries[1].values["speed"], 2);
        assert!(!evening.updated_at.is_empty());
        assert!(s.save_playlist(&Playlist::default()).is_err());
        assert!(s.delete_playlist("evening").unwrap());
        assert!(!s.delete_playlist("evening").unwrap());
        assert!(s.playlists().unwrap().is_empty());
        assert!(s.delete_layout("Desk").unwrap());
        // Placements go with their layout.
        let left: i64 = s
            .conn
            .query_row("SELECT COUNT(*) FROM placements", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0);
    }

    #[test]
    fn merged_effects_keep_other_values_and_replace_presets() {
        let mut s = Store::in_memory().unwrap();
        let mut keep = Map::new();
        keep.insert("speed".into(), json!(5));
        keep.insert("mine".into(), json!("kept"));
        s.set_effect_params("signalrgb:Rainbow", &keep).unwrap();
        s.save_preset("signalrgb:Rainbow", "A", &keep).unwrap();

        let mut params = Map::new();
        params.insert("speed".into(), json!(2));
        params.insert("color".into(), json!("#ff0000"));
        let mut preset = Map::new();
        preset.insert("speed".into(), json!(9));
        let state = EffectState {
            params,
            presets: BTreeMap::from([("A".to_string(), preset.clone())]),
        };
        let written = s
            .merge_effects(&BTreeMap::from([("signalrgb:Rainbow".to_string(), state)]))
            .unwrap();
        assert_eq!(written, (2, 1));
        let values = s.effect_params("signalrgb:Rainbow").unwrap();
        assert_eq!(values["speed"], 2);
        assert_eq!(values["color"], "#ff0000");
        assert_eq!(values["mine"], "kept");
        assert_eq!(s.presets("signalrgb:Rainbow").unwrap()["A"], preset);
    }

    #[test]
    fn export_import_round_trips_byte_for_byte() {
        let mut a = Store::in_memory().unwrap();
        sample(&mut a);
        let first = serde_json::to_string_pretty(&a.export().unwrap()).unwrap();
        let mut b = Store::in_memory().unwrap();
        b.set("stale", &1).unwrap();
        b.import(&serde_json::from_str(&first).unwrap()).unwrap();
        let second = serde_json::to_string_pretty(&b.export().unwrap()).unwrap();
        assert_eq!(first, second);
        assert!(!second.contains("stale"));
        let mut bad: StateExport = serde_json::from_str(&first).unwrap();
        bad.version = EXPORT_VERSION + 1;
        assert!(b.import(&bad).is_err());
    }

    #[test]
    fn a_file_database_reopens_with_its_data() {
        let dir = std::env::temp_dir().join(format!("lightloom-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("state.sqlite");
        {
            let s = Store::open(&path).unwrap();
            s.set("lighting.enabled", &false).unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert_eq!(s.get::<bool>("lighting.enabled").unwrap(), Some(false));
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
