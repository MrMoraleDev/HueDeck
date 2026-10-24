// SPDX-License-Identifier: GPL-3.0-or-later
//! What the API exchanges with the daemon and with clients: the read model
//! ([`Snapshot`]), the state-changing requests ([`Request`]), push events
//! ([`Event`]) and errors ([`ApiError`]). JSON field names are camelCase.

use lightloom_engine::Placement;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Everything a client shows, as of one moment.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// Lightloom version.
    pub version: String,
    /// Lifecycle state of the daemon (`Running`, `ScanningHardware`, ...).
    pub state: String,
    /// What is on the canvas.
    pub lighting: Lighting,
    /// Devices: running sessions first by key, then known devices that are disabled or absent.
    pub devices: Vec<DeviceView>,
    /// The effect library, by id.
    pub effects: Vec<EffectView>,
    /// Saved layouts, by name.
    pub layouts: Vec<String>,
    /// Effect playlists, by id.
    #[serde(default)]
    pub playlists: Vec<Playlist>,
    /// User settings.
    pub settings: Settings,
    /// Running software that holds the same devices (reported, never terminated).
    pub conflicts: Vec<String>,
    /// Problems worth showing (renderer missing, plugin directories empty, ...).
    pub warnings: Vec<String>,
}

/// The canvas: effect, brightness, layout, renderer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lighting {
    /// Current effect id.
    pub effect: Option<String>,
    /// Its title.
    pub effect_name: Option<String>,
    /// Global brightness `0..=100`.
    pub brightness: u8,
    /// `false`: the canvas is paused and every device shows black.
    pub enabled: bool,
    /// Current layout name.
    pub layout: String,
    /// Its brightness `0..=100`.
    pub layout_brightness: u8,
    /// Who paints the canvas.
    pub renderer: RendererView,
    /// The playlist playing now.
    #[serde(default)]
    pub playlist: Option<PlaylistPlayback>,
    /// The auto-next timer (SignalRGB's "Effect Timer Interval").
    #[serde(default)]
    pub timer: EffectTimer,
    /// When the playlist or the timer shows the next effect (Unix time,
    /// milliseconds); `None` when nothing will switch by itself.
    #[serde(default)]
    pub next_switch_ms: Option<u64>,
}

/// The auto-next timer: while no playlist plays, show the next effect of the
/// library every `interval` seconds. It also is the duration of playlist
/// entries that have none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EffectTimer {
    /// Advance through the library by itself (off by default).
    pub enabled: bool,
    /// Seconds per effect ([`EffectTimer::DEFAULT_INTERVAL`] by default).
    pub interval: u32,
}

impl EffectTimer {
    /// SignalRGB's default: 300 000 ms.
    pub const DEFAULT_INTERVAL: u32 = 300;
    /// Shortest interval or entry duration, seconds.
    pub const MIN_INTERVAL: u32 = 1;
    /// Longest interval or entry duration, seconds (one day).
    pub const MAX_INTERVAL: u32 = 86_400;
}

impl Default for EffectTimer {
    fn default() -> Self {
        Self {
            enabled: false,
            interval: Self::DEFAULT_INTERVAL,
        }
    }
}

/// An effect playlist: effects shown one after the other, in order or shuffled.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    /// Stable id (made from the name when the playlist is created).
    #[serde(default)]
    pub id: String,
    /// Display name.
    pub name: String,
    /// Play the entries in a random order (a new one every round).
    #[serde(default)]
    pub shuffle: bool,
    /// The effects, in order.
    #[serde(default)]
    pub entries: Vec<PlaylistEntry>,
    /// Last change (RFC 3339); set by the daemon.
    #[serde(default)]
    pub updated_at: String,
}

/// One entry of a playlist.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistEntry {
    /// Effect id.
    pub effect: String,
    /// A preset of the effect loaded when the entry starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// Control values set when the entry starts (after the preset's).
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub values: Map<String, Value>,
    /// Seconds the entry lasts; `None`: the timer's interval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<u32>,
}

