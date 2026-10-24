// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin module: MSI Mystic Light motherboard controllers (the 185-, 162- and 112-byte
// configuration report variants), Direct mode. Shared, byte for byte, by plugins/msi-mystic-light
// (vendor id 0x1462) and plugins/msi-mystic-light-common (vendor id 0x0DB0); the two copies must
// stay identical (each plugin's tests check it).
// Adapted from OpenRGB Controllers/MSIMotherboardController/ (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB :
//   MSIMotherboardControllerDetect.cpp, MSIMotherboardCommon.h,
//   MSIMotherboard185Controller/MSIMotherboard185Controller.{cpp,h} and RGBController_MSIMotherboard185.{cpp,h}
//     ("Direct mode functionality has been implemented based on the mystic-why project provided by
//     Aleksandr Garashchenko (https://github.com/garashchenko/mystic-why)"),
//   MSIMotherboard162Controller/MSIMotherboard162Controller.{cpp,h} and RGBController_MSIMotherboard162.{cpp,h},
//   MSIMotherboard112Controller/MSIMotherboard112Controller.{cpp,h} and RGBController_MSIMotherboard112.{cpp,h}.
// Original authors, as in those files:
//   Copyright (C) 2020 T-bond (03 Apr 2020: 185- and 162-byte controllers)
//   Copyright (C) 2021 Adam Honse (06 Mar 2021: MSIMotherboardCommon.h, 185- and 162-byte controllers)
//   Copyright (C) 2022 thombo (17 Dec 2022: 112-byte controller)
//
// What it does (docs/hardware/msi-mystic-light.md has the details):
//   * Detection as in DetectMSIMotherboardControllers(): GET_FEATURE 0x52 with a 200-byte buffer; the
//     returned length picks the variant (185/186, 162/163, 112/113 bytes). Anything else (including
//     OpenRGB's 761-byte boards, which this plugin does not drive) disables all writes.
//   * The controller constructors: two firmware queries (output report 01 B0 / 01 B6, 64 bytes, each
//     followed by one input report), the board lookup (product id, or for the common product id 0x0076
//     the first four hex digits of the serial number), then GET_FEATURE 0x52 of the variant's length.
//     That second read is the restore image; without it nothing is ever written.
//   * OpenRGB's "Direct" mode, as RGBController_MSIMotherboard{185,162,112} drive it with the mode's
//     own defaults (two documented deviations: 162-byte boards and the 7D88 case below):
//       185 per-LED boards: the 0x52 "enable per-LED" report, then 725-byte 0x53 colour streams
//         (one synchronised stream, or one per ARGB header plus the onboard stream);
//       185 zone-based boards and 112-byte boards: one 0x52 report with one colour per zone per frame;
//       162-byte boards ("Direct" is the static effect): the board's own 0x52 report with static
//         colours, per frame. Always brightness 100 % and the custom-colour bit set: OpenRGB's
//         GetDeviceConfig() would, when the first zone reads as Static, copy that zone's brightness and
//         (through its inverted rainbow flag) clear the custom-colour bit, which turns the colours off.
//       185 per-LED boards with no streamed zone (only JRGB/PIPE zones, e.g. MPG Z790-S WIFI 7D88):
//         OpenRGB sends the switch but never a stream (dark board); HueDeck writes nothing at all.
//   * The "save to flash" byte (last byte of every 0x52 report) is 0, except in the reports of
//     save() ("Save to devices", a HueDeck extension the user has to ask for and confirm): OpenRGB's
//     DeviceSaveMode() for its Static and Breathing modes, i.e. Update(true) (185: the board's
//     own report then the new one, both with the save byte; 162: the new one). 112-byte boards
//     cannot be saved (not ported). OpenRGB's other hardware effects are not ported.
//   * Exit modes (HueDeck extension): showStatic() writes OpenRGB's Static mode with the last
//     colours, or black, with the save byte 0 (volatile).
//   * Shutdown writes the restore image back (save byte 0), which leaves Direct mode and puts the
//     board's own effect back. OpenRGB has no exit step; this is HueDeck's choice. 185-byte images get
//     the J_CORSAIR +9 read-back bias removed first (as plugins/msi-mystic-light-7c91 does).

import {
  MSI_ZONE as Z,
  PID_COMMON,
  ZONE_SETS_185,
  BOARDS_185,
  DEFAULT_185,
  ZONE_SETS_162,
  BOARDS_162,
  DEFAULT_162,
} from "./boards.js";

// ---------------------------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------------------------
export const CONFIG_REPORT = 0x52;          // FeaturePacket_185/162/112 report id
const PER_LED_REPORT = 0x53;                // FeaturePacket_PerLED_185 report id
const DETECT_LENGTH = 200;                  // temp_buffer[200] in DetectMSIMotherboardControllers()
const PER_LED_LEDS = 240;                   // NUMOF_PER_LED_MODE_LEDS
const PER_LED_LENGTH = 5 + 3 * PER_LED_LEDS; // sizeof(FeaturePacket_PerLED_185) = 725
const FW_LENGTH = 64;                       // ReadFwVersion() request/response size
const FW_APROM = 0xB0;
const FW_LDROM = 0xB6;
const FW_READ_TIMEOUT_MS = 1000;            // hid_read() blocks in OpenRGB; HueDeck reads with a timeout

// Zone record fields (struct ZoneData / RainbowZoneData / CorsairZoneData in MSIMotherboardCommon.h).
const EFFECT = 0;
const COLOR = 1;                            // R, G, B
const SPEED_BRIGHTNESS = 4;                 // CorsairZoneData: fan_flags
const COLOR2 = 5;                           // R, G, B; CorsairZoneData: corsair_quantity, padding[0..1]
const COLOR_FLAGS = 8;                      // CorsairZoneData: padding[2]
const PADDING = 9;                          // CorsairZoneData: padding[3]
const LED_COUNT = 10;                       // RainbowZoneData: cycle_or_led_num; CorsairZoneData: is_individual

// MSI_MODE values used here.
const MODE_STATIC = 1;
const MODE_BREATHING = 2;                   // MSI_MODE_BREATHING
const SPEED_MEDIUM = 1;                     // MSI_SPEED_MEDIUM, OpenRGB's default speed
const SAVE_ON = 1;                          // save_data = true
const MODE_DOUBLE_FLASHING = 4;
const MODE_LIGHTNING = 5;
const MSI_DIRECT_MODE = 0x25;

// Flag bits (MSIMotherboardCommon.h).
const BRIGHTNESS_SHIFT = 2;
const BRIGHTNESS_MASK = 0x1F << BRIGHTNESS_SHIFT;
const SPEED_MASK = 0x03;
const SYNC_SETTING_JRGB = 0x80;             // MSI_SPEED_BRIGHTNESS_FLAG_SYNC_SETTING_JRGB
const SYNC_ONBOARD = 0x01;
const SYNC_JRAINBOW1 = 0x02;
const SYNC_JRAINBOW2 = 0x04;
const SYNC_JCORSAIR = 0x08;
const SYNC_JPIPE1 = 0x10;
const SYNC_JPIPE2 = 0x20;
const USE_CUSTOM_COLOR = 0x80;
const PER_LED_BASIC_SYNC_MODE = USE_CUSTOM_COLOR | SYNC_ONBOARD | SYNC_JPIPE1 | SYNC_JPIPE2;
const PER_LED_FULL_SYNC_MODE = PER_LED_BASIC_SYNC_MODE | SYNC_JRAINBOW1 | SYNC_JRAINBOW2 | SYNC_JCORSAIR;
const BRIGHTNESS_LEVEL_100 = 10;

