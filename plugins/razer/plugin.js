// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: Razer devices with the 90-byte Razer report (keyboards, laptops, mice, keypads,
// mousemats, headsets, speakers and accessories, and the devices paired to a Razer wireless
// receiver), the Razer Chroma Addressable RGB Controller and the Kraken V4 family.
// Adapted from OpenRGB Controllers/RazerController/RazerControllerDetect.cpp,
// RazerController/RazerController.cpp and .h, RazerController/RGBController_Razer.cpp and .h,
// RazerController/RGBController_RazerAddressable.cpp and .h, RazerKrakenV4Controller/
// RazerKrakenV4Controller.cpp and .h, RazerKrakenV4Controller/RGBController_RazerKrakenV4.cpp and
// .h, RazerDevices.cpp and .h (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Adam Honse (CalcProgrammer1)  22 Jan 2021" (RazerControllerDetect.cpp,
// RazerController, RGBController_Razer); "Adam Honse (CalcProgrammer1)  11 Apr 2021"
// (RGBController_RazerAddressable); "Adam Honse (CalcProgrammer1)  21 Oct 2025"
// (RazerKrakenV4Controller, RGBController_RazerKrakenV4); "Adam Honse (CalcProgrammer1)
// 04 Sep 2021" (RazerDevices). The product tables in devices.js are generated from OpenRGB's
// tables; the protocol below is written by hand. The DeathAdder V2 Pro (1532:007C/007D) keeps the
// packets of Lightloom's own recording (see RECORDED_PROFILE below), which is not OpenRGB code.
// Tested on hardware (2026-10-22): DeathAdder V2 Pro wired (1532:007C) and wireless through the
// dock (1532:007D), Razer Mouse Dock Chroma (1532:007E), with Razer Synapse 4 (RazerAppEngine.exe,
// reported as a conflicting process) closed.
//
// Protocol notes: docs/hardware/razer.md. In short: 91-byte feature reports (report id 0x00, or
// 0x07 on the Leviathan V2 / V2 X) with transaction id, data size, command class and id, 80
// argument bytes and an XOR checksum. Per frame one custom-frame report per matrix row (standard
// 0x03 0x0B, extended 0x0F 0x03, linear 0x03 0x0C, or per-LED reports on a few old devices),
// then the "custom frame" effect (0x03 0x0A / 0x0F 0x02). The ARGB controller takes 321-byte
// feature reports on its second interface; the Kraken V4 family takes 64-byte output reports.
// Never sent: DPI, polling rate, battery or power settings, and the "save" storage flag outside
// SaveToDevice() (Lightloom's "Save to devices", after the user confirmed it; extended-matrix
// devices only: docs/hardware/razer.md, "Save to devices").
// Products with a battery (battery.js) are asked for their battery level and charging status
// (read-only queries 0x07 0x80 / 0x07 0x84, facts from OpenRazer) every 45 seconds.

/* global device, battery */

import * as rz from "./razer-protocol.js";
import {
  DETECTORS, DEVICES, KEYMAPS, REPORT_INDEX_7, SOFTWARE_MODE_AT_START, LED_ID_ZERO, KEEPALIVE,
  MAX_BRIGHTNESS_100, KRAKEN_V4_WIRELESS, VENDOR_ID, STANDARD, EXTENDED, LINEAR, EXTENDED_ARGB, CUSTOM,
} from "./devices.js";
import { BATTERY } from "./battery.js";

const PRODUCT_IDS = [...new Set(DETECTORS.map((d) => d[1]))];

// Product ids of OpenRGB's RAZER_MATRIX_TYPE_CUSTOM devices (RazerController.cpp, the nested
// switch(dev_pid) blocks of razer_set_custom_frame() and friends).
const TARTARUS_CHROMA = 0x0208;
const DEATHADDER_CHROMA = 0x0043;
const NAGA_EPIC_CHROMA = 0x003E;
const MAMBA_2012_WIRED = 0x0024;
const MAMBA_2012_WIRELESS = 0x0025;
const ORBWEAVER_CHROMA = 0x0207;

const ARGB_CONTROLLER = 0x0F1F;
const ARGB_LEDS_PER_CHANNEL = 80;

/** RazerController.h RAZER_MAX_PAIRED_DEVICES. */
const MAX_PAIRED = 8;
/** Unchanged frames are sent again after this long. */
const RESEND_MS = 1000;
/** KeepaliveThreadFunction(): a device-mode request every second (last_update_time is never set). */
const KEEPALIVE_MS = 1000;

const LAMPARRAY_USAGE_PAGE = 0x59;

/** Battery queries: the first one this long after Initialize(), then every BATTERY_POLL_MS. */
const BATTERY_FIRST_MS = 2000;
const BATTERY_POLL_MS = 45000;
/** Reads of the answer to a battery query, 2 ms apart (OpenRazer reads once, about 1 ms after). */
const BATTERY_READS = 10;
/** SignalRGB battery states (docs/host-api/hid-usb.md section 7). */
const BATTERY_UNKNOWN = 0;
const BATTERY_DRAINING = 1;
const BATTERY_CHARGING = 2;
const BATTERY_FULL = 4;

// Lightloom's own recording of the DeathAdder V2 Pro (docs/hardware/razer-deathadder-v2-pro.md,
// the notes of the retired single-device plugin): the wired mouse and its receiver keep exactly
// the packets of that recording (tested on 007C and 007D, 2026-10-22), which the mouse is known to accept, instead of OpenRGB's (transaction id 0x3F,
// LED id 0x05, effect report after every frame). See docs/hardware/razer.md.
const RECORDED_PROFILE = new Set([0x007C, 0x007D]);
const RECORDED_TRANSACTION_IDS = [0x1F, 0x3F, 0x9F, 0xFF];

