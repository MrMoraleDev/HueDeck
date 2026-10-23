// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: Razer report builders (no device I/O here), used by plugin.js.
// Adapted from OpenRGB Controllers/RazerController/RazerController/RazerController.cpp and
// RazerController.h, and Controllers/RazerController/RazerKrakenV4Controller/
// RazerKrakenV4Controller.cpp and RazerKrakenV4Controller.h (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Adam Honse (CalcProgrammer1)  22 Jan 2021" (RazerController.cpp/.h),
// "Adam Honse (CalcProgrammer1)  21 Oct 2025" (RazerKrakenV4Controller.cpp/.h).
//
// Every function returns the whole report as a byte array, report id first, exactly as OpenRGB
// hands it to hidapi (struct razer_report: 91 bytes; razer_argb_report: 321 bytes;
// razer_kraken_v4_report: 64 bytes).

/** sizeof(razer_report): report id, status, transaction id, remaining packets (2), protocol
 * type, data size, command class, command id, 80 argument bytes, crc, reserved. */
export const REPORT_LENGTH = 91;
/** sizeof(razer_argb_report): hid id, report id, channel 1, channel 2, pad, last index, 315 colour bytes. */
export const ARGB_REPORT_LENGTH = 321;
/** sizeof(razer_kraken_v4_report). */
export const KRAKEN_V4_REPORT_LENGTH = 64;

const ARGS = 9;

export const STORAGE_NO_SAVE = 0x00;
export const DEVICE_MODE_HARDWARE = 0x00;
export const DEVICE_MODE_SOFTWARE = 0x03;

export const LED_ID_ZERO = 0x00;
export const LED_ID_SCROLL_WHEEL = 0x01;
export const LED_ID_LOGO = 0x04;
export const LED_ID_BACKLIGHT = 0x05;
export const LED_ID_ARGB_CH_1 = 0x1A;

export const STATUS_NEW = 0x00;
export const STATUS_BUSY = 0x01;
export const STATUS_SUCCESS = 0x02;

export const COMMAND_ID_GET_DEVICE_MODE = 0x84;
export const COMMAND_ID_GET_KEYBOARD_INFO = 0x86;
export const COMMAND_ID_GET_WIRELESS_CONNECTIONS = 0xBF;

/** razer_calculate_crc(): XOR of bytes 3..88 (the report id field shifts OpenRazer's 2..87). */
export function crc(report) {
  let value = 0;
  for (let i = 3; i < 89; i++) {
    value ^= report[i];
  }
  return value;
}

/**
 * razer_create_report(): zeroed report with report id, transaction id, data size, command class
 * and command id. `ctx` = { reportIndex, transactionId }. The crc is filled in here (OpenRGB's
 * razer_usb_send() does it just before sending).
 */
export function report(ctx, commandClass, commandId, dataSize, args = []) {
  const bytes = new Array(REPORT_LENGTH).fill(0);
  bytes[0] = ctx.reportIndex & 0xFF;
  bytes[1] = 0x00;
  bytes[2] = ctx.transactionId & 0xFF;
  bytes[6] = dataSize & 0xFF;
  bytes[7] = commandClass & 0xFF;
  bytes[8] = commandId & 0xFF;
  for (let i = 0; i < args.length && i < 80; i++) {
    bytes[ARGS + i] = args[i] & 0xFF;
  }
  bytes[89] = crc(bytes);
  return bytes;
}

/** Whether `response` (a get_report answer, report id first) is the final answer to `request`
 * (razer_usb_query()/razer_usb_send(): same transaction id, class and command; status neither new
 * nor busy). */
export function isFinalResponse(request, response) {
  if (!response || response.length < ARGS) {
    return false;
  }
  return response[2] === request[2] && response[7] === request[7] && response[8] === request[8]
    && response[1] !== STATUS_NEW && response[1] !== STATUS_BUSY;
}

/** Argument byte `i` of a response (report id first). */
export function arg(response, i) {
  return response && response.length > ARGS + i ? response[ARGS + i] : 0;
}

// --- requests ---------------------------------------------------------------------------------

/** razer_get_device_mode() request: class 0x00, id 0x84, size 0x02. */
export function getDeviceMode(ctx) {
  return report(ctx, 0x00, COMMAND_ID_GET_DEVICE_MODE, 0x02);
}

