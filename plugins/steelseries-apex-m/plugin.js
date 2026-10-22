// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: the per-key lighting of the SteelSeries Apex M750 keyboard.
// Adapted from OpenRGB Controllers/SteelSeriesController/SteelSeriesApexMController/
// SteelSeriesApexMController.cpp and .h, SteelSeriesApexBaseController.cpp and .h,
// SteelSeriesApexController/RGBController_SteelSeriesApex.cpp and .h, SteelSeriesApexRegions.h,
// SteelSeriesControllerDetect.cpp and SteelSeriesDevices.h, and RGBController/
// RGBControllerKeyNames.cpp (key names) (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Florian Heilmann (FHeilmann)  12 Oct 2020" (SteelSeriesApexMController);
// "Florian Heilmann (FHeilmann)  19 Oct 2020" (SteelSeriesApexBaseController); "Eric Samuelson
// (edbgon)  05 Jul 2020" (RGBController_SteelSeriesApex); "Joseph East (dripsnek)  2021"
// (SteelSeriesApexRegions.h); "Chris M (Dr_No)  25 Jan 2022" (RGBControllerKeyNames.cpp);
// SteelSeriesControllerDetect.cpp and SteelSeriesDevices.h name no author.
// (OpenRGB's other Apex driver, SteelSeriesApexController.cpp, is not used here.)
//
// Protocol notes: docs/hardware/steelseries-apex-m.md. In short: interface 2, 513-byte feature
// reports with report id 0x00. At start three reports "enable LED control"; every update is one
// report 00 00 01 8E 01 03 06 16 followed by 132 RGB slots, the slot -> key table being OpenRGB's
// keys_m (slots without a key get FF 32 00, as in OpenRGB), with its numeric-pad entries
// corrected to the keys their labels name (see KEYS_M below).

/* global device */

const PRODUCTS = [{ pid: 0x0616, name: "SteelSeries Apex M750" }];

const NA = -1;