// Placeholder geometry for the scan; Initialize() sets the product's own.
export function Name() { return "Razer"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/razer"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return PRODUCT_IDS.slice(); }
export function Type() { return "Hid"; }
export function DeviceType() { return "other"; }
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ["LED 1"]; }
export function LedPositions() { return [[0, 0]]; }
export function ImageUrl() { return ""; }
// Razer's own software drives the lights while it runs; Lightloom reports it, never closes it.
export function ConflictingProcesses() { return ["RazerAppEngine.exe", "RazerCentralService.exe", "Razer Synapse 3.exe", "Razer Synapse.exe"]; }
// Lightloom extensions (docs/host-api/save-shutdown.md).
export function SaveCapabilities() { return { static: true, effects: ["spectrum"] }; }
export function ShutdownModes() { return ["restore", "keep", "off"]; }

let main = null;          // the opened device (a "target", see makeTarget())
let paired = [];          // devices paired to a receiver, each a target with a subdevice
let protocol = null;      // "razer" | "razer-argb" | "kraken-v4" | "recorded"
let argbEndpoints = null; // { report: endpoint, argb: endpoint } for the ARGB controller
let argbSizes = null;     // channel LED counts last sent with the zone-size report
let lastKeepaliveAt = 0;
let power = null;         // battery polling state, see batterySetup()

// --- detection ---------------------------------------------------------------------------------

/**
 * REGISTER_HID_DETECTOR_IPU / _I / _PU rows of the product (devices.js DETECTORS). The ARGB
 * controller's detector names interface 0; OpenRGB opens interface 1 next to it for the ARGB
 * frames, so that interface is accepted too. LampArray collections (usage page 0x59) are left to
 * Lightloom's generic LampArray plugin.
 */
export function Validate(endpoint) {
  if (endpoint.usage_page === LAMPARRAY_USAGE_PAGE) {
    return false;
  }
  const pid = device.productId();
  if (pid === ARGB_CONTROLLER) {
    return endpoint.interface === 0 || endpoint.interface === 1;
  }
  return DETECTORS.some((row) => row[1] === pid && matches(row, endpoint));
}

function matches(row, endpoint) {
  const [, , , iface, page, usage] = row;
  return (iface === -1 || endpoint.interface === iface)
    && (page === -1 || endpoint.usage_page === page)
    && (usage === -1 || endpoint.usage === usage);
}

// --- lifecycle ---------------------------------------------------------------------------------

export function Initialize() {
  main = null;
  paired = [];
  protocol = null;
  argbEndpoints = null;
  argbSizes = null;
  lastKeepaliveAt = 0;
  power = null;

  const pid = device.productId();
  const rows = DETECTORS.filter((row) => row[1] === pid);
  const entry = DEVICES[pid];
  if (rows.length === 0 || !entry) {
    device.log(`Razer: product id 0x${hex4(pid)} is not one this plugin knows.`);
    return;
  }
  const name = detectorName(rows);
  protocol = RECORDED_PROFILE.has(pid) ? "recorded" : rows[0][2];
  main = makeTarget(pid, name);
  device.setName(name);
  if (protocol === "razer-argb") {
    argbInitialize();
    return;
  }
  device.setSize([main.layout.width, main.layout.height]);
  device.setControllableLeds(main.layout.ledNames, main.layout.ledPositions);

  if (protocol === "recorded") {
    recordedInitialize();
    batterySetup([main]);
    return;
  }
  if (protocol === "kraken-v4") {
    // RGBController_RazerKrakenV4: Direct mode sends nothing (DeviceUpdateMode only handles Wave).
    return;
  }

  // An interface-only detector line (Tiamat 7.1 V2), or several detector lines for one product
  // (BlackWidow V4 / V4 Pro, firmware before and after 1.5): use the collection with the 91-byte
  // report among the collections the detector lines match.
  if (rows.length > 1 || rows.every((row) => row[4] === -1)) {
    useEndpoint(findFeatureEndpoint(rows[0][3], rz.REPORT_LENGTH, rows));
  }
  // RazerController::RazerController(): software device mode for a few products; then
  // RGBController_Razer's constructor (SetupZones(): the keyboard layout).
  if (SOFTWARE_MODE_AT_START.includes(pid)) {
    usbSend(main, rz.deviceMode(main.ctx, rz.DEVICE_MODE_SOFTWARE, 0x00));
  }
  applyKeyboardLayout(main);
  // DetectRazerControllers(): a receiver lists its paired devices; each one known to the device
  // table gets its own controller on the same handle, with its own transaction id.
  for (const pairedPid of getPairedDevicePids(main)) {
    if (pairedPid === pid || !DEVICES[pairedPid]) {
      continue;
    }
    const target = makeTarget(pairedPid, `${DEVICES[pairedPid].name} (${entry.name})`, `paired-${hex4(pairedPid)}`);
    if (SOFTWARE_MODE_AT_START.includes(pairedPid)) {
      usbSend(target, rz.deviceMode(target.ctx, rz.DEVICE_MODE_SOFTWARE, 0x00));
    }
    target.waitForResponse = true;   // paired_controller->SetWaitForResponse(true)
    paired.push(target);
    device.createSubdevice(target.subdeviceId);
    device.setSubdeviceName(target.subdeviceId, target.name);
    device.setSubdeviceSize(target.subdeviceId, target.layout.width, target.layout.height);
    device.setSubdeviceLeds(target.subdeviceId, target.layout.ledNames, target.layout.ledPositions);
    applyKeyboardLayout(target);
  }
  for (const target of [main, ...paired]) {
    // Switching to Direct: RGBController_Razer::DeviceUpdateMode() sends no effect for Direct and
    // then SetBrightness(brightness), which is the maximum by default. Lightloom's colours already
    // carry the user's brightness.
    setBrightness(target, target.maxBrightness);
  }
  batterySetup([main, ...paired]);
}

export function Render() {
  if (!main) {
    return;
  }
  switch (protocol) {
    case "recorded":
      recordedRender();
      batteryPoll();
      return;
    case "kraken-v4":
      renderTarget(main, (colours) => krakenV4SetDirect(main, colours));
      return;
    case "razer-argb":
      argbRender();
      return;
    default:
      for (const target of [main, ...paired]) {
        renderTarget(target, (colours) => setLeds(target, colours));
      }
      keepalive();
      batteryPoll();
  }
}

