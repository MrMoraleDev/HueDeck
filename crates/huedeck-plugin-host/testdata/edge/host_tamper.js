// SPDX-License-Identifier: Apache-2.0 OR MIT
// Tries to replace the host helper object to spoof its metadata (HueDeck test fixture).
globalThis.__huedeckHost = { readMeta() { return "{}"; } };
export function Name() { return "Edge Host Tamper"; }
