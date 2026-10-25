// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: EVision keyboards with the V2 firmware protocol (Sonix-based; the BYGG CSB
// ICL01 320F:5041 and the Endorfy Omnis 3299:0012), per-key colours in the keyboard's direct
// mode. The protocol shared by plugin.js and endorfy-omnis.js.
// Adapted from OpenRGB Controllers/EVisionKeyboardController/EVisionV2KeyboardController/
// EVisionV2KeyboardController.cpp, EVisionV2KeyboardController.h,
// EVisionV2KeyboardControllerDetect.cpp and RGBController_EVisionV2Keyboard.cpp
// (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: Le Philousophe (25 Dec 2022)
//
// Protocol notes: docs/hardware/evision-keyboard.md. In short: every request is a 64-byte output
// report with report id 4 (a 16-bit checksum, a command, a size, an offset and up to 56 data
// bytes) on the vendor collection (interface 1, usage page 0xFF1C), and the keyboard answers each
// one with a 64-byte input report that echoes it. Lightloom reads the keyboard's capabilities once
// (read only), then sends the colour buffer with "send dynamic colours" (0x12) on every changed
// frame, refreshes it after 200 ms without a change, and hands the keyboard back to its own
// lighting with "end dynamic colours" (0x13). Nothing is written to the keyboard's profiles,
// stored modes or custom colour sets.

/* global device */

const REPORT_ID = 0x04;                        // EVISION_V2_REPORT_ID
const PACKET_SIZE = 64;                        // EVISION_V2_PACKET_SIZE, report id included
const DATA_SIZE = PACKET_SIZE - 8;             // data bytes per request
const INTERFACE = 1;
const USAGE_PAGE = 0xFF1C;                     // EVISION_KEYBOARD_USAGE_PAGE

const CMD_READ_CAPABILITIES = 0x03;            // EVISION_V2_CMD_READ_CAPABILITIES
const CMD_SEND_DYNAMIC_COLORS = 0x12;          // EVISION_V2_CMD_SEND_DYNAMIC_COLORS
const CMD_END_DYNAMIC_COLORS = 0x13;           // EVISION_V2_CMD_END_DYNAMIC_COLORS

const CAPABILITIES_SIZE = 7;                   // what the controller's constructor reads
const ERROR = -256;                            // Query()'s "bad answer"

// RefreshLedDirect(): one byte at the first blank cell of the colour buffer.
const BLANK_SPACE = 6;
const KEEPALIVE_OFFSET = BLANK_SPACE * 3;
// KeepaliveThread(): refresh when the colours have not been sent for more than 200 ms.
const KEEPALIVE_MS = 200;

// OpenRGB's hid_read blocks until the answer comes; Lightloom waits this long for it.
const READ_TIMEOUT_MS = 100;
// Input reports with another report id are skipped while waiting (OpenRGB loops on them).
const MAX_OTHER_REPORTS = 8;
// After this many frames in a row that the keyboard did not confirm, a new frame is tried only
// once per BACKOFF_MS, so a keyboard that stopped answering does not hold every Render() for the
// read timeout (OpenRGB has no back-off; it retries on every update).
const MAX_FAILED_FRAMES = 3;
const BACKOFF_MS = 1000;

/**
 * The EVision V2 detectors (REGISTER_HID_DETECTOR_IP): interface 1, usage page 0xFF1C, any
 * usage.
 */
export function validate(endpoint) {
  return endpoint.interface === INTERFACE && endpoint.usage_page === USAGE_PAGE;
}

/**
 * The 64 bytes Query() hands to hid_write: report id, checksum (the sum of bytes 3..63 as a
 * 16-bit number, low byte first), command, size, offset (low byte first), a zero byte and the
 * data.
 */
