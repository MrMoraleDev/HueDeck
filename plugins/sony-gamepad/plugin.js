// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: the light bar of the Sony DualShock 4 (and of its USB wireless adapter), and the
// light bar and the five player LEDs of the Sony DualSense and DualSense Edge, over USB.
// Adapted from OpenRGB Controllers/SonyGamepadController/SonyGamepadControllerDetect.cpp,
// SonyDS4Controller/SonyDS4Controller.cpp, SonyDS4Controller/SonyDS4Controller.h,
// SonyDS4Controller/RGBController_SonyDS4.cpp, SonyDualSenseController/SonyDualSenseController.cpp,
// SonyDualSenseController/SonyDualSenseController.h and
// SonyDualSenseController/RGBController_SonyDualSense.cpp (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors, as in those files' headers:
//   "Pol Rius (alpemwarrior)                     24 Sep 2020"
//   "Flora Aubry                                 01 Jul 2022"
//   "Yoan Berthelot                              06 Mar 2025"
//
// Where this differs from OpenRGB, and why (docs/hardware/sony-gamepad.md has the details):
//   * USB only. OpenRGB also builds Bluetooth reports (0x11 / 0x31 with a CRC-32); this plugin
//     does not, and Validate() refuses Bluetooth collections.
//   * Every output report keeps OpenRGB's bytes but only the light bar and player LED "valid"
//     flags. OpenRGB's DualShock 4 report also enables the rumble motors (its zero motor bytes stop
//     any rumble), and its DualSense report also enables vibration, haptics, both adaptive
//     triggers, the microphone LED (which its Direct mode lights) and undocumented flags. Lightloom
//     clears those flags (and the microphone LED byte), so the controller ignores those fields.
//   * OpenRGB's "Mic Off (Direct)" and "Mic Pulse (Direct)" modes only change the microphone LED;
//     they are left out. Its Direct mode brightness (player LEDs) is the "Player LED Brightness"
//     parameter.
//   * OpenRGB sends nothing when it exits; Shutdown() turns the LEDs it drove off (black), since
//     the protocol as OpenRGB uses it has no command that hands the lighting back.
//
// Tested on hardware (2026-10-22): DualSense (054C:0CE6) over USB, light bar. The DualShock 4 ids
// and the DualSense Edge (0x0DF2) are untested.

/* global device */

// ---------------------------------------------------------------------------------------------
// Products (SonyGamepadControllerDetect.cpp)
// ---------------------------------------------------------------------------------------------
const SONY_VID = 0x054C;
const SONY_DS4_V1_PID = 0x05C4;
const SONY_DS4_V2_PID = 0x09CC;
const SONY_DS4_RECEIVER_PID = 0x0BA0;
const SONY_DUALSENSE_PID = 0x0CE6;
const SONY_DUALSENSE_EDGE_PID = 0x0DF2;

const DS4 = "ds4";
const DUALSENSE = "dualsense";

// Names: RGBController_SonyDS4 calls every DualShock 4 product "Sony DualShock 4";
// RGBController_SonyDualSense takes the detector's name ("Sony DualSense", "Sony DualSense Edge").
const PRODUCTS = new Map([
  [SONY_DS4_V1_PID, { name: "Sony DualShock 4", protocol: DS4 }],
  [SONY_DS4_V2_PID, { name: "Sony DualShock 4", protocol: DS4 }],
  [SONY_DS4_RECEIVER_PID, { name: "Sony DualShock 4", protocol: DS4 }],
  [SONY_DUALSENSE_PID, { name: "Sony DualSense", protocol: DUALSENSE }],
  [SONY_DUALSENSE_EDGE_PID, { name: "Sony DualSense Edge", protocol: DUALSENSE }],
]);

// Geometry. DualShock 4: one zone with one LED (OpenRGB: zone "Controller Zone", LED
// "Controller LED"). DualSense: zone "Lightbar" (one LED, OpenRGB's "LED 1") and the linear zone
// "Player LEDs" ("Player 1".."Player 5"), one row each; the light bar sits above the middle of the
// player row, as on the controller. Order = OpenRGB's colour order on the wire.
const DS4_GEOMETRY = { size: [1, 1], names: ["Lightbar"], positions: [[0, 0]] };
const DUALSENSE_GEOMETRY = {
  size: [5, 2],
  names: ["Lightbar", "Player 1", "Player 2", "Player 3", "Player 4", "Player 5"],
  positions: [[2, 0], [0, 1], [1, 1], [2, 1], [3, 1], [4, 1]],
};

