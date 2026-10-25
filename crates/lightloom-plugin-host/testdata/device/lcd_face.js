// SPDX-License-Identifier: GPL-3.0-or-later
//
// Synthetic device plugin for the device-session tests: `@SignalRGB/lcd`.
// Initialize sets up a 4x2 LCD and logs what the module answers (errors, the
// other members); every Render writes the RGB565 frame after a report id.

import LCD from "@SignalRGB/lcd";

export function Name() { return "LCD probe"; }
export function Publisher() { return "Lightloom tests"; }
export function VendorId() { return 0x1234; }
export function ProductId() { return 0x1cd0; }
export function Size() { return [1, 1]; }

/* global device */
export function Validate(endpoint) { return endpoint.interface === 0; }

function caught(f) {
  try {
    f();
    return "no error";
  } catch (e) {
    return `${e.name} ${e.code}`;
  }
}

export function Initialize() {
  const before = { initialized: LCD.initialized, early: caught(() => LCD.getFrame({ format: "RGB" })) };
  const badSettings = caught(() => LCD.initialize({ width: "4", height: 2 }));
  const badSize = caught(() => LCD.initialize({ width: 0, height: 2 }));
  LCD.initialize({ width: 4, height: 2, circularPreview: true });
  const rgb = LCD.getFrame({ format: "rgb" });
  const jpeg = LCD.getFrame({ format: "jpeg", quality: 50 });
  const cells = LCD.getSlicedFrame({ cols: 2, rows: 1, format: "RGBA" });
  const crop = LCD.getImageBuffer(1, 0, 2, 2, { outputWidth: 4, outputHeight: 4, format: "RGBA" });
  device.log(
    `probe ${JSON.stringify({
      before,
      badSettings,
      badSize,
      initialized: LCD.initialized,
      isArray: Array.isArray(rgb),
      rgb: rgb.length,
      rgbFirst: rgb.slice(0, 3),
      jpegMagic: jpeg.slice(0, 2),
      cells: cells.map((c) => c.length),
      crop: crop.length,
      q565: caught(() => LCD.getFrame({ format: "NZXT::Q565" })),
      nzxt565: caught(() => LCD.getFrame({ format: "NZXT::RGB565" })),
      gif: caught(() => LCD.getFrame({ format: "GIF" })),
      noFormat: caught(() => LCD.getFrame({})),
      grid: caught(() => LCD.getSlicedFrame({ columns: 0, rows: 1, format: "RGB" })),
      bounds: caught(() => LCD.getImageBuffer(9, 0, 1, 1)),
    })}`,
  );
}

export function Render() {
  const frame = LCD.getFrame({ format: "RGB565" });
  device.write([0x00].concat(frame), frame.length + 1);
}

export function Shutdown() {}
