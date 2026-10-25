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
    /// Network plugins' discovery services (while network lights are on).
    #[serde(default)]
    pub network: Vec<NetworkServiceView>,
    /// Running software that holds the same devices (reported, never terminated).
    pub conflicts: Vec<String>,
    /// Problems worth showing (renderer missing, plugin directories empty, ...).
    pub warnings: Vec<String>,
    /// PawnIO / RAM lighting (Settings).
    pub pawnio: PawnIoView,
}

/// PawnIO and the Lightloom I/O broker: what RAM lighting needs (Windows).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PawnIoView {
    /// The Install / Remove buttons can work: Windows, `lightloom-setup.exe`
    /// next to `lightloomd.exe`, real hardware mode.
    pub supported: bool,
    /// PawnIO's uninstall entry exists.
    pub installed: bool,
    /// Its `DisplayVersion`.
    pub version: Option<String>,
    /// 2.2.0 or later.
    pub version_ok: bool,
    /// `official` / `unrestricted` / `unknown`, as the broker reported it
    /// (`None` while Lightloom has not talked to the broker).
    pub edition: Option<String>,
    /// The `LightloomIoBroker` service: `notInstalled`, `stopped`, `pending`, `running`, `unknown`.
    pub service: String,
    /// The broker answered and Lightloom uses it.
    pub broker_healthy: bool,
    /// Chipset SMBus buses (RAM) in use.
    pub ram_buses: usize,
    /// One line for the user.
    pub status: String,
    /// The last Install / Remove run (running or ended).
    pub job: Option<PawnIoJob>,
}

/// An Install / Remove run of the elevated helper.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PawnIoJob {
    /// `install` / `remove`.
    pub action: String,
    /// Still running (or the UAC prompt still open).
    pub running: bool,
    /// How it ended; `None` while running.
    pub ok: Option<bool>,
    /// What to tell the user.
    pub message: String,
}

/// What the PawnIO / RAM lighting buttons ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PawnIoAction {
    /// Install PawnIO when needed (verified download) and (re)register the I/O broker.
    Install,
    /// Remove the I/O broker; PawnIO stays.
    Remove,
}

/// `GET /v1/update`: the update check (Settings → Updates). Nothing is
/// fetched unless the user presses *Check for updates* or turned the weekly
/// check on (docs/privacy.md).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateView {
    /// The running version.
    pub current_version: String,
    /// Check once a week by itself (off by default).
    pub check_weekly: bool,
    /// Offer pre-releases too (off by default).
    pub beta: bool,
    /// A check runs now.
    pub checking: bool,
    /// When the last check ended (Unix seconds); `None`: never.
    pub last_check: Option<u64>,
    /// Why the last check failed; `None` when it worked.
    pub error: Option<String>,
    /// A newer release found by the last check; `None`: up to date (or never checked).
    pub available: Option<UpdateRelease>,
    /// *Download* downloads, verifies and opens the installer here (the
    /// per-user MSI install on Windows); otherwise it opens the release page.
    pub installable: bool,
    /// Why *Download* opens the release page instead (portable copy, Linux, ...).
    pub install_note: Option<String>,
    /// The last download (running or ended).
    pub job: Option<UpdateJob>,
}

/// A release on GitHub.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRelease {
    /// Its version (`1.2.3`, `1.3.0-beta.1`).
    pub version: String,
    /// Its tag (`v1.2.3`).
    pub tag: String,
    /// Its title.
    pub name: String,
    /// Release notes as plain text (HTML tags and control characters removed, shortened).
    pub notes: String,
    /// The release page (always under `https://github.com/MrMoraleDev/Lightloom/`).
    pub url: String,
    /// Publication date (ISO 8601), as GitHub gives it.
    pub published_at: Option<String>,
    /// A pre-release (only offered with the beta channel on).
    pub prerelease: bool,
    /// The download for this system; `None`: open the release page.
    pub asset: Option<UpdateAsset>,
}

/// One file of a release.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAsset {
    /// File name (`Lightloom-1.2.3-x64.msi`).
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// Its download address.
    pub url: String,
}

/// The download of an update.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateJob {
    /// The release's version.
    pub version: String,
    /// Still downloading.
    pub running: bool,
    /// How it ended; `None` while running.
    pub ok: Option<bool>,
    /// Bytes downloaded so far.
    pub downloaded: u64,
    /// Bytes in all (`0`: unknown).
    pub total: u64,
    /// What to tell the user.
    pub message: String,
}