// SteelSeriesApexRegions.h led_names (the OpenRGB LED list of every Apex keyboard), as the
// RGBControllerKeyNames.cpp strings.
const LED_NAMES = [
  "Key: A", //   0 KEY_EN_A
  "Key: B", //   1 KEY_EN_B
  "Key: C", //   2 KEY_EN_C
  "Key: D", //   3 KEY_EN_D
  "Key: E", //   4 KEY_EN_E
  "Key: F", //   5 KEY_EN_F
  "Key: G", //   6 KEY_EN_G
  "Key: H", //   7 KEY_EN_H
  "Key: I", //   8 KEY_EN_I
  "Key: J", //   9 KEY_EN_J
  "Key: K", //  10 KEY_EN_K
  "Key: L", //  11 KEY_EN_L
  "Key: M", //  12 KEY_EN_M
  "Key: N", //  13 KEY_EN_N
  "Key: O", //  14 KEY_EN_O
  "Key: P", //  15 KEY_EN_P
  "Key: Q", //  16 KEY_EN_Q
  "Key: R", //  17 KEY_EN_R
  "Key: S", //  18 KEY_EN_S
  "Key: T", //  19 KEY_EN_T
  "Key: U", //  20 KEY_EN_U
  "Key: V", //  21 KEY_EN_V
  "Key: W", //  22 KEY_EN_W
  "Key: X", //  23 KEY_EN_X
  "Key: Y", //  24 KEY_EN_Y
  "Key: Z", //  25 KEY_EN_Z
  "Key: 1", //  26 KEY_EN_1
  "Key: 2", //  27 KEY_EN_2
  "Key: 3", //  28 KEY_EN_3
  "Key: 4", //  29 KEY_EN_4
  "Key: 5", //  30 KEY_EN_5
  "Key: 6", //  31 KEY_EN_6
  "Key: 7", //  32 KEY_EN_7
  "Key: 8", //  33 KEY_EN_8
  "Key: 9", //  34 KEY_EN_9
  "Key: 0", //  35 KEY_EN_0
  "Key: Enter", //  36 KEY_EN_ANSI_ENTER
  "Key: Escape", //  37 KEY_EN_ESCAPE
  "Key: Backspace", //  38 KEY_EN_BACKSPACE
  "Key: Tab", //  39 KEY_EN_TAB
  "Key: Space", //  40 KEY_EN_SPACE
  "Key: -", //  41 KEY_EN_MINUS
  "Key: =", //  42 KEY_EN_EQUALS
  "Key: [", //  43 KEY_EN_LEFT_BRACKET
  "Key: ]", //  44 KEY_EN_RIGHT_BRACKET
  "Key: #", //  45 KEY_EN_POUND
  "Key: ;", //  46 KEY_EN_SEMICOLON
  "Key: '", //  47 KEY_EN_QUOTE
  "Key: `", //  48 KEY_EN_BACK_TICK
  "Key: ,", //  49 KEY_EN_COMMA
  "Key: .", //  50 KEY_EN_PERIOD
  "Key: /", //  51 KEY_EN_FORWARD_SLASH
  "Key: Caps Lock", //  52 KEY_EN_CAPS_LOCK
  "Key: F1", //  53 KEY_EN_F1
  "Key: F2", //  54 KEY_EN_F2
  "Key: F3", //  55 KEY_EN_F3
  "Key: F4", //  56 KEY_EN_F4
  "Key: F5", //  57 KEY_EN_F5
  "Key: F6", //  58 KEY_EN_F6
  "Key: F7", //  59 KEY_EN_F7
  "Key: F8", //  60 KEY_EN_F8
  "Key: F9", //  61 KEY_EN_F9
  "Key: F10", //  62 KEY_EN_F10
  "Key: F11", //  63 KEY_EN_F11
  "Key: F12", //  64 KEY_EN_F12
  "Key: Print Screen", //  65 KEY_EN_PRINT_SCREEN
  "Key: Scroll Lock", //  66 KEY_EN_SCROLL_LOCK
  "Key: Pause/Break", //  67 KEY_EN_PAUSE_BREAK
  "Key: Insert", //  68 KEY_EN_INSERT
  "Key: Home", //  69 KEY_EN_HOME
  "Key: Page Up", //  70 KEY_EN_PAGE_UP
  "Key: Delete", //  71 KEY_EN_DELETE
  "Key: End", //  72 KEY_EN_END
  "Key: Page Down", //  73 KEY_EN_PAGE_DOWN
  "Key: Right Arrow", //  74 KEY_EN_RIGHT_ARROW
  "Key: Left Arrow", //  75 KEY_EN_LEFT_ARROW
  "Key: Down Arrow", //  76 KEY_EN_DOWN_ARROW
  "Key: Up Arrow", //  77 KEY_EN_UP_ARROW
  "Key: \\ (ISO)", //  78 KEY_EN_ISO_BACK_SLASH
  "Key: Left Control", //  79 KEY_EN_LEFT_CONTROL
  "Key: Left Shift", //  80 KEY_EN_LEFT_SHIFT
  "Key: Left Alt", //  81 KEY_EN_LEFT_ALT
  "Key: Left Windows", //  82 KEY_EN_LEFT_WINDOWS
  "Key: Right Control", //  83 KEY_EN_RIGHT_CONTROL
  "Key: Right Shift", //  84 KEY_EN_RIGHT_SHIFT
  "Key: Right Alt", //  85 KEY_EN_RIGHT_ALT
  "Key: Right Windows", //  86 KEY_EN_RIGHT_WINDOWS
  "Key: Right Fn", //  87 KEY_EN_RIGHT_FUNCTION
  "Key: \\ (ANSI)", //  88 KEY_EN_ANSI_BACK_SLASH
  "Key: _", //  89 KEY_JP_RO
  "Key: かな", //  90 KEY_JP_KANA
  "Key: ¥", //  91 KEY_JP_YEN
  "Key: 変換", //  92 KEY_JP_HENKAN
  "Key: 無変換", //  93 KEY_JP_MUHENKAN
  "Key: Num Lock", //  94 KEY_EN_NUMPAD_LOCK
  "Key: Number Pad /", //  95 KEY_EN_NUMPAD_DIVIDE
  "Key: Number Pad *", //  96 KEY_EN_NUMPAD_TIMES
  "Key: Number Pad -", //  97 KEY_EN_NUMPAD_MINUS
  "Key: Number Pad +", //  98 KEY_EN_NUMPAD_PLUS
  "Key: Number Pad Enter", //  99 KEY_EN_NUMPAD_ENTER
  "Key: Number Pad 1", // 100 KEY_EN_NUMPAD_1
  "Key: Number Pad 2", // 101 KEY_EN_NUMPAD_2
  "Key: Number Pad 3", // 102 KEY_EN_NUMPAD_3
  "Key: Number Pad 4", // 103 KEY_EN_NUMPAD_4
  "Key: Number Pad 5", // 104 KEY_EN_NUMPAD_5
  "Key: Number Pad 6", // 105 KEY_EN_NUMPAD_6
  "Key: Number Pad 7", // 106 KEY_EN_NUMPAD_7
  "Key: Number Pad 8", // 107 KEY_EN_NUMPAD_8
  "Key: Number Pad 9", // 108 KEY_EN_NUMPAD_9
  "Key: Number Pad 0", // 109 KEY_EN_NUMPAD_0
  "Key: Number Pad .", // 110 KEY_EN_NUMPAD_PERIOD
  "Key: Media Play/Pause", // 111 KEY_EN_MEDIA_PLAY_PAUSE
];