export function requestPacket(cmd, offset, data, size) {
  const packet = new Array(PACKET_SIZE).fill(0);
  packet[0] = REPORT_ID;
  packet[3] = cmd & 0xFF;
  packet[4] = size & 0xFF;
  packet[5] = offset & 0xFF;
  packet[6] = (offset >> 8) & 0xFF;
  if (data) {
    for (let i = 0; i < size; i++) {
      packet[8 + i] = data[i] & 0xFF;
    }
  }
  let sum = 0;
  for (let i = 3; i < PACKET_SIZE; i++) {
    sum = (sum + packet[i]) & 0xFFFF;
  }
  packet[1] = sum & 0xFF;
  packet[2] = (sum >> 8) & 0xFF;
  return packet;
}

/**
 * Query(): send one request and wait for the keyboard's answer. Returns { result, data }:
 * result is the size the keyboard answered (data holds that many bytes), -status when the
 * keyboard refused the request, or -256 when no matching answer came. An answer must echo the
 * report id, the checksum bytes, the command and the offset, as OpenRGB checks.
 */
function query(cmd, offset = 0, data = null, size = 0) {
  const packet = requestPacket(cmd, offset, data, size);
  device.clearReadBuffer();
  if (device.write(packet, PACKET_SIZE) < 0) {
    return { result: ERROR };
  }
  let answer = [];
  let read = 0;
  for (let i = 0; i <= MAX_OTHER_REPORTS; i++) {
    answer = device.read([REPORT_ID], PACKET_SIZE, READ_TIMEOUT_MS);
    read = device.getLastReadSize();
    if (read === 0 || answer[0] === REPORT_ID) {
      break;
    }
  }
  if (read !== PACKET_SIZE || answer[0] !== REPORT_ID ||
      answer[1] !== packet[1] || answer[2] !== packet[2] || answer[3] !== packet[3] ||
      answer[5] !== packet[5] || answer[6] !== packet[6]) {
    return { result: ERROR };
  }
  if (answer[7] !== 0) {
    return { result: -answer[7] };
  }
  const answered = answer[4];
  if (answered > DATA_SIZE) {
    return { result: ERROR };
  }
  return { result: answered, data: answer.slice(8, 8 + answered) };
}

/**
 * Read(): `size` bytes from `offset`, in requests of at most 56 bytes, advancing by what the
 * keyboard answered. Returns the bytes, or null when a request failed or came back empty.
 */
function readBlock(cmd, offset, size) {
  const bytes = [];
  while (size > 0) {
    const { result, data } = query(cmd, offset, null, Math.min(size, DATA_SIZE));
    if (result <= 0 || result > size) {
      return null;
    }
    bytes.push(...data);
    offset += result;
    size -= result;
  }
  return bytes;
}

/**
 * Write(): `bytes` from `offset`, in requests of at most 56 bytes. Like OpenRGB, the rest is not
 * sent once a request is refused or unanswered. Returns whether every request was confirmed.
 */
function writeBlock(cmd, offset, bytes) {
  for (let pos = 0; pos < bytes.length; pos += DATA_SIZE) {
    const size = Math.min(bytes.length - pos, DATA_SIZE);
    if (query(cmd, offset + pos, bytes.slice(pos, pos + size), size).result <= 0) {
      return false;
    }
  }
  return true;
}

/**
 * One keyboard with `layout` (layouts.js): Initialize/Render/Shutdown of the plugin.
 */
