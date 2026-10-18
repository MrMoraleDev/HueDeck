// SPDX-License-Identifier: MIT
//
// HueDeck first-party plugin: Razer DeathAdder V2 Pro (wired 1532:007C, wireless receiver
// 1532:007D), the logo light.
//
// Protocol facts come from HueDeck's own recordings of the mouse's HID traffic, written up in
// docs/hardware/razer-deathadder-v2-pro.md; no code of any other project was used. In short:
// 91-byte feature reports (report id 0) with a transaction id, an argument count, a command class
// and id, the arguments and an XOR checksum. The lighting is switched to host frames once
// (0x0F 0x02); each frame then sets the colour (0x0F 0x03) without waiting for a reply. At the
// end the mouse gets its own lighting back (0x00 0x04). DPI, polling rate, battery and buttons
// are never touched.

/* global device */

export function Name() { return "Razer DeathAdder V2 Pro"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/razer-deathadder-v2-pro"; }
export function VendorId() { return 0x1532; }
// 0x007C wired (tested); 0x007D through the wireless receiver (untested).
export function ProductId() { return [0x007C, 0x007D]; }
export function Type() { return "Hid"; }
export function DeviceType() { return "mouse"; }
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ["Logo"]; }
export function LedPositions() { return [[0, 0]]; }
export function ImageUrl() { return ""; }
// Razer's own software drives the lights while it runs; HueDeck reports it, never closes it.
export function ConflictingProcesses() { return ["RazerCentralService.exe", "Razer Synapse 3.exe"]; }

const REPORT_LENGTH = 91;
const STATUS_SUCCESS = 0x02;
const STATUS_BUSY = 0x01;
// Transaction ids Razer mice answer to; this one answers 0x1F.
const TRANSACTION_IDS = [0x1F, 0x3F, 0x9F, 0xFF];

let transactionId = TRANSACTION_IDS[0];

export function Validate(endpoint) {
  return endpoint.interface === 0 && endpoint.usage_page === 0x0001 && endpoint.usage === 0x0002;
}

export function Initialize() {
  transactionId = findTransactionId();
  // Host frames from now on.
  const reply = request(0x0F, 0x02, [0x00, 0x00, 0x08, 0x00, 0x01, 0x00]);
  if (!reply) {
    device.log("The mouse did not confirm frame mode; frames are sent anyway.");
  }
}

export function Render() {
  const [r, g, b] = device.color(0, 0);
  send(0x0F, 0x03, [0x00, 0x00, 0x00, 0x00, 0x00, r & 0xFF, g & 0xFF, b & 0xFF]);
}

export function Shutdown() {
  send(0x0F, 0x03, [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
  request(0x00, 0x04, [0x00, 0x00]);
}

/** The first transaction id the mouse answers a serial-number query with; 0x1F if none does. */
function findTransactionId() {
  for (const id of TRANSACTION_IDS) {
    transactionId = id;
    if (request(0x00, 0x82, [0x00, 0x00])) {
      return id;
    }
  }
  device.log("No transaction id answered; using 0x1F.");
  return TRANSACTION_IDS[0];
}

/** A report for `class_`/`command` with `args`, checksum included. */
function report(id, class_, command, args) {
  const bytes = new Array(REPORT_LENGTH).fill(0);
  bytes[2] = id;
  bytes[6] = args.length;
  bytes[7] = class_;
  bytes[8] = command;
  for (let i = 0; i < args.length; i++) {
    bytes[9 + i] = args[i] & 0xFF;
  }
  let crc = 0;
  for (let i = 3; i < 89; i++) {
    crc ^= bytes[i];
  }
  bytes[89] = crc;
  return bytes;
}

function send(class_, command, args) {
  return device.send_report(report(transactionId, class_, command, args), REPORT_LENGTH);
}

/** Send and read the reply; the reply when the mouse confirms it, else null. */
function request(class_, command, args) {
  if (send(class_, command, args) < 0) {
    return null;
  }
  for (let attempt = 0; attempt < 5; attempt++) {
    device.pause(attempt === 0 ? 2 : 10);
    const reply = device.get_report([0x00], REPORT_LENGTH);
    if (!reply || reply.length < 9) {
      continue;
    }
    const matches = reply[2] === transactionId && reply[7] === class_ && reply[8] === command;
    if (matches && reply[1] === STATUS_SUCCESS) {
      return reply;
    }
    // Busy, or still the previous command's reply: ask again; anything else is a refusal.
    if (matches && reply[1] !== STATUS_BUSY) {
      return null;
    }
  }
  return null;
}
