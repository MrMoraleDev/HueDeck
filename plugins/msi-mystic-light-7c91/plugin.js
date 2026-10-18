// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck first-party plugin: MSI Mystic Light "185-byte" motherboard controller, per-LED mode.
// Target board: MSI MAG B550 TOMAHAWK (MS-7C91), USB HID 1462:7C91.
//
// Written in the SignalRGB-compatible plugin shape (metadata exports + Validate / Initialize /
// Render / Shutdown). Facts about the wire format come from docs/hardware/msi-mystic-light-185.md:
// public documentation and HueDeck's own recordings of this board's traffic. No code was copied
// from any other RGB project.
//
// The controller takes two HID feature reports:
//   * 0x52, 185 bytes including the report id: one record per lighting zone (10 bytes; the two
//     JRAINBOW records and the JCORSAIR record are 11) and a "save to flash" byte at 0xB8.
//     Each 10-byte record: +0 effect, +1..+3 colour 1 (R, G, B), +4 options (bit 0-1 speed,
//     bit 2-6 brightness, bit 7 "sync with JRGB"), +5..+7 colour 2, +8 sync flags (bit 0-5) and
//     colour flag (bit 7), +9 padding. A JRAINBOW record adds the LED count of its strip at +10.
//   * 0x53, 725 bytes: a per-LED colour stream of 240 LEDs, for zones in per-LED mode (0x25).
// This plugin puts the onboard master zone in synchronised per-LED mode, so every other zone
// follows it, and then sends one 0x53 report per frame: the 6 onboard LEDs, the two 12 V RGB
// headers (JRGB1, JRGB2) and the LEDs of the strips on the two 5 V ARGB headers (JRAINBOW1,
// JRAINBOW2), whose LED counts come from the channels' components.
//
// Policy (a wrong 0x52 packet can misconfigure the controller, and byte 0xB8 != 0 writes the
// configuration to flash):
//   * Nothing is written before the current 0x52 report has been read ("restore image").
//   * The save byte is always 0.
//   * Shutdown(false) blanks the stream and writes the restore image back, which also leaves
//     per-LED mode; Shutdown(true) (system suspend) paints the shutdown colour.

// ---------------------------------------------------------------------------------------------
// Metadata (call-if-function exports, SignalRGB-compatible)
// ---------------------------------------------------------------------------------------------
export function Name() { return "MSI Mystic Light 185 (MAG B550 Tomahawk)"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.2.0"; }
export function Documentation() { return "hardware/msi-mystic-light-185"; }
export function VendorId() { return 0x1462; }
export function ProductId() { return 0x7C91; }
export function Type() { return "Hid"; }
export function DeviceType() { return "motherboard"; }
// The six onboard LEDs in a row; the headers are subdevices and channels with their own places.
export function Size() { return [ONBOARD_LEDS, 1]; }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return ONBOARD_NAMES; }
export function LedPositions() { return ONBOARD_NAMES.map((name, index) => [index, 0]); }
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
// Report layout
// ---------------------------------------------------------------------------------------------
const CONFIG_ID = 0x52;
const CONFIG_LENGTH = 185;
const SAVE_FLAG_OFFSET = 0xB8;           // 0 = apply only, non-zero = persist to flash. Always 0 here.

const STREAM_ID = 0x53;
const STREAM_LENGTH = 725;
// Report id, effect 0x25 (per-LED), area 0x06 (onboard; in synchronised mode it carries every
// zone), start index 0, then a zero byte; 240 LEDs of R, G, B follow.
const STREAM_HEADER = [STREAM_ID, 0x25, 0x06, 0x00, 0x00];
const STREAM_LEDS = 240;

// Stream order recorded on this board (2026-10-18): onboard LEDs 1-6, JRGB1, JRGB2, the LEDs of
// the JRAINBOW1 strip, then those of the JRAINBOW2 strip; later slots stay black.
const ONBOARD_LEDS = 6;
const ONBOARD_NAMES = ["Onboard LED 1", "Onboard LED 2", "Onboard LED 3",
  "Onboard LED 4", "Onboard LED 5", "Onboard LED 6"];
