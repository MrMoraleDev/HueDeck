// SPDX-License-Identifier: GPL-3.0-or-later
//
// HyperX keyboards in OpenRGB's Direct mode: Alloy Elite RGB, Alloy FPS RGB, Alloy Elite 2,
// Alloy Origins, Alloy Origins 60 / 65, Eve 1800 and Origins 2 65 (Kingston 0x0951 and HP 0x03F0
// product ids). Shared, byte for byte, by plugins/hyperx-keyboard and plugins/hyperx-keyboard-hp
// (a plugin has a single vendor id, hence two directories; the tests check the copies match).
// Adapted from OpenRGB Controllers/HyperXKeyboardController/: HyperXAlloyEliteController/*,
// HyperXAlloyFPSController/*, HyperXAlloyElite2Controller/*, HyperXAlloyOriginsController/*,
// HyperXAlloyOrigins60and65Controller/*, HyperXEve1800Controller/*, HyperXOrigins2_65Controller/*
// (controller, RGBController and detector files; GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors, as in the file headers: "Adam Honse (CalcProgrammer1)  30 Jan 2020"
// (HyperXAlloyEliteController, HyperXAlloyFPSController), "Adam Honse (CalcProgrammer1)
// 02 Feb 2020" (RGBController_HyperXAlloyElite, RGBController_HyperXAlloyFPS), "KundaPanda (vojdo)
// 02 Apr 2021" (Alloy Elite 2), "Adam Honse (CalcProgrammer1)  11 Jul 2020" (Alloy Origins),
// "Derek Huber  18 Mar 2023" (Alloy Origins 60 and 65), "Ricardo Amorim  28 Mar 2026"
// (Origins 2 65); the Eve 1800 files and the detector files name no author ("This file is part of
// the OpenRGB project").
//
// Protocol notes: docs/hardware/hyperx-keyboard.md. Only the Direct (per-LED) reports are sent:
// never OpenRGB's profile commands (Alloy Elite SetMode / SendColor / SelectProfile, which write
// the keyboard's stored profile), never the brightness commands (Eve 1800 0x40, Origins Core
// 0xA7), never anything that is not lighting.

import { LAYOUTS } from "./layouts.js";

const NO_PAGE = null;

// The *ControllerDetect.cpp registrations (the _WIN32 branch where there is one). `iface`,
// `page` and `usage` are the REGISTER_HID_DETECTOR_I / _IP / _IPU arguments; `report` says which
// collection of an interface-only registration takes the reports (OpenRGB opens every
// collection of the interface; Lightloom opens one, so it looks for that one).
export const PRODUCTS = [
  { vid: 0x0951, pid: 0x16BE, name: "HyperX Alloy Elite RGB", variant: "ALLOY_ELITE", iface: 2, page: 0xFF01 },
  { vid: 0x0951, pid: 0x16DC, name: "HyperX Alloy FPS RGB", variant: "ALLOY_FPS", iface: 2, page: 0xFF01 },
  { vid: 0x0951, pid: 0x1711, name: "HyperX Alloy Elite 2", variant: "ALLOY_ELITE_2", iface: 3, page: 0xFF90 },
  { vid: 0x03F0, pid: 0x058F, name: "HyperX Alloy Elite 2 (HP)", variant: "ALLOY_ELITE_2", iface: 3, page: 0xFF90 },
  { vid: 0x0951, pid: 0x1734, name: "HyperX Alloy Origins 60", variant: "ALLOY_ORIGINS_60", iface: 3, page: NO_PAGE },
  { vid: 0x03F0, pid: 0x0C8E, name: "HyperX Alloy Origins 60 (HP)", variant: "ALLOY_ORIGINS_60", iface: 3, page: NO_PAGE },
  { vid: 0x03F0, pid: 0x038F, name: "HyperX Alloy Origins 65 (HP)", variant: "ALLOY_ORIGINS_65", iface: 3, page: NO_PAGE },
  { vid: 0x0951, pid: 0x16E5, name: "HyperX Alloy Origins", variant: "ALLOY_ORIGINS", iface: 3, page: NO_PAGE },
  { vid: 0x03F0, pid: 0x0591, name: "HyperX Alloy Origins (HP)", variant: "ALLOY_ORIGINS", iface: 3, page: 0xFF90 },
  { vid: 0x03F0, pid: 0x08C2, name: "HyperX Eve 1800 (HP)", variant: "EVE_1800", iface: 2, page: NO_PAGE },
  { vid: 0x03F0, pid: 0x0CC2, name: "HyperX Origins 2 65 (HP)", variant: "ORIGINS_2_65", iface: 3, page: NO_PAGE },
];

