// SPDX-License-Identifier: GPL-3.0-or-later
// Tries to replace the host helper object to spoof its metadata (HueDeck test fixture).
globalThis.__huedeckHost = { readMeta() { return "{}"; } };
export function Name() { return "Edge Host Tamper"; }