/**
 * OpenRGB has no exit step. Lightloom hands the lights back to a hardware effect: the spectrum
 * cycle (RazerController::SetModeSpectrumCycle() at full brightness), the wave on the Kraken V4
 * family (SetModeWave()), and the products OpenRGB switches to software mode go back to hardware
 * mode. The DeathAdder V2 Pro keeps its recorded shutdown (black frame, hardware mode).
 * After a "Save to devices" in this session, a target that stored something shows it again
 * instead of the spectrum cycle: the same effect and brightness reports, with storage 0x00.
 */
export function Shutdown(_systemSuspending, mode) {
  if (!main) {
    return;
  }
  if (mode === "keep" || mode === "off") {
    // Lightloom exit modes: the last frame (or a black one) stays on the device, which is left in
    // its custom-frame / software mode; nothing is stored.
    for (const target of protocol === "razer-argb" || protocol === "kraken-v4" || protocol === "recorded" ? [main] : [main, ...paired]) {
      const colours = mode === "keep" && target.lastColours
        ? target.lastColours
        : framePixels(target, mode === "keep");
      sendFrame(target, colours);
    }
    return;
  }
  switch (protocol) {
    case "recorded":
      recordedShutdown();
      if (main.stored) {
        showStored(main);
      }
      return;
    case "kraken-v4":
      write(rz.krakenV4Wave(main.ctx), rz.KRAKEN_V4_REPORT_LENGTH);
      write(rz.krakenV4Brightness(main.ctx, 255), rz.KRAKEN_V4_REPORT_LENGTH);
      return;
    case "razer-argb":
      useEndpoint(argbEndpoints?.report);
      setModeSpectrumCycle(main);
      setBrightness(main, main.maxBrightness);
      return;
    default:
      for (const target of [main, ...paired]) {
        if (target.stored) {
          showStored(target);
        } else {
          setModeSpectrumCycle(target);
          setBrightness(target, target.maxBrightness);
        }
        if (SOFTWARE_MODE_AT_START.includes(target.pid)) {
          usbSend(target, rz.deviceMode(target.ctx, rz.DEVICE_MODE_HARDWARE, 0x00));
        }
      }
  }
}

// --- "Save to devices" (Lightloom extension) ------------------------------------------------------

/**
 * Store a static colour (the average of the device's LEDs on the frame shown) or the spectrum
 * cycle in the device itself, with OpenRazer's VARSTORE storage flag: the extended-matrix static
 * or spectrum effect report (razer_create_mode_static/spectrum_cycle_extended_matrix_report) and
 * the extended brightness report at full brightness, each with storage 0x01 instead of OpenRGB's
 * 0x00, for the device and each extended-matrix device paired to it. Standard-matrix effect
 * reports carry no storage byte, so those devices (and the ARGB controller and Kraken V4) are not
 * offered. Afterwards the device goes back to custom frames for the live effect. A preview only
 * describes it.
 */
export function SaveToDevice(request) {
  if (!main) {
    return { unsupported: "the device was not recognised" };
  }
  const effect = request && request.effect ? String(request.effect) : null;
  if (effect !== null && effect !== "spectrum") {
    return { unsupported: `the device has no "${effect}" effect Lightloom can store` };
  }
  const targets = saveTargets();
  if (targets.length === 0) {
    return { unsupported: "only Razer devices with the extended matrix protocol can store an effect (standard-matrix effect reports have no storage flag)" };
  }
  const plan = targets.map((target) => ({ target, colour: averageColour(framePixels(target, true)) }));
  const result = {
    effect: effect ?? "static",
    zones: effect === null ? plan.map(({ target, colour }) => ({ name: target.name, color: hex(colour) })) : [],
    note: effect === null ? "One colour for each device (the average of its LEDs)." : "The device's own colour cycle.",
  };
  if (!request || request.action !== "save") {
    return result;
  }
  for (const { target, colour } of plan) {
    const ledId = saveLedId(target);
    const effectReport = effect === "spectrum"
      ? rz.spectrumExtended(target.ctx, rz.STORAGE_SAVE, ledId)
      : rz.staticExtended(target.ctx, rz.STORAGE_SAVE, ledId, colour);
    if (usbSend(target, effectReport) < 0) {
      throw new Error(`Razer: storing the effect on ${target.name} failed`);
    }
    usbSend(target, rz.brightnessExtended(target.ctx, rz.STORAGE_SAVE, ledId, target.maxBrightness));
    target.lastColours = null;
    target.stored = { effect: effect ?? "static", colour };
  }
  // Back to custom frames: the next Render() sends the frame and the custom-frame effect again
  // (the recorded DeathAdder V2 Pro needs its frame-mode request first).
  if (protocol === "recorded") {
    recordedRequest(0x0F, 0x02, [0x00, 0x00, 0x08, 0x00, 0x01, 0x00]);
  }
  return result;
}

/**
 * Shutdown(false, "restore") after a save: what the target stored, set live (the effect report and
 * the brightness report SaveToDevice() sent, with storage 0x00, so nothing is written again).
 */
function showStored(target) {
  const ledId = saveLedId(target);
  usbSend(target, target.stored.effect === "spectrum"
    ? rz.spectrumExtended(target.ctx, rz.STORAGE_NO_SAVE, ledId)
    : rz.staticExtended(target.ctx, rz.STORAGE_NO_SAVE, ledId, target.stored.colour));
  usbSend(target, rz.brightnessExtended(target.ctx, rz.STORAGE_NO_SAVE, ledId, target.maxBrightness));
}

/** The targets whose effect reports carry a storage byte (extended matrix). */
function saveTargets() {
  if (protocol !== "razer" && protocol !== "recorded") {
    return [];
  }
  return [main, ...paired].filter((t) => t.matrix === EXTENDED);
}

/** The LED id of a stored effect: the target's own (OpenRGB's dev_led_id); the recorded
 * DeathAdder V2 Pro uses 0x00, the LED id of its recorded effect report. */
function saveLedId(target) {
  return protocol === "recorded" && target === main ? rz.LED_ID_ZERO : target.ledId;
}

