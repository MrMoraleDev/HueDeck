// SPDX-License-Identifier: GPL-3.0-or-later
// The daemon's read model (crates/lightloom-api/src/model.rs), camelCase.

export interface Placement {
  position: { x: number; y: number };
  scale?: { x: number; y: number };
  rotation: number;
  flipX: boolean;
  flipY: boolean;
  enabled: boolean;
  brightness: number;
  lightingMode: "Canvas" | "Forced";
  forcedColor: string;
}

export interface Endpoint {
  uid: string;
  kind: "device" | "subdevice" | "component";
  name: string;
  channel?: string | null;
  size: [number, number];
  leds: number;
  placement: Placement;
  /** Shows its custom per-LED colours instead of the effect. */
  customLeds?: boolean;
}

export interface ChannelView {
  name: string;
  ledLimit: number;
  components: { ComponentId: string; ProductName?: string | null; DisplayName: string; LedCount: number }[];
}

export interface Parameter {
  property: string;
  label: string;
  group?: string | null;
  description?: string | null;
  tooltip?: string | null;
  type: "color" | "combobox" | "number" | "boolean" | "hue" | "textfield" | "keycode";
  values: string[];
  min?: number | null;
  max?: number | null;
  step?: number | null;
  default?: unknown;
  /** false: apply when the control is released / committed, not while it moves. */
  live?: boolean;
  /** Display order (lower first; unordered ones keep their place after). */
  order?: number | null;
  /** Textfield regex the whole value must match. */
  filter?: string | null;
  value?: unknown;
}

/** A message a device plugin shows on its card (DeviceMessages, addMessage, notify). */
export interface DeviceMessage {
  id: string;
  title?: string | null;
  message: string;
  tooltip?: string | null;
  /** notify priority: 0 info, 1 important, 2 and 3 errors; absent for plain messages. */
  priority?: number | null;
}

export interface DeviceView {
  key: string;
  vid: number;
  pid: number;
  name: string;
  plugin: string;
  deviceType?: string;
  pluginPath: string;
  state: string;
  enabled: boolean;
  connected: boolean;
  frames: number;
  renderErrors: number;
  ioErrors: number;
  /** SMBus devices: "<bus name> · 0x<address>"; network devices: "network · <ip>". */
  bus?: string | null;
  defaultScale: number;
  image?: string | null;
  parameters: Parameter[];
  /** Plugin messages (may change while the device runs). */
  messages?: DeviceMessage[];
  endpoints: Endpoint[];
  channels: ChannelView[];
  /** What "Save to devices" can store in the device (null: nothing). */
  save?: SaveSupport | null;
  /** Exit modes the device's plugin carries out ("restore" always). */
  shutdownModes: ShutdownMode[];
  /** This device's own exit mode (null: the setting's). */
  shutdownMode?: ShutdownMode | null;
  /** The plugin's last battery report (devices with a battery only). */
  battery?: BatteryView | null;
  /** Pulse red while the battery is low. */
  blinkLowBattery: boolean;
  /** Driven by a third-party plugin read in place (a local SignalRGB installation's). */
  thirdParty?: boolean;
  /** Why the device cannot be driven as plugged in ("needs WinUSB driver: ..."). */
  driverProblem?: string;
  /** Where to read about driverProblem. */
  helpUrl?: string;
  /** The plugin's manifest lists this very product as tested. */
  tested?: boolean;
  /** The plugin's version (its manifest's, else its Version() export). */
  pluginVersion?: string;
  /** hid, rawusb, hybrid, serial, smbus, network or lamparray ("" for absent devices). */
  pluginType?: string;
  /** Interfaces as enumeration saw them, one line each (no paths, no serials). */
  interfaces?: string[];
  /** The device's LCD, while its plugin drives one. */
  lcd?: LcdView | null;
}

/** A device's LCD (docs/host-api/lcd-lightloom.md). */
export interface LcdView {
  width: number;
  height: number;
  circular: boolean;
  /** The face shown (null: none installed, a test pattern shows). */
  face: string | null;
  framerate: number;
  /** "cef": the face page renders; "mock": a test pattern (no renderer). */
  renderer: "cef" | "mock" | string;
  running: boolean;
  status?: string | null;
}

/** An LCD face: an HTML page with onReady(). */
export interface LcdFace {
  id: string;
  title: string;
  description: string;
  publisher: string;
  usesSensors: boolean;
}

export type ShutdownMode = "restore" | "keep" | "off";

export interface SaveSupport {
  static: boolean;
  effects: string[];
  /** Seconds before the device may be saved again. */
  waitSecs: number;
}