// Subdevice ids and channel names are the ones SignalRGB setups use, so imported layouts and
// channel setups apply to them.
const HEADERS = [
  { id: "12v RGB Header 1", name: "JRGB1 (12 V RGB header)" },
  { id: "12v RGB Header 2", name: "JRGB2 (12 V RGB header)" },
];
const STRIPS = [
  { channel: "JRainbow 1", record: 0x1F },
  { channel: "JRainbow 2", record: 0x2A },
];
const STRIP_COUNT = 10;                  // LED count byte inside a JRAINBOW record
const STRIP_LED_LIMIT = 120;             // LEDs one ARGB header takes
const STRIP_LED_BUDGET = STREAM_LEDS - ONBOARD_LEDS - HEADERS.length;   // both strips together

// The 0x52 packet that switches the controller to synchronised per-LED mode, record by record as
// recorded on this board (2026-10-18). Every zone is static black except the onboard master zone
// (0x4A): effect 0x25 (per-LED), options 0xA9 (JRGB sync), flags 0xBF (every sync flag and the
// colour flag). The strip LED counts (+10 of the JRAINBOW records) are filled in per session.
const ZONE_STATIC = (options) => [0x01, 0x00, 0x00, 0x00, options, 0x00, 0x00, 0x00, 0x80, 0x00];
const MASTER_ZONE = 0x4A;
const CONFIG_RECORDS = [
  [0x01, ZONE_STATIC(0x08)],                                   // JRGB1
  [0x0B, ZONE_STATIC(0x2A)],                                   // JPIPE1
  [0x15, ZONE_STATIC(0x2A)],                                   // JPIPE2
  [0x1F, [...ZONE_STATIC(0x29), 0x00]],                        // JRAINBOW1, +10 LED count
  [0x2A, [...ZONE_STATIC(0x29), 0x00]],                        // JRAINBOW2, +10 LED count
  [0x35, [0x01, 0x00, 0x00, 0x00, 0x29, 0x00, 0x00, 0x00, 0x82, 0x00, 0x01]],  // JCORSAIR
  [0x40, ZONE_STATIC(0x28)],                                   // JCORSAIR outer
  [MASTER_ZONE, [0x25, 0x00, 0x00, 0x00, 0xA9, 0x00, 0x00, 0x00, 0xBF, 0x00]], // onboard master
  ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map((slot) => [MASTER_ZONE + 10 * slot, ZONE_STATIC(0x28)]),
  [0xAE, ZONE_STATIC(0x2A)],                                   // JRGB2
];

// Bytes whose GET value is not simply the last SET value (bench, 2026-10-18, this board; see the
// hardware doc): the J_CORSAIR record +9 (0x3E) reads back as the written value + 4; colour 2 of
// an onboard record is not always retained; the master zone's sync flags read back without the
// flags of headers this board lacks (0xBF written, 0x87 read).
const CORSAIR_FLAGS_OFFSET = 0x35 + 9;
const READBACK_BIAS = 4;
const MASTER_SYNC_OFFSET = MASTER_ZONE + 8;

// Output report 0x01 (64 bytes) 0xBA asks the controller whether the BIOS lets it drive the
// onboard LEDs ("EZ LED Control"); the reply 0x5A carries 1 (on) or 0 (off) at byte 6.
const COMMAND_LENGTH = 64;
const EZ_LED_QUERY = 0xBA;
const EZ_LED_REPLY = 0x5A;
const EZ_LED_STATE = 6;

// ---------------------------------------------------------------------------------------------
// Plugin state
// ---------------------------------------------------------------------------------------------
let restoreImage = null;                 // 185-byte copy of report 0x52 as read in Initialize()
let writesEnabled = false;               // true only after a valid restore image was captured
let lastConfig = null;                   // last 0x52 packet written (recognises our own state on re-Initialize)
let configuredCounts = null;             // strip LED counts the controller is configured for
let lastStream = null;                   // last 0x53 packet written
let lastStreamAt = -Infinity;            // Date.now() of that write
let wrote = false;                       // whether this session changed the controller at all

// Render() resends an unchanged stream only once this much time has passed, so a static colour is
// not pushed to the controller 30 times per second. A changed stream is sent immediately.
const KEEPALIVE_MS = 1000;

// ---------------------------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------------------------
// A single HID interface (usage page 0x0001, usage 0x0000, feature reports up to 725 bytes): a
// non-composite USB device reports interface 0; -1 is what a host reports for a HID transport
// without a USB interface.
export function Validate(endpoint) {
  return endpoint.interface === 0 || endpoint.interface === -1;
}