// SteelSeriesApexRegions.h MATRIX_MAP_ANSI (6 x 22, values index LED_NAMES). The M750 has no
// serial-number SKU in OpenRGB (GetSerial() answers only for the APEX protocol), so no regional
// patch applies and this matrix is used as is.
const MATRIX_MAP_ANSI = [
  [ 37,  NA,  53,  54,  55,  56,  NA,  57,  58,  59,  60,  61,  62,  63,  64,  65,  66,  67,  NA,  NA,  NA,  NA],
  [ 48,  26,  27,  28,  29,  30,  31,  32,  33,  34,  35,  41,  42,  38,  NA,  68,  69,  70,  94,  95,  96,  97],
  [ 39,  NA,  16,  22,   4,  17,  19,  24,  20,   8,  14,  15,  43,  44,  88,  71,  72,  73, 106, 107, 108,  98],
  [ 52,  NA,   0,  18,   3,   5,   6,   7,   9,  10,  11,  46,  47,  36,  NA,  NA,  NA,  NA, 103, 104, 105,  NA],
  [ 80,  NA,  25,  23,   2,  21,   1,  13,  12,  49,  50,  51,  84,  NA,  NA,  NA,  77,  NA, 100, 101, 102,  99],
  [ 79,  82,  81,  NA,  NA,  NA,  NA,  40,  NA,  NA,  NA,  85,  86,  87,  83,  75,  76,  74, 109,  NA, 110,  NA],
];