/** One device in the answer of POST /v1/save or /v1/save/preview. */
export interface SaveResult {
  key: string;
  name: string;
  ok: boolean;
  dryRun: boolean;
  result: { effect?: string; zones?: { name: string; color: string }[]; note?: string } | null;
  error?: string | null;
}

export interface BatteryView {
  /** Percent; null until the plugin reported one. */
  level?: number | null;
  state: "unknown" | "discharging" | "charging" | "fullCharging" | "full" | "wirelessCharging";
  charging: boolean;
  status: "ok" | "low" | "critical";
  /** Unix time of the report, milliseconds. */
  updatedMs: number;
}

export interface EffectControl {
  property: string;
  label: string;
  type: "number" | "hue" | "boolean" | "color" | "textfield" | "combobox" | "sensor";
  default?: string;
  min?: number;
  max?: number;
  step?: number;
  values?: string[];
  tooltip?: string;
}

export interface EffectView {
  id: string;
  title: string;
  description: string;
  publisher: string;
  source: "lightloom" | "user" | "signalrgb" | "developer";
  controls: EffectControl[];
  values: Record<string, unknown>;
  presets: string[];
  /** A third-party effect read in place (SignalRGB's). */
  thirdParty?: boolean;
  /** What the effect reads; "meters" marks game integrations. */
  uses?: ("audio" | "video" | "meters" | "engine" | "sensors" | "webgl" | "input")[];
}

/** An effect in the SignalRGB import (preview or result). */
export interface SignalRgbImportEffect {
  id: string;
  title: string | null;
  /** In the library now (the SignalRGB library is on). */
  available: boolean;
  thirdParty: true;
}

/** What the SignalRGB import brings in (`GET /v1/import/signalrgb`), or brought in (`POST`). */
export interface SignalRgbImport {
  available: boolean;
  layout: string | null;
  placements: number;
  layoutBrightness: number | null;
  globalBrightness: number | null;
  selectedEffect: SignalRgbImportEffect | null;
  effects: (SignalRgbImportEffect & { values: number; presets: string[] })[];
  channels: number;
  channelSetups: { device: string; channel: string; components: number; leds: number }[];
  devices: { key: string; name: string | null; known: boolean; enabled: boolean | null; brightness: number | null }[];
  notes: string[];
  imported?: boolean;
  valuesImported?: number;
  presetsImported?: number;
}

/** The auto-next timer (SignalRGB's "Effect Timer Interval"). */
export interface EffectTimer {
  enabled: boolean;
  /** Seconds per effect. */
  interval: number;
}

export interface PlaylistEntry {
  effect: string;
  preset?: string;
  values?: Record<string, unknown>;
  /** Seconds; absent: the timer's interval. */
  duration?: number;
}

export interface Playlist {
  id: string;
  name: string;
  shuffle: boolean;
  entries: PlaylistEntry[];
  updatedAt: string;
}

export interface Lighting {
  effect?: string | null;
  effectName?: string | null;
  brightness: number;
  enabled: boolean;
  layout: string;
  layoutBrightness: number;
  renderer: { kind: string; running: boolean; fps: number; status?: string | null };
  /** The playlist playing now; `entry` indexes its entries. */
  playlist?: { id: string; name: string; entry: number } | null;
  timer: EffectTimer;
  /** When the next effect comes by itself (Unix ms). */
  nextSwitchMs?: number | null;
}

export interface Settings {
  signalrgb: boolean;
  signalrgbAvailable: boolean;
  gpuCompositing: boolean;
  smbus: boolean;
  network: boolean;
  /** OpenRGB SDK server on 127.0.0.1:6742 (opt-in). */
  openrgbSdk: boolean;
  /** Black canvas while the PC is locked or the display is off (off by default). */
  lightsOffWhenIdle: boolean;
  /** Port of the dashboard and the API (127.0.0.1 only). */
  /** What devices show when Lightloom closes. */
  shutdownMode: ShutdownMode;
  port: number;
  /** Set when the configured port was busy at startup and the default one is used. */
  portNotice: string | null;
  fps: number;
  dryRun: boolean;
  batteryLow: number;
  batteryCritical: number;
  batteryNotifications: boolean;
  notificationsAvailable: boolean;
  /** lightloom:// links are registered for this user (opt-in). */
  urlScheme?: boolean;
  /** This OS and install can register the URL scheme. */
  urlSchemeAvailable?: boolean;
  /** The output device engine.audio captures (null: the system default). */
  audioDevice?: string | null;
  /** The display screen capture uses (null: the primary one). */
  display?: string | null;
  /** Why the registration is not what the setting says. */
  urlSchemeNotice?: string | null;
}

