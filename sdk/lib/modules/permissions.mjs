// SPDX-License-Identifier: GPL-3.0-or-later
//
// `@SignalRGB/permissions` as a HueDeck device session answers it (host.js `permissionsModule`):
// HueDeck has no partner gate, so every partner permission is granted and never changes.

const PARTNERS = ["MSI", "Lian-Li"];

const permissions = Object.freeze({
  permissions: () => Object.fromEntries(PARTNERS.map((p) => [p, { fans: true, lighting: true, macros: true }])),
  setCallback: () => undefined,
});

export default permissions;
