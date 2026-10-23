// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: per-key keyboard and case lighting of Lenovo Legion laptops with the ITE USB
// lighting controller (048D, usage page 0xFF89, usage 0x07): Legion Y740, Legion 7 / 7S Gen 5 and
// Gen 6 (OpenRGB's LenovoUSBController), and Legion 7 / 7S Gen 7, Gen 8, Gen 9 and Legion 7 Gen 10
// in Direct mode (OpenRGB's LenovoGen7And8USBController).
// Adapted from OpenRGB Controllers/LenovoControllers/LenovoUSBControllerDetect.cpp, LenovoDevices.h,
// LenovoUSBController/LenovoUSBController.cpp and .h, LenovoUSBController/RGBController_LenovoUSB.cpp
// and .h, LenovoUSBController_Gen7_8/LenovoUSBController_Gen7_8.cpp and .h,
// LenovoUSBController_Gen7_8/RGBController_Lenovo_Gen7_8.cpp and .h, and
// RGBController/RGBControllerKeyNames.cpp (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Cooper Hall (geobot19)  17 Apr 2022" (LenovoUSBControllerDetect.cpp,
// LenovoDevices.h, LenovoUSBController, RGBController_LenovoUSB); "Chris M (Dr_No)  25 Jan 2022"
// (RGBControllerKeyNames.cpp); the Gen 7/8 files name no author ("This file is part of the OpenRGB
// project"). devices.js is generated from LenovoDevices.h and RGBControllerKeyNames.cpp.
//
// Protocol notes: docs/hardware/lenovo-legion-usb.md. In short, feature reports with report id
// 0x07. Y740..Y760S (192 bytes): 07 B2 software mode, information queries, 07 A<zone> <n> 00 then
// <led> R G B per LED (47 per report), 07 B1 back to hardware mode on exit. Gen 7..10 (960
// bytes): 07 CA profile query, 07 D0 .. 01 <profile> direct on, 07 A1 .. <led lo> <led hi> R G B
// per LED, 07 D0 .. 02 <profile> direct off on exit. OpenRGB's SAVE_PROFILE (0xCB), brightness and
// profile writes are never sent. The Legion 5 Gen 10 (0xC195) is held back: OpenRGB has no Direct
// mode for it, only modes stored in the laptop's lighting profiles.

/* global device */

import { LED_LISTS, MAPS, ZONES } from "./devices.js";

const VENDOR_ID = 0x048D;
const USAGE_PAGE = 0xFF89;
const USAGE = 0x07;
const REPORT_ID = 0x07;

const LEGACY = "legacy";
const GEN7 = "gen7";

// LenovoUSBControllerDetect.cpp, in registration order (LEGION_5GEN10 0xC195 held back).
const PRODUCTS = [
  { pid: 0xC935, name: "Lenovo Legion Y740", protocol: LEGACY },
  { pid: 0xC956, name: "Lenovo Legion 7 Gen 5", protocol: LEGACY },
  { pid: 0xC957, name: "Lenovo Legion 7S Gen 5", protocol: LEGACY },
  { pid: 0xC968, name: "Lenovo Legion 7 Gen 6", protocol: LEGACY },
  { pid: 0xC967, name: "Lenovo Legion 7S Gen 6", protocol: LEGACY },
  { pid: 0xC977, name: "Lenovo Legion 7S Gen 7", protocol: GEN7 },
  { pid: 0xC978, name: "Lenovo Legion 7 Gen 7", protocol: GEN7 },
  { pid: 0xC988, name: "Lenovo Legion 7 Gen 8", protocol: GEN7 },
  { pid: 0xC987, name: "Lenovo Legion 7S Gen 8", protocol: GEN7 },
  { pid: 0xC997, name: "Lenovo Legion 7 Gen 9", protocol: GEN7 },
  { pid: 0xC998, name: "Lenovo Legion 7 Gen 9", protocol: GEN7 },
  { pid: 0xC197, name: "Lenovo Legion 7 Gen 10", protocol: GEN7 },
];

