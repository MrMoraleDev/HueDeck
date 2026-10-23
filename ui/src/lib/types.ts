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
  /** SMBus devices: "<bus name> · 0x<address>"; network devices: "network · <ip>". */
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
  network: boolean;
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