/** A target's colours (OpenRGB order): the frame shown when `keep`, black otherwise. */
function framePixels(target, keep) {
  if (protocol === "razer-argb") {
    const n = main.entry.rows * ARGB_LEDS_PER_CHANNEL;
    return keep && main.lastColours ? main.lastColours : new Array(n).fill([0, 0, 0]);
  }
  const colours = coloursOf(target);
  return keep ? colours : colours.map(() => [0, 0, 0]);
}

/** Send one frame through the target's own protocol (as Render() does). */
function sendFrame(target, colours) {
  switch (protocol) {
    case "recorded": {
      const [r, g, b] = colours[0] ?? [0, 0, 0];
      recordedSend(0x0F, 0x03, [0x00, 0x00, 0x00, 0x00, 0x00, r, g, b]);
      return;
    }
    case "kraken-v4":
      krakenV4SetDirect(target, colours);
      return;
    case "razer-argb":
      if (argbEndpoints) {
        setLeds(main, colours);
      }
      return;
    default:
      setLeds(target, colours);
  }
}

function averageColour(colours) {
  const lit = colours.length > 0 ? colours : [[0, 0, 0]];
  const sum = [0, 0, 0];
  for (const c of lit) {
    for (let i = 0; i < 3; i++) {
      sum[i] += c[i] & 0xFF;
    }
  }
  return sum.map((s) => Math.round(s / lit.length));
}

function hex(rgb) {
  return `#${rgb.map((v) => (v & 0xFF).toString(16).padStart(2, "0")).join("")}`;
}

// --- targets and geometry -----------------------------------------------------------------------

/** One OpenRGB RazerController: the product's table entry, protocol settings and LED grid. */
function makeTarget(pid, name, subdeviceId = null) {
  const entry = DEVICES[pid];
  return {
    pid,
    name,
    entry,
    subdeviceId,
    matrix: entry.matrix,
    ctx: {
      reportIndex: REPORT_INDEX_7.includes(pid) ? 0x07 : 0x00,
      transactionId: entry.transactionId,
      wirelessFlag: KRAKEN_V4_WIRELESS.includes(pid) ? 0x80 : 0x00,
    },
    ledId: LED_ID_ZERO.includes(pid) ? rz.LED_ID_ZERO : rz.LED_ID_BACKLIGHT,
    maxBrightness: MAX_BRIGHTNESS_100.includes(pid) ? 100 : 255,
    waitForResponse: false,
    layout: layoutOf(entry, "ansi"),
    lastColours: null,
    lastSentAt: 0,
    // What SaveToDevice() stored in this session ({ effect: "static" | "spectrum", colour }), shown
    // again by Shutdown(false, "restore"); null: nothing stored since Initialize().
    stored: null,
  };
}

/** The detector name OpenRGB gives the device (for 0x0F21, the row whose collection exists). */
function detectorName(rows) {
  if (rows.length > 1 && typeof device.getHidEndpoints === "function") {
    const endpoints = device.getHidEndpoints() || [];
    const found = rows.find((row) => endpoints.some((e) => matches(row, e)));
    if (found) {
      return found[0];
    }
  }
  return rows[0][0];
}

/**
 * The LED grid of a table entry. Zones are stacked top to bottom in table order; a zone's LED
 * (row, col) sits at [col, zone offset + row]. `cells[k]` is the place of OpenRGB's LED k (zone
 * order, row by row), the colour index SetLEDs() reads. Keyboard zones name their cells with the
 * KeyboardLayoutManager names of the layout; cells without a key are sampled but not listed.
 */
function layoutOf(entry, variant) {
  const cells = [];
  const ledNames = [];
  const ledPositions = [];
  let width = 1;
  let y = 0;
  for (const zone of entry.zones) {
    const names = zone.keymap ? keymapNames(zone.keymap, variant) : null;
    const count = zone.rows * zone.cols;
    width = Math.max(width, zone.cols);
    for (let r = 0; r < zone.rows; r++) {
      for (let c = 0; c < zone.cols; c++) {
        const cell = [c, y + r];
        cells.push(cell);
        let ledName;
        if (names) {
          ledName = names[r][c];
          if (!ledName) {
            continue;
          }
        } else if (count === 1) {
          ledName = zone.name;
        } else if (zone.rows === 1) {
          ledName = `${zone.name} LED ${c + 1}`;
        } else {
          ledName = `${zone.name} LED ${r * zone.cols + c + 1}`;
        }
        ledNames.push(ledName);
        ledPositions.push(cell.slice());
      }
    }
    y += zone.rows;
  }
  return { width, height: Math.max(1, y), cells, ledNames, ledPositions };
}

function keymapNames(id, variant) {
  const keymap = KEYMAPS[id];
  const names = keymap.ansi.map((row) => row.slice());
  if (variant !== "ansi") {
    for (const [r, c, name] of keymap[variant]) {
      names[r][c] = name;
    }
  }
  return names;
}

/**
 * RGBController_Razer::SetupZones() -> GetKeyboardLayoutType(): asks the keyboard for its layout
 * (0x00 0x86) and renames the keys. Only targets with a keyboard zone ask.
 */
function applyKeyboardLayout(target) {
  if (!target.entry.zones.some((z) => z.keymap)) {
    return;
  }
  const variant = layoutVariant(getKeyboardLayout(target));
  if (variant === "ansi") {
    return;
  }
  target.layout = layoutOf(target.entry, variant);
  if (target.subdeviceId) {
    device.setSubdeviceLeds(target.subdeviceId, target.layout.ledNames, target.layout.ledPositions);
  } else {
    device.setControllableLeds(target.layout.ledNames, target.layout.ledPositions);
  }
}

/** GetKeyboardLayoutType() and the KEYBOARD_LAYOUT choice of SetupZones(). */
function layoutVariant(layout) {
  switch (layout) {
    case 2: case 6: case 7: case 9: case 13: case 14: case 15: case 16: case 17: case 18: case 19:
      return "iso";
    case 4:
      return "azerty";
    case 3:
      return "qwertz";
    default:            // US, Russian, CHT, Turkish, Thai, Arabic (ANSI), Japan (JIS), unknown
      return "ansi";
  }
}