/** razer_get_keyboard_info() request: class 0x00, id 0x86, size 0x00. */
export function getKeyboardInfo(ctx) {
  return report(ctx, 0x00, COMMAND_ID_GET_KEYBOARD_INFO, 0x00);
}

/** GetPairedDevicePIDs() request: class 0x00, id 0xBF, size 0x50. */
export function getWirelessConnections(ctx) {
  return report(ctx, 0x00, COMMAND_ID_GET_WIRELESS_CONNECTIONS, 0x50);
}

// Battery queries (read-only). Not in OpenRGB: the command class, ids, data size and answer byte
// are protocol facts from OpenRazer's driver (razerchromacommon.c, GPL-2.0-or-later; no code
// copied): class 0x07, size 0x02, no arguments; the answer is argument 1. See docs/hardware/razer.md.
export const COMMAND_CLASS_MISC = 0x07;
export const COMMAND_ID_GET_BATTERY_LEVEL = 0x80;
export const COMMAND_ID_GET_CHARGING_STATUS = 0x84;
export const STATUS_NOT_SUPPORTED = 0x05;

/** Battery level request: 0x07 0x80, size 0x02; answer argument 1 = level 0..255. */
export function getBatteryLevel(ctx) {
  return report(ctx, COMMAND_CLASS_MISC, COMMAND_ID_GET_BATTERY_LEVEL, 0x02);
}

/** Charging status request: 0x07 0x84, size 0x02; answer argument 1 = 1 while charging. */
export function getChargingStatus(ctx) {
  return report(ctx, COMMAND_CLASS_MISC, COMMAND_ID_GET_CHARGING_STATUS, 0x02);
}

// --- settings ---------------------------------------------------------------------------------

/** razer_create_device_mode_report(mode, param). */
export function deviceMode(ctx, mode, param = 0x00) {
  return report(ctx, 0x00, 0x04, 0x02, [mode, param]);
}

/** razer_create_brightness_standard_report(). */
export function brightnessStandard(ctx, storage, ledId, brightness) {
  return report(ctx, 0x03, 0x03, 0x03, [storage, ledId, brightness]);
}

/** razer_create_brightness_extended_matrix_report(). */
export function brightnessExtended(ctx, storage, ledId, brightness) {
  return report(ctx, 0x0F, 0x04, 0x03, [storage, ledId, brightness]);
}

/** razer_create_addressable_size_report(): LED count per ARGB channel (0x19 marks a used channel). */
export function addressableSize(ctx, sizes) {
  const args = [0x06];
  for (let i = 0; i < 6; i++) {
    const size = sizes[i] & 0xFF;
    args.push(size === 0 ? i + 1 : 0x19, size);
  }
  return report(ctx, 0x0F, 0x08, 0x0D, args);
}

// --- frames -----------------------------------------------------------------------------------

/** razer_create_custom_frame_standard_matrix_report(): 0x03 0x0B, [0xFF, row, start, stop, rgb...]. */
export function customFrameStandard(ctx, row, startCol, stopCol, rgb) {
  const length = ((stopCol + 1) - startCol) * 3;
  return report(ctx, 0x03, 0x0B, length + 4, [0xFF, row, startCol, stopCol, ...rgb.slice(0, length)]);
}

/** razer_create_custom_frame_extended_matrix_report(): 0x0F 0x03, [0, 0, row, start, stop, rgb...]. */
export function customFrameExtended(ctx, row, startCol, stopCol, rgb) {
  const length = ((stopCol + 1) - startCol) * 3;
  return report(ctx, 0x0F, 0x03, length + 5, [0x00, 0x00, row, startCol, stopCol, ...rgb.slice(0, length)]);
}

/** razer_create_custom_frame_linear_report(): 0x03 0x0C, size 0x32, [start, stop, rgb...]. */
export function customFrameLinear(ctx, startCol, stopCol, rgb) {
  const length = ((stopCol + 1) - startCol) * 3;
  return report(ctx, 0x03, 0x0C, 0x32, [startCol, stopCol, ...rgb.slice(0, length)]);
}

/**
 * razer_create_custom_frame_argb_report(): sent unnumbered (hid id 0x00) on the ARGB interface;
 * byte 1 is 0x04 for rows 0..4 and 0x84 from row 5, then the channel twice, a pad byte, the last
 * LED index and (stop + 1) RGB triplets.
 */