// SteelSeriesApexMController.cpp keys_m: wire slot -> OpenRGB LED index (the comments are
// OpenRGB's labels of the physical keys). The values are copied unchanged; keysMKey() below
// corrects the numeric-pad ones.
const KEYS_M = [
  //    LCTRL  LWIN  LALT   XXX SPACE   XXX   XXX   XXX   XXX  RALT  RWIN   FNC RCTRL   XXX   XXX  LEFT  DOWN RIGHT   XXX    #0   XXX    #.
    79,   82,   81,   NA,   40,   NA,   NA,   NA,   NA,   85,   86,   87,   83,   NA,   NA,   75,   76,   74,   NA,  104,   NA,  105,
  //    LSHFT     Z     X     C     V     B     N     M     ,     .     /   XXX RSHFT   XXX   XXX   XXX    UP   XXX    #1    #2    #3 #ENTR
    80,   25,   23,    2,   21,    1,   13,   12,   49,   50,   51,   NA,   84,   NA,   NA,   NA,   77,   NA,   95,   96,   97,   94,
  //    CAPLK     A     S     D     F     G     H     J     K     L     ;     '   XXX ENTER   XXX   XXX   XXX   XXX    #4    #5    #6   XXX
    52,    0,   18,    3,    5,    6,    7,    9,   10,   11,   46,   47,   NA,   36,   NA,   NA,   NA,   NA,   98,   99,  100,   NA,
  //      TAB     Q     W     E     R     T     Y     U     I     O     P     [     ]   XXX     \ DELTE   END  PGDN    #7    #8    #9    #+
    39,   16,   22,    4,   17,   19,   24,   20,    8,   14,   15,   43,   44,   NA,   88,   71,   72,   73,  101,  102,  103,   93,
  //        `     1     2     3     4     5     6     7     8     9     0     -     =   XXX BKSPC INSRT  HOME  PGUP NUMLK    #/    #*    #-
    48,   26,   27,   28,   29,   30,   31,   32,   33,   34,   35,   41,   42,   NA,   38,   68,   69,   70,   89,   90,   91,   92,
  //      ESC    F1    F2    F3    F4   XXX    F5    F6    F7    F8   XXX    F9   F10   F11   F12 PRTSC SCRLK PAUSE   XXX   XXX   XXX   XXX
    37,   53,   54,   55,   56,   NA,   57,   58,   59,   60,   NA,   61,   62,   63,   64,   65,   66,   67,   NA,   NA,   NA,   NA,
];

const PACKET_SIZE = 513;
// Render() resends unchanged colours only after this long.
const KEEPALIVE_MS = 1000;

// keys_m was written against an older led_names list, from before the five Japanese keys
// (KEY_JP_RO .. KEY_JP_MUHENKAN, now 89..93) were inserted ahead of the numeric pad. Its values
// 89..105 are the numeric-pad keys of that old list (NUMLK=89, #/=90, #*=91, #-=92, #+=93,
// #ENTR=94, #1..#9=95..103, #0=104, #.=105, exactly as the slot labels in keys_m say), which are
// 94..110 in today's led_names. HueDeck feeds each of those slots from the key its label names
// (value + 5), so the canvas cell of "Number Pad 0" lights the #0 key. OpenRGB itself sends
// today's LED 104 ("Number Pad 5") to the #0 slot; the wire slot order is unchanged.
const NUMPAD_OLD_FIRST = 89;
const NUMPAD_OLD_LAST = 105;
const NUMPAD_SHIFT = 5;

function keysMKey(value) {
  return value >= NUMPAD_OLD_FIRST && value <= NUMPAD_OLD_LAST ? value + NUMPAD_SHIFT : value;
}

// LEDs: the keys the wire table drives, in slot order, named after the OpenRGB LED (without the
// "Key: " prefix) of keysMKey(keys_m[slot]) and placed at that LED's MATRIX_MAP_ANSI cell.
const { LEDS, SIZE } = buildLayout();

