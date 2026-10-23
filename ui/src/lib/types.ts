// SPDX-License-Identifier: GPL-3.0-or-later
// The daemon's read model (crates/huedeck-api/src/model.rs), camelCase.

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
  pluginPath: string;
  state: string;
  enabled: boolean;
  connected: boolean;
  frames: number;
  renderErrors: number;
  ioErrors: number;
  /** SMBus devices: "<bus name> · 0x<address>". */
  bus?: string | null;
  defaultScale: number;
  image?: string | null;
  parameters: Parameter[];
  endpoints: Endpoint[];
  channels: ChannelView[];
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
  source: "huedeck" | "user" | "signalrgb";
  controls: EffectControl[];
  values: Record<string, unknown>;
  presets: string[];
}

export interface Lighting {
  effect?: string | null;
  effectName?: string | null;
  brightness: number;
  enabled: boolean;
  layout: string;
  layoutBrightness: number;
  renderer: { kind: string; running: boolean; fps: number; status?: string | null };
}

export interface Settings {
  signalrgb: boolean;
  signalrgbAvailable: boolean;
  gpuCompositing: boolean;
  smbus: boolean;
  /** Port of the dashboard and the API (127.0.0.1 only). */
  port: number;
  /** Set when the configured port was busy at startup and the default one is used. */
  portNotice: string | null;
  fps: number;
  dryRun: boolean;
}

export interface Snapshot {
  version: string;
  state: string;
  lighting: Lighting;
  devices: DeviceView[];
  effects: EffectView[];
  layouts: string[];
  settings: Settings;
  conflicts: string[];
  warnings: string[];
  pawnio: PawnIoView;
}

export interface PawnIoJob {
  action: "install" | "remove";
  running: boolean;
  ok?: boolean | null;
  message: string;
}

/** PawnIO and the HueDeck I/O service: what RAM lighting needs (Windows). */
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

export interface LibraryComponent {
  productName: string;
  displayName: string;
  brand: string;
  type: string;
  ledCount: number;
  width: number;
  height: number;
}