// OpenRGB's "Direct" (and, on 162-byte boards, "Direct" = Static) mode entry with the mode's own
// defaults (SetupMode()): per-LED colours (rainbow flag off), brightness 100 %, no speed. On 162-byte
// boards this deliberately ignores what GetDeviceConfig() copies from a first zone that reads as
// Static (docs/hardware/msi-mystic-light.md, "162-byte").
const DIRECT_BRIGHTNESS = BRIGHTNESS_LEVEL_100;
const DIRECT_SPEED = 0;

// MSIMotherboard185Controller.cpp
const SYNC_PER_LED_MODE_JRAINBOW_LED_COUNT = 40;
const SYNC_PER_LED_MODE_CORSAIR_LED_COUNT = 120;
const JRAINBOW1_MAX_LED_COUNT = 200;
const JRAINBOW2_MAX_LED_COUNT = 240;
const JCORSAIR_MAX_LED_COUNT = 240;
const NON_SYNC_PAUSE_MS = 13;               // std::this_thread::sleep_for(13ms)
const DIRECT_PER_LED = "PER_LED";
const DIRECT_ZONE_BASED = "ZONE_BASED";
const DIRECT_DISABLED = "DISABLED";
const MIXED_MODE_MIN_PID = 0x7D03;
const ZONE_BASED_MAX_LEDS = 30;             // leds_max of a resizable zone on a zone-based board
// J_CORSAIR record +9 (0x3E) reads back as the written value + 4 (seen on the 7C91,
// docs/hardware/msi-mystic-light-185.md); the restore removes it so the byte does not climb by 4 per
// session. Values below 4 are written as read, as the 7C91 plugin does. On a board without the bias
// the byte would instead sink by 4 per session until it is below 4 (OpenRGB's own reports write 0
// there, so that end state is one OpenRGB produces too).
const CORSAIR_READBACK_BIAS = 4;

// Variants: report length and record offsets (FeaturePacket_185/162/112).
const LAYOUT_185 = {
  length: 185,
  J_RGB_1: 0x01, J_PIPE_1: 0x0B, J_PIPE_2: 0x15, J_RAINBOW_1: 0x1F, J_RAINBOW_2: 0x2A,
  J_CORSAIR: 0x35, J_CORSAIR_OUTERLL120: 0x40, ON_BOARD_LED: 0x4A,
  // on_board_led_1..9
  ON_BOARD: [0x54, 0x5E, 0x68, 0x72, 0x7C, 0x86, 0x90, 0x9A, 0xA4],
  J_RGB_2: 0xAE, SAVE: 0xB8,
};
const LAYOUT_162 = {
  length: 162,
  J_RGB_1: 1, J_RAINBOW_1: 11, J_CORSAIR_1: 21, J_CORSAIR_OUTERLL120: 31, ON_BOARD_LED: 41,
  // on_board_led_1..10
  ON_BOARD: [51, 61, 71, 81, 91, 101, 111, 121, 131, 141],
  J_RGB_2: 151, SAVE: 161,
};
const LAYOUT_112 = {
  length: 112,
  J_RGB_1: 1, J_RAINBOW_1: 11, J_CORSAIR_1: 21, J_CORSAIR_OUTERLL120: 31, ON_BOARD_LED: 41,
  // on_board_led_1..6
  ON_BOARD: [51, 61, 71, 81, 91, 101],
  SAVE: 111,
};

// Zone names (led_zones[] of the RGBControllers).
const ZONE_NAMES_185 = {
  J_RGB_1: "JRGB1", J_RGB_2: "JRGB2", J_RAINBOW_1: "JRAINBOW1", J_RAINBOW_2: "JRAINBOW2",
  J_RAINBOW_3: "JRAINBOW3", J_CORSAIR: "JCORSAIR", J_PIPE_1: "PIPE1", J_PIPE_2: "PIPE2",
  ON_BOARD_LED_0: "ONBOARD",
};
const ZONE_NAMES_162 = {
  J_RGB_1: "JRGB1", J_RGB_2: "JRGB2", J_RAINBOW_1: "JRAINBOW1", J_RAINBOW_2: "JRAINBOW2",
  J_CORSAIR: "JCORSAIR", ON_BOARD_LED_0: "Onboard LEDs",
};
const ZONE_NAMES_112 = {
  J_RGB_1: "JRGB1", J_RAINBOW_1: "JRAINBOW1", J_CORSAIR: "JCORSAIR", ON_BOARD_LED_0: "Onboard LEDs",
};
// led_zones[] order of RGBController_MSIMotherboard112 (zones_set of MSIMotherboard112Controller.cpp).
const ZONES_112 = ["J_RGB_1", "J_RAINBOW_1", "J_CORSAIR", "ON_BOARD_LED_0"];
const ONBOARD_LEDS_112 = 7;

const RESIZABLE_185 = new Set([Z.J_RAINBOW_1, Z.J_RAINBOW_2, Z.J_RAINBOW_3, Z.J_CORSAIR]);

// ---------------------------------------------------------------------------------------------
// Board lookup
// ---------------------------------------------------------------------------------------------

/** Variant for the length GET_FEATURE 0x52 returned (DetectMSIMotherboardControllers()). */
export function variantForLength(length) {
  for (const size of [185, 162, 112]) {
    if (length >= size && length <= size + 1) {
      return size;
    }
  }
  return 0;
}

/**
 * The product id the 185-byte controller looks its board up with: the USB product id, or for the
 * common product id the first four characters of the serial number read as hex (std::stoi(.., 16):
 * leading hex digits; NaN, which matches no row, when there are none).
 */
export function boardPid185(productId, serial) {
  if (productId !== PID_COMMON) {
    return productId;
  }
  const value = parseInt(String(serial ?? "").substring(0, 4), 16);
  return Number.isNaN(value) ? NaN : value;
}

/** board_configs[] row of the 185-byte controller (first match), or the default configuration. */
export function boardConfig185(pid) {
  return BOARDS_185.find((row) => row.pid === pid) ?? DEFAULT_185;
}

/** board_configs[] row of the 162-byte controller (first match), or the default configuration. */
export function boardConfig162(pid) {
  return BOARDS_162.find((row) => row.pid === pid) ?? DEFAULT_162;
}

// ---------------------------------------------------------------------------------------------
// Default packets
// ---------------------------------------------------------------------------------------------

// A record as its struct's default member initialisers leave it.
function zoneDefault() { return [MODE_STATIC, 0, 0, 0, 0, 0, 0, 0, 0, 0]; }
function rainbowDefault() { return [...zoneDefault(), 100]; }
function corsairDefault() { return [MODE_STATIC, 0, 0, 0, 40, 0, 0, 0, 0, 0, 0]; }

function put(packet, offset, bytes) {
  bytes.forEach((value, index) => { packet[offset + index] = value & 0xFF; });
}