// ---------------------------------------------------------------------------------------------
// Metadata
// ---------------------------------------------------------------------------------------------
export function Name() { return "Sony DualShock 4 / DualSense"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/sony-gamepad"; }
export function VendorId() { return SONY_VID; }
// The DualSense Edge takes the DualSense code path unchanged (same OpenRGB detector function).
export function ProductId() {
  return [SONY_DS4_V1_PID, SONY_DS4_V2_PID, SONY_DS4_RECEIVER_PID, SONY_DUALSENSE_PID, SONY_DUALSENSE_EDGE_PID];
}
export function Type() { return "Hid"; }
export function DeviceType() { return "other"; }
// Placeholder (the light bar alone, common to every product); Initialize() sets the product's own.
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ["Lightbar"]; }
export function LedPositions() { return [[0, 0]]; }
export function ImageUrl() { return ""; }

// OpenRGB's DualSense Direct mode has a brightness from SONY_DUALSENSE_BRIGHTNESS_MIN (0) to
// SONY_DUALSENSE_BRIGHTNESS_MAX (2), default 1; it only goes to the led_brightness byte.
const BRIGHTNESS = { Low: 0, Medium: 1, High: 2 };
const DEFAULT_BRIGHTNESS = "Medium";

export function ControllableParameters() {
  return [
    { property: "playerLedBrightness", group: "lighting", label: "Player LED Brightness",
      description: "DualSense and DualSense Edge only: brightness of the five player LEDs below the touch pad.",
      type: "combobox", values: ["Low", "Medium", "High"], default: DEFAULT_BRIGHTNESS },
  ];
}

// ---------------------------------------------------------------------------------------------
// DualShock 4: output report 0x05 (SonyDS4Controller::sendReportUSB)
// ---------------------------------------------------------------------------------------------
// OpenRGB writes 11 bytes; on Windows the write is padded with zeros to the collection's 32-byte
// output report (hidapi does the same for OpenRGB).
const DS4_REPORT_ID = 0x05;
const DS4_REPORT_LENGTH = 11;
// Byte 1, valid_flag0. OpenRGB sends 0x07: motors (0x01), light bar colour (0x02) and light bar
// blink (0x04). The motor flag is cleared: with it the controller applies the zero motor bytes
// 4 and 5 and stops any rumble. Blink on/off times 0 (bytes 9, 10) keep the light bar steady.
const DS4_VALID_FLAGS = 0x02 | 0x04;
// SonyDS4Controller's constructor: GET_FEATURE 0x02 (64 bytes), then up to five input reports;
// a report 0x11 (17) means Bluetooth, where OpenRGB switches to the 0x11 output report.
const DS4_PROBE_FEATURE_ID = 0x02;
const DS4_PROBE_LENGTH = 64;
const DS4_PROBE_READS = 5;
const DS4_BT_INPUT_REPORT_ID = 0x11;
// OpenRGB's hid_read() blocks; the controller sends an input report every few milliseconds.
const INPUT_TIMEOUT_MS = 100;

function ds4Report(rgb) {
  return [DS4_REPORT_ID, DS4_VALID_FLAGS, 0x00, 0x00, 0x00, 0x00,
    rgb[0] & 0xFF, rgb[1] & 0xFF, rgb[2] & 0xFF, 0x00, 0x00];
}

// ---------------------------------------------------------------------------------------------
// DualSense: output report 0x02 (SonyDualSenseController::SetColors, USB branch)
// ---------------------------------------------------------------------------------------------
const DS_REPORT_ID = 0x02;
const DS_REPORT_LENGTH = 48;            // SONY_DUALSENSE_USB_PACKET_SIZE
// Byte offsets, report id = byte 0.
const DS_VALID_FLAG0 = 1;
const DS_VALID_FLAG1 = 2;
const DS_VALID_FLAG2 = 39;
const DS_LED_BRIGHTNESS = 43;
const DS_PLAYER_LEDS = 44;
const DS_LIGHTBAR_RED = 45;             // then green (46) and blue (47)
// OpenRGB sends valid_flag0 0x0F (vibration, haptics select, right and left trigger effects):
// all cleared. valid_flag1 0x55: light bar colour (0x04) and player LEDs (0x10) kept; microphone
// LED (0x01) and 0x40 cleared. valid_flag2 0xFF: player LED brightness (0x01) and light bar setup
// (0x02, lightbar_setup byte 42 stays 0 as in OpenRGB) kept; 0x04 (vibration) to 0x80 cleared.
// Byte 9 (microphone LED), where OpenRGB writes its mode value, stays 0.
const DS_FLAG0 = 0x00;
const DS_FLAG1 = 0x04 | 0x10;
const DS_FLAG2 = 0x01 | 0x02;
// player_leds: OpenRGB adds 0x20 to the five LED bits (Player 1 = bit 0 .. Player 5 = bit 4).
const DS_PLAYER_LEDS_BASE = 0x20;