export function Initialize() {
  writesEnabled = false;
  configuredCounts = null;               // (re-)configure before the first frame
  lastStreamAt = -Infinity;

  // GET_REPORT(Feature, 0x52). The host returns the report-id-prefixed byte array; the number of
  // bytes the device actually returned is getLastReadSize() when the host provides it. Both the
  // reported size and the array itself must cover all 185 bytes before we trust the image.
  const image = device.get_report([CONFIG_ID], CONFIG_LENGTH);
  const size = readSize(image);
  if (size !== CONFIG_LENGTH || !isByteArray(image, CONFIG_LENGTH) || (image[0] & 0xFF) !== CONFIG_ID) {
    device.log(`MSI 185: GET feature report 0x52 returned ${size} bytes (expected ${CONFIG_LENGTH}); ` +
               "no restore image, all writes disabled.", { toFile: true });
    return;
  }
  const current = [];
  for (let i = 0; i < CONFIG_LENGTH; i++) {
    current.push(image[i] & 0xFF);
  }

  // The host calls Initialize() again after a system resume (and after a hot re-plug). When the
  // controller still holds the configuration WE wrote last, the board's real "before HueDeck"
  // state is the image captured the first time; keep it so that Shutdown(false) still restores
  // the pre-run appearance. Anything else (cold boot, another application wrote in between) is a
  // genuinely new state and becomes the restore image.
  if (restoreImage === null || lastConfig === null || !readsBackAs(current, lastConfig)) {
    restoreImage = current;
  }
  writesEnabled = true;

  for (const header of HEADERS) {
    device.createSubdevice(header.id);
    device.setSubdeviceName(header.id, header.name);
    device.setSubdeviceSize(header.id, 1, 1);
    device.setSubdeviceLeds(header.id, ["LED"], [[0, 0]]);
  }
  device.SetLedLimit(STRIP_LED_BUDGET);
  for (const strip of STRIPS) {
    device.addChannel(strip.channel, STRIP_LED_LIMIT);
  }

  configure(stripCounts());
  checkEzLed();
  device.log("MSI 185: captured the restore image; per-LED mode for the onboard LEDs, JRGB1/2 and JRAINBOW1/2.", { toFile: true });
}

export function Render() {
  if (!writesEnabled) {
    return;
  }
  paint(colourSource());
}

export function Shutdown(SystemSuspending) {
  if (!writesEnabled) {
    return;
  }
  if (SystemSuspending) {
    const rgb = hexToRgb(param("shutdownColor", "#000000"));
    paint(() => rgb);
    return;
  }
  // Normal exit: blank the stream, then put the configuration back exactly as we found it (save
  // flag forced to 0), which also leaves per-LED mode. Nothing to undo when nothing was written.
  if (!wrote) {
    return;
  }
  sendStream(blankStream());
  sendConfig(writableFrom(restoreImage));
}

// ---------------------------------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------------------------------
// One frame: reconfigure when a strip's LED count changed, then send the stream unless it is
// unchanged and was sent less than KEEPALIVE_MS ago.
function paint(source) {
  const counts = stripCounts();
  if (configuredCounts === null || !sameValues(counts, configuredCounts)) {
    configure(counts);
  }
  if (configuredCounts === null) {
    return;
  }
  const stream = buildStream(source, configuredCounts);
  if (lastStream !== null && sameValues(stream, lastStream) && Date.now() - lastStreamAt < KEEPALIVE_MS) {
    return;
  }
  sendStream(stream);
}

// Where each kind of LED takes its colour from: the canvas, or one colour for everything.
function colourSource() {
  if (param("LightingMode", "Canvas") === "Forced") {
    const rgb = hexToRgb(param("forcedColor", "#009bde"));
    return () => rgb;
  }
  return null;
}

// The 0x53 report for `counts` strip LEDs. `source` is null for the canvas, or a function that
// returns the one colour every LED gets.
function buildStream(source, counts) {
  const packet = new Array(STREAM_LENGTH).fill(0);
  STREAM_HEADER.forEach((value, index) => { packet[index] = value; });
  let slot = 0;
  const put = (rgb) => {
    const offset = STREAM_HEADER.length + 3 * slot;
    packet[offset] = rgb[0] & 0xFF;
    packet[offset + 1] = rgb[1] & 0xFF;
    packet[offset + 2] = rgb[2] & 0xFF;
    slot++;
  };
  for (let x = 0; x < ONBOARD_LEDS; x++) {
    put(source ? source() : rgbOrBlack(device.color(x, 0)));
  }
  for (const header of HEADERS) {
    put(source ? source() : rgbOrBlack(device.subdeviceColor(header.id, 0, 0)));
  }
  STRIPS.forEach((strip, index) => {
    const colours = source ? [] : channelColours(strip.channel);
    for (let led = 0; led < counts[index]; led++) {
      put(source ? source() : colours.slice(3 * led, 3 * led + 3).concat([0, 0, 0]));
    }
  });
  return packet;
}

