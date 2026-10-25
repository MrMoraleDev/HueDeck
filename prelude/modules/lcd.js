// SPDX-License-Identifier: GPL-3.0-or-later
//
// Facade for "@SignalRGB/lcd" (Rust-backed frame sink from the renderer's
// per-device LCD-face browser).
//
// Corpus (2026-10-17, 1006 plugin .js files): 42 files import it
// (`import LCD from "@SignalRGB/lcd"`, both quote styles). Member calls:
//   LCD.initialize({width, height[, circular][, circularPreview]})   45
//   LCD.getFrame({format: "JPEG"|"jpeg"|"RGB"|"RGBA"|"NZXT::Q565"|"NZXT::RGB565"[, quality]}) -> byte[]  52
// Device sessions use the native module instead (js/host.js `lcdModule`, docs/host-api/lcd-lightloom.md).
// Docs/exe-only (0 uses): getSlicedFrame(options), getImageBuffer().
import { createFacade } from "../HostApiMissing.js";

export const MEMBERS = Object.freeze(["initialize", "getFrame", "getSlicedFrame", "getImageBuffer"]);

const LCD = createFacade("@SignalRGB/lcd", MEMBERS, {
  hint: "device sessions get the native module (docs/host-api/lcd-lightloom.md); this facade is used outside them",
});

export default LCD;