const LEGION_Y740 = 0xC935;
const LEGION_Y750 = 0xC956;
const LEGION_Y750S = 0xC957;
const LEGION_Y760 = 0xC968;
const LEGION_Y760S = 0xC967;
const LEGION_7GEN7 = 0xC978;
const LEGION_7GEN10 = 0xC197;

// LenovoUSBController.h
const LEGACY_PACKET_SIZE = 192;
const LEGACY_ZONE_ID_0 = 0xA0;
const LEGACY_MAX_LEDS_PER_PACKET = 0x2F;
const SOFTWARE_MODE = 0xB2;
const HARDWARE_MODE = 0xB1;
// LenovoUSBController_Gen7_8.h
const GEN7_PACKET_SIZE = 960;
const DIRECT_MODE = 0xA1;
const GET_ACTIVE_PROFILE = 0xCA;
const SET_DIRECT_MODE = 0xD0;

const KEEPALIVE_MS = 1000;

export function Name() { return "Lenovo Legion USB"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/lenovo-legion-usb"; }
export function VendorId() { return VENDOR_ID; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "keyboard"; }
// Placeholders; Initialize() sets the product's layout.
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return []; }
export function LedPositions() { return []; }
export function ImageUrl() { return ""; }
// Lenovo's own software drives the lighting while it runs; HueDeck reports it, never closes it.
export function ConflictingProcesses() { return ["LegionSpace.exe", "LegionZone.exe", "LenovoVantage.exe", "LenovoVantageService.exe"]; }

let product = null;
let leds = [];        // { name, cell: [x, y], value: zone id << 8 | led number }, in OpenRGB order
let profileId = 0x01;
let lastColours = null;
let lastSentAt = 0;

// REGISTER_HID_DETECTOR_PU(..., 0xFF89, 0x07): that usage page and usage, any interface.
export function Validate(endpoint) {
  return endpoint != null && endpoint.usage_page === USAGE_PAGE && endpoint.usage === USAGE;
}

export function Initialize() {
  lastColours = null;
  lastSentAt = 0;
  leds = [];
  profileId = 0x01;
  product = PRODUCTS.find((p) => p.pid === device.productId()) ?? null;
  if (!product) {
    device.log(`Lenovo Legion USB: product id 0x${hex4(device.productId())} is not one this plugin drives.`);
    return;
  }
  let zones;
  if (product.protocol === LEGACY) {
    // LenovoUSBController constructor: setDeviceSoftwareMode().
    sendLegacyBasic(SOFTWARE_MODE);
    zones = legacyZones(product.pid);
  } else {
    // LenovoRGBController_Gen7_8 constructor: getCurrentProfileId() (the brightness and
    // profile-settings reads it also makes only fill OpenRGB's mode list: not sent).
    profileId = currentProfileId();
    zones = gen7Zones(product.pid);
  }
  const placed = placeLeds(zones);
  leds = placed.leds;
  device.setName(product.name);
  device.setSize(placed.size);
  device.setControllableLeds(leds.map((l) => l.name), leds.map((l) => l.cell.slice()));
  device.log(`Lenovo Legion USB: ${product.name}, zones ${zones.join(", ")}.`);
  if (product.protocol === GEN7 && product.pid !== LEGION_7GEN10) {
    // DeviceUpdateLEDs in Direct with direct mode not yet enabled: setLedsDirectOn(profile_id).
    setLedsDirect(0x01);
  }
}