// Every LED black.
function blankStream() {
  const packet = new Array(STREAM_LENGTH).fill(0);
  STREAM_HEADER.forEach((value, index) => { packet[index] = value; });
  return packet;
}

// A 0x53 report that is zero after the report id, sent before every configuration as recorded
// (what it does is not known).
function resetStream() {
  const packet = new Array(STREAM_LENGTH).fill(0);
  packet[0] = STREAM_ID;
  return packet;
}

// ---------------------------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------------------------
// LED counts for the two strips as the controller is told them: what the channels' components
// add up to, at least 1 (the smallest count seen on the wire), at most one header's limit, and
// both together within what the stream has room for.
function stripCounts() {
  const counts = [];
  let room = STRIP_LED_BUDGET;
  STRIPS.forEach((strip, index) => {
    const channel = device.channel(strip.channel);
    const wanted = channel ? Number(channel.LedCount()) || 0 : 0;
    const stripsAfter = STRIPS.length - 1 - index;
    const count = Math.max(1, Math.min(wanted, STRIP_LED_LIMIT, room - stripsAfter));
    counts.push(count);
    room -= count;
  });
  return counts;
}

// As recorded: the reset 0x53 report, then the per-LED configuration.
function configure(counts) {
  const packet = configPacket(counts);
  sendFeature(resetStream(), "0x53");
  if (sendConfig(packet)) {
    configuredCounts = counts.slice();
    lastStream = null;                    // the next frame always goes out
  }
}

function configPacket(counts) {
  const packet = new Array(CONFIG_LENGTH).fill(0);
  packet[0] = CONFIG_ID;
  for (const [offset, bytes] of CONFIG_RECORDS) {
    bytes.forEach((value, index) => { packet[offset + index] = value; });
  }
  STRIPS.forEach((strip, index) => { packet[strip.record + STRIP_COUNT] = counts[index]; });
  packet[SAVE_FLAG_OFFSET] = 0x00;
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
  const ignored = new Set([SAVE_FLAG_OFFSET, MASTER_SYNC_OFFSET]);
  for (let slot = 0; slot < 10; slot++) {
    for (let i = 0; i < 3; i++) {
      ignored.add(MASTER_ZONE + 10 * slot + 5 + i);
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

// ---------------------------------------------------------------------------------------------
// I/O
// ---------------------------------------------------------------------------------------------
function sendConfig(packet) {
  if (!sendFeature(packet, "0x52")) {
    return false;
  }
  lastConfig = packet.slice();
  return true;
}

function sendStream(packet) {
  if (sendFeature(packet, "0x53")) {
    lastStream = packet.slice();
    lastStreamAt = Date.now();
  }
}

// SET_REPORT(Feature). The host returns bytes written or -1; it never throws.
function sendFeature(packet, label) {
  if (device.send_report(packet, packet.length) < 0) {
    device.log(`MSI 185: send_report(${label}) failed.`, { toFile: true });
    return false;
  }
  wrote = true;
  return true;
}

// Tells the user when the BIOS keeps the onboard LEDs off; changes nothing.
function checkEzLed() {
  const query = new Array(COMMAND_LENGTH).fill(0xCC);
  query[0] = 0x01;
  query[1] = EZ_LED_QUERY;
  device.clearReadBuffer();
  if (device.write(query, COMMAND_LENGTH) < 0) {
    return;
  }
  const reply = device.read([0x01], COMMAND_LENGTH, 100);
  if (isByteArray(reply, EZ_LED_STATE + 1) && (reply[1] & 0xFF) === EZ_LED_REPLY && (reply[EZ_LED_STATE] & 0xFF) === 0) {
    device.log("MSI 185: EZ LED Control is off in the BIOS, so the onboard LEDs stay dark; the headers still work.", { toFile: true });
  }
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------
// A channel's colours as a flat R, G, B array (empty for an unknown channel).
function channelColours(name) {
  const channel = device.channel(name);
  const colours = channel ? channel.getColors("Inline", "RGB") : [];
  return isByteArray(colours, 0) ? Array.from(colours, (value) => value & 0xFF) : [];
}

function rgbOrBlack(rgb) {
  return isByteArray(rgb, 3) ? rgb : [0, 0, 0];
}

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

function sameValues(a, b) {
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