/** An `action` event: an action URL for the dashboard. */
export type DashboardAction =
  | { action: "view"; page: string; tab: string; uid?: string | null }
  | { action: "raiseWindow" }
  | { action: "toast"; title: string; message: string };

export interface Snapshot {
  version: string;
  state: string;
  lighting: Lighting;
  devices: DeviceView[];
  effects: EffectView[];
  layouts: string[];
  playlists: Playlist[];
  settings: Settings;
  network: NetworkService[];
  conflicts: string[];
  warnings: string[];
  pawnio: PawnIoView;
  /** OS and build ("Windows 11 Pro 25H2 (build 26200.6899)"), for device reports. */
  os?: string;
  /** LCD faces devices with a screen can show. */
  lcdFaces?: LcdFace[];
}

export interface PawnIoJob {
  action: "install" | "remove";
  running: boolean;
  ok?: boolean | null;
  message: string;
}

/** PawnIO and the Lightloom I/O service: what RAM lighting needs (Windows). */
export interface PawnIoView {
  supported: boolean;
  installed: boolean;
  version?: string | null;
  versionOk: boolean;
  edition?: string | null;
  service: "notInstalled" | "stopped" | "pending" | "running" | "unknown";
  brokerHealthy: boolean;
  ramBuses: number;
  status: string;
  job?: PawnIoJob | null;
}

export interface NetworkController {
  id: string;
  name: string;
  ip?: string | null;
  status?: string | null;
  paired?: boolean | null;
  canPair: boolean;
  /** The device key once the plugin announced it. */
  device?: string | null;
}

/** `GET /v1/sensors` (read-only hardware sensors). */
/** GET /v1/inputs: what audio and screen capture can open, and the choice. */
export interface InputsView {
  supported: boolean;
  audioDevices: string[];
  audioDevice: string | null;
  displays: { name: string; width: number; height: number; primary: boolean; adapter: string }[];
  display: string | null;
}

/** One line of GET /v1/logs. */
export interface LogEntry {
  time?: string;
  level?: "ERROR" | "WARN" | "INFO" | "DEBUG" | "TRACE";
  target?: string;
  source?: string;
  message: string;
  file?: string;
}

/** GET /v1/logs. */
export interface LogsView {
  from: "file" | "memory";
  available: ("file" | "memory")[];
  files: string[];
  entries: LogEntry[];
  truncated: boolean;
  sources: string[];
}

export interface LogQuery {
  from?: "file" | "memory";
  level?: string;
  q?: string;
  source?: string;
  limit?: number;
}

export interface SensorsView {
  enabled: boolean;
  polling: boolean;
  intervalMs: number;
  ageMs: number | null;
  sensors: Sensor[];
  sources: { name: string; sensors: number; error: string | null }[];
}

/** One sensor: what `engine.getSensorValue(name)` returns, plus its name. */
export interface Sensor {
  name: string;
  value: number;
  units: string;
  min: number;
  max: number;
  type: "Temperature" | "Fan" | "Voltage" | "Load" | "Clock" | "Power" | "Other";
}

export interface NetworkService {
  plugin: string;
  running: boolean;
  error?: string | null;
  canForget: boolean;
  controllers: NetworkController[];
  log: string[];
}

export interface LibraryComponent {
  productName: string;
  displayName: string;
  brand: string;
  type: string;
  ledCount: number;
  width: number;
  height: number;
}

/** One LED in the LED painter (GET /v1/devices/{key}/leds). */
export interface LedSpot {
  /** Index into the endpoint's colours. */
  slot: number;
  x: number;
  y: number;
  name?: string;
}

export interface LedEndpointView {
  uid: string;
  kind: "device" | "subdevice" | "component";
  name: string;
  channel?: string | null;
  size: [number, number];
  slots: number;
  leds: LedSpot[];
  custom: boolean;
  /** "#rrggbb" per slot. */
  colors: string[];
  zones: Record<string, number[]>;
}

export interface LedMapView {
  key: string;
  deviceType: string;
  endpoints: LedEndpointView[];
}

/** PATCH /v1/devices/{key}/leds/{uid}: applied as colors, fill, set, zones, then mode. */
export interface LedEdit {
  mode?: "custom" | "effect";
  colors?: string[];
  fill?: string;
  set?: Record<number, string>;
  zones?: Record<string, string>;
}