/// `GET /v1/sensors`: the hardware sensors (read-only), as of the last poll.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorsView {
    /// Sensors are read at all (`[sensors] enabled`).
    pub enabled: bool,
    /// Polling runs now (an effect or a client reads sensors).
    pub polling: bool,
    /// Poll interval in milliseconds.
    pub interval_ms: u64,
    /// Milliseconds since the last poll (`None`: never polled).
    pub age_ms: Option<u64>,
    /// Every sensor, by source then name.
    pub sensors: Vec<SensorView>,
    /// How each source did at the last poll.
    pub sources: Vec<SensorSourceView>,
}

/// `GET /v1/inputs`: what audio and screen capture can open, and the choice.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputsView {
    /// This OS has the capture backends (Windows: WASAPI loopback, DXGI duplication).
    pub supported: bool,
    /// Output devices loopback capture can open, by name. Microphones and
    /// other capture endpoints are never opened, so they are not listed.
    pub audio_devices: Vec<String>,
    /// The chosen output device; `None`: the system default.
    pub audio_device: Option<String>,
    /// Displays screen capture can open.
    pub displays: Vec<DisplayView>,
    /// The chosen display; `None`: the primary display.
    pub display: Option<String>,
}

/// One display for screen capture.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayView {
    /// Device name (`\\.\DISPLAY1`), what `display` names.
    pub name: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// The primary display.
    pub primary: bool,
    /// Graphics adapter.
    pub adapter: String,
}

/// One sensor: what `engine.getSensorValue(name)` returns, plus its name.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorView {
    /// Display name (the argument of `engine.getSensorValue`).
    pub name: String,
    /// Current value.
    pub value: f64,
    /// Unit (`°C`, `%`, `RPM`, `MHz`, …).
    pub units: String,
    /// Expected minimum.
    pub min: f64,
    /// Expected maximum.
    pub max: f64,
    /// `Temperature`, `Fan`, `Load`, `Clock`, `Voltage`, `Power` or `Other`.
    #[serde(rename = "type")]
    pub kind: String,
}

/// One sensor source.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SensorSourceView {
    /// `system`, `nvapi`, `hwmon` or `lhm`.
    pub name: String,
    /// Sensors it reported.
    pub sensors: usize,
    /// Why it failed at the last poll.
    pub error: Option<String>,
}

/// One network plugin's discovery service.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkServiceView {
    /// The plugin's `Name()` (the id commands use).
    pub plugin: String,
    /// The service runs.
    pub running: bool,
    /// Why it stopped, if it did.
    pub error: Option<String>,
    /// `discovery.forgetController` exists (dashboard *Forget*).
    pub can_forget: bool,
    /// What `service.controllers` holds.
    pub controllers: Vec<NetworkControllerView>,
    /// Last service log lines.
    pub log: Vec<String>,
}

/// One controller a network service lists.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkControllerView {
    /// Its id.
    pub id: String,
    /// `name` (or the id).
    pub name: String,
    /// `ip`.
    pub ip: Option<String>,
    /// `status`.
    pub status: Option<String>,
    /// `paired`, when the plugin reports it.
    pub paired: Option<bool>,
    /// It has `startLink()` and is not paired (dashboard *Pair*).
    pub can_pair: bool,
    /// It was announced: the device key it runs as.
    pub device: Option<String>,
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
    /// The plugin's parameters, each with its current `value`: the
    /// `ControllableParameters()` entries, then as changed at run time by
    /// `device.addProperty` / `removeProperty` while the device runs.
    pub parameters: Vec<Value>,
    /// Messages on the device card: `DeviceMessages()`, then as changed at run
    /// time by `device.addMessage` / `removeMessage` / `notify` / `denotify`.
    #[serde(default)]
    pub messages: Vec<DeviceMessageView>,
    /// The device and its children as canvas endpoints.
    pub endpoints: Vec<Endpoint>,
    /// Channel names (component editor).
    pub channels: Vec<ChannelView>,
    /// What "Save to devices" can store in this device (`None`: nothing).
    pub save: Option<SaveSupport>,
    /// Exit modes ("When Lightloom closes") this device's plugin carries out,
    /// natively or through its `shutdownColor` parameter (`restore` always).
    pub shutdown_modes: Vec<String>,
    /// This device's own exit mode (`None`: the setting's).
    pub shutdown_mode: Option<String>,
    /// The last battery report (devices whose plugin has the `battery` feature),
    /// kept after the device goes away.
    pub battery: Option<BatteryView>,
    /// Pulse the device red while its battery is low (user setting).
    pub blink_low_battery: bool,
    /// Driven by a third-party plugin read in place (a local SignalRGB
    /// installation's), not by one Lightloom ships or the user wrote.
    #[serde(default)]
    pub third_party: bool,
    /// Why the device cannot be driven (or only partly) as it is plugged in:
    /// "needs WinUSB driver: ..." for a raw USB device Windows bound no
    /// WinUSB driver to. Lightloom never installs one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver_problem: Option<String>,
    /// Where to read about [`DeviceView::driver_problem`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help_url: Option<String>,
}