/** The colours of a target's grid, OpenRGB's LED order (black past the zones' LEDs). */
function coloursOf(target) {
  const total = target.entry.rows * target.entry.cols;
  const colours = new Array(total);
  for (let k = 0; k < total; k++) {
    const cell = target.layout.cells[k];
    colours[k] = cell ? sample(target, cell[0], cell[1]) : [0, 0, 0];
  }
  return colours;
}

function sample(target, x, y) {
  const c = target.subdeviceId ? device.subdeviceColor(target.subdeviceId, x, y) : device.color(x, y);
  return [c[0] & 0xFF, c[1] & 0xFF, c[2] & 0xFF];
}

/** Send a target's frame when it changed, or again after RESEND_MS. */
function renderTarget(target, send) {
  const colours = coloursOf(target);
  const now = Date.now();
  if (target.lastColours && sameColours(colours, target.lastColours) && now - target.lastSentAt < RESEND_MS) {
    return;
  }
  send(colours);
  target.lastColours = colours;
  target.lastSentAt = now;
}

function keepalive() {
  if (!KEEPALIVE.includes(main.pid)) {
    return;
  }
  const now = Date.now();
  if (now - lastKeepaliveAt < KEEPALIVE_MS) {
    return;
  }
  lastKeepaliveAt = now;
  // razer_get_device_mode(): 2 ms, request, 5 ms, read the answer (ignored).
  device.pause(2);
  usbSend(main, rz.getDeviceMode(main.ctx));
  device.pause(5);
  device.get_report([main.ctx.reportIndex], rz.REPORT_LENGTH);
}

// --- battery ---------------------------------------------------------------------------------------

/**
 * The first target (the opened device, then the paired ones) whose product has a battery gets the
 * "battery" feature; one battery per Lightloom device. Nothing is sent here.
 */
function batterySetup(targets) {
  const target = targets.find((t) => BATTERY[t.pid]);
  if (!target) {
    return;
  }
  const info = BATTERY[target.pid];
  if (typeof device.addFeature === "function") {
    device.addFeature("battery");
  }
  power = {
    target,
    chargingStatus: info.chargingStatus,
    ctx: { reportIndex: target.ctx.reportIndex, transactionId: info.transactionId },
    nextAt: Date.now() + BATTERY_FIRST_MS,
    step: "level",
    level: null,        // last level read, percent
    reported: null,     // { level, state } last handed to the host
    silent: false,      // "not answering" already logged
  };
}

/**
 * Called after the frame: on a poll tick one query (the level; the charging status on the next
 * frame), each a single request and at most BATTERY_READS short reads.
 */
function batteryPoll() {
  if (!power) {
    return;
  }
  const now = Date.now();
  if (now < power.nextAt) {
    return;
  }
  if (power.step === "level") {
    const answer = batteryQuery(rz.getBatteryLevel(power.ctx));
    if (!answer.ok) {
      batteryUnanswered(answer, now);
      return;
    }
    power.level = Math.min(100, Math.max(0, Math.round((rz.arg(answer.response, 1) * 100) / 255)));
    if (power.chargingStatus) {
      power.step = "charging";
      power.nextAt = now;           // next frame
      return;
    }
    batteryReport(power.level, BATTERY_DRAINING);
    power.nextAt = now + BATTERY_POLL_MS;
    return;
  }
  power.step = "level";
  power.nextAt = now + BATTERY_POLL_MS;
  const answer = batteryQuery(rz.getChargingStatus(power.ctx));
  if (!answer.ok) {
    batteryUnanswered(answer, now);
    return;
  }
  const charging = rz.arg(answer.response, 1) === 1;
  let state = BATTERY_DRAINING;
  if (charging) {
    state = power.level >= 100 ? BATTERY_FULL : BATTERY_CHARGING;
  }
  batteryReport(power.level, state);
}

/** One battery request and its answer: { ok, response } or { ok: false, status }. */
function batteryQuery(request) {
  if (device.send_report(request, rz.REPORT_LENGTH) < 0) {
    return { ok: false, status: null };
  }
  for (let read = 0; read < BATTERY_READS; read++) {
    device.pause(2);
    const response = device.get_report([power.ctx.reportIndex], rz.REPORT_LENGTH);
    if (!response || response.length < 11) {
      continue;
    }
    if (response[2] !== request[2] || response[7] !== request[7] || response[8] !== request[8]) {
      continue;
    }
    if (response[1] === rz.STATUS_SUCCESS) {
      return { ok: true, response };
    }
    if (response[1] !== rz.STATUS_NEW && response[1] !== rz.STATUS_BUSY) {
      return { ok: false, status: response[1] };
    }
  }
  return { ok: false, status: null };
}

/**
 * No usable answer (asleep, out of range, switched off, or garbage): the level stays, the state
 * becomes unknown until the next answer; logged once. "Not supported" stops the polling.
 */
function batteryUnanswered(answer, now) {
  power.step = "level";
  power.nextAt = now + BATTERY_POLL_MS;
  if (answer.status === rz.STATUS_NOT_SUPPORTED) {
    device.log(`${power.target.name}: the battery query is not supported; battery polling stopped.`);
    power = null;
    return;
  }
  if (!power.silent) {
    device.log(`${power.target.name}: no battery answer (device asleep or off?); state unknown until it answers.`);
    power.silent = true;
  }
  if (power.reported) {
    batteryReport(power.reported.level, BATTERY_UNKNOWN);
  }
}

/** Hand level and state to the host when either changed. */
function batteryReport(level, state) {
  if (state !== BATTERY_UNKNOWN) {
    power.silent = false;
  }
  if (power.reported && power.reported.level === level && power.reported.state === state) {
    return;
  }
  power.reported = { level, state };
  const api = globalThis.battery;
  if (!api) {
    return;
  }
  if (typeof api.setBatteryLevel === "function") {
    api.setBatteryLevel(level);
  }
  if (typeof api.setBatteryState === "function") {
    api.setBatteryState(state);
  }
}

// --- RazerController: frames and effects ------------------------------------------------------------