function dualSenseReport(lightbar, players, brightness) {
  const report = new Array(DS_REPORT_LENGTH).fill(0x00);
  report[0] = DS_REPORT_ID;
  report[DS_VALID_FLAG0] = DS_FLAG0;
  report[DS_VALID_FLAG1] = DS_FLAG1;
  report[DS_VALID_FLAG2] = DS_FLAG2;
  report[DS_LED_BRIGHTNESS] = (0x02 - brightness) & 0xFF;
  let bits = 0;
  for (let i = 0; i < players.length; i++) {
    if (isLit(players[i])) {
      bits |= 1 << i;
    }
  }
  report[DS_PLAYER_LEDS] = DS_PLAYER_LEDS_BASE + bits;
  report[DS_LIGHTBAR_RED] = lightbar[0] & 0xFF;
  report[DS_LIGHTBAR_RED + 1] = lightbar[1] & 0xFF;
  report[DS_LIGHTBAR_RED + 2] = lightbar[2] & 0xFF;
  return report;
}

// ---------------------------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------------------------
let product = null;
let enabled = false;

// OpenRGB's REGISTER_HID_DETECTOR takes any collection of these product ids. Over USB the
// controllers have one HID collection, the game pad (usage page 0x0001, usage 0x0005; interface 3
// on the DualSense). Interface -1 is a HID device without a USB interface (Bluetooth): refused.
export function Validate(endpoint) {
  return endpoint.interface >= 0 && endpoint.usage_page === 0x0001 && endpoint.usage === 0x0005;
}

export function Initialize() {
  enabled = false;
  product = PRODUCTS.get(Number(device.productId())) || null;
  if (product === null) {
    device.log(`Sony gamepad: unknown product id 0x${hex4(device.productId())}; nothing is sent.`);
    return;
  }
  const geometry = product.protocol === DS4 ? DS4_GEOMETRY : DUALSENSE_GEOMETRY;
  device.setName(product.name);
  device.setSize(geometry.size);
  device.setControllableLeds(geometry.names, geometry.positions);
  // The DualSense controller object does no I/O when OpenRGB creates it; the DualShock 4 one
  // probes for Bluetooth.
  if (product.protocol === DS4 && ds4SendsBluetoothReports()) {
    device.log("Sony DualShock 4: the controller sends Bluetooth input reports (0x11); only the USB report is supported, nothing is sent.");
    return;
  }
  enabled = true;
}

export function Render() {
  if (!enabled) {
    return;
  }
  if (product.protocol === DS4) {
    device.write(ds4Report(sample(DS4_GEOMETRY.positions[0])), DS4_REPORT_LENGTH);
  } else {
    const [lightbar, ...players] = DUALSENSE_GEOMETRY.positions.map(sample);
    device.write(dualSenseReport(lightbar, players, playerBrightness()), DS_REPORT_LENGTH);
  }
}

// Light bar black, player LEDs off; the same on system suspend (SystemSuspending is not needed).
export function Shutdown() {
  if (!enabled) {
    return;
  }
  const black = [0, 0, 0];
  if (product.protocol === DS4) {
    device.write(ds4Report(black), DS4_REPORT_LENGTH);
  } else {
    device.write(dualSenseReport(black, [black, black, black, black, black], playerBrightness()), DS_REPORT_LENGTH);
  }
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------
// SonyDS4Controller's constructor, read-only: hid_get_feature_report(0x02, 64) and up to five
// hid_read(64) looking for an input report 0x11.
function ds4SendsBluetoothReports() {
  device.get_report([DS4_PROBE_FEATURE_ID], DS4_PROBE_LENGTH);
  for (let i = 0; i < DS4_PROBE_READS; i++) {
    const input = device.read([0x00], DS4_PROBE_LENGTH, INPUT_TIMEOUT_MS);
    if (device.getLastReadSize() > 0 && input[0] === DS4_BT_INPUT_REPORT_ID) {
      return true;
    }
  }
  return false;
}

function sample(position) {
  return device.color(position[0], position[1]);
}

function hex4(value) {
  return (Number(value) & 0xFFFF).toString(16).toUpperCase().padStart(4, "0");
}

// OpenRGB lights a player LED when its RGBColor is not 0, i.e. any channel is not 0.
function isLit(rgb) {
  return ((rgb[0] & 0xFF) | (rgb[1] & 0xFF) | (rgb[2] & 0xFF)) !== 0;
}

// The parameter is a host-provided global; the declared default when it is absent or unknown.
function playerBrightness() {
  const value = globalThis.playerLedBrightness;
  return Object.prototype.hasOwnProperty.call(BRIGHTNESS, value) ? BRIGHTNESS[value] : BRIGHTNESS[DEFAULT_BRIGHTNESS];
}
