// SPDX-License-Identifier: GPL-3.0-or-later
// The daemon's native API (crates/lightloom-api/src/server.rs). Requests that change
// state carry the token lightloomd embeds in the page it serves.

import type {
  EffectTimer,
  LibraryComponent,
  Placement,
  Playlist,
  SaveResult,
  ShutdownMode,
  SignalRgbImport,
  Snapshot,
} from "./types";

function token(): string {
  const meta = document.querySelector<HTMLMetaElement>('meta[name="lightloom-token"]')?.content ?? "";
  if (meta && !meta.includes("%")) return meta;
  // `npm run dev`: the page does not come from lightloomd; paste the api-token file into localStorage.
  try {
    return localStorage.getItem("lightloom-token") ?? "";
  } catch {
    return "";
  }
}

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message);
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const headers: Record<string, string> = {};
  if (method !== "GET") headers["Authorization"] = `Bearer ${token()}`;
  if (body !== undefined) headers["Content-Type"] = "application/json";
  const res = await fetch(path, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  const data = text ? JSON.parse(text) : null;
  if (!res.ok) {
    const e = data?.error ?? {};
    throw new ApiError(res.status, e.code ?? "error", e.message ?? res.statusText);
  }
  return data as T;
}

const enc = encodeURIComponent;

export const api = {
  state: () => request<Snapshot>("GET", "/v1/state"),
  applyEffect: (id: string) => request("POST", `/v1/effects/${enc(id)}/apply`),
  setEffectValues: (id: string, values: Record<string, unknown>) =>
    request("PATCH", `/v1/effects/${enc(id)}/values`, values),
  savePreset: (id: string, name: string) => request("PUT", `/v1/effects/${enc(id)}/presets/${enc(name)}`),
  applyPreset: (id: string, name: string) => request("POST", `/v1/effects/${enc(id)}/presets/${enc(name)}/apply`),
  deletePreset: (id: string, name: string) => request("DELETE", `/v1/effects/${enc(id)}/presets/${enc(name)}`),
  shuffleEffect: () => request("POST", "/v1/effects/shuffle"),
  createPlaylist: (p: Omit<Playlist, "id" | "updatedAt">) => request<Playlist>("POST", "/v1/playlists", p),
  savePlaylist: (p: Playlist) => request<Playlist>("PUT", `/v1/playlists/${enc(p.id)}`, p),
  deletePlaylist: (id: string) => request("DELETE", `/v1/playlists/${enc(id)}`),
  playPlaylist: (id: string, start?: string) => request("POST", `/v1/playlists/${enc(id)}/play`, { start }),
  stopPlaylist: () => request("POST", "/v1/playlists/stop"),
  setTimer: (patch: Partial<EffectTimer>) => request<EffectTimer>("PATCH", "/v1/lighting/timer", patch),
  setLighting: (patch: { brightness?: number; enabled?: boolean }) => request("PATCH", "/v1/lighting", patch),
  setDevice: (
    key: string,
    patch: { enabled?: boolean; parameters?: Record<string, unknown>; blinkLowBattery?: boolean; shutdownMode?: ShutdownMode | "default" },
  ) =>
    request("PATCH", `/v1/devices/${enc(key)}`, patch),
  setChannel: (key: string, channel: string, components: unknown[]) =>
    request("PUT", `/v1/devices/${enc(key)}/channels/${enc(channel)}`, components),
  setPlacement: (uid: string, placement: Placement) => request("PUT", `/v1/placements/${enc(uid)}`, placement),
  applyLayout: (name: string) => request("POST", `/v1/layouts/${enc(name)}/apply`),
  saveLayoutAs: (name: string) => request("PUT", `/v1/layouts/${enc(name)}`),
  deleteLayout: (name: string) => request("DELETE", `/v1/layouts/${enc(name)}`),
  setLayoutBrightness: (brightness: number) => request("PATCH", "/v1/layout", { brightness }),
  components: () => request<LibraryComponent[]>("GET", "/v1/components"),
  setSettings: (patch: {
    signalrgb?: boolean;
    gpuCompositing?: boolean;
    smbus?: boolean;
    network?: boolean;
    batteryLow?: number;
    batteryCritical?: number;
    batteryNotifications?: boolean;
    shutdownMode?: ShutdownMode;
    openrgbSdk?: boolean;
  }) =>
    request("PATCH", "/v1/settings", patch),
  /** Move the dashboard and the API to `port`; answers with the new address. */
  setPort: (port: number) => request<{ port: number; url: string }>("PATCH", "/v1/settings", { port }),
  networkAdd: (plugin: string, ip: string) => request("POST", `/v1/network/${enc(plugin)}/add`, { ip }),
  networkAction: (plugin: string, id: string, action: "pair" | "forget") =>
    request("POST", `/v1/network/${enc(plugin)}/controllers/${enc(id)}/${action}`),
  /** What the SignalRGB import would bring in (read-only). */
  previewSignalRgb: () => request<SignalRgbImport>("GET", "/v1/import/signalrgb"),
  importSignalRgb: () => request<SignalRgbImport>("POST", "/v1/import/signalrgb", {}),
  exportState: () => request<unknown>("GET", "/v1/export"),
  importState: (state: unknown) => request("POST", "/v1/import", state),
  rescan: () => request("POST", "/v1/rescan"),
  /** What "Save to devices" would store (nothing is written); every device when `devices` is empty. */
  savePreview: (devices: Record<string, { effect: string | null }> = {}) =>
    request<{ devices: SaveResult[] }>("POST", "/v1/save/preview", { devices }),
  /** Store lighting in the devices' own memory; only after the user confirmed it. */
  saveToDevices: (devices: Record<string, { effect: string | null }>) =>
    request<{ devices: SaveResult[] }>("POST", "/v1/save", { confirm: true, devices }),
  shutdown: () => request("POST", "/v1/shutdown"),
  tap: (x: number, y: number) => request("POST", "/v1/canvas/tap", { x, y }),
};

/** ws:// URL of a stream on the daemon serving this page. */
export function wsUrl(path: string): string {
  const proto = location.protocol === "https:" ? "wss:" : "ws:";
  return `${proto}//${location.host}${path}`;
}
