// SPDX-License-Identifier: Apache-2.0 OR MIT
//
// Facade for "@SignalRGB/lcd" (Rust-backed frame sink from the renderer's
// per-device LCD-face browser).
//
// Corpus (2026-10-17, 1006 plugin .js files): 42 files import it
// (`import LCD from "@SignalRGB/lcd"`, both quote styles). Member calls:
//   LCD.initialize({width, height[, circular][, circularPreview]})   45
//   LCD.getFrame({format: "JPEG"|"jpeg"|"RGB"|"RGBA"|"NZXT::Q565"|"NZXT::RGB565"[, quality]}) -> byte[]  52
// Docs/exe-only (0 uses): getSlicedFrame(options), getImageBuffer().
import { createFacade } from "../HostApiMissing.js";

export const MEMBERS = Object.freeze(["initialize", "getFrame", "getSlicedFrame", "getImageBuffer"]);

const LCD = createFacade("@SignalRGB/lcd", MEMBERS, {
  hint: "LCD faces arrive with the huedeck-render LCD browser (M6)",
});

export default LCD;
