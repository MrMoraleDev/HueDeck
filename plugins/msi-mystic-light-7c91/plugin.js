// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck first-party plugin: MSI Mystic Light "185-byte" motherboard controller.
// Target board for M1: MSI MAG B550 TOMAHAWK (MS-7C91), USB HID 1462:7C91.
//
// Written in the SignalRGB-compatible plugin shape (metadata exports + Validate /
// Initialize / Render / Shutdown) so the same compat loader that later runs imported
// third-party plugins runs this file. Clean-room: facts about the wire format come from
// docs/hardware/msi-mystic-light-185.md (two independent public descriptions agree);
// no code was copied from any other RGB project.
//
// Wire format in one paragraph (details and the "unverified" column in the hardware doc):
// HID *feature* report id 0x52, 185 bytes including the report id. The payload is a table
// of fixed-size zone records (10 bytes; the two JRAINBOW records and the JCORSAIR record are
// 11 bytes), followed by one "save to flash" byte at offset 0xB8 (184). Each 10-byte record:
//   +0 effect number (0x01 = static)      +1..+3 colour 1 R,G,B
//   +4 bit0-1 speed, bit2-6 brightness, bit7 "sync with JRGB"
//   +5..+7 colour 2 R,G,B                 +8 bit0-5 sync flags, bit7 "use my colour" flag
//   +9 padding
//
// M1 policy (deliberately conservative - a wrong 185-byte packet can misconfigure the
// controller, and byte 184 != 0 writes the configuration to flash):
//   * Never write before a successful read of the current 0x52 report ("restore image").
//   * Every packet we send is a COPY of the restore image; we overwrite only the six onboard
//     zone records (static effect + our colour) and force the save flag to 0.
//   * JRGB1/JRGB2 (12 V headers), JRAINBOW1/JRAINBOW2 (5 V ARGB headers), the Corsair slot and
//     the unused onboard slots are left exactly as read. Channels for the ARGB headers (per-LED
//     report 0x53) arrive in M2 - see the addChannel comment in Initialize().
//   * Shutdown(false) writes the restore image back; Shutdown(true) (system suspend) paints the
//     shutdown colour (black by default).