/// What "Save to devices" can store in a device (the plugin's `SaveCapabilities()`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSupport {
    /// The colours of the current frame.
    #[serde(rename = "static")]
    pub static_colors: bool,
    /// Built-in effects (`spectrum`, `breathing`, ...).
    pub effects: Vec<String>,
    /// Seconds before the device may be saved again (one save per minute).
    pub wait_secs: u64,
}

/// What to store in one device.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSaveChoice {
    /// A built-in effect the device advertises; `None`: the current colours.
    #[serde(default)]
    pub effect: Option<String>,
}

/// A message a device plugin shows on its card.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceMessageView {
    /// `DeviceMessages()` `property`, `addMessage` id, or the alert id `notify` returned.
    pub id: String,
    /// `notify` title.
    pub title: Option<String>,
    /// The text.
    pub message: String,
    /// Hover text.
    pub tooltip: Option<String>,
    /// `notify` priority: 0 info, 1 important, 2 and 3 errors; `None` for plain messages.
    pub priority: Option<i32>,
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
    /// A third-party effect read in place (`source` `signalrgb`).
    #[serde(default)]
    pub third_party: bool,
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
    /// Drive SMBus devices (GPU RGB through NvAPI and AMD ADL; RAM RGB through the I/O
    /// broker when it is installed and healthy); on unless turned off.
    pub smbus: bool,
    /// Network lights: run the network plugins' discovery and drive the LAN
    /// devices they announce (off unless turned on).
    #[serde(default)]
    pub network: bool,
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
    /// What devices show when Lightloom closes: `restore` (their own lighting),
    /// `keep` (the last colours, not stored) or `off`.
    pub shutdown_mode: String,
    /// Low-battery threshold, percent (warnings, `battery.lowBatteryWarningLevel`).
    pub battery_low: u8,
    /// Critical battery threshold, percent.
    pub battery_critical: u8,
    /// Show low and critical battery alerts as system notifications.
    pub battery_notifications: bool,
    /// This OS has system notifications Lightloom can show.
    pub notifications_available: bool,
    /// `lightloom://` links open Lightloom actions: the per-user URL-scheme
    /// registration (`HKCU\Software\Classes\lightloom`) is on (opt-in).
    #[serde(default)]
    pub url_scheme: bool,
    /// This OS (and install) can register the URL scheme.
    #[serde(default)]
    pub url_scheme_available: bool,
    /// Why the registration is not what the setting says (it failed, or the
    /// `lightloom` command-line client is missing); `None` normally.
    #[serde(default)]
    pub url_scheme_notice: Option<String>,
    /// Black canvas while the PC is locked or the display is off (off by default).
    #[serde(default)]
    pub lights_off_when_idle: bool,
    /// The output device `engine.audio` captures (chosen in the dashboard, else
    /// `[audio] device` in config.toml); `None`: the system default.
    #[serde(default)]
    pub audio_device: Option<String>,
    /// The display `engine.zone` and the meters capture (chosen in the dashboard,
    /// else `[screen] display`); `None`: the primary display.
    #[serde(default)]
    pub display: Option<String>,
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
        /// This device's exit mode (`restore`, `keep`, `off`; `default` or an
        /// empty string: the setting's).
        #[serde(default)]
        shutdown_mode: Option<String>,
        /// Pulse the device red while its battery is low.
        #[serde(default)]
        blink_low_battery: Option<bool>,
    },
    /// "Save to devices": store lighting in the devices' own memory (flash or
    /// EEPROM), so it shows at boot without Lightloom. A preview says what would be
    /// stored and writes nothing; a save needs `confirm` (the user said yes) and
    /// is refused for a device saved less than a minute ago.
    #[serde(rename_all = "camelCase")]
    SaveToDevices {
        /// Only say what would be stored.
        #[serde(default)]
        preview: bool,
        /// The user confirmed the save.
        #[serde(default)]
        confirm: bool,
        /// Device key -> what to store. A preview of every device when empty.
        #[serde(default)]
        devices: std::collections::BTreeMap<String, DeviceSaveChoice>,
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
        /// What devices show when Lightloom closes (`restore`, `keep`, `off`).
        #[serde(default)]
        shutdown_mode: Option<String>,
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
        /// Network lights (starts or stops the discovery services).
        #[serde(default)]
        network: Option<bool>,
        /// Black canvas while the PC is locked or the display is off.
        #[serde(default)]
        lights_off_when_idle: Option<bool>,
    },
    /// Choose the audio output device and the display the inputs capture.
    SetInputs {
        /// Output device name (`""`: the system default); `None` leaves it.
        #[serde(default)]
        audio_device: Option<String>,
        /// Display name (`""`: the primary display); `None` leaves it.
        #[serde(default)]
        display: Option<String>,
    },
    /// *Add by IP*: ask a network plugin to look at `ip`.
    NetworkAdd {
        /// Plugin `Name()`.
        plugin: String,
        /// The address.
        ip: String,
    },
    /// *Pair* (`startLink()`) or *Forget* (`forgetController`) a network controller.
    NetworkAction {
        /// Plugin `Name()`.
        plugin: String,
        /// Controller id.
        id: String,
        /// `pair` or `forget`.
        action: String,
    },
    /// Import the local SignalRGB setup (read in place): its canvas placements
    /// as a layout (with the layout and per-device brightness and enable), the
    /// component arrays of its channels, effect control values and presets, the
    /// selected effect, the global brightness and per-device enable.
    ImportSignalRgb {
        /// Layout name (default `SignalRGB`).
        name: Option<String>,
    },
    /// What [`Request::ImportSignalRgb`] would import, without changing anything.
    PreviewSignalRgb {
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
    /// PawnIO / RAM lighting status, read again now (read-only).
    PawnIoStatus,
    /// Start the elevated helper for RAM lighting (Windows shows one UAC prompt).
    PawnIoSetup {
        /// Install or remove.
        action: PawnIoAction,
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
    /// Run a `lightloom://` action URL (docs/integrations/actions.md).
    Action {
        /// The URL (or a bare action path).
        url: String,
    },
    /// Register (`true`) or remove (`false`) the per-user `lightloom://` URL scheme.
    SetUrlScheme {
        /// Registered.
        enabled: bool,
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
            Request::SaveToDevices { .. } => "saveToDevices",
            Request::SetChannel { .. } => "setChannel",
            Request::SetPlacement { .. } => "setPlacement",
            Request::ApplyLayout { .. } => "applyLayout",
            Request::SaveLayoutAs { .. } => "saveLayoutAs",
            Request::DeleteLayout { .. } => "deleteLayout",
            Request::SetLayoutBrightness { .. } => "setLayoutBrightness",
            Request::CanvasEvent { .. } => "canvasEvent",
            Request::CanvasTap { .. } => "canvasTap",
            Request::SetSettings { .. } => "setSettings",
            Request::SetInputs { .. } => "setInputs",
            Request::NetworkAdd { .. } => "networkAdd",
            Request::NetworkAction { .. } => "networkAction",
            Request::ImportSignalRgb { .. } => "importSignalRgb",
            Request::PreviewSignalRgb { .. } => "previewSignalRgb",
            Request::GetLayout { .. } => "getLayout",
            Request::ListComponents => "listComponents",
            Request::Export => "export",
            Request::Import { .. } => "import",
            Request::PawnIoStatus => "pawnIoStatus",
            Request::PawnIoSetup { .. } => "pawnIoSetup",
            Request::SavePlaylist { .. } => "savePlaylist",
            Request::DeletePlaylist { .. } => "deletePlaylist",
            Request::PlayPlaylist { .. } => "playPlaylist",
            Request::StopPlaylist => "stopPlaylist",
            Request::SetEffectTimer { .. } => "setEffectTimer",
            Request::LoadEffect { .. } => "loadEffect",
            Request::EffectImage { .. } => "effectImage",
            Request::Action { .. } => "action",
            Request::SetUrlScheme { .. } => "setUrlScheme",
            Request::Rescan => "rescan",
            Request::Shutdown => "shutdown",
        }
    }
}

/// Something changed; clients refetch what they show (`GET /v1/state`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    /// `state` (the snapshot changed), `stats` (counters only), `log`,
    /// `battery` (a low or critical battery alert: `{key, name, level, severity}`)
    /// or `action` (an action URL for the dashboard: `{action: "view", page, tab, uid}`,
    /// `{action: "raiseWindow"}` or `{action: "toast", title, message}`).
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
        assert_eq!(
            serde_json::to_value(Request::PawnIoSetup {
                action: PawnIoAction::Remove
            })
            .unwrap(),
            serde_json::json!({"op": "pawnIoSetup", "action": "remove"})
        );
    }
}