/// The playlist playing now.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistPlayback {
    /// Playlist id.
    pub id: String,
    /// Its name.
    pub name: String,
    /// The entry shown now (index into `entries`).
    pub entry: usize,
}

/// The canvas renderer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RendererView {
    /// `cef` (effects) or `null` (solid colour / test patterns without the renderer).
    pub kind: String,
    /// A renderer process is up (always true for `null`).
    pub running: bool,
    /// Frames per second.
    pub fps: u32,
    /// Starts, frames, restarts (CEF client status line).
    pub status: Option<String>,
}

/// One device.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceView {
    /// `vid:pid:serial`.
    pub key: String,
    /// USB vendor id.
    pub vid: u16,
    /// USB product id.
    pub pid: u16,
    /// Name (plugin-given).
    pub name: String,
    /// Plugin name.
    pub plugin: String,
    /// The plugin's `DeviceType()` (`keyboard`, `gpu`, `other`, ...).
    #[serde(default)]
    pub device_type: String,
    /// Plugin file.
    pub plugin_path: String,
    /// `running`, `starting`, `stopping`, `done`, `disabled` or `absent`.
    pub state: String,
    /// Driven at all.
    pub enabled: bool,
    /// Plugged in now.
    pub connected: bool,
    /// `Render()` calls.
    pub frames: u64,
    /// Failed `Render()` calls.
    pub render_errors: u64,
    /// Failed I/O operations: HID operations, or for SMBus devices failed or
    /// refused bus operations.
    pub io_errors: u64,
    /// SMBus devices: `<bus name> · 0x<address>`; `None` for HID devices.
    pub bus: Option<String>,
    /// `DefaultScale()`.
    pub default_scale: f32,
    /// `ImageUrl()` or a `data:` URL of `Image()`.
    pub image: Option<String>,
    /// `ControllableParameters()` entries, each with its current `value`.
    pub parameters: Vec<Value>,
    /// The device and its children as canvas endpoints.
    pub endpoints: Vec<Endpoint>,
    /// Channel names (component editor).
    pub channels: Vec<ChannelView>,
    /// The last battery report (devices whose plugin has the `battery` feature),
    /// kept after the device goes away.
    pub battery: Option<BatteryView>,
    /// Pulse the device red while its battery is low (user setting).
    pub blink_low_battery: bool,
}

/// A device's battery, as its plugin last reported it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatteryView {
    /// Percent; `None` until the plugin reported a level.
    pub level: Option<u8>,
    /// `unknown`, `discharging`, `charging`, `fullCharging`, `full` or `wirelessCharging`.
    pub state: String,
    /// On external power (charging, or full on the charger).
    pub charging: bool,
    /// `ok`, `low` or `critical` under the user's thresholds.
    pub status: String,
    /// When the plugin last reported (Unix time, milliseconds).
    pub updated_ms: u64,
}

/// A canvas endpoint (device, subdevice or component instance).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    /// Placement key.
    pub uid: String,
    /// `device`, `subdevice` or `component`.
    pub kind: String,
    /// Display name.
    pub name: String,
    /// The channel of a component.
    pub channel: Option<String>,
    /// LED grid in cells.
    pub size: [u32; 2],
    /// Number of LEDs.
    pub leds: usize,
    /// Where it sits.
    pub placement: Placement,
}

/// A channel and the component instances assigned to it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelView {
    /// Channel name.
    pub name: String,
    /// LED limit (0 = none).
    pub led_limit: u32,
    /// Component instances in chain order (SignalRGB's JSON shape).
    pub components: Vec<Value>,
}

/// One effect of the library.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectView {
    /// Stable id (`lightloom:plasma`, `signalrgb:Rainbow`, ...).
    pub id: String,
    /// `<title>`.
    pub title: String,
    /// `<meta description>`.
    pub description: String,
    /// `<meta publisher>`.
    pub publisher: String,
    /// `lightloom`, `user` or `signalrgb`.
    pub source: String,
    /// `<meta property>` controls (effect-meta schema shape).
    pub controls: Vec<Value>,
    /// Stored control values (defaults apply to the others).
    pub values: Map<String, Value>,
    /// Preset names.
    pub presets: Vec<String>,
    /// What the effect reads (`audio`, `video`, `meters`, `input`, `webgl`,
    /// `engine`); effects with `meters` are game integrations.
    #[serde(default)]
    pub uses: Vec<String>,
}