export function customFrameArgb(row, stopCol, rgb) {
  const bytes = new Array(ARGB_REPORT_LENGTH).fill(0);
  bytes[0] = 0x00;
  bytes[1] = row < 5 ? 0x04 : 0x84;
  bytes[2] = row & 0xFF;
  bytes[3] = row & 0xFF;
  bytes[4] = 0x00;
  bytes[5] = stopCol & 0xFF;
  const length = (stopCol + 1) * 3;
  for (let i = 0; i < length && i < 315; i++) {
    bytes[6 + i] = (rgb[i] ?? 0) & 0xFF;
  }
  return bytes;
}

/** razer_create_mode_custom_standard_matrix_report(storage): 0x03 0x0A, [0x05, storage]. */
export function modeCustomStandard(ctx, storage) {
  return report(ctx, 0x03, 0x0A, 0x02, [0x05, storage]);
}

/** razer_create_mode_custom_extended_matrix_report(): 0x0F 0x02, size 0x0C, [0x00, 0x00, 0x08]. */
export function modeCustomExtended(ctx) {
  return report(ctx, 0x0F, 0x02, 0x0C, [0x00, 0x00, 0x08]);
}

/** razer_create_set_led_rgb_report(): 0x03 0x01, [storage, led, r, g, b]. */
export function setLedRgb(ctx, storage, ledId, rgb) {
  return report(ctx, 0x03, 0x01, 0x05, [storage, ledId, rgb[0], rgb[1], rgb[2]]);
}

/** razer_create_set_led_effect_report(): 0x03 0x02, [storage, led, min(effect, 5)]. */
export function setLedEffect(ctx, storage, ledId, effect) {
  return report(ctx, 0x03, 0x02, 0x03, [storage, ledId, effect > 5 ? 5 : effect]);
}

/** razer_create_mode_spectrum_cycle_standard_matrix_report(): 0x03 0x0A, size 0x01, [0x04]. */
export function spectrumStandard(ctx) {
  return report(ctx, 0x03, 0x0A, 0x01, [0x04]);
}

/** razer_create_mode_spectrum_cycle_extended_matrix_report(): 0x0F 0x02, size 0x06, [storage, led, 0x03]. */
export function spectrumExtended(ctx, storage, ledId) {
  return report(ctx, 0x0F, 0x02, 0x06, [storage, ledId, 0x03]);
}

// --- Kraken V4 (64-byte output reports, report id 0x02) -------------------------------------

/**
 * razer_kraken_v4_create_report(): report id 0x02, transaction id, data size, class, id, the
 * wireless flag, 52 argument bytes, crc, reserved. OpenRGB's razer_usb_send() for these reports
 * does not compute the crc (only razer_usb_receive() does), so the crc byte goes out as 0x00.
 */
export function krakenV4Report(ctx, commandClass, commandId, dataSize, args = []) {
  const bytes = new Array(KRAKEN_V4_REPORT_LENGTH).fill(0);
  bytes[0] = 0x02;
  bytes[2] = ctx.transactionId & 0xFF;
  bytes[6] = dataSize & 0xFF;
  bytes[7] = commandClass & 0xFF;
  bytes[8] = commandId & 0xFF;
  bytes[9] = ctx.wirelessFlag & 0xFF;
  for (let i = 0; i < args.length && i < 52; i++) {
    bytes[10 + i] = args[i] & 0xFF;
  }
  return bytes;
}

/** RazerKrakenV4Controller::SetDirect(): 0x0F 0x03, size 5 + 3 * cols, [0, 0, 0, 0, cols - 1, rgb...]. */
export function krakenV4Direct(ctx, cols, rgb) {
  return krakenV4Report(ctx, 0x0F, 0x03, 5 + 3 * cols, [0x00, 0x00, 0x00, 0x00, cols - 1, ...rgb.slice(0, cols * 3)]);
}

/** RazerKrakenV4Controller::SetBrightness(): 0x00 0x00, size 0x05, [0xC1, 0x00, 0x01, brightness]. */
export function krakenV4Brightness(ctx, brightness) {
  return krakenV4Report(ctx, 0x00, 0x00, 0x05, [0xC1, 0x00, 0x01, brightness]);
}

/** RazerKrakenV4Controller::SetModeWave(): 0x00 0x00, size 0x05, [0xC0, 0x00, 0x01, 0x04]. */
export function krakenV4Wave(ctx) {
  return krakenV4Report(ctx, 0x00, 0x00, 0x05, [0xC0, 0x00, 0x01, 0x04]);
}
