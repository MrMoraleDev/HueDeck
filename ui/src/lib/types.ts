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
  value?: unknown;
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
  /** What the effect reads; "meters" marks game integrations. */
  uses?: ("audio" | "video" | "meters" | "engine" | "webgl" | "input")[];
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
}

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