// The HyperX Alloy Elite / FPS "keys" table: report byte of each of the 106 key LEDs.
const ELITE_KEYS = [
  0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x10, 0x11, 0x12, 0x13, 0x14,
  0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x20, 0x21, 0x22,
  0x23, 0x24, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B, 0x2C, 0x2D, 0x2E, 0x2F, 0x30,
  0x31, 0x32, 0x33, 0x34, 0x37, 0x38, 0x39, 0x3A, 0x3B, 0x3C, 0x3E, 0x3F, 0x41,
  0x44, 0x45, 0x48, 0x49, 0x4A, 0x4B, 0x4C, 0x4D, 0x4E, 0x4F, 0x51, 0x54, 0x55,
  0x58, 0x59, 0x5A, 0x5B, 0x5C, 0x5E, 0x5F, 0x61, 0x64, 0x65, 0x68, 0x69, 0x6A,
  0x6B, 0x6C, 0x6E, 0x6F, 0x74, 0x75, 0x78, 0x79, 0x7A, 0x7B, 0x7C, 0x7D, 0x7E,
  0x7F, 0x81, 0x84, 0x85, 0x88, 0x89, 0x8A, 0x8B, 0x8C, 0x8D, 0x8E, 0x8F, 0x91,
  0x94, 0x95,
];
// Alloy Elite extended report: where the R, G and B of the 22 non-key LEDs (light bar, media
// keys) go.
const EXT_RED = [0x08, 0x48, 0x88, 0x09, 0x89, 0x0A, 0x8A, 0x0B, 0x8B, 0x0C, 0x8C, 0x0D, 0x8D, 0x0E, 0x8F, 0x8E, 0x0F, 0x4F, 0x92, 0x13, 0x93, 0x12];
const EXT_GRN = [0x29, 0x28, 0x78, 0x19, 0x79, 0x1A, 0x7A, 0x1B, 0x7B, 0x1C, 0x7C, 0x1D, 0x7D, 0x1E, 0x6E, 0x7E, 0x1F, 0x6F, 0x82, 0x23, 0x83, 0x22];
const EXT_BLU = [0x39, 0x38, 0x68, 0x3A, 0x69, 0x2A, 0x6A, 0x2B, 0x6B, 0x2C, 0x6C, 0x2D, 0x6D, 0x2E, 0x5E, 0x5D, 0x2F, 0x5F, 0x72, 0x33, 0x73, 0x32];

// HyperXAlloyElite2Controller SKIP_INDICES and HyperXAlloyOriginsController skip_idx: wire slots
// that carry no LED (sent as 0).
const ELITE2_SKIP = [23, 29, 41, 47, 70, 71, 76, 77, 87, 88, 93, 99, 100, 102, 108, 113];
const ORIGINS_SKIP = [23, 29, 41, 47, 59, 70, 71, 87, 88, 93, 99, 100, 102, 108, 113, 114, 120, 123, 124];