// ---------------------------------------------------------------------------------------------
// Metadata (call-if-function exports, SignalRGB-compatible)
// ---------------------------------------------------------------------------------------------
export function Name() { return "MSI Mystic Light 185 (MAG B550 Tomahawk)"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/msi-mystic-light-185"; }
export function VendorId() { return 0x1462; }
export function ProductId() { return 0x7C91; }
export function Type() { return "Hid"; }
export function DeviceType() { return "motherboard"; }
// Six onboard LEDs laid out in a row; the ARGB headers become channels in M2 (no canvas cells).
export function Size() { return [ONBOARD_ZONES.length, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ONBOARD_ZONES.map((zone) => zone.name); }
export function LedPositions() { return ONBOARD_ZONES.map((zone, index) => [index, 0]); }
// No bundled artwork yet; an empty URL makes the host fall back to the DeviceType icon.
export function ImageUrl() { return ""; }
// MSI's own lighting services hold the HID handle; HueDeck reports them, never kills them.
export function ConflictingProcesses() {
  return ["LEDKeeper.exe", "LEDKeeper2.exe", "LightKeeperService.exe", "LightKeeperService2.exe"];
}

/* global device, LightingMode, forcedColor, shutdownColor */
export function ControllableParameters() {
  return [
    { property: "LightingMode", group: "lighting", label: "Lighting Mode",
      description: "Canvas follows the active effect; Forced paints one colour.",
      type: "combobox", values: ["Canvas", "Forced"], default: "Canvas" },
    { property: "forcedColor", group: "lighting", label: "Forced Color",
      description: "Colour used while Lighting Mode is Forced.",
      type: "color", min: "0", max: "360", default: "#009bde" },
    { property: "shutdownColor", group: "lighting", label: "Shutdown Color",
      description: "Colour painted when the system suspends. On a normal exit the lighting state read at start-up is restored instead.",
      type: "color", min: "0", max: "360", default: "#000000" },
  ];
}

// ---------------------------------------------------------------------------------------------
// Report layout constants (offsets are into the 185-byte buffer, index 0 = report id)
// ---------------------------------------------------------------------------------------------
const REPORT_ID = 0x52;
const REPORT_LENGTH = 185;
const SAVE_FLAG_OFFSET = 0xB8;          // 184: 0 = apply only, non-zero = persist to flash. Always 0 here.

// Bytes whose GET value is not simply the last SET value (bench, 2026-10-18, this board; see the
// hardware doc, section "Bench results"):
//  * J_CORSAIR record +9 (0x3E) reads back as the written value + 4. SignalRGB writes 0x00 there
//    and the controller then reports 0x04, so a naive read-modify-write grows it by 4 per session.
//    Every packet therefore writes (read value - 4), which keeps the GET value stable.
//  * Colour 2 (+5..+7) of an onboard record is not always retained (slot 6 reads back 0), so it is
//    ignored when recognising our own packet in a GET.
const CORSAIR_FLAGS_OFFSET = 0x35 + 9;
const READBACK_BIAS = 4;

// Zone record field offsets (relative to the record start).
const Z_EFFECT = 0;                      // effect number
const Z_COLOR1 = 1;                      // +1 R, +2 G, +3 B
const Z_OPTIONS = 4;                     // bit0-1 speed, bit2-6 brightness, bit7 JRGB sync
const Z_COLOR2 = 5;                      // +5 R, +6 G, +7 B (second LED on boards with 2 LEDs per zone)
const Z_SYNC = 8;                        // bit0-5 sync flags (onboard, rainbow1, rainbow2, corsair, pipe1, pipe2), bit7 colour flag
// +9 padding: left untouched.

const EFFECT_STATIC = 0x01;              // "Static" - the only effect used for direct zone colour
const OPTIONS_STATIC = 0x28;             // speed 0, brightness 10/31, JRGB-sync clear (known-good value)
const SYNC_NONE_OWN_COLOR = 0x80;        // no sync flags, colour flag set: this zone shows ITS colour bytes

// Onboard LED zones we drive. The record table also holds JRGB1 (0x01), JPIPE1 (0x0B), JPIPE2 (0x15),
// JRAINBOW1 (0x1F, 11 bytes), JRAINBOW2 (0x2A, 11 bytes), JCORSAIR/JRAINBOW3 (0x35, 11 bytes),
// JCORSAIR outer (0x40), onboard slots 7..10 (0x86, 0x90, 0x9A, 0xA4) and JRGB2 (0xAE) - all untouched.
// Which six of the ten onboard slots are wired on the B550 Tomahawk follows the SignalRGB
// configuration table (slots 0x4A..0x7C) and is marked unverified in the hardware doc until the
// M1 hardware test paints a distinct colour per LED.
const ONBOARD_ZONES = [
  { name: "Onboard LED 1", offset: 0x4A },
  { name: "Onboard LED 2", offset: 0x54 },
  { name: "Onboard LED 3", offset: 0x5E },
  { name: "Onboard LED 4", offset: 0x68 },
  { name: "Onboard LED 5", offset: 0x72 },
  { name: "Onboard LED 6", offset: 0x7C },
];

// ---------------------------------------------------------------------------------------------
// Plugin state
// ---------------------------------------------------------------------------------------------
let restoreImage = null;                 // 185-byte copy of report 0x52 as read in Initialize()
let writesEnabled = false;               // true only after a valid restore image was captured
let lastSent = null;                     // last packet we wrote (used to recognise our own state on re-Initialize)
let lastSentAt = -Infinity;              // Date.now() of that write

// Render() resends an unchanged packet only once this much time has passed, so a static colour is
// not pushed to the controller 30 times per second. A changed packet is sent immediately.
const KEEPALIVE_MS = 1000;

// ---------------------------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------------------------
// Facts observed on this board (2026-10-17, read-only HidP_GetCaps probe): a single HID interface,
// device path HID\VID_1462&PID_7C91\...&0&0000 (no &MI_xx, no &ColNN), usage_page 0x0001,
// usage 0x0000, input/output 64 bytes, feature report maximum 725 bytes. The controller does NOT
// use a vendor-defined usage page, so we select on the interface number only. A non-composite USB
// device reports interface 0 (hidapi, SignalRGB and HueDeck agree; verified in M0). -1 is accepted
// too: it is what a host reports for a HID transport without a USB interface.
export function Validate(endpoint) {
  return endpoint.interface === 0 || endpoint.interface === -1;
}

export function Initialize() {
  writesEnabled = false;
  lastSentAt = -Infinity;                // the first Render() after (re-)Initialize always writes

  // GET_REPORT(Feature, 0x52). The host returns the report-id-prefixed byte array; the number of
  // bytes the device actually returned is getLastReadSize() when the host provides it. Both the
  // reported size and the array itself must cover all 185 bytes before we trust the image.
  const image = device.get_report([REPORT_ID], REPORT_LENGTH);
  const size = readSize(image);

  if (size !== REPORT_LENGTH || !isByteArray(image, REPORT_LENGTH) || (image[0] & 0xFF) !== REPORT_ID) {
    device.log(`MSI 185: GET feature report 0x52 returned ${size} bytes (expected ${REPORT_LENGTH}); ` +
               "no restore image, all writes disabled.", { toFile: true });
    return;
  }

  const current = [];
  for (let i = 0; i < REPORT_LENGTH; i++) {
    current.push(image[i] & 0xFF);
  }

  // The host calls Initialize() again after a system resume (and after a hot re-plug). When the
  // controller still holds the packet WE wrote last (e.g. the suspend colour), the board's real
  // "before HueDeck" state is the image captured the first time; keep it so that Shutdown(false)
  // still restores the pre-run appearance. Anything else (cold boot, another application wrote
  // in between) is a genuinely new state and becomes the restore image.
  if (restoreImage === null || lastSent === null || !readsBackAs(current, lastSent)) {
    restoreImage = current;
  }
  writesEnabled = true;
  device.log("MSI 185: captured 185-byte restore image; direct zone mode enabled for 6 onboard LEDs.", { toFile: true });

  // M2: ARGB headers become component channels driven by the per-LED feature report 0x53:
  //   device.addChannel("JRAINBOW1", 120);
  //   device.addChannel("JRAINBOW2", 120);
  // That requires the 185-byte "switch to per-LED mode" record (effect 0x25 on the onboard zone 0,
  // LED counts in the JRAINBOW records) and is intentionally not sent in M1.
}

export function Render() {
  if (!writesEnabled) {
    return;
  }
  const packet = buildPacket(colourForLed);
  if (lastSent !== null && sameBytes(packet, lastSent) && Date.now() - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  sendPacket(packet);
}

export function Shutdown(SystemSuspending) {
  if (!writesEnabled) {
    return;
  }
  if (SystemSuspending) {
    const rgb = hexToRgb(param("shutdownColor", "#000000"));
    sendPacket(buildPacket(() => rgb));
    return;
  }
  // Normal exit: put the controller back exactly as we found it (save flag forced to 0 so the
  // restore never persists anything to flash). Nothing to undo when nothing was written: a
  // restore would re-enter whatever mode the image holds (per-LED mode with no colour stream
  // after SignalRGB, for example) although HueDeck never changed the board.
  if (lastSent === null) {
    return;
  }
  sendPacket(writableFrom(restoreImage));
}

// ---------------------------------------------------------------------------------------------
// Packet construction
// ---------------------------------------------------------------------------------------------
// Start from the restore image as it must be written and rewrite only the fields we understand.
function buildPacket(colourFor) {
  const packet = writableFrom(restoreImage);

  for (let index = 0; index < ONBOARD_ZONES.length; index++) {
    const rgb = colourFor(index);
    writeStaticZone(packet, ONBOARD_ZONES[index].offset, rgb);
  }
  return packet;
}

// The bytes to SET so that a later GET returns `image`: the save flag cleared and the read-back
// bias of the J_CORSAIR flags byte removed.
function writableFrom(image) {
  const packet = image.slice();
  packet[SAVE_FLAG_OFFSET] = 0x00;
  const flags = packet[CORSAIR_FLAGS_OFFSET] & 0xFF;
  packet[CORSAIR_FLAGS_OFFSET] = flags >= READBACK_BIAS ? flags - READBACK_BIAS : flags;
  return packet;
}

// Whether `current` (a GET) is what the controller reports after we SET `sent`.
function readsBackAs(current, sent) {
  if (current.length !== sent.length) {
    return false;
  }
  const ignored = new Set([SAVE_FLAG_OFFSET]);
  for (const zone of ONBOARD_ZONES) {
    for (let i = 0; i < 3; i++) {
      ignored.add(zone.offset + Z_COLOR2 + i);
    }
  }
  for (let i = 0; i < sent.length; i++) {
    if (ignored.has(i)) {
      continue;
    }
    let expected = sent[i] & 0xFF;
    if (i === CORSAIR_FLAGS_OFFSET) {
      expected = (expected + READBACK_BIAS) & 0xFF;
    }
    if ((current[i] & 0xFF) !== expected) {
      return false;
    }
  }
  return true;
}

// Static effect with a single colour. Colour 2 is set to the same value so that boards wiring
// two LEDs to one zone light uniformly. Byte +9 (padding) is not modified.
function writeStaticZone(packet, offset, rgb) {
  packet[offset + Z_EFFECT] = EFFECT_STATIC;
  packet[offset + Z_COLOR1] = rgb[0] & 0xFF;
  packet[offset + Z_COLOR1 + 1] = rgb[1] & 0xFF;
  packet[offset + Z_COLOR1 + 2] = rgb[2] & 0xFF;
  packet[offset + Z_OPTIONS] = OPTIONS_STATIC;
  packet[offset + Z_COLOR2] = rgb[0] & 0xFF;
  packet[offset + Z_COLOR2 + 1] = rgb[1] & 0xFF;
  packet[offset + Z_COLOR2 + 2] = rgb[2] & 0xFF;
  packet[offset + Z_SYNC] = SYNC_NONE_OWN_COLOR;
}

function colourForLed(index) {
  if (param("LightingMode", "Canvas") === "Forced") {
    return hexToRgb(param("forcedColor", "#009bde"));
  }
  const position = LedPositions()[index];
  const rgb = device.color(position[0], position[1]);
  return isByteArray(rgb, 3) ? rgb : [0, 0, 0];
}

// SET_REPORT(Feature). The host returns bytes written or -1; it never throws.
function sendPacket(packet) {
  const written = device.send_report(packet, REPORT_LENGTH);
  if (written < 0) {
    device.log("MSI 185: send_report(0x52) failed.", { toFile: true });
    return;
  }
  lastSent = packet.slice();
  lastSentAt = Date.now();
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------
// Accepts plain arrays and typed arrays alike (the host may hand back either).
function isByteArray(value, minLength) {
  return value != null && typeof value.length === "number" && value.length >= minLength;
}

function readSize(image) {
  if (typeof device.getLastReadSize === "function") {
    return device.getLastReadSize();
  }
  return isByteArray(image, 0) ? image.length : 0;
}

function sameBytes(a, b) {
  if (a.length !== b.length) {
    return false;
  }
  for (let i = 0; i < a.length; i++) {
    if ((a[i] & 0xFF) !== (b[i] & 0xFF)) {
      return false;
    }
  }
  return true;
}

// ControllableParameters are injected by the host as globals; fall back to the declared default
// when running under a harness that did not set them.
function param(name, fallback) {
  const value = globalThis[name];
  return value === undefined ? fallback : value;
}

// "#RRGGBB" (hash optional) -> [r, g, b]; anything malformed is treated as black.
function hexToRgb(hex) {
  const digits = String(hex).trim().replace(/^#/, "");
  if (digits.length !== 6 || !/^[0-9a-fA-F]+$/.test(digits)) {
    return [0, 0, 0];
  }
  const value = parseInt(digits, 16);
  return [(value >> 16) & 0xFF, (value >> 8) & 0xFF, value & 0xFF];
}
