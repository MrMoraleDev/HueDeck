// SPDX-License-Identifier: GPL-3.0-or-later
// The daemon's native API (crates/huedeck-api/src/server.rs). Requests that change
// state carry the token huedeckd embeds in the page it serves.

import type { LibraryComponent, PawnIoView, Placement, Snapshot } from "./types";

function token(): string {
  const meta = document.querySelector<HTMLMetaElement>('meta[name="huedeck-token"]')?.content ?? "";
  if (meta && !meta.includes("%")) return meta;
  // `npm run dev`: the page does not come from huedeckd; paste the api-token file into localStorage.
  try {
    return localStorage.getItem("huedeck-token") ?? "";
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
  setLighting: (patch: { brightness?: number; enabled?: boolean }) => request("PATCH", "/v1/lighting", patch),
  setDevice: (key: string, patch: { enabled?: boolean; parameters?: Record<string, unknown> }) =>
    request("PATCH", `/v1/devices/${enc(key)}`, patch),
  setChannel: (key: string, channel: string, components: unknown[]) =>
    request("PUT", `/v1/devices/${enc(key)}/channels/${enc(channel)}`, components),
  setPlacement: (uid: string, placement: Placement) => request("PUT", `/v1/placements/${enc(uid)}`, placement),
  applyLayout: (name: string) => request("POST", `/v1/layouts/${enc(name)}/apply`),
  saveLayoutAs: (name: string) => request("PUT", `/v1/layouts/${enc(name)}`),
  deleteLayout: (name: string) => request("DELETE", `/v1/layouts/${enc(name)}`),
  setLayoutBrightness: (brightness: number) => request("PATCH", "/v1/layout", { brightness }),
  components: () => request<LibraryComponent[]>("GET", "/v1/components"),
  setSettings: (patch: { signalrgb?: boolean; gpuCompositing?: boolean; smbus?: boolean }) =>
    request("PATCH", "/v1/settings", patch),
  importSignalRgb: () => request<{ placements: number; channels: number; layout: string }>(
    "POST",
    "/v1/import/signalrgb",
    {},
  ),
  exportState: () => request<unknown>("GET", "/v1/export"),
  importState: (state: unknown) => request("POST", "/v1/import", state),
  pawnio: () => request<PawnIoView>("GET", "/v1/pawnio"),
  pawnioInstall: () => request<PawnIoView>("POST", "/v1/pawnio/install"),
  pawnioRemove: () => request<PawnIoView>("POST", "/v1/pawnio/remove"),
  rescan: () => request("POST", "/v1/rescan"),
  shutdown: () => request("POST", "/v1/shutdown"),
  tap: (x: number, y: number) => request("POST", "/v1/canvas/tap", { x, y }),
};

/** ws:// URL of a stream on the daemon serving this page. */
export function wsUrl(path: string): string {
  const proto = location.protocol === "https:" ? "wss:" : "ws:";
  return `${proto}//${location.host}${path}`;
}