/** RazerController::SetLEDs(): one custom-frame report per matrix row, then the custom effect. */
function setLeds(target, colours) {
  const { rows, cols } = target.entry;
  for (let row = 0; row < rows; row++) {
    const rgb = [];
    for (let col = 0; col < cols; col++) {
      const c = colours[row * cols + col];
      rgb.push(c[0], c[1], c[2]);
    }
    device.pause(1);
    setCustomFrame(target, row, 0, cols - 1, rgb);
  }
  device.pause(1);
  setModeCustom(target);
}

/** razer_set_custom_frame(). */
function setCustomFrame(target, row, startCol, stopCol, rgb) {
  const ctx = target.ctx;
  switch (target.matrix) {
    case STANDARD:
      usbSend(target, rz.customFrameStandard(ctx, row, startCol, stopCol, rgb));
      break;
    case EXTENDED:
      usbSend(target, rz.customFrameExtended(ctx, row, startCol, stopCol, rgb));
      break;
    case LINEAR:
      usbSend(target, rz.customFrameLinear(ctx, startCol, stopCol, rgb));
      break;
    case EXTENDED_ARGB:
      sendArgb(rz.customFrameArgb(row, stopCol, rgb));
      break;
    case CUSTOM:
      switch (target.pid) {
        case TARTARUS_CHROMA:
          usbSend(target, rz.setLedRgb(ctx, rz.STORAGE_NO_SAVE, rz.LED_ID_BACKLIGHT, rgb.slice(0, 3)));
          break;
        case DEATHADDER_CHROMA:
          usbSend(target, rz.setLedRgb(ctx, rz.STORAGE_NO_SAVE, rz.LED_ID_SCROLL_WHEEL, rgb.slice(0, 3)));
          usbSend(target, rz.setLedRgb(ctx, rz.STORAGE_NO_SAVE, rz.LED_ID_LOGO, rgb.slice(3, 6)));
          break;
        case NAGA_EPIC_CHROMA:
          usbSend(target, rz.setLedRgb(ctx, rz.STORAGE_NO_SAVE, rz.LED_ID_SCROLL_WHEEL, rgb.slice(0, 3)));
          device.pause(1);
          usbSend(target, rz.setLedRgb(ctx, rz.STORAGE_NO_SAVE, rz.LED_ID_BACKLIGHT, rgb.slice(3, 6)));
          break;
        case MAMBA_2012_WIRED:
        case MAMBA_2012_WIRELESS:
          usbSend(target, rz.setLedRgb(ctx, rz.STORAGE_NO_SAVE, rz.LED_ID_SCROLL_WHEEL, rgb.slice(0, 3)));
          break;
        case ORBWEAVER_CHROMA:
          // Presented as 4 x 5; the hardware matrix has a border column and a gap in row 4.
          if (row !== 3) {
            usbSend(target, rz.customFrameStandard(ctx, row + 1, startCol + 1, stopCol + 1, rgb));
          } else {
            const adjusted = new Array(6 * 3).fill(0);
            for (let i = 0; i < 3; i++) adjusted[i] = rgb[i];
            for (let i = 0; i < 12; i++) adjusted[6 + i] = rgb[3 + i];
            usbSend(target, rz.customFrameStandard(ctx, row + 1, startCol + 1, stopCol + 2, adjusted));
          }
          break;
        default:
          break;
      }
      break;
    default:
      break;
  }
}

/** razer_set_mode_custom(). */
function setModeCustom(target) {
  const ctx = target.ctx;
  switch (target.matrix) {
    case STANDARD:
    case LINEAR:
      usbSend(target, rz.modeCustomStandard(ctx, rz.STORAGE_NO_SAVE));
      break;
    case EXTENDED:
      usbSend(target, rz.modeCustomExtended(ctx));
      break;
    case CUSTOM:
      ledEffects(target, 0);
      if (target.pid === ORBWEAVER_CHROMA) {
        usbSend(target, rz.modeCustomStandard(ctx, rz.STORAGE_NO_SAVE));
      }
      break;
    default:            // EXTENDED_ARGB, NONE: nothing
      break;
  }
}

/** The per-LED effect reports of the CUSTOM devices (effect 0: custom/static, 4: spectrum). */
function ledEffects(target, effect) {
  const ctx = target.ctx;
  switch (target.pid) {
    case TARTARUS_CHROMA:
      usbSend(target, rz.setLedEffect(ctx, rz.STORAGE_NO_SAVE, rz.LED_ID_BACKLIGHT, effect));
      break;
    case DEATHADDER_CHROMA:
      usbSend(target, rz.setLedEffect(ctx, rz.STORAGE_NO_SAVE, rz.LED_ID_SCROLL_WHEEL, effect));
      usbSend(target, rz.setLedEffect(ctx, rz.STORAGE_NO_SAVE, rz.LED_ID_LOGO, effect));
      break;
    case NAGA_EPIC_CHROMA:
      usbSend(target, rz.setLedEffect(ctx, rz.STORAGE_NO_SAVE, rz.LED_ID_SCROLL_WHEEL, effect));
      device.pause(1);
      usbSend(target, rz.setLedEffect(ctx, rz.STORAGE_NO_SAVE, rz.LED_ID_BACKLIGHT, effect));
      break;
    case MAMBA_2012_WIRED:
    case MAMBA_2012_WIRELESS:
      usbSend(target, rz.setLedEffect(ctx, rz.STORAGE_NO_SAVE, rz.LED_ID_SCROLL_WHEEL, effect));
      break;
    default:
      break;
  }
}

/** razer_set_brightness(). */
function setBrightness(target, brightness) {
  const ctx = target.ctx;
  switch (target.matrix) {
    case STANDARD:
    case LINEAR:
      usbSend(target, rz.brightnessStandard(ctx, rz.STORAGE_NO_SAVE, target.ledId, brightness));
      break;
    case EXTENDED:
      usbSend(target, rz.brightnessExtended(ctx, rz.STORAGE_NO_SAVE, target.ledId, brightness));
      break;
    case EXTENDED_ARGB:
      for (let channel = 0; channel < 6; channel++) {
        usbSend(target, rz.brightnessExtended(ctx, rz.STORAGE_NO_SAVE, rz.LED_ID_ARGB_CH_1 + channel, brightness));
      }
      break;
    case CUSTOM:
      if (target.pid === ORBWEAVER_CHROMA) {
        usbSend(target, rz.brightnessStandard(ctx, rz.STORAGE_NO_SAVE, target.ledId, brightness));
      }
      break;
    default:
      break;
  }
}

