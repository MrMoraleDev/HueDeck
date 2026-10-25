// SPDX-License-Identifier: GPL-3.0-or-later
// The macro runtime's API (GET/PATCH /v1/macros, docs/macros.md) and the prompts macro
// scripts open with `system.prompt` (an `action` event `{action: "prompt", ...}`).

import { request } from "./api";

export interface MacroChoice {
  key: string;
  label: string;
}

export interface MacroOption {
  property: string;
  label: string;
  type: string;
  default: unknown;
  values?: MacroChoice[];
  filter?: string;
  min?: number;
  max?: number;
  step?: number;
  description?: string;
  value: unknown;
}

export interface MacroScript {
  id: string;
  kind: "action" | "input";
  source: "user" | "signalrgb";
  file: string;
  name: string;
  description: string;
  options: MacroOption[];
  handlers: string[];
  enabled: boolean;
  actions: string[];
  running: boolean;
  error: string | null;
  warnings: string[];
}

export interface MacrosView {
  enabled: boolean;
  keyboardHook: boolean;
  mouseHook: boolean;
  allowLaunch: boolean;
  hooksAvailable: boolean;
  hooks: { keyboard: boolean; mouse: boolean };
  hookError: string | null;
  emergencyStop: string;
  folders: { source: string; kind: string; path: string }[];
  scripts: MacroScript[];
}

export interface MacroPrompt {
  action: "prompt";
  id: number;
  script: string;
  title: string;
  message: string;
  placeholder: string;
  confirmText: string;
  cancelText: string;
}

const enc = encodeURIComponent;

export const macrosApi = {
  get: () => request<MacrosView>("GET", "/v1/macros"),
  setSettings: (patch: Partial<Pick<MacrosView, "enabled" | "keyboardHook" | "mouseHook" | "allowLaunch">>) =>
    request<MacrosView>("PATCH", "/v1/macros", patch),
  setScript: (id: string, patch: { enabled?: boolean; options?: Record<string, unknown>; actions?: string[] }) =>
    request<MacrosView>("PATCH", `/v1/macros/scripts/${enc(id)}`, patch),
  run: (id: string) => request("POST", `/v1/macros/scripts/${enc(id)}/run`),
  answer: (id: number, text: string | null) => request("POST", `/v1/macros/prompts/${id}`, { text }),
};

/** Open prompts, oldest first. */
class Prompts {
  open = $state<MacroPrompt[]>([]);

  /** An `action` event: `true` when it was a macro prompt. */
  take(a: { action: string }): boolean {
    if (a.action !== "prompt") return false;
    this.open = [...this.open, a as MacroPrompt];
    return true;
  }

  async answer(p: MacroPrompt, text: string | null) {
    this.open = this.open.filter((x) => x.id !== p.id);
    try {
      await macrosApi.answer(p.id, text);
    } catch {
      // The script stopped meanwhile: nothing to answer.
    }
  }
}

export const prompts = new Prompts();
