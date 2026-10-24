// SPDX-License-Identifier: GPL-3.0-or-later
// `@SignalRGB/udp` in plugin tests: forwards to the mock net installed by installNetGlobals().
export default new Proxy({}, { get: (_, key) => globalThis.__lightloomMockNet?.udp?.[key] });