// FeaturePacket_185 as default-constructed.
function default185() {
  const L = LAYOUT_185;
  const p = new Array(L.length).fill(0);
  p[0] = CONFIG_REPORT;
  for (const off of [L.J_RGB_1, L.J_PIPE_1, L.J_PIPE_2, L.J_CORSAIR_OUTERLL120, L.ON_BOARD_LED, ...L.ON_BOARD, L.J_RGB_2]) {
    put(p, off, zoneDefault());
  }
  put(p, L.J_RAINBOW_1, rainbowDefault());
  put(p, L.J_RAINBOW_2, rainbowDefault());
  put(p, L.J_CORSAIR, corsairDefault());
  p[L.SAVE] = 0;
  return p;
}

// FeaturePacket_112 / FeaturePacket_162 as default-constructed.
function defaultPlain(L) {
  const p = new Array(L.length).fill(0);
  p[0] = CONFIG_REPORT;
  const records = [L.J_RGB_1, L.J_RAINBOW_1, L.J_CORSAIR_1, L.J_CORSAIR_OUTERLL120, L.ON_BOARD_LED, ...L.ON_BOARD];
  if (L.J_RGB_2 !== undefined) {
    records.push(L.J_RGB_2);
  }
  for (const off of records) {
    put(p, off, zoneDefault());
  }
  p[L.SAVE] = 0;
  return p;
}

// enable_per_led_msg after the MSIMotherboard185Controller constructor's assignments.
function enablePerLedMessage() {
  const L = LAYOUT_185;
  const p = default185();
  const set = (off, field, value) => { p[off + field] = value; };
  set(L.J_RGB_1, SPEED_BRIGHTNESS, 0x08); set(L.J_RGB_1, COLOR_FLAGS, 0x80);
  set(L.J_PIPE_1, SPEED_BRIGHTNESS, 0x2A); set(L.J_PIPE_1, COLOR_FLAGS, 0x80);
  set(L.J_PIPE_2, SPEED_BRIGHTNESS, 0x2A); set(L.J_PIPE_2, COLOR_FLAGS, 0x80);
  for (const off of [L.J_RAINBOW_1, L.J_RAINBOW_2]) {
    set(off, SPEED_BRIGHTNESS, 0x29); set(off, COLOR_FLAGS, 0x80); set(off, LED_COUNT, 0x28);
  }
  set(L.J_CORSAIR, 4, 0x29);                // fan_flags
  set(L.J_CORSAIR, 5, 0x00);                // corsair_quantity
  set(L.J_CORSAIR, 8, 0x82);                // padding[2]
  set(L.J_CORSAIR, 10, 0x78);               // is_individual
  set(L.J_CORSAIR_OUTERLL120, SPEED_BRIGHTNESS, 0x28); set(L.J_CORSAIR_OUTERLL120, COLOR_FLAGS, 0x80);
  set(L.ON_BOARD_LED, EFFECT, MSI_DIRECT_MODE);
  set(L.ON_BOARD_LED, SPEED_BRIGHTNESS, 0x29 | SYNC_SETTING_JRGB);
  set(L.ON_BOARD_LED, COLOR_FLAGS, PER_LED_FULL_SYNC_MODE);
  for (const off of L.ON_BOARD) {
    set(off, SPEED_BRIGHTNESS, 0x28); set(off, COLOR_FLAGS, 0x80);
  }
  set(L.J_RGB_2, SPEED_BRIGHTNESS, 0x2A); set(L.J_RGB_2, COLOR_FLAGS, 0x80);
  return p;
}

// zone_based_per_led_data after the MSIMotherboard185Controller constructor's assignments.
function zoneBased185() {
  const L = LAYOUT_185;
  const p = default185();
  for (const off of [L.J_RGB_1, L.J_PIPE_1, L.J_PIPE_2, L.J_RAINBOW_1, L.J_RAINBOW_2, L.ON_BOARD_LED, ...L.ON_BOARD, L.J_RGB_2]) {
    p[off + SPEED_BRIGHTNESS] = BRIGHTNESS_LEVEL_100 << BRIGHTNESS_SHIFT;
    p[off + COLOR_FLAGS] |= USE_CUSTOM_COLOR;
  }
  p[L.SAVE] = 0;
  return p;
}

// zone_based_per_led_data after the MSIMotherboard112Controller constructor's assignments.
function zoneBased112() {
  const L = LAYOUT_112;
  const p = defaultPlain(L);
  for (const off of [L.J_RGB_1, L.J_RAINBOW_1, L.ON_BOARD_LED, ...L.ON_BOARD]) {
    p[off + SPEED_BRIGHTNESS] = BRIGHTNESS_LEVEL_100 << BRIGHTNESS_SHIFT;
    p[off + COLOR_FLAGS] |= USE_CUSTOM_COLOR;
  }
  p[L.SAVE] = 0;
  return p;
}

// FeaturePacket_PerLED_185 with its header (report id, hdr0..hdr3) and black LEDs.
function perLedPacket(hdr1, hdr2) {
  const p = new Array(PER_LED_LENGTH).fill(0);
  p[0] = PER_LED_REPORT;
  p[1] = 0x25;
  p[2] = hdr1;
  p[3] = hdr2;
  p[4] = 0x00;
  return p;
}

// ---------------------------------------------------------------------------------------------
// The controller
// ---------------------------------------------------------------------------------------------

/**
 * Creates the Direct-mode driver for the device the host opened (`dev` is the host's `device`;
 * `label` prefixes log lines). Every method returns quietly when the device was not recognised or
 * may not be written; `ready` tells.
 */