// Per variant: the frame writer, and how long an unchanged frame may go unsent. 0 = a frame on
// every Render(): OpenRGB's keepalive threads resend whenever 50 ms passed without an update (the
// keyboards leave direct control otherwise); the Alloy Elite 2's waits 1 s. `report` is the
// report an interface-only registration's collection must declare.
const VARIANTS = {
  ALLOY_ELITE: { frame: alloyEliteFrame, resendMs: 0 },
  ALLOY_FPS: { frame: alloyFpsFrame, resendMs: 0 },
  ALLOY_ELITE_2: { frame: elite2Frame, resendMs: 1000 },
  ALLOY_ORIGINS: { frame: originsFrame, resendMs: 0, report: { kind: "feature", reportId: 0x00, bytes: 65 } },
  ALLOY_ORIGINS_60: { frame: origins6065Frame, resendMs: 0, report: { kind: "feature", reportId: 0x00, bytes: 65 } },
  ALLOY_ORIGINS_65: { frame: origins6065Frame, resendMs: 0, report: { kind: "feature", reportId: 0x00, bytes: 65 } },
  EVE_1800: { frame: eve1800Frame, resendMs: 0, report: { kind: "output", reportId: 0x44, bytes: 65 } },
  ORIGINS_2_65: { frame: origins265Frame, resendMs: 0, report: { kind: "output", reportId: 0x44, bytes: 65 } },
};

export function productFor(vid, pid) {
  return PRODUCTS.find((p) => p.vid === vid && p.pid === pid) ?? null;
}

export function validate(product, endpoint) {
  if (!product || endpoint.interface !== product.iface) {
    return false;
  }
  return product.page === NO_PAGE || endpoint.usage_page === product.page;
}

/** Sets the device's name and LED grid; returns the session state (null for an unknown product). */
export function start(dev, product) {
  if (!product) {
    return null;
  }
  const layout = LAYOUTS[product.variant];
  const variant = VARIANTS[product.variant];
  dev.setName(product.name);
  dev.setSize(layout.size.slice());
  dev.setControllableLeds(layout.leds.map((l) => l[0]), layout.leds.map((l) => [l[2], l[3]]));
  if (product.page === NO_PAGE && variant.report) {
    pickCollection(dev, product.iface, variant.report);
  }
  // OpenRGB's constructors send nothing: the first Direct frame takes the keyboard over.
  return { product, layout, variant, last: null, sentAt: 0 };
}

export function render(dev, st) {
  if (!st) {
    return;
  }
  const colours = st.layout.leds.map((l) => rgb(dev.color(l[2], l[3])));
  const now = Date.now();
  if (st.variant.resendMs > 0 && st.last !== null && same(colours, st.last) && now - st.sentAt < st.variant.resendMs) {
    return;
  }
  st.variant.frame(dev, wire(st.layout, colours));
  st.last = colours;
  st.sentAt = now;
}

/**
 * OpenRGB has no exit step and these keyboards have no "back to the stored profile" command in
 * it (the Alloy Elite's profile commands write the stored profile): one black Direct frame. The
 * keyboards that need a steady stream (OpenRGB's keepalive) then fall back to their own lighting
 * once frames stop coming.
 */
export function shutdown(dev, st) {
  if (!st) {
    return;
  }
  st.variant.frame(dev, wire(st.layout, st.layout.leds.map(() => [0, 0, 0])));
}

// The colours in OpenRGB's LED order (wire order); LEDs OpenRGB names "" (no key) are black.
function wire(layout, colours) {
  const out = [];
  for (let i = 0; i < layout.wireLeds; i++) {
    out.push([0, 0, 0]);
  }
  layout.leds.forEach((l, i) => {
    out[l[1]] = colours[i];
  });
  return out;
}

// ---------------------------------------------------------------------------------------------
// Alloy Elite RGB / Alloy FPS RGB: 264-byte feature reports 07 16 <channel> A0, one per colour
// channel, key LEDs at ELITE_KEYS.
// ---------------------------------------------------------------------------------------------
function eliteDirect(dev, channel, values) {
  const buf = new Array(264).fill(0);
  buf[0x00] = 0x07;
  buf[0x01] = 0x16;
  buf[0x02] = channel;
  buf[0x03] = 0xA0;
  for (let i = 0; i < 106; i++) {
    buf[ELITE_KEYS[i]] = values[i];
  }
  dev.send_report(buf, 264);
}