/// User settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Use the local SignalRGB installation's plugins, effects, components and
    /// channel setup, read in place (opt-in).
    pub signalrgb: bool,
    /// A SignalRGB installation exists on this machine.
    pub signalrgb_available: bool,
    /// Composite effects on the GPU instead of in software.
    pub gpu_compositing: bool,
    /// Drive SMBus devices (GPU RGB through NvAPI; RAM RGB through the I/O
    /// broker when it is installed and healthy); on unless turned off.
    pub smbus: bool,
    /// Port of the native API and the dashboard now (loopback only).
    pub port: u16,
    /// Why the daemon does not serve on the configured port (it was busy at
    /// startup and the default port was used instead); `None` normally.
    #[serde(default)]
    pub port_notice: Option<String>,
    /// Serve the OpenRGB SDK protocol on 127.0.0.1:6742 so OpenRGB SDK
    /// clients see and drive Lightloom's devices (opt-in).
    #[serde(default)]
    pub openrgb_sdk: bool,
    /// Effect frame rate.
    pub fps: u32,
    /// Never write to devices.
    pub dry_run: bool,
    /// Low-battery threshold, percent (warnings, `battery.lowBatteryWarningLevel`).
    pub battery_low: u8,
    /// Critical battery threshold, percent.
    pub battery_critical: u8,
    /// Show low and critical battery alerts as system notifications.
    pub battery_notifications: bool,
    /// This OS has system notifications Lightloom can show.
    pub notifications_available: bool,
}