/** razer_set_mode_spectrum_cycle(). */
function setModeSpectrumCycle(target) {
  const ctx = target.ctx;
  switch (target.matrix) {
    case STANDARD:
    case LINEAR:
      usbSend(target, rz.spectrumStandard(ctx));
      break;
    case EXTENDED:
    case EXTENDED_ARGB:
      usbSend(target, rz.spectrumExtended(ctx, rz.STORAGE_NO_SAVE, target.ledId));
      break;
    case CUSTOM:
      ledEffects(target, 4);
      if (target.pid === ORBWEAVER_CHROMA) {
        usbSend(target, rz.spectrumStandard(ctx));
      }
      break;
    default:
      break;
  }
}

// --- RazerController: transfers ------------------------------------------------------------------------

/**
 * razer_usb_send(): SET_FEATURE of the 91-byte report. A paired device (wait_for_response) is
 * then polled every 2 ms, up to 50 times, until the receiver has relayed this command.
 */
function usbSend(target, report) {
  const ret = device.send_report(report, rz.REPORT_LENGTH);
  if (target.waitForResponse && ret >= 0) {
    for (let attempt = 0; attempt < 50; attempt++) {
      device.pause(2);
      const response = device.get_report([target.ctx.reportIndex], rz.REPORT_LENGTH);
      if (!response || response.length === 0) {
        break;
      }
      if (rz.isFinalResponse(report, response)) {
        break;
      }
    }
  }
  return ret;
}

/** razer_usb_query(): up to 3 sends, each followed by up to 25 reads 2 ms apart; the final answer or null. */
function usbQuery(target, report) {
  for (let send = 0; send < 3; send++) {
    device.pause(2);
    usbSend(target, report);
    for (let read = 0; read < 25; read++) {
      device.pause(2);
      const response = device.get_report([target.ctx.reportIndex], rz.REPORT_LENGTH);
      if (rz.isFinalResponse(report, response)) {
        return response;
      }
    }
  }
  return null;
}

/** GetPairedDevicePIDs(): slot count, then per slot a status byte and a big-endian product id. */
function getPairedDevicePids(target) {
  const response = usbQuery(target, rz.getWirelessConnections(target.ctx));
  if (!response || response[1] !== rz.STATUS_SUCCESS) {
    return [];
  }
  const pids = [];
  const slots = rz.arg(response, 0);
  for (let slot = 0; slot < slots && (slot * 3) + 3 < 80; slot++) {
    const pid = (rz.arg(response, slot * 3 + 2) << 8) | rz.arg(response, slot * 3 + 3);
    if (pid !== 0x0000 && pid !== 0xFFFF && pids.length < MAX_PAIRED) {
      pids.push(pid);
    }
  }
  return pids;
}

/** razer_get_keyboard_info(): 1 ms, request, 1 ms, read; the layout is argument 0 (0 when unread). */
function getKeyboardLayout(target) {
  device.pause(1);
  usbSend(target, rz.getKeyboardInfo(target.ctx));
  device.pause(1);
  const response = device.get_report([target.ctx.reportIndex], rz.REPORT_LENGTH);
  return rz.arg(response, 0);
}

// --- Chroma Addressable RGB Controller ---------------------------------------------------------------

function argbInitialize() {
  argbEndpoints = findArgbEndpoints();
  device.setSize([1, 1]);
  device.setControllableLeds([], []);
  device.SetLedLimit(ARGB_LEDS_PER_CHANNEL * main.entry.zones.length);
  for (const zone of main.entry.zones) {
    device.addChannel(zone.name, ARGB_LEDS_PER_CHANNEL);
  }
  // RGBController_RazerAddressable::DeviceUpdateMode(Direct): the LEDs first (Render() sends
  // them), then SetBrightness(255) on the six ARGB channels.
  if (!argbEndpoints) {
    // Without both interfaces nothing can be addressed: send nothing at all.
    device.log("Razer ARGB controller: interfaces 0 and 1 are both needed; one is missing.");
    return;
  }
  useEndpoint(argbEndpoints.report);
  setBrightness(main, 255);
}

/** The interface 0 collection (90-byte reports) and the interface 1 collection (ARGB frames). */
function findArgbEndpoints() {
  const report = findFeatureEndpoint(0, rz.REPORT_LENGTH);
  const argb = findFeatureEndpoint(1, rz.ARGB_REPORT_LENGTH);
  return report && argb ? { report, argb } : null;
}

/**
 * The collection of `iface` whose report descriptor declares a feature report of at least
 * `bytes` bytes (Lightloom's getHidReportFields()), else the interface's first collection; null
 * when the host lists no collection of that interface. OpenRGB's interface-only detectors run on
 * every collection of the interface (DetectionManager calls the detector per collection), so the
 * one that takes the report ends up driven; Lightloom opens one, so it looks for that one.
 */
function findFeatureEndpoint(iface, bytes, rows = null) {
  if (typeof device.getHidEndpoints !== "function") {
    return null;
  }
  const candidates = (device.getHidEndpoints() || [])
    .filter((e) => e.usage_page !== LAMPARRAY_USAGE_PAGE)
    .filter((e) => (rows ? rows.some((row) => matches(row, e)) : e.interface === iface));
  if (candidates.length > 1 && typeof device.getHidReportFields === "function") {
    for (const e of candidates) {
      useEndpoint(e);
      const fields = device.getHidReportFields();
      const reports = fields && Array.isArray(fields.reports) ? fields.reports : [];
      if (reports.some((r) => r.kind === "feature" && r.bytes >= bytes)) {
        return e;
      }
    }
    // None declares the report: leave the first candidate active, not the last one probed.
    useEndpoint(candidates[0]);
  }
  return candidates[0] ?? null;
}

function useEndpoint(e) {
  if (e && typeof device.set_endpoint === "function") {
    device.set_endpoint(e.interface, e.usage, e.usage_page, e.collection);
  }
}