// HyperXAlloyEliteController::SetLEDsDirect: red, green, blue (5 ms apart), then the extended
// report with the 22 light-bar and media-key LEDs. OpenRGB's ext_color_data is not cleared, so
// the bytes the three tables leave unset carry stack garbage there; Lightloom sends 0.
function alloyEliteFrame(dev, colours) {
  const ext = new Array(150).fill(0);
  for (let i = 0; i < 22; i++) {
    ext[EXT_RED[i]] = colours[i + 106][0];
    ext[EXT_GRN[i]] = colours[i + 106][1];
    ext[EXT_BLU[i]] = colours[i + 106][2];
  }
  eliteDirect(dev, 0x01, colours.slice(0, 106).map((c) => c[0]));
  dev.pause(5);
  eliteDirect(dev, 0x02, colours.slice(0, 106).map((c) => c[1]));
  dev.pause(5);
  eliteDirect(dev, 0x03, colours.slice(0, 106).map((c) => c[2]));
  dev.pause(5);
  const buf = new Array(264).fill(0);
  buf[0x00] = 0x07;
  buf[0x01] = 0x16;
  buf[0x02] = 0x04;
  buf[0x03] = 0xA0;
  for (let i = 0x08; i < 0x94; i++) {
    buf[i] = ext[i];
  }
  dev.send_report(buf, 264);
}

// HyperXAlloyFPSController::SetLEDsDirect: red, green, blue, 10 ms apart.
function alloyFpsFrame(dev, colours) {
  eliteDirect(dev, 0x01, colours.map((c) => c[0]));
  dev.pause(10);
  eliteDirect(dev, 0x02, colours.map((c) => c[1]));
  dev.pause(10);
  eliteDirect(dev, 0x03, colours.map((c) => c[2]));
}

// ---------------------------------------------------------------------------------------------
// 65-byte feature reports: 00 04 F2 ... start, then groups of 81 R G B, 16 per report.
// ---------------------------------------------------------------------------------------------
function directStart(byte9) {
  const buf = new Array(65).fill(0);
  buf[0x01] = 0x04;
  buf[0x02] = 0xF2;
  buf[0x09] = byte9;
  return buf;
}

// HyperXAlloyElite2Controller::SetLEDsDirect: the start report, then the colours in groups of
// four (81 R G B) with an empty group at every SKIP index, 16 groups per report, then empty
// reports up to 9 in all.
function elite2Frame(dev, colours) {
  dev.send_report(directStart(0x00), 65);
  let buf = new Array(65).fill(0);
  let bufIdx = 1;
  let colorIdx = 0;
  let packets = 0;
  let skipped = 0;
  let skipPtr = 0;
  while (colorIdx < colours.length) {
    buf[bufIdx] = 0x81;
    if (ELITE2_SKIP[skipPtr] === colorIdx + skipped) {
      buf[bufIdx + 1] = 0;
      buf[bufIdx + 2] = 0;
      buf[bufIdx + 3] = 0;
      skipPtr++;
      if (skipPtr >= ELITE2_SKIP.length) {
        skipPtr = 0;
      }
      skipped++;
    } else {
      const [r, g, b] = colours[colorIdx];
      buf[bufIdx + 1] = r;
      buf[bufIdx + 2] = g;
      buf[bufIdx + 3] = b;
      colorIdx++;
    }
    bufIdx += 4;
    if (bufIdx >= 65 || colorIdx === colours.length) {
      dev.send_report(buf, 65);
      buf = new Array(65).fill(0);
      bufIdx = 1;
      packets++;
    }
  }
  for (let extra = 0; packets + extra < 9; extra++) {
    dev.send_report(buf, 65);
  }
}

function groupsReport(colours) {
  const buf = new Array(65).fill(0);
  const n = Math.min(colours.length, 16);
  for (let i = 0; i < n; i++) {
    buf[i * 4 + 1] = 0x81;
    buf[i * 4 + 2] = colours[i][0];
    buf[i * 4 + 3] = colours[i][1];
    buf[i * 4 + 4] = colours[i][2];
  }
  return buf;
}

// SetLEDsDirect of HyperXAlloyOriginsController (9 reports) and
// HyperXAlloyOrigins60and65Controller (5 reports): 16 colours per report; a report with nothing
// left to send carries one black group.
function groupsFrame(dev, colours, byte9, reports) {
  dev.send_report(directStart(byte9), 65);
  let sent = 0;
  for (let p = 0; p < reports; p++) {
    const left = colours.length - sent;
    if (left > 0) {
      const n = Math.min(left, 16);
      dev.send_report(groupsReport(colours.slice(sent, sent + n)), 65);
      sent += n;
    } else {
      dev.send_report(groupsReport([[0, 0, 0]]), 65);
    }
  }
}