export function Render() {
  if (!product) {
    return;
  }
  const colours = leds.map((l) => device.color(l.cell[0], l.cell[1]));
  const now = Date.now();
  if (lastColours !== null && sameColours(colours, lastColours) && now - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  let ok;
  if (product.protocol === LEGACY) {
    ok = legacyUpdateLeds(colours);
  } else {
    if (product.pid === LEGION_7GEN10) {
      // "Gen10 may ignore A1 updates unless D0 is reasserted": direct on before every update.
      setLedsDirect(0x01);
    }
    ok = gen7UpdateLeds(colours);
  }
  if (ok) {
    lastColours = colours;
    lastSentAt = now;
  } else {
    lastColours = null;
  }
}

// Y740..Y760S: ~RGBController_LenovoUSB() sends setDeviceHardwareMode() (07 B1), which "releases
// the device from software mode so that onboard controls can be used". Gen 7..10: leaving Direct
// in DeviceUpdateMode sends setLedsDirectOff(profile_id), back to the profile's own lighting.
export function Shutdown(SystemSuspending) {
  if (!product) {
    return;
  }
  if (product.protocol === LEGACY) {
    sendLegacyBasic(HARDWARE_MODE);
  } else {
    setLedsDirect(0x02);
  }
  lastColours = null;
}

// ---------------------------------------------------------------------------------------------
// Zones (RGBController_LenovoUSB constructor + SetupZones; LenovoRGBController_Gen7_8::SetupZones)
// ---------------------------------------------------------------------------------------------
function legacyZones(pid) {
  let keyboard = "ANSI";
  let fifteen = false;
  if (pid === LEGION_Y740) {
    let v = information(0x01);
    fifteen = v !== null && v <= 100;
    v = information(0x04);
    if (v !== null && v >= 16 && v <= 48) {
      keyboard = "ISO";
    }
  } else if (pid === LEGION_Y750) {
    const v = information(0x04);
    if (v === 41) {
      keyboard = "JAPAN";
    } else if (v !== null && v >= 16 && v <= 40) {
      keyboard = "ISO";
    }
  } else if (pid === LEGION_Y750S || pid === LEGION_Y760S) {
    const v = information(0x01);
    if (v === 0x97) {
      keyboard = "JAPAN";
    } else if (v === (pid === LEGION_Y750S ? 0x91 : 0x8F)) {
      keyboard = "ISO";
    }
  } else if (pid === LEGION_Y760) {
    const v = information(0x07);
    if (v === 41) {
      keyboard = "JAPAN";
    } else if (v !== null && v >= 16 && v <= 40) {
      keyboard = "ISO";
    }
  }
  const y760Keyboard = { ANSI: "lenovo_legion_Y760_kbd_ansi", ISO: "lenovo_legion_Y760_kbd_iso", JAPAN: "lenovo_legion_Y760_kbd_jp" }[keyboard];
  if (pid === LEGION_Y740) {
    const size = fifteen ? "15" : "17";
    const kind = keyboard === "ISO" ? "iso" : "ansi";
    return [`lenovo_legion_Y740_${size}_kbd_${kind}`, "lenovo_legion_Y740_logo", "lenovo_legion_Y740_pwrbtn",
      "lenovo_legion_Y740_vents", "lenovo_legion_Y740_ports"];
  }
  if (pid === LEGION_Y750) {
    return [y760Keyboard, "lenovo_legion_Y750_logo", "lenovo_legion_Y750_vents", "lenovo_legion_Y750_neon"];
  }
  if (pid === LEGION_Y750S || pid === LEGION_Y760S) {
    return [y760Keyboard];
  }
  return [y760Keyboard, "lenovo_legion_Y760_logo", "lenovo_legion_Y760_vent_left", "lenovo_legion_Y760_vent_right",
    "lenovo_legion_Y760_vent_back_right", "lenovo_legion_Y760_vent_back_left", "lenovo_legion_Y760_neon"];
}

function gen7Zones(pid) {
  const zones = ["legion7_gen7and8_kbd_ansi", "legion7_gen7and8_neon"];
  if (pid === LEGION_7GEN7) {
    zones.push("lenovo_legion_7gen7_logo", "lenovo_legion_7gen7_vents");
  }
  if (pid === LEGION_7GEN10) {
    zones.push("lenovo_legion_7gen7_logo", "lenovo_legion_7gen10_vents");
  }
  return zones;
}

// HueDeck's grid: a matrix zone at its map cells (an LED missing from OpenRGB's map goes to the next
// free cell right of the LED before it), each linear zone one row, consecutive single-LED zones
// share one row. LED order and values are OpenRGB's: zones in order, LEDs start..end, value
// (zone id << 8) + led number.
function placeLeds(zoneNames) {
  const out = [];
  let row = 0;
  let width = 1;
  let singleRow = -1;
  let singleCol = 0;
  for (const zoneName of zoneNames) {
    const zone = ZONES[zoneName];
    const list = LED_LISTS[zone.leds].slice(zone.start, zone.end + 1);
    if (zone.type === "ZONE_TYPE_MATRIX") {
      singleRow = -1;
      const map = MAPS[zone.map];
      const cells = new Array(list.length).fill(null);
      const used = new Set();
      for (let r = 0; r < zone.height; r++) {
        for (let c = 0; c < zone.width; c++) {
          const v = map[r * zone.width + c];
          if (v !== null) {
            cells[v] = [c, r];
            used.add(`${c},${r}`);
          }
        }
      }
      for (let i = 0; i < list.length; i++) {
        if (cells[i] === null) {
          let [c, r] = cells[i - 1];
          c += 1;
          while (used.has(`${c},${r}`)) {
            c += 1;
          }
          cells[i] = [c, r];
          used.add(`${c},${r}`);
        }
      }
      list.forEach(([num, name], i) => {
        out.push({ name, cell: [cells[i][0], row + cells[i][1]], value: (zone.id << 8) + num });
      });
      row += zone.height;
      width = Math.max(width, zone.width);
    } else if (zone.type === "ZONE_TYPE_SINGLE") {
      if (singleRow < 0) {
        singleRow = row;
        singleCol = 0;
        row += 1;
      }
      for (const [num, name] of list) {
        out.push({ name, cell: [singleCol, singleRow], value: (zone.id << 8) + num });
        singleCol += 1;
      }
      width = Math.max(width, singleCol);
    } else {
      singleRow = -1;
      list.forEach(([num, name], i) => {
        out.push({ name, cell: [i, row], value: (zone.id << 8) + num });
      });
      row += 1;
      width = Math.max(width, list.length);
    }
  }
  return { leds: out, size: [width, row] };
}

// ---------------------------------------------------------------------------------------------
// Y740..Y760S (LenovoUSBController)
// ---------------------------------------------------------------------------------------------
function sendLegacyBasic(instruction) {
  const buf = new Array(LEGACY_PACKET_SIZE).fill(0);
  buf[0] = REPORT_ID;
  buf[1] = instruction;
  return device.send_report(buf, LEGACY_PACKET_SIZE) !== -1;
}

// getInformation(id): the request, then GET_FEATURE of report 0x07; OpenRGB uses byte 4 of the
// reply when the reply is longer than 4 bytes. null when there is none.
function information(id) {
  sendLegacyBasic(id);
  const reply = device.get_report([REPORT_ID], LEGACY_PACKET_SIZE);
  return Array.isArray(reply) && reply.length > 4 ? reply[4] & 0xFF : null;
}

// RGBController_LenovoUSB::DeviceUpdateLEDs: consecutive LEDs of one zone id go to setZoneLeds.
function legacyUpdateLeds(colours) {
  let ok = true;
  let prev = 0;
  let current = [];
  leds.forEach((led, i) => {
    const zoneId = led.value >> 8;
    if (zoneId !== prev && prev !== 0) {
      ok = setZoneLeds(prev, current) && ok;
      current = [];
    }
    prev = zoneId;
    current.push([led.value & 0xFF, colours[i]]);
  });
  if (current.length > 0) {
    ok = setZoneLeds(prev, current) && ok;
  }
  return ok;
}

// LenovoUSBController::setZoneLeds: 07 A<zone> <count> 00, then <led> R G B, 47 LEDs per report.
function setZoneLeds(zoneNum, pairs) {
  let ok = true;
  for (let cur = 0; cur < pairs.length;) {
    const buf = new Array(LEGACY_PACKET_SIZE).fill(0);
    buf[0] = REPORT_ID;
    buf[1] = (LEGACY_ZONE_ID_0 + zoneNum) & 0xFF;
    let count = 0;
    for (; count < LEGACY_MAX_LEDS_PER_PACKET && cur < pairs.length; count++, cur++) {
      const offset = count * 4 + 4;
      const [num, [r, g, b]] = pairs[cur];
      buf[offset] = num;
      buf[offset + 1] = r & 0xFF;
      buf[offset + 2] = g & 0xFF;
      buf[offset + 3] = b & 0xFF;
    }
    buf[2] = count;
    ok = device.send_report(buf, LEGACY_PACKET_SIZE) !== -1 && ok;
  }
  return ok;
}

// ---------------------------------------------------------------------------------------------
// Gen 7..10 (LenovoGen7And8USBController)
// ---------------------------------------------------------------------------------------------
// 07 <cmd> C0 03 <payload>; the Gen 10 (0xC197) puts the payload length in bytes 2-3 instead.
function gen7Buffer(cmd, payload, gen10Length) {
  const buf = new Array(GEN7_PACKET_SIZE).fill(0);
  buf[0] = REPORT_ID;
  buf[1] = cmd;
  buf[2] = 0xC0;
  buf[3] = 0x03;
  payload.forEach((b, i) => { buf[4 + i] = b & 0xFF; });
  if (product.pid === LEGION_7GEN10) {
    buf[2] = gen10Length & 0xFF;
    buf[3] = (gen10Length >> 8) & 0xFF;
  }
  return buf;
}

// getCurrentProfileId(): byte 4 of the reply, or 1.
function currentProfileId() {
  device.send_report(gen7Buffer(GET_ACTIVE_PROFILE, [], 1), GEN7_PACKET_SIZE);
  const reply = device.get_report([REPORT_ID], GEN7_PACKET_SIZE);
  return Array.isArray(reply) && reply.length > 4 ? reply[4] & 0xFF : 0x01;
}

// setLedsDirectOn (0x01) / setLedsDirectOff (0x02).
function setLedsDirect(onOff) {
  return device.send_report(gen7Buffer(SET_DIRECT_MODE, [onOff, profileId], 2), GEN7_PACKET_SIZE) !== -1;
}

// setLedsDirect(leds, colors): 07 A1, then <value lo> <value hi> R G B per LED.
function gen7UpdateLeds(colours) {
  const buf = new Array(GEN7_PACKET_SIZE).fill(0);
  buf[0] = REPORT_ID;
  buf[1] = DIRECT_MODE;
  let i = 4;
  let count = 0;
  const gen10 = product.pid === LEGION_7GEN10;
  if (!gen10) {
    buf[2] = 0xC0;
    buf[3] = 0x03;
  }
  for (let index = 0; index < leds.length; index++) {
    if (gen10 ? i + 5 > GEN7_PACKET_SIZE : i >= GEN7_PACKET_SIZE) {
      break;
    }
    const [r, g, b] = colours[index];
    buf[i++] = leds[index].value & 0xFF;
    buf[i++] = (leds[index].value >> 8) & 0xFF;
    buf[i++] = r & 0xFF;
    buf[i++] = g & 0xFF;
    buf[i++] = b & 0xFF;
    count += 1;
  }
  if (gen10) {
    buf[2] = (count * 5) & 0xFF;
    buf[3] = ((count * 5) >> 8) & 0xFF;
  }
  return device.send_report(buf, GEN7_PACKET_SIZE) !== -1;
}

function sameColours(a, b) {
  for (let i = 0; i < a.length; i++) {
    if (a[i][0] !== b[i][0] || a[i][1] !== b[i][1] || a[i][2] !== b[i][2]) {
      return false;
    }
  }
  return true;
}

function hex4(n) {
  return (Number(n) & 0xFFFF).toString(16).toUpperCase().padStart(4, "0");
}
