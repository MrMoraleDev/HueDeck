// SPDX-License-Identifier: GPL-3.0-or-later
// `@SignalRGB/tcp` in plugin tests: forwards to the mock net installed by installNetGlobals().
export default new Proxy({}, { get: (_, key) => globalThis.__huedeckMockNet?.tcp?.[key] });