// The Alloy Origins inserts a black colour at every skip index first (each insert shifts the
// rest, as std::vector::insert does).
function originsFrame(dev, colours) {
  const withBlanks = colours.slice();
  for (const idx of ORIGINS_SKIP) {
    withBlanks.splice(idx, 0, [0, 0, 0]);
  }
  groupsFrame(dev, withBlanks, 0x09, 9);
}

function origins6065Frame(dev, colours) {
  groupsFrame(dev, colours, 0x05, 5);
}

// ---------------------------------------------------------------------------------------------
// 65-byte output reports with report id 0x44: 44 01 04 start, then 44 02 <seq> 00 and R G B
// triplets, 20 per report.
// ---------------------------------------------------------------------------------------------
function triplets(seq, colours) {
  const buf = new Array(65).fill(0);
  buf[0x00] = 0x44;
  buf[0x01] = 0x02;
  buf[0x02] = seq;
  const n = Math.min(colours.length, 20);
  for (let i = 0; i < n; i++) {
    buf[4 + i * 3] = colours[i][0];
    buf[4 + i * 3 + 1] = colours[i][1];
    buf[4 + i * 3 + 2] = colours[i][2];
  }
  return buf;
}

function start44() {
  const buf = new Array(65).fill(0);
  buf[0x00] = 0x44;
  buf[0x01] = 0x01;
  buf[0x02] = 0x04;
  return buf;
}

// HyperXEve1800Controller::SetLEDsDirect: the start report and one report with the 10 zones.
function eve1800Frame(dev, colours) {
  dev.write(start44(), 65);
  dev.write(triplets(0x00, colours.length > 0 ? colours : [[0, 0, 0]]), 65);
}

// HyperXOrigins2_65Controller::SetLEDsDirect: the start report and four numbered reports.
function origins265Frame(dev, colours) {
  dev.write(start44(), 65);
  let sent = 0;
  for (let p = 0; p < 4; p++) {
    const left = colours.length - sent;
    if (left > 0) {
      const n = Math.min(left, 20);
      dev.write(triplets(p, colours.slice(sent, sent + n)), 65);
      sent += n;
    } else {
      dev.write(triplets(p, [[0, 0, 0]]), 65);
    }
  }
}

// ---------------------------------------------------------------------------------------------

/**
 * OpenRGB's REGISTER_HID_DETECTOR_I opens every collection of the interface and the one that
 * takes the reports drives the keyboard. Lightloom opens one: when the interface has several
 * collections, make active the first whose report descriptor declares `report` (Lightloom's
 * getHidReportFields()); otherwise leave the host's choice.
 */
function pickCollection(dev, iface, report) {
  if (typeof dev.getHidEndpoints !== "function" || typeof dev.set_endpoint !== "function"
    || typeof dev.getHidReportFields !== "function") {
    return;
  }
  const candidates = (dev.getHidEndpoints() || []).filter((e) => e.interface === iface);
  if (candidates.length < 2) {
    return;
  }
  for (const e of candidates) {
    dev.set_endpoint(e.interface, e.usage, e.usage_page, e.collection);
    const fields = dev.getHidReportFields();
    const reports = fields && Array.isArray(fields.reports) ? fields.reports : [];
    if (reports.some((r) => r.kind === report.kind && r.reportId === report.reportId && r.bytes >= report.bytes)) {
      return;
    }
  }
  const first = candidates[0];
  dev.set_endpoint(first.interface, first.usage, first.usage_page, first.collection);
}

function rgb(c) {
  return c && c.length >= 3 ? [c[0] & 0xFF, c[1] & 0xFF, c[2] & 0xFF] : [0, 0, 0];
}

function same(a, b) {
  for (let i = 0; i < a.length; i++) {
    if (a[i][0] !== b[i][0] || a[i][1] !== b[i][1] || a[i][2] !== b[i][2]) {
      return false;
    }
  }
  return true;
}