/// A state change asked of the daemon.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Request {
    /// Show an effect (optionally with control values, which are stored).
    #[serde(rename_all = "camelCase")]
    ApplyEffect {
        /// Effect id.
        id: String,
        /// Control values.
        #[serde(default)]
        values: Map<String, Value>,
    },
    /// Store control values of an effect; the current effect gets them live.
    #[serde(rename_all = "camelCase")]
    SetEffectValues {
        /// Effect id.
        id: String,
        /// Control values.
        values: Map<String, Value>,
    },
    /// Save the effect's stored values as a preset.
    SavePreset {
        /// Effect id.
        effect: String,
        /// Preset name.
        name: String,
    },
    /// Load a preset into the effect's values (and show the effect).
    ApplyPreset {
        /// Effect id.
        effect: String,
        /// Preset name.
        name: String,
    },
    /// Delete a preset.
    DeletePreset {
        /// Effect id.
        effect: String,
        /// Preset name.
        name: String,
    },
    /// Show a random effect of the library, game integrations (effects with
    /// screen meters) and developer effects excluded; stops a playlist.
    ShuffleEffect,
    /// Show the next effect of the library, or of the playing playlist
    /// (`previous` goes back).
    StepEffect {
        /// `true`: the previous one.
        #[serde(default)]
        previous: bool,
    },
    /// Global brightness and the canvas pause.
    SetLighting {
        /// `0..=100`.
        brightness: Option<u8>,
        /// `false` pauses the canvas (every device black).
        enabled: Option<bool>,
    },
    /// Enable/disable a device or set its plugin parameters.
    #[serde(rename_all = "camelCase")]
    SetDevice {
        /// Device key.
        key: String,
        /// Drive it or not.
        enabled: Option<bool>,
        /// `ControllableParameters` values (applied live, stored).
        parameters: Option<Map<String, Value>>,
        /// Pulse the device red while its battery is low.
        #[serde(default)]
        blink_low_battery: Option<bool>,
    },
    /// Assign component instances to a channel (the device session restarts).
    SetChannel {
        /// Device key.
        device: String,
        /// Channel name.
        channel: String,
        /// Component instances in chain order (SignalRGB's JSON shape;
        /// `{"ProductName": ...}` from the library gets a new `ComponentId`).
        components: Vec<Value>,
    },
    /// Move an endpoint in the current layout.
    SetPlacement {
        /// Endpoint uid.
        uid: String,
        /// Its placement.
        placement: Placement,
    },
    /// Switch to a saved layout.
    ApplyLayout {
        /// Layout name.
        name: String,
    },
    /// Save the current layout under a new name (and switch to it).
    SaveLayoutAs {
        /// Layout name.
        name: String,
    },
    /// Delete a layout (not the current one).
    DeleteLayout {
        /// Layout name.
        name: String,
    },
    /// The current layout's brightness.
    SetLayoutBrightness {
        /// `0..=100`.
        brightness: u8,
    },
    /// `onCanvasApiEvent({sender, event})` in the current effect.
    CanvasEvent {
        /// Who sent it.
        sender: String,
        /// What happened.
        event: String,
    },
    /// `onCanvasTapped(x, y)` in the current effect.
    CanvasTap {
        /// X in canvas pixels.
        x: f64,
        /// Y in canvas pixels.
        y: f64,
    },
    /// User settings.
    #[serde(rename_all = "camelCase")]
    SetSettings {
        /// Use the local SignalRGB installation (plugins, effects, components, channels).
        signalrgb: Option<bool>,
        /// GPU compositing for effects.
        gpu_compositing: Option<bool>,
        /// Drive SMBus devices (scans the buses when turned on).
        #[serde(default)]
        smbus: Option<bool>,
        /// Low-battery threshold, percent (`1..=95`, above the critical one).
        #[serde(default)]
        battery_low: Option<u8>,
        /// Critical battery threshold, percent (below the low one).
        #[serde(default)]
        battery_critical: Option<u8>,
        /// Battery alerts as system notifications.
        #[serde(default)]
        battery_notifications: Option<bool>,
        /// Port of the native API and the dashboard (saved; the API side has
        /// already bound it and moves the listener).
        #[serde(default)]
        port: Option<u16>,
        /// Serve the OpenRGB SDK protocol (127.0.0.1:6742).
        #[serde(default)]
        openrgb_sdk: Option<bool>,
    },
    /// Import the local SignalRGB setup (read in place): its canvas placements
    /// as a layout, and the component arrays of its channels.
    ImportSignalRgb {
        /// Layout name (default `SignalRGB`).
        name: Option<String>,
    },
    /// A saved layout with its placements (read-only).
    GetLayout {
        /// Layout name.
        name: String,
    },
    /// The component library for the channel editor (read-only).
    ListComponents,
    /// Everything persisted, as one JSON document.
    Export,
    /// Replace everything persisted with an export document.
    Import {
        /// The document.
        state: Value,
    },
    /// Create (`id` absent) or replace a playlist; answers with it.
    SavePlaylist {
        /// Playlist id; `None` makes one from the name.
        #[serde(default)]
        id: Option<String>,
        /// The playlist (its `id` and `updatedAt` are ignored).
        playlist: Playlist,
    },
    /// Delete a playlist (it stops when it plays).
    DeletePlaylist {
        /// Playlist id.
        id: String,
    },
    /// Play a playlist from its first entry, or from the entry showing `start`.
    PlayPlaylist {
        /// Playlist id.
        id: String,
        /// Effect id of the entry to start with.
        #[serde(default)]
        start: Option<String>,
    },
    /// Stop the playlist (the effect stays).
    StopPlaylist,
    /// The auto-next timer.
    SetEffectTimer {
        /// Advance through the library by itself.
        #[serde(default)]
        enabled: Option<bool>,
        /// Seconds per effect.
        #[serde(default)]
        interval: Option<u32>,
    },
    /// Add a developer's effect page (`developer:<uid>`) and show it.
    LoadEffect {
        /// Name of the effect (letters, digits, space, `-`, `_`, `.`).
        uid: String,
        /// The page.
        html: String,
    },
    /// Where an effect's picture is (read-only): `{path, contentType}`.
    EffectImage {
        /// Effect id.
        id: String,
    },
    /// Enumerate devices again now.
    Rescan,
    /// Stop the daemon (every plugin restores its device first).
    Shutdown,
}

