// SPDX-License-Identifier: GPL-3.0-or-later
// Settings → Updates (crates/lightloom-daemon/src/update.rs): the daemon asks GitHub only
// when the user checks, or once a week when they turned that on (docs/privacy.md).

import { request } from "./api";

export interface UpdateAsset {
  name: string;
  size: number;
  url: string;
}

export interface UpdateRelease {
  version: string;
  tag: string;
  name: string;
  /** Plain text (the daemon removes markup); shown as text, never as HTML. */
  notes: string;
  /** The release page on github.com. */
  url: string;
  publishedAt: string | null;
  prerelease: boolean;
  asset: UpdateAsset | null;
}

export interface UpdateJob {
  version: string;
  running: boolean;
  ok: boolean | null;
  downloaded: number;
  total: number;
  message: string;
}

export interface UpdateView {
  currentVersion: string;
  checkWeekly: boolean;
  beta: boolean;
  checking: boolean;
  /** Unix seconds. */
  lastCheck: number | null;
  error: string | null;
  available: UpdateRelease | null;
  /** Download downloads, verifies and opens the installer; otherwise it opens the release page. */
  installable: boolean;
  installNote: string | null;
  job: UpdateJob | null;
}

export const updates = {
  get: () => request<UpdateView>("GET", "/v1/update"),
  check: () => request<UpdateView>("POST", "/v1/update/check"),
  setOptions: (patch: { checkWeekly?: boolean; beta?: boolean }) => request<UpdateView>("PATCH", "/v1/update", patch),
  install: () => request<UpdateView>("POST", "/v1/update/install"),
};

/** Only release pages of Lightloom's own repository are opened. */
export function isReleaseUrl(url: string): boolean {
  return url.startsWith("https://github.com/MrMoraleDev/Lightloom/releases/");
}