export function Name() { return "SteelSeries Apex M750"; }
export function Publisher() { return "HueDeck"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/steelseries-apex-m"; }
export function VendorId() { return 0x1038; }
export function ProductId() { return PRODUCTS.map((p) => p.pid); }
export function Type() { return "Hid"; }
export function DeviceType() { return "keyboard"; }
export function Size() { return SIZE.slice(); }
export function DefaultPosition() { return [0, 0]; }
export function DefaultScale() { return 8.0; }
export function LedNames() { return LEDS.map((l) => l.name); }
export function LedPositions() { return LEDS.map((l) => l.cell.slice()); }
export function ImageUrl() { return ""; }
// SteelSeries' own software drives the lights while it runs; HueDeck reports it, never closes it.
export function ConflictingProcesses() { return ["SteelSeriesGG.exe", "SteelSeriesEngine.exe"]; }

let enabled = false;
let lastColours = null;
let lastSentAt = 0;

// REGISTER_HID_DETECTOR_I(..., 2): any collection of interface 2.
export function Validate(endpoint) {
  return endpoint.interface === 2;
}

export function Initialize() {
  lastColours = null;
  lastSentAt = 0;
  const product = PRODUCTS.find((p) => p.pid === device.productId());
  enabled = Boolean(product);
  if (!product) {
    device.log(`SteelSeries Apex M750: product id 0x${hex4(device.productId())} is not one this plugin knows.`);
    return;
  }
  device.setName(product.name);
  device.setSize(SIZE.slice());
  device.setControllableLeds(LEDS.map((l) => l.name), LEDS.map((l) => l.cell.slice()));
  // SteelSeriesApexMController constructor: EnableLEDControl(). OpenRGB's Direct mode then sends
  // nothing of its own (SetMode is empty for the M750); the colours follow in Render().
  enableLedControl();
}

export function Render() {
  if (!enabled) {
    return;
  }
  const colours = LEDS.map((l) => {
    const [r, g, b] = device.color(l.cell[0], l.cell[1]);
    return [r & 0xFF, g & 0xFF, b & 0xFF];
  });
  const now = Date.now();
  if (lastColours !== null && sameColours(colours, lastColours) && now - lastSentAt < KEEPALIVE_MS) {
    return;
  }
  if (setLedsDirect(colours)) {
    lastColours = colours;
    lastSentAt = now;
  }
}

// OpenRGB has no exit step, and its "Onboard" mode sends nothing on the M750 (SetMode is empty):
// the keys are switched off (black, in Direct).
export function Shutdown(SystemSuspending) {
  if (!enabled) {
    return;
  }
  setLedsDirect(LEDS.map(() => [0, 0, 0]));
}

function buildLayout() {
  const cellOf = new Map();
  MATRIX_MAP_ANSI.forEach((row, y) => row.forEach((idx, x) => {
    if (idx !== NA) {
      cellOf.set(idx, [x, y]);
    }
  }));
  const leds = [];
  KEYS_M.forEach((value, slot) => {
    if (value === NA) {
      return;
    }
    const idx = keysMKey(value);
    const cell = cellOf.get(idx);
    if (!cell) {
      throw new Error(`Apex M750: keys_m slot ${slot} (LED ${idx}) has no matrix cell`);
    }
    leds.push({ name: LED_NAMES[idx].replace(/^Key: /, ""), slot, cell });
  });
  return { LEDS: leds, SIZE: [MATRIX_MAP_ANSI[0].length, MATRIX_MAP_ANSI.length] };
}

// SteelSeriesApexMController::EnableLEDControl: one buffer reused for three feature reports.
function enableLedControl() {
  const buf = new Array(PACKET_SIZE).fill(0);
  buf[0x04] = 0x01;
  buf[0x05] = 0x00;
  buf[0x06] = 0x85;
  let ok = device.send_report(buf.slice(), PACKET_SIZE) !== -1;
  buf[0x04] = 0x03;
  buf[0x05] = 0x01;
  buf[0x06] = 0x00;
  buf[0x07] = 0xFF;
  ok = (device.send_report(buf.slice(), PACKET_SIZE) !== -1) && ok;
  // The third report repeats the first, but byte 7 keeps the 0xFF of the second (the C++ does not
  // clear the buffer).
  buf[0x04] = 0x01;
  buf[0x05] = 0x00;
  buf[0x06] = 0x85;
  ok = (device.send_report(buf.slice(), PACKET_SIZE) !== -1) && ok;
  return ok;
}

// SteelSeriesApexMController::SetLEDsDirect: `colours` in LEDS order (one per driven slot).
function setLedsDirect(colours) {
  const buf = new Array(PACKET_SIZE).fill(0);
  buf[0x03] = 0x01;
  buf[0x04] = 0x8E;
  buf[0x05] = 0x01;
  buf[0x06] = 0x03;
  buf[0x07] = 0x06;
  buf[0x08] = 0x16;
  for (let i = 0; i < KEYS_M.length; i++) {
    buf[i * 3 + 9] = 0xFF;
    buf[i * 3 + 10] = 0x32;
    buf[i * 3 + 11] = 0x00;
  }
  LEDS.forEach((led, i) => {
    const [r, g, b] = colours[i];
    buf[led.slot * 3 + 9] = r;
    buf[led.slot * 3 + 10] = g;
    buf[led.slot * 3 + 11] = b;
  });
  return device.send_report(buf, PACKET_SIZE) !== -1;
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