function argbRender() {
  const channels = main.entry.zones.map((zone) => {
    const channel = device.channel(zone.name);
    const count = channel ? Math.min(channel.LedCount(), ARGB_LEDS_PER_CHANNEL) : 0;
    const rgb = channel && count > 0 ? channel.getColors("Inline", "RGB").slice(0, count * 3) : [];
    return { count, rgb };
  });
  // DeviceConfigureZone() -> SetAddressableZoneSizes(): OpenRGB sends it only when the user
  // resizes a zone, never at start. The first frame's counts are only a baseline (nothing is
  // sent, the controller keeps its own sizes); the report goes out only after a channel count
  // changes during the session, and never with every size 0. Whether the controller stores
  // these sizes persistently is unknown. Direct frames do not need it (SetLEDs sends 80 LEDs
  // per channel whatever the size).
  const sizes = channels.map((c) => c.count);
  while (sizes.length < 6) sizes.push(0);
  if (!argbSizes) {
    argbSizes = sizes;
  } else if (sizes.some((s, i) => s !== argbSizes[i])) {
    if (sizes.some((s) => s > 0)) {
      useEndpoint(argbEndpoints?.report);
      usbSend(main, rz.addressableSize(main.ctx, sizes));
    }
    argbSizes = sizes;
  }
  // RGBController_RazerAddressable::DeviceUpdateLEDs(): 80 colours per zone (OpenRGB leaves the
  // unused ones uninitialised; Lightloom sends black), then SetLEDs() row by row (6 x 80).
  const colours = [];
  for (let zone = 0; zone < main.entry.rows; zone++) {
    const rgb = channels[zone] ? channels[zone].rgb : [];
    for (let i = 0; i < ARGB_LEDS_PER_CHANNEL; i++) {
      colours.push([rgb[i * 3] ?? 0, rgb[i * 3 + 1] ?? 0, rgb[i * 3 + 2] ?? 0]);
    }
  }
  const now = Date.now();
  if (main.lastColours && sameColours(colours, main.lastColours) && now - main.lastSentAt < RESEND_MS) {
    return;
  }
  setLeds(main, colours);
  main.lastColours = colours;
  main.lastSentAt = now;
}

/** razer_usb_send_argb(): SET_FEATURE of the 321-byte report on the ARGB interface. */
function sendArgb(report) {
  useEndpoint(argbEndpoints?.argb);
  device.send_report(report, rz.ARGB_REPORT_LENGTH);
  useEndpoint(argbEndpoints?.report);
}

// --- Kraken V4 ---------------------------------------------------------------------------------------

/** RazerKrakenV4Controller::SetDirect(): one output report with the first `cols` colours. */
function krakenV4SetDirect(target, colours) {
  const cols = target.entry.cols;
  const rgb = [];
  for (let i = 0; i < cols; i++) {
    const c = colours[i] ?? [0, 0, 0];
    rgb.push(c[0], c[1], c[2]);
  }
  write(rz.krakenV4Direct(target.ctx, cols, rgb), rz.KRAKEN_V4_REPORT_LENGTH);
}

function write(bytes, length) {
  return device.write(bytes, length);
}

// --- DeathAdder V2 Pro: Lightloom's recorded packets --------------------------------------------------

function recordedInitialize() {
  main.ctx.transactionId = recordedTransactionId();
  // Host frames from now on: 0x0F 0x02 [0x00, 0x00, 0x08, 0x00, 0x01, 0x00].
  const reply = recordedRequest(0x0F, 0x02, [0x00, 0x00, 0x08, 0x00, 0x01, 0x00]);
  if (!reply) {
    device.log("The mouse did not confirm frame mode; frames are sent anyway.");
  }
}

function recordedRender() {
  const [r, g, b] = sample(main, 0, 0);
  recordedSend(0x0F, 0x03, [0x00, 0x00, 0x00, 0x00, 0x00, r, g, b]);
}

function recordedShutdown() {
  recordedSend(0x0F, 0x03, [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
  recordedRequest(0x00, 0x04, [0x00, 0x00]);
}

/** The first transaction id the mouse answers a serial-number query (0x00 0x82) with; 0x1F if none. */
function recordedTransactionId() {
  for (const id of RECORDED_TRANSACTION_IDS) {
    main.ctx.transactionId = id;
    if (recordedRequest(0x00, 0x82, [0x00, 0x00])) {
      return id;
    }
  }
  device.log("No transaction id answered; using 0x1F.");
  return RECORDED_TRANSACTION_IDS[0];
}

function recordedSend(commandClass, commandId, args) {
  return device.send_report(rz.report(main.ctx, commandClass, commandId, args.length, args), rz.REPORT_LENGTH);
}

/** Send and read the reply: the reply when the mouse confirms it (status 0x02), else null. */
function recordedRequest(commandClass, commandId, args) {
  if (recordedSend(commandClass, commandId, args) < 0) {
    return null;
  }
  for (let attempt = 0; attempt < 5; attempt++) {
    device.pause(attempt === 0 ? 2 : 10);
    const reply = device.get_report([0x00], rz.REPORT_LENGTH);
    if (!reply || reply.length < 9) {
      continue;
    }
    const same = reply[2] === main.ctx.transactionId && reply[7] === commandClass && reply[8] === commandId;
    if (same && reply[1] === rz.STATUS_SUCCESS) {
      return reply;
    }
    if (same && reply[1] !== rz.STATUS_BUSY) {
      return null;
    }
  }
  return null;
}

// --- helpers ------------------------------------------------------------------------------------------

function sameColours(a, b) {
  if (!a || !b || a.length !== b.length) {
    return false;
  }
  for (let i = 0; i < a.length; i++) {
    if (a[i][0] !== b[i][0] || a[i][1] !== b[i][1] || a[i][2] !== b[i][2]) {
      return false;
    }
  }
  return true;
}

function hex4(v) {
  return (Number(v) & 0xFFFF).toString(16).toUpperCase().padStart(4, "0");
}

// Exposed for the tests.
export const _internals = { layoutOf, layoutVariant, BATTERY_POLL_MS, BATTERY_FIRST_MS };
