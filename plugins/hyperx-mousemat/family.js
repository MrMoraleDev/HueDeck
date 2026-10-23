// SPDX-License-Identifier: GPL-3.0-or-later
//
// HyperX RGB mousemats in OpenRGB's Direct mode: Fury Ultra, Pulsefire Mat RGB Mouse Pad XL
// (Kingston 0x0951) and Pulsefire Mat (HP 0x03F0). Shared, byte for byte, by
// plugins/hyperx-mousemat and plugins/hyperx-mousemat-hp (a plugin has a single vendor id, hence
// two directories; the tests check the copies match).
// Adapted from OpenRGB Controllers/HyperXMousematController/HyperXMousematController.cpp, .h,
// RGBController_HyperXMousemat.cpp, .h and HyperXMousematControllerDetect.cpp (GPL-2.0-or-later)
// at commit d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original author, as in every file header: "Adam Honse (CalcProgrammer1)  25 Oct 2020".
//
// Protocol notes: docs/hardware/hyperx-mousemat.md. Only the Direct reports are sent.

// HyperXMousematControllerDetect.cpp registrations (the _WIN32 branch for the XL pad) and the LED
// counts its detector passes: the XL pad 2 underglow LEDs, the others 15 underglow + 5 strip.
export const PRODUCTS = [
  { vid: 0x0951, pid: 0x1705, name: "HyperX Fury Ultra", variant: "MAT_15_5", iface: 0 },
  { vid: 0x03F0, pid: 0x0F8D, name: "HyperX Pulsefire Mat", variant: "MAT_15_5", iface: 1, page: 0xFF90, usage: 0xFF00 },
  { vid: 0x0951, pid: 0x1741, name: "HyperX Pulsefire Mat RGB Mouse Pad XL", variant: "FURY_A_XL", iface: 1, page: 0xFF90, usage: 0xFF00 },
];

// RGBController_HyperXMousemat::SetupZones: "Underglow" (row 0) then "LED Strip" (row 1); a zone
// with more than one LED names them "<zone> LED <n>".
const VARIANTS = {
  MAT_15_5: { leds: zoneLeds(15, 5), size: [15, 2] },
  FURY_A_XL: { leds: zoneLeds(2, 0), size: [2, 1] },
};

function zoneLeds(first, second) {
  const leds = [];
  for (let i = 0; i < first; i++) leds.push([first > 1 ? `Underglow LED ${i + 1}` : "Underglow", i, 0]);
  for (let i = 0; i < second; i++) leds.push([second > 1 ? `LED Strip LED ${i + 1}` : "LED Strip", i, 1]);
  return leds;
}

export function productFor(vid, pid) {
  return PRODUCTS.find((p) => p.vid === vid && p.pid === pid) ?? null;
}

// REGISTER_HID_WRAPPED_DETECTOR_I (interface only) and _IPU (interface, usage page, usage).
export function validate(product, endpoint) {
  if (!product || endpoint.interface !== product.iface) {
    return false;
  }
  if (product.page === undefined) {
    return true;
  }
  return endpoint.usage_page === product.page && endpoint.usage === product.usage;
}

/** Sets the device's name and LED grid; returns the session state (null for an unknown product). */
export function start(dev, product) {
  if (!product) {
    return null;
  }
  const variant = VARIANTS[product.variant];
  dev.setName(product.name);
  dev.setSize(variant.size.slice());
  dev.setControllableLeds(variant.leds.map((l) => l[0]), variant.leds.map((l) => [l[1], l[2]]));
  // OpenRGB's constructor sends nothing: the first Direct frame takes the mat over.
  return { variant };
}

// OpenRGB's keepalive resends whenever 50 ms passed without an update: a frame on every Render().
export function render(dev, st) {
  if (!st) {
    return;
  }
  frame(dev, st.variant.leds.map((l) => rgb(dev.color(l[1], l[2]))));
}

// OpenRGB has no exit step and no "back to the stored lighting" command for the mats: one black
// Direct frame.
export function shutdown(dev, st) {
  if (!st) {
    return;
  }
  frame(dev, st.variant.leds.map(() => [0, 0, 0]));
}

// HyperXMousematController::SendDirect: 65-byte feature reports 00 04 F2 ... [9] = 02, then two
// reports of 16 groups 81 R G B (colours 0-15 and 16-31). The C++ reads 32 colours whatever the
// LED count (past the end of its colour vector for 20 or 2 LEDs); Lightloom sends those black.
function frame(dev, colours) {
  const all = colours.slice();
  while (all.length < 32) all.push([0, 0, 0]);
  const start = new Array(65).fill(0);
  start[0x01] = 0x04;
  start[0x02] = 0xF2;
  start[0x09] = 0x02;
  dev.send_report(start, 65);
  for (let half = 0; half < 2; half++) {
    const buf = new Array(65).fill(0);
    for (let i = 0; i < 16; i++) {
      const [r, g, b] = all[half * 16 + i];
      buf[i * 4 + 1] = 0x81;
      buf[i * 4 + 2] = r;
      buf[i * 4 + 3] = g;
      buf[i * 4 + 4] = b;
    }
    dev.send_report(buf, 65);
  }
}

function rgb(c) {
  return c && c.length >= 3 ? [c[0] & 0xFF, c[1] & 0xFF, c[2] & 0xFF] : [0, 0, 0];
}