export function createKeyboard(layout) {
  let mapSize = 0;              // key slots in the colour buffer (capabilities byte 5); 0 = unknown
  let lastFrame = null;         // the colour buffer the keyboard confirmed last
  let lastSentAt = -Infinity;   // Date.now() of the last 0x12 request
  let failedFrames = 0;         // frames in a row the keyboard did not confirm

  /**
   * The controller's constructor: read the capabilities (magic 0xAA 0x55, the key map size and
   * the macro memory size). Switching to OpenRGB's Direct mode sends nothing.
   */
  function initialize() {
    mapSize = 0;
    lastFrame = null;
    lastSentAt = -Infinity;
    failedFrames = 0;
    const caps = readBlock(CMD_READ_CAPABILITIES, 0, CAPABILITIES_SIZE);
    if (caps === null) {
      device.log("EVision V2: no answer to the capabilities request; no colours are sent.");
      return;
    }
    // OpenRGB gives up only when both magic bytes differ.
    if (caps[0] !== 0xAA && caps[1] !== 0x55) {
      device.log(`EVision V2: unexpected capabilities ${hex(caps)}; no colours are sent.`);
      return;
    }
    mapSize = caps[5];
    device.log(`EVision V2: ${mapSize} key slots, ${caps[6] * 0x80} bytes of macro memory.`);
    const outside = layout.leds.filter((led) => led[3] >= mapSize).map((led) => led[0]);
    if (outside.length > 0) {
      device.log(`EVision V2: keys outside the keyboard's ${mapSize} slots are left dark: ${outside.join(", ")}.`);
    }
  }

  /** SetLedsDirect(): the colour of LED i at 3 * slot[i], R G B, in a 3 * map size buffer. */
  function colourBuffer() {
    const bytes = new Array(3 * mapSize).fill(0);
    for (const [, x, y, slot] of layout.leds) {
      if (slot >= mapSize) {
        continue;
      }
      const [r, g, b] = device.color(x, y);
      bytes[3 * slot] = r & 0xFF;
      bytes[3 * slot + 1] = g & 0xFF;
      bytes[3 * slot + 2] = b & 0xFF;
    }
    return bytes;
  }

  function render() {
    if (mapSize === 0) {
      return;
    }
    const bytes = colourBuffer();
    const now = Date.now();
    if (lastFrame !== null && sameBytes(bytes, lastFrame)) {
      // KeepaliveThread() / RefreshLedDirect(): one byte at the blank cell. OpenRGB sends 0 there;
      // Lightloom sends the byte the keyboard already has, the same 0 on the EVision V2 layout but
      // F5's red on the Endorfy layout, where cell 6 is a key.
      if (now - lastSentAt > KEEPALIVE_MS) {
        query(CMD_SEND_DYNAMIC_COLORS, KEEPALIVE_OFFSET, [lastFrame[KEEPALIVE_OFFSET] ?? 0], 1);
        lastSentAt = now;
      }
      return;
    }
    if (failedFrames >= MAX_FAILED_FRAMES && now - lastSentAt < BACKOFF_MS) {
      return;
    }
    if (writeBlock(CMD_SEND_DYNAMIC_COLORS, 0, bytes)) {
      if (failedFrames >= MAX_FAILED_FRAMES) {
        device.log("EVision V2: the keyboard confirms colours again.");
      }
      lastFrame = bytes;
      failedFrames = 0;
    } else {
      lastFrame = null;
      failedFrames++;
      if (failedFrames === MAX_FAILED_FRAMES) {
        device.log(`EVision V2: ${MAX_FAILED_FRAMES} frames in a row not confirmed; trying once a second.`);
      }
    }
    lastSentAt = now;
  }

  /**
   * EndLedsDirect(), which OpenRGB sends when the controller goes away: the keyboard goes back
   * to its own (stored) lighting. The same on suspend: colours sent now would not outlast the
   * missing refreshes. Sent even when the capabilities were not confirmed, as OpenRGB's
   * KeepaliveThread() does on exit; 0x13 is volatile and stores nothing. The same for every exit
   * mode: OpenRGB refreshes direct colours after 200 ms without an update, which says the firmware
   * leaves direct mode by itself when the host stops, so a last frame (or a black one) would not
   * stay; "keep" and "off" therefore end like "restore" (ShutdownModes() lists only that).
   */
  function shutdown() {
    query(CMD_END_DYNAMIC_COLORS);
    lastFrame = null;
  }

  return { initialize, render, shutdown };
}

function sameBytes(a, b) {
  if (a.length !== b.length) {
    return false;
  }
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) {
      return false;
    }
  }
  return true;
}

function hex(bytes) {
  return Array.from(bytes, (b) => (b & 0xFF).toString(16).toUpperCase().padStart(2, "0")).join(" ");
}