export function createMysticLight(dev, { label = "MSI Mystic Light" } = {}) {
  const log = (text) => dev.log(`${label}: ${text}`, { toFile: true });

  let variant = 0;            // 185, 162 or 112 once recognised
  let restoreImage = null;    // the 0x52 report as read in initialize()
  let ready = false;          // writes allowed (restore image captured, Direct mode available)
  let wrote = false;          // this session changed the controller
  let name = label;
  let zones = [];             // { key, type, name, kind: "row" | "channel", count, max, row }
  let lastFrame = null;       // packets of the last frame sent (for the keepalive skip)
  let lastFrameAt = -Infinity;
  const KEEPALIVE_MS = 1000;

  // 185-byte controller state
  let cfg = null;             // board configuration row
  let mixedMode = false;
  let data = null;            // FeaturePacket_185 "data" (the image read by ReadSettings)
  let enableMsg = null;       // enable_per_led_msg
  let zoneBased = null;       // zone_based_per_led_data (185 or 112)
  let perLedSync = null;      // per_led_data_onboard_and_sync
  let perLedJr1 = null;
  let perLedJr2 = null;
  let perLedJc = null;
  let slots = null;           // per-LED stream slot of each zone's first LED (synchronised layout)
  let directMode = false;
  let directConfigured = false;
  let syncDirectMode = true;
  let noOnboards = true;
  let noJr1 = true;
  let noJr2 = true;
  let noJc = true;

  // 162-byte controller state
  let data162 = null;         // FeaturePacket_162 "data"
  let onboard162 = 0;

  // ------------------------------------------------------------------------------- I/O
  function send(packet) {
    const result = dev.send_report(packet, packet.length);
    if (result < 0) {
      log(`send_report(0x${packet[0].toString(16)}) failed.`);
      return false;
    }
    wrote = true;
    return true;
  }

  function readSize(bytes) {
    if (typeof dev.getLastReadSize === "function") {
      return Number(dev.getLastReadSize()) || 0;
    }
    return bytes && typeof bytes.length === "number" ? bytes.length : 0;
  }

  // ReadFwVersion(): request 01 B0 CC.. and 01 B6 CC.. (64 bytes), one input report after each.
  // Returns the APROM version byte (reply[2]). The input queue is emptied before each query and a
  // reply counts only if it answers that query (01 <command>); otherwise the version is 0, which
  // refuses a board with a minimum firmware (OpenRGB has a zeroed buffer then, and no such check).
  function readFirmwareVersion() {
    const request = new Array(FW_LENGTH).fill(0xCC);
    request[0] = 0x01;
    let aprom = 0;
    for (const command of [FW_APROM, FW_LDROM]) {
      request[1] = command;
      if (typeof dev.clearReadBuffer === "function") {
        dev.clearReadBuffer();
      }
      dev.write(request.slice(), FW_LENGTH);
      const reply = dev.read([0x01], FW_LENGTH, FW_READ_TIMEOUT_MS);
      const valid = reply && reply.length > 2 && readSize(reply) > 2
        && (reply[0] & 0xFF) === 0x01 && (reply[1] & 0xFF) === command;
      if (command === FW_APROM) {
        aprom = valid ? reply[2] & 0xFF : 0;
        if (!valid) {
          log("no answer to the APROM firmware query; version taken as 0.");
        }
      }
    }
    return aprom;
  }

  // ReadSettings(): GET_FEATURE 0x52 of the variant's length. The image is kept only when the full
  // report came back (OpenRGB ignores the result; HueDeck refuses to write without it).
  function readSettings(length) {
    const bytes = dev.get_report([CONFIG_REPORT], length);
    const size = readSize(bytes);
    if (!bytes || bytes.length < length || size < length || size > length + 1 || (bytes[0] & 0xFF) !== CONFIG_REPORT) {
      log(`GET_FEATURE 0x52 returned ${size} bytes, expected ${length}: no restore image, nothing will be written.`);
      return null;
    }
    return Array.from(bytes, (b) => b & 0xFF).slice(0, length);
  }

  // ------------------------------------------------------------------------------- 185: records
  function zoneData185(zone) {
    const L = LAYOUT_185;
    switch (zone) {
      case Z.J_RGB_1: return L.J_RGB_1;
      case Z.J_RGB_2: return mixedMode ? L.ON_BOARD[5] : L.J_RGB_2;
      case Z.J_RAINBOW_1: return L.J_RAINBOW_1;
      case Z.J_RAINBOW_2: return L.J_RAINBOW_2;
      case Z.J_RAINBOW_3: return L.J_CORSAIR;
      case Z.J_PIPE_1: return L.J_PIPE_1;
      case Z.J_PIPE_2: return L.J_PIPE_2;
      case Z.ON_BOARD_LED_0: return L.ON_BOARD_LED;
      case Z.ON_BOARD_LED_6: return mixedMode ? L.J_CORSAIR_OUTERLL120 : L.ON_BOARD[5];
      case Z.J_CORSAIR_OUTERLL120: return L.J_CORSAIR_OUTERLL120;
      case Z.J_CORSAIR: return L.J_CORSAIR;
      default:
        if (zone >= Z.ON_BOARD_LED_1 && zone <= Z.ON_BOARD_LED_9) {
          return L.ON_BOARD[zone - Z.ON_BOARD_LED_1];
        }
        return -1;
    }
  }

  // GetRainbowZoneData(): the record whose byte +10 is the zone's LED count.
  function rainbowZoneData185(zone) {
    const L = LAYOUT_185;
    switch (zone) {
      case Z.J_RAINBOW_1: return L.J_RAINBOW_1;
      case Z.J_RAINBOW_2: return L.J_RAINBOW_2;
      case Z.J_RAINBOW_3: return L.J_CORSAIR;
      default: return -1;
    }
  }

  function maxDirectLeds185(zone) {
    const perLed = cfg.directMode === DIRECT_PER_LED;
    switch (zone) {
      case Z.J_RGB_1:
      case Z.J_RGB_2: return 1;
      case Z.J_PIPE_1: return cfg.pipe1;
      case Z.J_PIPE_2: return cfg.pipe2;
      case Z.J_RAINBOW_1: return perLed ? JRAINBOW1_MAX_LED_COUNT : 1;
      case Z.J_RAINBOW_2:
      case Z.J_RAINBOW_3: return perLed ? JRAINBOW2_MAX_LED_COUNT : 1;
      case Z.J_CORSAIR: return perLed ? JCORSAIR_MAX_LED_COUNT : 1;
      case Z.ON_BOARD_LED_0: return cfg.onboard;
      default: return 1;
    }
  }

  // GetPerLedZoneData(): the buffer and the slot of the zone's LED 0.
  function perLedZoneData185(zone) {
    switch (zone) {
      case Z.J_RAINBOW_1: return syncDirectMode ? [perLedSync, slots.jrainbow1] : [perLedJr1, 0];
      case Z.J_RAINBOW_2: return syncDirectMode ? [perLedSync, slots.jrainbow2] : [perLedJr2, 0];
      case Z.ON_BOARD_LED_0: return [perLedSync, slots.onboard];
      case Z.J_RAINBOW_3:
      case Z.J_CORSAIR: return syncDirectMode ? [perLedSync, slots.jcorsair] : [perLedJc, 0];
      case Z.J_RGB_1: return [perLedSync, slots.jrgb];
      case Z.J_RGB_2: return [perLedSync, slots.jrgb + 1];
      case Z.J_PIPE_1: return [perLedSync, slots.pipe1];
      case Z.J_PIPE_2: return [perLedSync, slots.pipe2];
      default: return null;
    }
  }

  // MSIMotherboard185Controller::SetLedColor()
  function setLedColor185(zone, index, rgb) {
    if (cfg.directMode === DIRECT_PER_LED) {
      const target = perLedZoneData185(zone);
      if (target === null) {
        return;
      }
      let maxSize = maxDirectLeds185(zone);
      if (syncDirectMode) {
        if (zone === Z.J_RAINBOW_1 || zone === Z.J_RAINBOW_2) {
          maxSize = SYNC_PER_LED_MODE_JRAINBOW_LED_COUNT;
        } else if (zone === Z.J_RAINBOW_3 || zone === Z.J_CORSAIR) {
          maxSize = SYNC_PER_LED_MODE_CORSAIR_LED_COUNT;
        }
      }
      const [buffer, slot0] = target;
      const slot = slot0 + index;
      if (index < maxSize && slot < PER_LED_LEDS) {
        const offset = 5 + 3 * slot;
        buffer[offset] = rgb[0] & 0xFF;
        buffer[offset + 1] = rgb[1] & 0xFF;
        buffer[offset + 2] = rgb[2] & 0xFF;
      }
      return;
    }
    if ((zone === Z.J_RAINBOW_1 || zone === Z.J_RAINBOW_2 || zone === Z.J_PIPE_1 || zone === Z.J_PIPE_2) && index !== 0) {
      return;
    }
    let target = zone;
    if (zone >= Z.ON_BOARD_LED_0) {
      target = zone + index + 1;
    }
    const off = zoneData185(target);
    if (off < 0) {
      return;
    }
    put(zoneBased, off + COLOR, rgb);
    put(zoneBased, off + COLOR2, rgb);
  }

  // MSIMotherboard185Controller::SelectPerLedProtocol()
  function selectPerLedProtocol() {
    const L = LAYOUT_185;
    const jrainbow1Size = data[L.J_RAINBOW_1 + LED_COUNT];
    const jrainbow2Size = data[L.J_RAINBOW_2 + LED_COUNT];
    const jrainbow3Size = data[L.J_CORSAIR + LED_COUNT];
    syncDirectMode = !(jrainbow1Size > SYNC_PER_LED_MODE_JRAINBOW_LED_COUNT
      || jrainbow2Size > SYNC_PER_LED_MODE_JRAINBOW_LED_COUNT
      || jrainbow3Size > SYNC_PER_LED_MODE_CORSAIR_LED_COUNT);
    if (syncDirectMode) {
      enableMsg[L.J_RAINBOW_1 + EFFECT] = MODE_STATIC;
      enableMsg[L.J_RAINBOW_1 + LED_COUNT] = SYNC_PER_LED_MODE_JRAINBOW_LED_COUNT;
      enableMsg[L.J_RAINBOW_2 + EFFECT] = MODE_STATIC;
      enableMsg[L.J_RAINBOW_2 + LED_COUNT] = SYNC_PER_LED_MODE_JRAINBOW_LED_COUNT;
      enableMsg[L.J_CORSAIR + EFFECT] = MODE_STATIC;
      enableMsg[L.J_CORSAIR + LED_COUNT] = SYNC_PER_LED_MODE_CORSAIR_LED_COUNT;
      enableMsg[L.ON_BOARD_LED + COLOR_FLAGS] = PER_LED_FULL_SYNC_MODE;
    } else {
      if (!noJr1) {
        enableMsg[L.J_RAINBOW_1 + EFFECT] = MSI_DIRECT_MODE;
        enableMsg[L.J_RAINBOW_1 + LED_COUNT] = JRAINBOW1_MAX_LED_COUNT;
      }
      if (!noJr2) {
        enableMsg[L.J_RAINBOW_2 + EFFECT] = MSI_DIRECT_MODE;
        enableMsg[L.J_RAINBOW_2 + LED_COUNT] = JRAINBOW2_MAX_LED_COUNT;
      }
      if (!noJc) {
        enableMsg[L.J_CORSAIR + EFFECT] = MSI_DIRECT_MODE;
        enableMsg[L.J_CORSAIR + LED_COUNT] = JCORSAIR_MAX_LED_COUNT;
      }
      enableMsg[L.ON_BOARD_LED + COLOR_FLAGS] = PER_LED_BASIC_SYNC_MODE;
    }
    if (!directMode) {
      return false;
    }
    if (cfg.directMode === DIRECT_PER_LED) {
      if (!send(enableMsg.slice())) {
        return false;
      }
      directConfigured = true;
    }
    return true;
  }

  // MSIMotherboard185Controller::SetZoneLEDCount()
  function setZoneLedCount185(zone, count) {
    const off = rainbowZoneData185(zone);
    if (off < 0) {
      return;
    }
    data[off + LED_COUNT] = count & 0xFF;
    selectPerLedProtocol();
  }

  // MSIMotherboard185Controller::SetDirectMode(true)
  function setDirectMode185() {
    directMode = true;
    directConfigured = false;
    selectPerLedProtocol();
  }

  // MSIMotherboard185Controller::Update() with direct_mode set: the packets of one frame.
  function updatePackets185() {
    if (cfg.directMode !== DIRECT_PER_LED) {
      return [{ packet: zoneBased.slice() }];
    }
    if (noOnboards && noJr1 && noJr2 && noJc) {
      return null;            // "Skip sending update if there are no LEDs configured"
    }
    if (syncDirectMode) {
      return [{ packet: perLedSync.slice() }];
    }
    const out = [];
    if (!noJr1) {
      out.push({ packet: perLedJr1.slice(), pause: NON_SYNC_PAUSE_MS });
    }
    if (!noJr2) {
      out.push({ packet: perLedJr2.slice(), pause: NON_SYNC_PAUSE_MS });
    }
    if (!noJc) {
      out.push({ packet: perLedJc.slice(), pause: NON_SYNC_PAUSE_MS });
    }
    out.push({ packet: perLedSync.slice() });
    return out;
  }

  // ------------------------------------------------------------------------------- 162 / 112
  function zoneData162(zone) {
    const L = LAYOUT_162;
    switch (zone) {
      case Z.J_RGB_1: return L.J_RGB_1;
      case Z.J_RGB_2: return L.J_RGB_2;
      case Z.J_RAINBOW_1: return L.J_RAINBOW_1;
      case Z.J_RAINBOW_2: return L.ON_BOARD[9];
      case Z.ON_BOARD_LED_0: return L.ON_BOARD_LED;
      case Z.J_CORSAIR: return L.J_CORSAIR_1;
      default:
        if (zone >= Z.ON_BOARD_LED_1 && zone <= Z.ON_BOARD_LED_10) {
          return L.ON_BOARD[zone - Z.ON_BOARD_LED_1];
        }
        return -1;
    }
  }

  function zoneData112(zone) {
    const L = LAYOUT_112;
    switch (zone) {
      case Z.J_RGB_1: return L.J_RGB_1;
      case Z.J_RAINBOW_1: return L.J_RAINBOW_1;
      case Z.ON_BOARD_LED_0: return L.ON_BOARD_LED;
      case Z.J_CORSAIR: return L.J_CORSAIR_1;
      default:
        if (zone >= Z.ON_BOARD_LED_1 && zone <= Z.ON_BOARD_LED_6) {
          return L.ON_BOARD[zone - Z.ON_BOARD_LED_1];
        }
        return -1;
    }
  }

  // The mode fields SetMode() writes into one record (MSIMotherboard162Controller::SetMode()).
  function applyMode162(off, syncBits) {
    const p = data162;
    p[off + EFFECT] = MODE_STATIC;
    p[off + SPEED_BRIGHTNESS] = ((DIRECT_BRIGHTNESS << BRIGHTNESS_SHIFT) & BRIGHTNESS_MASK) | (DIRECT_SPEED & SPEED_MASK);
    p[off + COLOR_FLAGS] = (p[off + COLOR_FLAGS] | USE_CUSTOM_COLOR) & 0xFF;
    p[off + PADDING] = 0x00;
    if (syncBits) {
      // mode (static) <= MSI_MODE_DOUBLE_FLASHING: clear the sync bits.
      p[off + SPEED_BRIGHTNESS] &= ~SYNC_SETTING_JRGB & 0xFF;
      p[off + COLOR_FLAGS] &= ~SYNC_ONBOARD & 0xFF;
    }
  }

  // RGBController_MSIMotherboard162::UpdateLed(): SetMode() then SetZoneColor() for LED value `zone`.
  function updateLed162(zone, rgb) {
    const off = zoneData162(zone);
    if (off < 0) {
      return;
    }
    // SetMode (MSI_MODE_STATIC <= MSI_MODE_DOUBLE_FLASHING)
    if (zone <= Z.ON_BOARD_LED_0) {
      applyMode162(off, true);
    }
    if (zone >= Z.ON_BOARD_LED_0 && MODE_STATIC <= MODE_DOUBLE_FLASHING) {
      const next = zoneData162(zone + 1);
      if (next >= 0) {
        applyMode162(next, false);
      }
    }
    // SetZoneColor (colour 1 = colour 2)
    if (zone <= Z.ON_BOARD_LED_0) {
      put(data162, off + COLOR, rgb);
      put(data162, off + COLOR2, rgb);
    }
    if (zone >= Z.ON_BOARD_LED_0) {
      const next = zoneData162(zone + 1);
      if (next >= 0) {
        put(data162, next + COLOR, rgb);
        put(data162, next + COLOR2, rgb);
      }
    }
  }

  // MSIMotherboard112Controller::SetLedColor() (direct mode)
  function setLedColor112(zone, rgb) {
    const target = zone >= Z.ON_BOARD_LED_0 ? zone + 1 : zone;
    const off = zoneData112(target);
    if (off < 0) {
      return;
    }
    put(zoneBased, off + COLOR, rgb);
    put(zoneBased, off + COLOR2, rgb);
  }

  // ------------------------------------------------------------------------------- static / save
  // The average colour of one zone on the frame `colourOf` describes (frame()'s argument).
  function zoneColour(zone, colourOf) {
    const colour = colourOf(zone);
    const n = Math.max(1, zone.count);
    const sum = [0, 0, 0];
    for (let i = 0; i < n; i++) {
      const c = colour(i);
      for (let k = 0; k < 3; k++) {
        sum[k] += c[k] & 0xFF;
      }
    }
    return sum.map((s) => Math.round(s / n));
  }

  // One record of OpenRGB's SetMode() + SetZoneColor() (colour 1 = colour 2) for `effect`.
  function staticRecord(p, off, effect, rgb, zone) {
    const speed = effect === MODE_BREATHING ? SPEED_MEDIUM : 0;
    p[off + EFFECT] = effect;
    p[off + SPEED_BRIGHTNESS] = ((BRIGHTNESS_LEVEL_100 << BRIGHTNESS_SHIFT) & BRIGHTNESS_MASK) | (speed & SPEED_MASK);
    p[off + COLOR_FLAGS] = (p[off + COLOR_FLAGS] | USE_CUSTOM_COLOR) & 0xFF;
    p[off + PADDING] = zone === Z.J_RAINBOW_3 ? 4 : 0;
    put(p, off + COLOR, rgb);
    put(p, off + COLOR2, rgb);
  }

  // OpenRGB's Static (or Breathing) mode on every zone of the board with the zone's colour, built
  // on the restore image (the configuration the board had at start): MSIMotherboard185Controller /
  // MSIMotherboard162Controller SetMode() and SetZoneColor(), then the save byte.
  function staticPacket(colourOf, effect, save) {
    const L = variant === 185 ? LAYOUT_185 : LAYOUT_162;
    const p = restoreImage.slice();
    if (variant === 185) {
      const at = L.J_CORSAIR + PADDING;
      p[at] = p[at] >= CORSAIR_READBACK_BIAS ? p[at] - CORSAIR_READBACK_BIAS : p[at];
    }
    for (const zone of zones) {
      const rgb = zoneColour(zone, colourOf);
      if (variant === 185) {
        const off = zoneData185(zone.type);
        if (off < 0) {
          continue;
        }
        staticRecord(p, off, effect, rgb, zone.type);
        const rainbow = rainbowZoneData185(zone.type);
        if (rainbow >= 0 && zone.count > 0) {
          p[rainbow + LED_COUNT] = zone.count & 0xFF;
        }
        if (zone.type === Z.ON_BOARD_LED_0) {
          // SetMode() for modes up to Lightning: the onboard zone leaves the JRGB / pipe sync;
          // SetZoneColor() / SetMode() copy the zone to every onboard LED record.
          p[off + SPEED_BRIGHTNESS] &= ~SYNC_SETTING_JRGB & 0xFF;
          p[off + COLOR_FLAGS] = (p[off + COLOR_FLAGS] | SYNC_ONBOARD) & ~(SYNC_JPIPE1 | SYNC_JPIPE2) & 0xFF;
          for (let i = 0; i < cfg.onboard; i++) {
            const led = zoneData185(Z.ON_BOARD_LED_1 + i);
            if (led >= 0) {
              staticRecord(p, led, effect, rgb, Z.ON_BOARD_LED_1 + i);
            }
          }
        }
      } else {
        // 162: what updateLed162() does for each LED value of the zone, in `effect`.
        for (let led = zone.count - 1; led >= 0; led--) {
          const type = zone.type + led;
          const off = zoneData162(type);
          if (off >= 0 && type <= Z.ON_BOARD_LED_0) {
            staticRecord(p, off, effect, rgb, type);
            p[off + SPEED_BRIGHTNESS] &= ~SYNC_SETTING_JRGB & 0xFF;
            p[off + COLOR_FLAGS] &= ~SYNC_ONBOARD & 0xFF;
          }
          if (type >= Z.ON_BOARD_LED_0) {
            const next = zoneData162(type + 1);
            if (next >= 0) {
              staticRecord(p, next, effect, rgb, type + 1);
            }
          }
        }
        p[L.ON_BOARD_LED + COLOR_FLAGS] = (p[L.ON_BOARD_LED + COLOR_FLAGS] | USE_CUSTOM_COLOR | SYNC_ONBOARD) & 0xFF;
      }
    }
    p[L.SAVE] = save ? SAVE_ON : 0;
    return p;
  }

  // Why save() cannot run on this board (null: it can).
  function saveRefusal() {
    if (!ready || restoreImage === null) {
      return "the board's configuration could not be read at start, so HueDeck writes nothing to it";
    }
    if (variant === 112) {
      return "112-byte Mystic Light boards cannot be saved by HueDeck";
    }
    return null;
  }

  /**
   * "Save to devices": what would be stored ({ zones: [{name, color}] }) or { unsupported }.
   * `colourOf` as for frame(); `effect` "breathing" or null (static).
   */
  function describeSave(colourOf, effect) {
    const refusal = saveRefusal();
    if (refusal !== null) {
      return { unsupported: refusal };
    }
    if (effect !== null && effect !== "breathing") {
      return { unsupported: `the board has no "${effect}" effect HueDeck can store` };
    }
    return {
      effect: effect ?? "static",
      zones: zones.map((zone) => {
        const rgb = zoneColour(zone, colourOf);
        return { name: zone.name, color: `#${rgb.map((v) => v.toString(16).padStart(2, "0")).join("")}` };
      }),
      note: "One colour per zone (the average of its LEDs): the board stores zones, not single LEDs.",
    };
  }

  /**
   * Store describeSave()'s configuration in the board's flash (OpenRGB's Update(true)), then go
   * back to Direct for the live effect. What the board now holds becomes the restore image.
   */
  function save(colourOf, effect) {
    const result = describeSave(colourOf, effect);
    if (result.unsupported) {
      return result;
    }
    const packet = staticPacket(colourOf, effect === "breathing" ? MODE_BREATHING : MODE_STATIC, true);
    if (variant === 185) {
      // ReadSettings(), the board's own state with the save byte, then the new state with it.
      const current = readSettings(LAYOUT_185.length);
      if (current === null) {
        throw new Error(`${label}: GET 0x52 failed before the save; nothing was written`);
      }
      const at = LAYOUT_185.J_CORSAIR + PADDING;
      current[at] = current[at] >= CORSAIR_READBACK_BIAS ? current[at] - CORSAIR_READBACK_BIAS : current[at];
      current[LAYOUT_185.SAVE] = SAVE_ON;
      if (!send(current) || !send(packet)) {
        throw new Error(`${label}: send_report(0x52) failed during the save`);
      }
      const stored = readSettings(LAYOUT_185.length);
      restoreImage = stored ?? packet.map((b, i) => (i === LAYOUT_185.SAVE ? 0 : b));
      directMode = false;
      setDirectMode185();
    } else {
      if (!send(packet)) {
        throw new Error(`${label}: send_report(0x52) failed during the save`);
      }
      restoreImage = packet.slice();
      restoreImage[LAYOUT_162.SAVE] = 0;
    }
    lastFrame = null;
    return result;
  }

  /** Exit modes "keep" / "off": OpenRGB's Static mode with these colours, save byte 0. */
  function showStatic(colourOf) {
    if (!ready || restoreImage === null) {
      return;
    }
    if (variant === 112) {
      frame(colourOf, true);
      return;
    }
    send(staticPacket(colourOf, MODE_STATIC, false));
    lastFrame = null;
  }

  // ------------------------------------------------------------------------------- zones
  // The zones of RGBController_MSIMotherboard{185,162,112}::SetupZones(), in order. Rows are
  // zones on the device's LED grid (one row each); channels are the resizable ARGB zones of
  // per-LED boards.
  function buildZones(channelCount) {
    const out = [];
    let row = 0;
    if (variant === 185) {
      const perLed = cfg.directMode === DIRECT_PER_LED;
      for (const key of ZONE_SETS_185[cfg.zones]) {
        const type = Z[key];
        const zoneName = ZONE_NAMES_185[key];
        if (zoneName === undefined) {
          continue;
        }
        if (!RESIZABLE_185.has(type)) {
          const count = maxDirectLeds185(type);
          const names = [];
          for (let i = 0; i < count; i++) {
            names.push(`${zoneName}, LED ${i + 1}`);
          }
          out.push({ key, type, name: zoneName, kind: "row", count, max: count, names, row: count > 0 ? row++ : -1 });
        } else if (!perLed) {
          // Zone-based board: a resizable zone is "effects only" and takes one colour (index 0).
          // OpenRGB creates it with 0 LEDs; HueDeck sizes it to 1 straight away (target).
          out.push({ key, type, name: zoneName, kind: "row", count: 0, target: 1, max: ZONE_BASED_MAX_LEDS, names: [zoneName], row: row++ });
        } else {
          // Per-LED board: an ARGB header whose LED count comes from the channel's components.
          const max = maxDirectLeds185(type);
          out.push({ key, type, name: zoneName, kind: "channel", count: 0, target: channelCount(zoneName, max), max, row: -1 });
        }
      }
      return out;
    }
    const keys = variant === 162 ? ZONE_SETS_162[cfg.zones] : ZONES_112;
    const names = variant === 162 ? ZONE_NAMES_162 : ZONE_NAMES_112;
    const onboard = variant === 162 ? onboard162 : ONBOARD_LEDS_112;
    for (const key of keys) {
      const type = Z[key];
      const zoneName = names[key];
      const count = type === Z.ON_BOARD_LED_0 ? onboard : 1;
      const ledNames = [];
      for (let i = 0; i < count; i++) {
        ledNames.push(count > 1 ? `${zoneName} LED ${i + 1}` : `${zoneName} LED`);
      }
      out.push({ key, type, name: zoneName, kind: "row", count, max: count, names: ledNames, row: count > 0 ? row++ : -1 });
    }
    return out;
  }

  // RGBController_MSIMotherboard185::SetupZones(): every zone's LED count goes to the controller in
  // zone order (SetZoneLEDCount sends the enable message for each JRAINBOW zone while Direct is active).
  function setupZones185() {
    for (const zone of zones) {
      setZoneLedCount185(zone.type, zone.count);
    }
  }

  // ------------------------------------------------------------------------------- public API
  function initialize({ productId, serial, channelCount }) {
    ready = false;
    wrote = false;
    lastFrame = null;
    lastFrameAt = -Infinity;
    restoreImage = null;

    // DetectMSIMotherboardControllers()
    const probe = dev.get_report([CONFIG_REPORT], DETECT_LENGTH);
    const probeSize = readSize(probe);
    variant = variantForLength(probeSize);
    if (variant === 0) {
      log(`GET_FEATURE 0x52 returned ${probeSize} bytes: not a 185-, 162- or 112-byte Mystic Light controller, nothing will be written.`);
      return { ok: false };
    }

    const aprom = readFirmwareVersion();

    if (variant === 185) {
      const pid = boardPid185(productId, serial);
      const row = BOARDS_185.find((r) => r.pid === pid);
      cfg = row ?? DEFAULT_185;
      if (row !== undefined && aprom < row.minFirmware) {
        log(`firmware (APROM 0x${aprom.toString(16)}) is below the minimum version ${row.minFirmware} OpenRGB requires for this board; nothing will be written.`);
        return { ok: false };
      }
      mixedMode = pid >= MIXED_MODE_MIN_PID;
      data = readSettings(LAYOUT_185.length);
      if (data === null) {
        return { ok: false };
      }
      restoreImage = data.slice();
      if (data[LAYOUT_185.ON_BOARD_LED + EFFECT] === MSI_DIRECT_MODE) {
        log("the board is already in per-LED mode (left by another program or an earlier session that did not exit); the restore image keeps that mode.");
      }
      name = row ? `MSI ${row.name}` : `${label} (185-byte)`;
      if (cfg.directMode === DIRECT_DISABLED) {
        log(`${row ? row.name : `board 0x${Number.isNaN(pid) ? "????" : pid.toString(16).toUpperCase()}`} has no Direct mode in OpenRGB; nothing will be written.`);
        return { ok: false };
      }

      // Per-LED buffers and the synchronised stream layout (MSIMotherboard185Controller constructor).
      perLedSync = perLedPacket(0x06, 0x00);
      perLedJr1 = perLedPacket(0x04, 0x00);
      perLedJr2 = perLedPacket(0x04, 0x01);
      perLedJc = perLedPacket(0x05, 0x00);
      const pipe1 = cfg.onboard;
      const pipe2 = pipe1 + cfg.pipe1;
      const jrgb = pipe2 + cfg.pipe2;
      const jrainbow1 = jrgb + cfg.jrgbs;
      const jrainbow2 = jrainbow1 + SYNC_PER_LED_MODE_JRAINBOW_LED_COUNT;
      const jcorsair = jrainbow2 + SYNC_PER_LED_MODE_JRAINBOW_LED_COUNT;
      slots = { onboard: 0, pipe1, pipe2, jrgb, jrainbow1, jrainbow2, jcorsair };
      const set = ZONE_SETS_185[cfg.zones];
      noOnboards = !set.includes("ON_BOARD_LED_0");
      noJr1 = !set.includes("J_RAINBOW_1");
      noJr2 = !set.includes("J_RAINBOW_2");
      noJc = !(set.includes("J_CORSAIR") || set.includes("J_RAINBOW_3"));
      if (cfg.directMode === DIRECT_PER_LED && noOnboards && noJr1 && noJr2 && noJc) {
        // OpenRGB sends the per-LED switch and then never a stream (Update() skips boards with no
        // streamed zone), which leaves the LEDs dark. HueDeck keeps the board's own effect instead.
        log(`${name}: no zone OpenRGB streams in per-LED mode; nothing will be written.`);
        return { ok: false };
      }
      enableMsg = enablePerLedMessage();
      zoneBased = zoneBased185();
      directMode = false;
      directConfigured = false;
      syncDirectMode = true;

      // RGBController constructor: SetupZones() with every resizable zone at 0 LEDs.
      zones = buildZones(channelCount);
      setupZones185();
      // GetDeviceMode() -> GetMode(): a board already in per-LED mode counts as Direct.
      if (data[LAYOUT_185.ON_BOARD_LED + EFFECT] === MSI_DIRECT_MODE) {
        directMode = true;
      }
      ready = true;
      // The saved zone sizes, as OpenRGB's profile manager applies them: ResizeZone() for each
      // resizable zone that has LEDs, in zone order (each one re-runs SetupZones()).
      for (const zone of zones) {
        if (zone.target !== undefined && zone.target > 0 && zone.target !== zone.count) {
          zone.count = Math.min(zone.target, zone.max);
          setupZones185();
        }
      }
      // DeviceUpdateMode() with Direct selected.
      setDirectMode185();
      return { ok: true };
    }

    const L = variant === 162 ? LAYOUT_162 : LAYOUT_112;
    const image = readSettings(L.length);
    if (image === null) {
      return { ok: false };
    }
    restoreImage = image.slice();
    if (variant === 162) {
      cfg = boardConfig162(productId);
      onboard162 = cfg.onboard;
      name = cfg.name ? `MSI ${cfg.name}` : `${label} (162-byte)`;
      data162 = image.slice();
      data162[L.SAVE] = 0;
      data162[L.ON_BOARD_LED + COLOR_FLAGS] = USE_CUSTOM_COLOR | SYNC_ONBOARD;
      zones = buildZones(channelCount);
      ready = true;
      // DeviceUpdateMode() = DeviceUpdateLEDs() with the colours GetDeviceConfig() read from the board.
      const boardColour = (zone) => {
        const off = zoneData162(zone.type);
        return off < 0 ? [0, 0, 0] : [image[off + COLOR], image[off + COLOR + 1], image[off + COLOR + 2]];
      };
      frame((zone) => {
        const rgb = boardColour(zone);
        return () => rgb;
      }, true);
      return { ok: true };
    }
    // 112
    cfg = { name: null };
    name = `${label} (112-byte)`;
    zoneBased = zoneBased112();
    zones = buildZones(channelCount);
    ready = true;
    // DeviceUpdateMode(): SetDirectMode(true); nothing is sent until the first frame.
    return { ok: true };
  }

  /**
   * One Render(): `colourOf(zone)` returns a function (index) -> [r, g, b] for that zone; `force`
   * sends even an unchanged frame.
   */
  function frame(colourOf, force = false) {
    if (!ready) {
      return;
    }
    let packets;
    if (variant === 185) {
      for (const zone of zones) {
        const colour = colourOf(zone);
        for (let led = zone.count - 1; led >= 0; led--) {
          setLedColor185(zone.type, led, colour(led));
        }
      }
      // Update(): a first frame without the enable message re-selects the protocol first.
      if (cfg.directMode === DIRECT_PER_LED && !(noOnboards && noJr1 && noJr2 && noJc) && !directConfigured) {
        if (!selectPerLedProtocol()) {
          return;
        }
        lastFrame = null;
      }
      packets = updatePackets185();
    } else if (variant === 162) {
      for (const zone of zones) {
        const colour = colourOf(zone);
        for (let led = zone.count - 1; led >= 0; led--) {
          updateLed162(zone.type + led, colour(led));
        }
      }
      data162[LAYOUT_162.SAVE] = 0;
      packets = [{ packet: data162.slice() }];
    } else {
      for (const zone of zones) {
        const colour = colourOf(zone);
        for (let led = zone.count - 1; led >= 0; led--) {
          setLedColor112(zone.type + led, colour(led));
        }
      }
      packets = [{ packet: zoneBased.slice() }];
    }
    if (packets === null) {
      return;
    }
    const now = Date.now();
    if (!force && lastFrame !== null && samePackets(packets, lastFrame) && now - lastFrameAt < KEEPALIVE_MS) {
      return;
    }
    for (const { packet, pause } of packets) {
      send(packet);
      if (pause) {
        dev.pause(pause);
      }
    }
    lastFrame = packets;
    lastFrameAt = now;
  }

  /**
   * LED counts of the channel zones changed (OpenRGB: ResizeZone() -> DeviceConfigureZone() ->
   * SetupZones()). `counts` maps zone name -> new count; zones are resized one at a time, in order.
   */
  function resize(counts) {
    if (!ready || variant !== 185) {
      return;
    }
    for (const zone of zones) {
      if (zone.kind !== "channel" || !(zone.name in counts)) {
        continue;
      }
      const count = Math.max(0, Math.min(zone.max, counts[zone.name] | 0));
      if (count === zone.count) {
        continue;
      }
      zone.count = count;
      setupZones185();
      lastFrame = null;
    }
  }

  // Writes the image read at start-up back (save byte 0; 185: J_CORSAIR +9 bias removed). Nothing to
  // undo if nothing was written.
  function restore() {
    if (restoreImage === null || !wrote) {
      return;
    }
    const packet = restoreImage.slice();
    const L = variant === 185 ? LAYOUT_185 : variant === 162 ? LAYOUT_162 : LAYOUT_112;
    if (variant === 185) {
      const at = LAYOUT_185.J_CORSAIR + PADDING;
      packet[at] = packet[at] >= CORSAIR_READBACK_BIAS ? packet[at] - CORSAIR_READBACK_BIAS : packet[at];
    }
    packet[L.SAVE] = 0;
    send(packet);
    wrote = false;
    lastFrame = null;
  }

  return {
    initialize,
    frame,
    resize,
    restore,
    describeSave,
    save,
    showStatic,
    get ready() { return ready; },
    get variant() { return variant; },
    get name() { return name; },
    get zones() { return zones; },
    get syncDirectMode() { return syncDirectMode; },
  };
}

function samePackets(a, b) {
  if (a.length !== b.length) {
    return false;
  }
  for (let i = 0; i < a.length; i++) {
    const x = a[i].packet;
    const y = b[i].packet;
    if (x.length !== y.length) {
      return false;
    }
    for (let j = 0; j < x.length; j++) {
      if (x[j] !== y[j]) {
        return false;
      }
    }
  }
  return true;
}