impl Request {
    /// Short name for logs.
    pub fn name(&self) -> &'static str {
        match self {
            Request::ApplyEffect { .. } => "applyEffect",
            Request::SetEffectValues { .. } => "setEffectValues",
            Request::SavePreset { .. } => "savePreset",
            Request::ApplyPreset { .. } => "applyPreset",
            Request::DeletePreset { .. } => "deletePreset",
            Request::ShuffleEffect => "shuffleEffect",
            Request::StepEffect { .. } => "stepEffect",
            Request::SetLighting { .. } => "setLighting",
            Request::SetDevice { .. } => "setDevice",
            Request::SetChannel { .. } => "setChannel",
            Request::SetPlacement { .. } => "setPlacement",
            Request::ApplyLayout { .. } => "applyLayout",
            Request::SaveLayoutAs { .. } => "saveLayoutAs",
            Request::DeleteLayout { .. } => "deleteLayout",
            Request::SetLayoutBrightness { .. } => "setLayoutBrightness",
            Request::CanvasEvent { .. } => "canvasEvent",
            Request::CanvasTap { .. } => "canvasTap",
            Request::SetSettings { .. } => "setSettings",
            Request::ImportSignalRgb { .. } => "importSignalRgb",
            Request::GetLayout { .. } => "getLayout",
            Request::ListComponents => "listComponents",
            Request::Export => "export",
            Request::Import { .. } => "import",
            Request::SavePlaylist { .. } => "savePlaylist",
            Request::DeletePlaylist { .. } => "deletePlaylist",
            Request::PlayPlaylist { .. } => "playPlaylist",
            Request::StopPlaylist => "stopPlaylist",
            Request::SetEffectTimer { .. } => "setEffectTimer",
            Request::LoadEffect { .. } => "loadEffect",
            Request::EffectImage { .. } => "effectImage",
            Request::Rescan => "rescan",
            Request::Shutdown => "shutdown",
        }
    }
}

/// Something changed; clients refetch what they show (`GET /v1/state`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    /// `state` (the snapshot changed), `stats` (counters only), `log`, or
    /// `battery` (a low or critical battery alert: `{key, name, level, severity}`).
    pub kind: String,
    /// Event payload (`log`: the line).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub data: Value,
}

impl Event {
    /// The snapshot changed.
    pub fn state() -> Self {
        Self {
            kind: "state".into(),
            data: Value::Null,
        }
    }

    /// A log line.
    pub fn log(line: impl Into<String>) -> Self {
        Self {
            kind: "log".into(),
            data: Value::String(line.into()),
        }
    }
}

/// A failed request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    /// HTTP status.
    pub status: u16,
    /// Machine-readable code (`not_found`, `bad_request`, `unavailable`, ...).
    pub code: String,
    /// What went wrong.
    pub message: String,
}

impl ApiError {
    /// 400.
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: 400,
            code: "bad_request".into(),
            message: message.into(),
        }
    }

    /// 404.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: 404,
            code: "not_found".into(),
            message: message.into(),
        }
    }

    /// 409.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: 409,
            code: "conflict".into(),
            message: message.into(),
        }
    }

    /// 500.
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: 500,
            code: "internal".into(),
            message: message.into(),
        }
    }

    /// 503: the daemon is shutting down or not ready.
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            status: 503,
            code: "unavailable".into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl std::error::Error for ApiError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_tagged_by_op() {
        let r: Request = serde_json::from_str(r#"{"op":"applyEffect","id":"lightloom:plasma"}"#).unwrap();
        assert_eq!(
            r,
            Request::ApplyEffect {
                id: "lightloom:plasma".into(),
                values: Map::new()
            }
        );
        let r: Request =
            serde_json::from_str(r#"{"op":"setSettings","gpuCompositing":true,"signalrgb":null}"#).unwrap();
        assert_eq!(r.name(), "setSettings");
        assert_eq!(
            serde_json::to_value(Request::StepEffect { previous: true }).unwrap(),
            serde_json::json!({"op": "stepEffect", "previous": true})
        );
    }
}
