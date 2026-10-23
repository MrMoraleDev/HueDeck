// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: Roccat Vulcan family key layouts, as data. Generated from the OpenRGB tables by
// a script in the porting session's scratch space; do not edit by hand.
// Adapted from OpenRGB Controllers/RoccatController/RoccatVulcanKeyboardController/
// RoccatVulcanKeyboardLayouts.h and RGBController_RoccatVulcanKeyboard.cpp (SetupZones), with key
// names from RGBController/RGBControllerKeyNames.cpp (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: Mola19 (29 Sep 2022, RoccatVulcanKeyboardLayouts.h; 17 Dec 2021,
// RGBController_RoccatVulcanKeyboard.cpp); RGBControllerKeyNames.cpp names no author.
//
// One entry per OpenRGB layout table (std::map<int, layout_info>), keyed by OpenRGB's layout
// (US, UK, JP). Per LED, in OpenRGB's LED order (led_names[0 .. size - 1], the order SendColors
// walks):
//   [name, id, x, y]  name: the key in HueDeck's key names (OpenRGB's own text without "Key: "
//                     where HueDeck has no name); id: led.value, the LED's wire id; x, y: its cell
//                     in OpenRGB's matrix map (keyboard zone) or, for the Vulcan II Max's linear
//                     zones, one row per zone below the keys. x and y are null for an LED OpenRGB
//                     places in no cell (never lit by HueDeck: always sent black).
// Corrections (see docs/hardware/roccat-vulcan.md): the Turtle Beach Vulcan II TKL Pro table names
// LED 56 (id 0x3A) "Y" a second time; its cell is the Z key, so it is named "Z" here. LEDs named
// KEY_EN_UNUSED ("") are named "LED 0x<id>".

export const LAYOUTS = Object.freeze({
  RoccatVulcan120AimoLayouts: Object.freeze({
    UK: Object.freeze({
      width: 24,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0x00, 0, 0], //   0 KEY_EN_ESCAPE
        ["`", 0x01, 0, 1], //   1 KEY_EN_BACK_TICK
        ["Tab", 0x02, 0, 2], //   2 KEY_EN_TAB
        ["Caps Lock", 0x03, 0, 3], //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0x04, 0, 4], //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0x05, 0, 5], //   5 KEY_EN_LEFT_CONTROL
        ["1", 0x06, 1, 1], //   6 KEY_EN_1
        ["ISO_<", 0x09, 1, 4], //   7 KEY_EN_ISO_BACK_SLASH
        ["Left Win", 0x0A, 1, 5], //   8 KEY_EN_LEFT_WINDOWS
        ["F1", 0x0B, 2, 0], //   9 KEY_EN_F1
        ["2", 0x0C, 2, 1], //  10 KEY_EN_2
        ["Q", 0x07, 2, 2], //  11 KEY_EN_Q
        ["A", 0x08, 2, 3], //  12 KEY_EN_A
        ["Z", 0x0F, 2, 4], //  13 KEY_EN_Z
        ["Left Alt", 0x10, 2, 5], //  14 KEY_EN_LEFT_ALT
        ["F2", 0x11, 3, 0], //  15 KEY_EN_F2
        ["3", 0x12, 3, 1], //  16 KEY_EN_3
        ["W", 0x0D, 3, 2], //  17 KEY_EN_W
        ["S", 0x0E, 3, 3], //  18 KEY_EN_S
        ["X", 0x15, 3, 4], //  19 KEY_EN_X
        ["F3", 0x17, 4, 0], //  20 KEY_EN_F3
        ["4", 0x18, 4, 1], //  21 KEY_EN_4
        ["E", 0x13, 4, 2], //  22 KEY_EN_E
        ["D", 0x14, 4, 3], //  23 KEY_EN_D
        ["C", 0x1B, 4, 4], //  24 KEY_EN_C
        ["F4", 0x1C, 5, 0], //  25 KEY_EN_F4
        ["5", 0x1D, 5, 1], //  26 KEY_EN_5
        ["R", 0x19, 5, 2], //  27 KEY_EN_R
        ["F", 0x1A, 5, 3], //  28 KEY_EN_F
        ["V", 0x20, 5, 4], //  29 KEY_EN_V
        ["6", 0x21, 6, 1], //  30 KEY_EN_6
        ["T", 0x1E, 6, 2], //  31 KEY_EN_T
        ["G", 0x1F, 6, 3], //  32 KEY_EN_G
        ["B", 0x24, 6, 4], //  33 KEY_EN_B
        ["Space", 0x25, 6, 5], //  34 KEY_EN_SPACE
        ["F5", 0x30, 7, 0], //  35 KEY_EN_F5
        ["7", 0x31, 7, 1], //  36 KEY_EN_7
        ["Y", 0x22, 7, 2], //  37 KEY_EN_Y
        ["H", 0x23, 7, 3], //  38 KEY_EN_H
        ["N", 0x34, 7, 4], //  39 KEY_EN_N
        ["F6", 0x35, 8, 0], //  40 KEY_EN_F6
        ["8", 0x36, 8, 1], //  41 KEY_EN_8
        ["U", 0x32, 8, 2], //  42 KEY_EN_U
        ["J", 0x33, 8, 3], //  43 KEY_EN_J
        ["M", 0x39, 8, 4], //  44 KEY_EN_M
        ["F7", 0x3B, 9, 0], //  45 KEY_EN_F7
        ["9", 0x3C, 9, 1], //  46 KEY_EN_9
        ["I", 0x37, 9, 2], //  47 KEY_EN_I
        ["K", 0x38, 9, 3], //  48 KEY_EN_K
        [",", 0x3F, 9, 4], //  49 KEY_EN_COMMA
        ["F8", 0x41, 10, 0], //  50 KEY_EN_F8
        ["0", 0x42, 10, 1], //  51 KEY_EN_0
        ["O", 0x3D, 10, 2], //  52 KEY_EN_O
        ["L", 0x3E, 10, 3], //  53 KEY_EN_L
        [".", 0x45, 10, 4], //  54 KEY_EN_PERIOD
        ["Right Alt", 0x46, 10, 5], //  55 KEY_EN_RIGHT_ALT
        ["F9", 0x4E, 11, 0], //  56 KEY_EN_F9
        ["-", 0x48, 11, 1], //  57 KEY_EN_MINUS
        ["P", 0x43, 11, 2], //  58 KEY_EN_P
        [";", 0x44, 11, 3], //  59 KEY_EN_SEMICOLON
        ["/", 0x4B, 11, 4], //  60 KEY_EN_FORWARD_SLASH
        ["Fn", 0x4C, 11, 5], //  61 KEY_EN_RIGHT_FUNCTION
        ["F10", 0x54, 12, 0], //  62 KEY_EN_F10
        ["=", 0x4F, 12, 1], //  63 KEY_EN_EQUALS
        ["[", 0x49, 12, 2], //  64 KEY_EN_LEFT_BRACKET
        ["'", 0x4A, 12, 3], //  65 KEY_EN_QUOTE
        ["Menu", 0x53, 12, 5], //  66 KEY_EN_MENU
        ["F11", 0x55, 13, 0], //  67 KEY_EN_F11
        ["Backspace", 0x57, 13, 1], //  68 KEY_EN_BACKSPACE
        ["]", 0x50, 13, 2], //  69 KEY_EN_RIGHT_BRACKET
        ["ISO_#", 0x60, 13, 3], //  70 KEY_EN_POUND
        ["Right Shift", 0x52, 13, 4], //  71 KEY_EN_RIGHT_SHIFT
        ["F12", 0x56, 14, 0], //  72 KEY_EN_F12
        ["Enter", 0x58, 14, 3], //  73 KEY_EN_ISO_ENTER
        ["Right Ctrl", 0x59, 14, 5], //  74 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 0x63, 16, 0], //  75 KEY_EN_PRINT_SCREEN
        ["Insert", 0x64, 16, 1], //  76 KEY_EN_INSERT
        ["Del", 0x65, 16, 2], //  77 KEY_EN_DELETE
        ["Left Arrow", 0x66, 16, 5], //  78 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 0x67, 17, 0], //  79 KEY_EN_SCROLL_LOCK
        ["Home", 0x68, 17, 1], //  80 KEY_EN_HOME
        ["End", 0x69, 17, 2], //  81 KEY_EN_END
        ["Up Arrow", 0x6A, 17, 4], //  82 KEY_EN_UP_ARROW
        ["Down Arrow", 0x6B, 17, 5], //  83 KEY_EN_DOWN_ARROW
        ["Pause Break", 0x6C, 18, 0], //  84 KEY_EN_PAUSE_BREAK
        ["Page Up", 0x6D, 18, 1], //  85 KEY_EN_PAGE_UP
        ["Page Down", 0x6E, 18, 2], //  86 KEY_EN_PAGE_DOWN
        ["Right Arrow", 0x6F, 18, 5], //  87 KEY_EN_RIGHT_ARROW
        ["NumLock", 0x71, 20, 1], //  88 KEY_EN_NUMPAD_LOCK
        ["Num 7", 0x72, 20, 2], //  89 KEY_EN_NUMPAD_7
        ["Num 4", 0x73, 20, 3], //  90 KEY_EN_NUMPAD_4
        ["Num 1", 0x74, 20, 4], //  91 KEY_EN_NUMPAD_1
        ["Num 0", 0x75, 20, 5], //  92 KEY_EN_NUMPAD_0
        ["Num /", 0x77, 21, 1], //  93 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 0x78, 21, 2], //  94 KEY_EN_NUMPAD_8
        ["Num 5", 0x79, 21, 3], //  95 KEY_EN_NUMPAD_5
        ["Num 2", 0x7A, 21, 4], //  96 KEY_EN_NUMPAD_2
        ["Num *", 0x7C, 22, 1], //  97 KEY_EN_NUMPAD_TIMES
        ["Num 9", 0x7D, 22, 2], //  98 KEY_EN_NUMPAD_9
        ["Num 6", 0x7E, 22, 3], //  99 KEY_EN_NUMPAD_6
        ["Num 3", 0x7F, 22, 4], // 100 KEY_EN_NUMPAD_3
        ["Num .", 0x80, 22, 5], // 101 KEY_EN_NUMPAD_PERIOD
        ["Num -", 0x81, 23, 1], // 102 KEY_EN_NUMPAD_MINUS
        ["Num +", 0x82, 23, 2], // 103 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 0x83, 23, 4], // 104 KEY_EN_NUMPAD_ENTER
      ]),
    }),
    US: Object.freeze({
      width: 24,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0x00, 0, 0], //   0 KEY_EN_ESCAPE
        ["`", 0x01, 0, 1], //   1 KEY_EN_BACK_TICK
        ["Tab", 0x02, 0, 2], //   2 KEY_EN_TAB
        ["Caps Lock", 0x03, 0, 3], //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0x04, 0, 4], //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0x05, 0, 5], //   5 KEY_EN_LEFT_CONTROL
        ["1", 0x06, 1, 1], //   6 KEY_EN_1
        ["Left Win", 0x0A, 1, 5], //   7 KEY_EN_LEFT_WINDOWS
        ["F1", 0x0B, 2, 0], //   8 KEY_EN_F1
        ["2", 0x0C, 2, 1], //   9 KEY_EN_2
        ["Q", 0x07, 2, 2], //  10 KEY_EN_Q
        ["A", 0x08, 2, 3], //  11 KEY_EN_A
        ["Z", 0x0F, 2, 4], //  12 KEY_EN_Z
        ["Left Alt", 0x10, 2, 5], //  13 KEY_EN_LEFT_ALT
        ["F2", 0x11, 3, 0], //  14 KEY_EN_F2
        ["3", 0x12, 3, 1], //  15 KEY_EN_3
        ["W", 0x0D, 3, 2], //  16 KEY_EN_W
        ["S", 0x0E, 3, 3], //  17 KEY_EN_S
        ["X", 0x15, 3, 4], //  18 KEY_EN_X
        ["F3", 0x17, 4, 0], //  19 KEY_EN_F3
        ["4", 0x18, 4, 1], //  20 KEY_EN_4
        ["E", 0x13, 4, 2], //  21 KEY_EN_E
        ["D", 0x14, 4, 3], //  22 KEY_EN_D
        ["C", 0x1B, 4, 4], //  23 KEY_EN_C
        ["F4", 0x1C, 5, 0], //  24 KEY_EN_F4
        ["5", 0x1D, 5, 1], //  25 KEY_EN_5
        ["R", 0x19, 5, 2], //  26 KEY_EN_R
        ["F", 0x1A, 5, 3], //  27 KEY_EN_F
        ["V", 0x20, 5, 4], //  28 KEY_EN_V
        ["6", 0x21, 6, 1], //  29 KEY_EN_6
        ["T", 0x1E, 6, 2], //  30 KEY_EN_T
        ["G", 0x1F, 6, 3], //  31 KEY_EN_G
        ["B", 0x24, 6, 4], //  32 KEY_EN_B
        ["Space", 0x25, 6, 5], //  33 KEY_EN_SPACE
        ["F5", 0x30, 7, 0], //  34 KEY_EN_F5
        ["7", 0x31, 7, 1], //  35 KEY_EN_7
        ["Y", 0x22, 7, 2], //  36 KEY_EN_Y
        ["H", 0x23, 7, 3], //  37 KEY_EN_H
        ["N", 0x34, 7, 4], //  38 KEY_EN_N
        ["F6", 0x35, 8, 0], //  39 KEY_EN_F6
        ["8", 0x36, 8, 1], //  40 KEY_EN_8
        ["U", 0x32, 8, 2], //  41 KEY_EN_U
        ["J", 0x33, 8, 3], //  42 KEY_EN_J
        ["M", 0x39, 8, 4], //  43 KEY_EN_M
        ["F7", 0x3B, 9, 0], //  44 KEY_EN_F7
        ["9", 0x3C, 9, 1], //  45 KEY_EN_9
        ["I", 0x37, 9, 2], //  46 KEY_EN_I
        ["K", 0x38, 9, 3], //  47 KEY_EN_K
        [",", 0x3F, 9, 4], //  48 KEY_EN_COMMA
        ["F8", 0x41, 10, 0], //  49 KEY_EN_F8
        ["0", 0x42, 10, 1], //  50 KEY_EN_0
        ["O", 0x3D, 10, 2], //  51 KEY_EN_O
        ["L", 0x3E, 10, 3], //  52 KEY_EN_L
        [".", 0x45, 10, 4], //  53 KEY_EN_PERIOD
        ["Right Alt", 0x46, 10, 5], //  54 KEY_EN_RIGHT_ALT
        ["F9", 0x4E, 11, 0], //  55 KEY_EN_F9
        ["-", 0x48, 11, 1], //  56 KEY_EN_MINUS
        ["P", 0x43, 11, 2], //  57 KEY_EN_P
        [";", 0x44, 11, 3], //  58 KEY_EN_SEMICOLON
        ["/", 0x4B, 11, 4], //  59 KEY_EN_FORWARD_SLASH
        ["Fn", 0x4C, 11, 5], //  60 KEY_EN_RIGHT_FUNCTION
        ["F10", 0x54, 12, 0], //  61 KEY_EN_F10
        ["=", 0x4F, 12, 1], //  62 KEY_EN_EQUALS
        ["[", 0x49, 12, 2], //  63 KEY_EN_LEFT_BRACKET
        ["'", 0x4A, 12, 3], //  64 KEY_EN_QUOTE
        ["Menu", 0x53, 12, 5], //  65 KEY_EN_MENU
        ["F11", 0x55, 13, 0], //  66 KEY_EN_F11
        ["Backspace", 0x57, 13, 1], //  67 KEY_EN_BACKSPACE
        ["]", 0x50, 13, 2], //  68 KEY_EN_RIGHT_BRACKET
        ["Right Shift", 0x52, 13, 4], //  69 KEY_EN_RIGHT_SHIFT
        ["F12", 0x56, 14, 0], //  70 KEY_EN_F12
        ["\\", 0x51, 14, 2], //  71 KEY_EN_ANSI_BACK_SLASH
        ["Enter", 0x58, 14, 3], //  72 KEY_EN_ANSI_ENTER
        ["Right Ctrl", 0x59, 14, 5], //  73 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 0x63, 16, 0], //  74 KEY_EN_PRINT_SCREEN
        ["Insert", 0x64, 16, 1], //  75 KEY_EN_INSERT
        ["Del", 0x65, 16, 2], //  76 KEY_EN_DELETE
        ["Left Arrow", 0x66, 16, 5], //  77 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 0x67, 17, 0], //  78 KEY_EN_SCROLL_LOCK
        ["Home", 0x68, 17, 1], //  79 KEY_EN_HOME
        ["End", 0x69, 17, 2], //  80 KEY_EN_END
        ["Up Arrow", 0x6A, 17, 4], //  81 KEY_EN_UP_ARROW
        ["Down Arrow", 0x6B, 17, 5], //  82 KEY_EN_DOWN_ARROW
        ["Pause Break", 0x6C, 18, 0], //  83 KEY_EN_PAUSE_BREAK
        ["Page Up", 0x6D, 18, 1], //  84 KEY_EN_PAGE_UP
        ["Page Down", 0x6E, 18, 2], //  85 KEY_EN_PAGE_DOWN
        ["Right Arrow", 0x6F, 18, 5], //  86 KEY_EN_RIGHT_ARROW
        ["NumLock", 0x71, 20, 1], //  87 KEY_EN_NUMPAD_LOCK
        ["Num 7", 0x72, 20, 2], //  88 KEY_EN_NUMPAD_7
        ["Num 4", 0x73, 20, 3], //  89 KEY_EN_NUMPAD_4
        ["Num 1", 0x74, 20, 4], //  90 KEY_EN_NUMPAD_1
        ["Num 0", 0x75, 20, 5], //  91 KEY_EN_NUMPAD_0
        ["Num /", 0x77, 21, 1], //  92 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 0x78, 21, 2], //  93 KEY_EN_NUMPAD_8
        ["Num 5", 0x79, 21, 3], //  94 KEY_EN_NUMPAD_5
        ["Num 2", 0x7A, 21, 4], //  95 KEY_EN_NUMPAD_2
        ["Num *", 0x7C, 22, 1], //  96 KEY_EN_NUMPAD_TIMES
        ["Num 9", 0x7D, 22, 2], //  97 KEY_EN_NUMPAD_9
        ["Num 6", 0x7E, 22, 3], //  98 KEY_EN_NUMPAD_6
        ["Num 3", 0x7F, 22, 4], //  99 KEY_EN_NUMPAD_3
        ["Num .", 0x80, 22, 5], // 100 KEY_EN_NUMPAD_PERIOD
        ["Num -", 0x81, 23, 1], // 101 KEY_EN_NUMPAD_MINUS
        ["Num +", 0x82, 23, 2], // 102 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 0x83, 23, 4], // 103 KEY_EN_NUMPAD_ENTER
      ]),
    }),
  }),
  RoccatPyroLayouts: Object.freeze({
    UK: Object.freeze({
      width: 24,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0x02, 0, 0], //   0 KEY_EN_ESCAPE
        ["`", 0x03, 0, 1], //   1 KEY_EN_BACK_TICK
        ["Tab", 0x04, 0, 2], //   2 KEY_EN_TAB
        ["Caps Lock", 0x05, 0, 3], //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0x00, 0, 4], //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0x01, 0, 5], //   5 KEY_EN_LEFT_CONTROL
        ["1", 0x08, 1, 1], //   6 KEY_EN_1
        ["ISO_<", 0x06, 1, 4], //   7 KEY_EN_ISO_BACK_SLASH
        ["Left Win", 0x07, 1, 5], //   8 KEY_EN_LEFT_WINDOWS
        ["F1", 0x0D, 2, 0], //   9 KEY_EN_F1
        ["2", 0x0E, 2, 1], //  10 KEY_EN_2
        ["Q", 0x09, 2, 2], //  11 KEY_EN_Q
        ["A", 0x0A, 2, 3], //  12 KEY_EN_A
        ["Z", 0x0B, 2, 4], //  13 KEY_EN_Z
        ["Left Alt", 0x0C, 2, 5], //  14 KEY_EN_LEFT_ALT
        ["F2", 0x14, 3, 0], //  15 KEY_EN_F2
        ["3", 0x15, 3, 1], //  16 KEY_EN_3
        ["W", 0x0F, 3, 2], //  17 KEY_EN_W
        ["S", 0x10, 3, 3], //  18 KEY_EN_S
        ["X", 0x11, 3, 4], //  19 KEY_EN_X
        ["F3", 0x19, 4, 0], //  20 KEY_EN_F3
        ["4", 0x1A, 4, 1], //  21 KEY_EN_4
        ["E", 0x16, 4, 2], //  22 KEY_EN_E
        ["D", 0x17, 4, 3], //  23 KEY_EN_D
        ["C", 0x18, 4, 4], //  24 KEY_EN_C
        ["F4", 0x1E, 5, 0], //  25 KEY_EN_F4
        ["5", 0x1F, 5, 1], //  26 KEY_EN_5
        ["R", 0x1B, 5, 2], //  27 KEY_EN_R
        ["F", 0x1C, 5, 3], //  28 KEY_EN_F
        ["V", 0x1D, 5, 4], //  29 KEY_EN_V
        ["6", 0x24, 6, 1], //  30 KEY_EN_6
        ["T", 0x20, 6, 2], //  31 KEY_EN_T
        ["G", 0x21, 6, 3], //  32 KEY_EN_G
        ["B", 0x22, 6, 4], //  33 KEY_EN_B
        ["Space", 0x23, 6, 5], //  34 KEY_EN_SPACE
        ["F5", 0x28, 7, 0], //  35 KEY_EN_F5
        ["7", 0x29, 7, 1], //  36 KEY_EN_7
        ["Y", 0x25, 7, 2], //  37 KEY_EN_Y
        ["H", 0x26, 7, 3], //  38 KEY_EN_H
        ["N", 0x27, 7, 4], //  39 KEY_EN_N
        ["F6", 0x2F, 8, 0], //  40 KEY_EN_F6
        ["8", 0x30, 8, 1], //  41 KEY_EN_8
        ["U", 0x2A, 8, 2], //  42 KEY_EN_U
        ["J", 0x2B, 8, 3], //  43 KEY_EN_J
        ["M", 0x2C, 8, 4], //  44 KEY_EN_M
        ["F7", 0x35, 9, 0], //  45 KEY_EN_F7
        ["9", 0x36, 9, 1], //  46 KEY_EN_9
        ["I", 0x31, 9, 2], //  47 KEY_EN_I
        ["K", 0x32, 9, 3], //  48 KEY_EN_K
        [",", 0x33, 9, 4], //  49 KEY_EN_COMMA
        ["F8", 0x3B, 10, 0], //  50 KEY_EN_F8
        ["0", 0x3C, 10, 1], //  51 KEY_EN_0
        ["O", 0x37, 10, 2], //  52 KEY_EN_O
        ["L", 0x38, 10, 3], //  53 KEY_EN_L
        [".", 0x39, 10, 4], //  54 KEY_EN_PERIOD
        ["Right Alt", 0x3A, 10, 5], //  55 KEY_EN_RIGHT_ALT
        ["F9", 0x41, 11, 0], //  56 KEY_EN_F9
        ["-", 0x42, 11, 1], //  57 KEY_EN_MINUS
        ["P", 0x3D, 11, 2], //  58 KEY_EN_P
        [";", 0x3E, 11, 3], //  59 KEY_EN_SEMICOLON
        ["/", 0x3F, 11, 4], //  60 KEY_EN_FORWARD_SLASH
        ["Fn", 0x40, 11, 5], //  61 KEY_EN_RIGHT_FUNCTION
        ["F10", 0x47, 12, 0], //  62 KEY_EN_F10
        ["=", 0x48, 12, 1], //  63 KEY_EN_EQUALS
        ["[", 0x43, 12, 2], //  64 KEY_EN_LEFT_BRACKET
        ["'", 0x44, 12, 3], //  65 KEY_EN_QUOTE
        ["Menu", 0x46, 12, 5], //  66 KEY_EN_MENU
        ["F11", 0x4D, 13, 0], //  67 KEY_EN_F11
        ["Backspace", 0x50, 13, 1], //  68 KEY_EN_BACKSPACE
        ["]", 0x49, 13, 2], //  69 KEY_EN_RIGHT_BRACKET
        ["ISO_#", 0x4A, 13, 3], //  70 KEY_EN_POUND
        ["Right Shift", 0x4B, 13, 4], //  71 KEY_EN_RIGHT_SHIFT
        ["F12", 0x4F, 14, 0], //  72 KEY_EN_F12
        ["Enter", 0x52, 14, 3], //  73 KEY_EN_ISO_ENTER
        ["Right Ctrl", 0x4C, 14, 5], //  74 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 0x53, 16, 0], //  75 KEY_EN_PRINT_SCREEN
        ["Insert", 0x54, 16, 1], //  76 KEY_EN_INSERT
        ["Del", 0x55, 16, 2], //  77 KEY_EN_DELETE
        ["Left Arrow", 0x56, 16, 5], //  78 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 0x57, 17, 0], //  79 KEY_EN_SCROLL_LOCK
        ["Home", 0x58, 17, 1], //  80 KEY_EN_HOME
        ["End", 0x59, 17, 2], //  81 KEY_EN_END
        ["Up Arrow", 0x5A, 17, 4], //  82 KEY_EN_UP_ARROW
        ["Down Arrow", 0x5B, 17, 5], //  83 KEY_EN_DOWN_ARROW
        ["Pause Break", 0x5C, 18, 0], //  84 KEY_EN_PAUSE_BREAK
        ["Page Up", 0x5D, 18, 1], //  85 KEY_EN_PAGE_UP
        ["Page Down", 0x5E, 18, 2], //  86 KEY_EN_PAGE_DOWN
        ["Right Arrow", 0x5F, 18, 5], //  87 KEY_EN_RIGHT_ARROW
        ["NumLock", 0x61, 20, 1], //  88 KEY_EN_NUMPAD_LOCK
        ["Num 7", 0x62, 20, 2], //  89 KEY_EN_NUMPAD_7
        ["Num 4", 0x63, 20, 3], //  90 KEY_EN_NUMPAD_4
        ["Num 1", 0x64, 20, 4], //  91 KEY_EN_NUMPAD_1
        ["Num 0", 0x65, 20, 5], //  92 KEY_EN_NUMPAD_0
        ["Num /", 0x67, 21, 1], //  93 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 0x68, 21, 2], //  94 KEY_EN_NUMPAD_8
        ["Num 5", 0x69, 21, 3], //  95 KEY_EN_NUMPAD_5
        ["Num 2", 0x6A, 21, 4], //  96 KEY_EN_NUMPAD_2
        ["Num *", 0x6C, 22, 1], //  97 KEY_EN_NUMPAD_TIMES
        ["Num 9", 0x6D, 22, 2], //  98 KEY_EN_NUMPAD_9
        ["Num 6", 0x6E, 22, 3], //  99 KEY_EN_NUMPAD_6
        ["Num 3", 0x6F, 22, 4], // 100 KEY_EN_NUMPAD_3
        ["Num .", 0x70, 22, 5], // 101 KEY_EN_NUMPAD_PERIOD
        ["Num -", 0x72, 23, 1], // 102 KEY_EN_NUMPAD_MINUS
        ["Num +", 0x73, 23, 2], // 103 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 0x75, 23, 4], // 104 KEY_EN_NUMPAD_ENTER
      ]),
    }),
    US: Object.freeze({
      width: 24,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0x02, 0, 0], //   0 KEY_EN_ESCAPE
        ["`", 0x03, 0, 1], //   1 KEY_EN_BACK_TICK
        ["Tab", 0x04, 0, 2], //   2 KEY_EN_TAB
        ["Caps Lock", 0x05, 0, 3], //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0x00, 0, 4], //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0x01, 0, 5], //   5 KEY_EN_LEFT_CONTROL
        ["1", 0x08, 1, 1], //   6 KEY_EN_1
        ["Left Win", 0x07, 1, 5], //   7 KEY_EN_LEFT_WINDOWS
        ["F1", 0x0D, 2, 0], //   8 KEY_EN_F1
        ["2", 0x0E, 2, 1], //   9 KEY_EN_2
        ["Q", 0x09, 2, 2], //  10 KEY_EN_Q
        ["A", 0x0A, 2, 3], //  11 KEY_EN_A
        ["Z", 0x0B, 2, 4], //  12 KEY_EN_Z
        ["Left Alt", 0x0C, 2, 5], //  13 KEY_EN_LEFT_ALT
        ["F2", 0x14, 3, 0], //  14 KEY_EN_F2
        ["3", 0x15, 3, 1], //  15 KEY_EN_3
        ["W", 0x0F, 3, 2], //  16 KEY_EN_W
        ["S", 0x10, 3, 3], //  17 KEY_EN_S
        ["X", 0x11, 3, 4], //  18 KEY_EN_X
        ["F3", 0x19, 4, 0], //  19 KEY_EN_F3
        ["4", 0x1A, 4, 1], //  20 KEY_EN_4
        ["E", 0x16, 4, 2], //  21 KEY_EN_E
        ["D", 0x17, 4, 3], //  22 KEY_EN_D
        ["C", 0x18, 4, 4], //  23 KEY_EN_C
        ["F4", 0x1E, 5, 0], //  24 KEY_EN_F4
        ["5", 0x1F, 5, 1], //  25 KEY_EN_5
        ["R", 0x1B, 5, 2], //  26 KEY_EN_R
        ["F", 0x1C, 5, 3], //  27 KEY_EN_F
        ["V", 0x1D, 5, 4], //  28 KEY_EN_V
        ["6", 0x24, 6, 1], //  29 KEY_EN_6
        ["T", 0x20, 6, 2], //  30 KEY_EN_T
        ["G", 0x21, 6, 3], //  31 KEY_EN_G
        ["B", 0x22, 6, 4], //  32 KEY_EN_B
        ["Space", 0x23, 6, 5], //  33 KEY_EN_SPACE
        ["F5", 0x28, 7, 0], //  34 KEY_EN_F5
        ["7", 0x29, 7, 1], //  35 KEY_EN_7
        ["Y", 0x25, 7, 2], //  36 KEY_EN_Y
        ["H", 0x26, 7, 3], //  37 KEY_EN_H
        ["N", 0x27, 7, 4], //  38 KEY_EN_N
        ["F6", 0x2F, 8, 0], //  39 KEY_EN_F6
        ["8", 0x30, 8, 1], //  40 KEY_EN_8
        ["U", 0x2A, 8, 2], //  41 KEY_EN_U
        ["J", 0x2B, 8, 3], //  42 KEY_EN_J
        ["M", 0x2C, 8, 4], //  43 KEY_EN_M
        ["F7", 0x35, 9, 0], //  44 KEY_EN_F7
        ["9", 0x36, 9, 1], //  45 KEY_EN_9
        ["I", 0x31, 9, 2], //  46 KEY_EN_I
        ["K", 0x32, 9, 3], //  47 KEY_EN_K
        [",", 0x33, 9, 4], //  48 KEY_EN_COMMA
        ["F8", 0x3B, 10, 0], //  49 KEY_EN_F8
        ["0", 0x3C, 10, 1], //  50 KEY_EN_0
        ["O", 0x37, 10, 2], //  51 KEY_EN_O
        ["L", 0x38, 10, 3], //  52 KEY_EN_L
        [".", 0x39, 10, 4], //  53 KEY_EN_PERIOD
        ["Right Alt", 0x3A, 10, 5], //  54 KEY_EN_RIGHT_ALT
        ["F9", 0x41, 11, 0], //  55 KEY_EN_F9
        ["-", 0x42, 11, 1], //  56 KEY_EN_MINUS
        ["P", 0x3D, 11, 2], //  57 KEY_EN_P
        [";", 0x3E, 11, 3], //  58 KEY_EN_SEMICOLON
        ["/", 0x3F, 11, 4], //  59 KEY_EN_FORWARD_SLASH
        ["Fn", 0x40, 11, 5], //  60 KEY_EN_RIGHT_FUNCTION
        ["F10", 0x47, 12, 0], //  61 KEY_EN_F10
        ["=", 0x48, 12, 1], //  62 KEY_EN_EQUALS
        ["[", 0x43, 12, 2], //  63 KEY_EN_LEFT_BRACKET
        ["'", 0x44, 12, 3], //  64 KEY_EN_QUOTE
        ["Menu", 0x46, 12, 5], //  65 KEY_EN_MENU
        ["F11", 0x4D, 13, 0], //  66 KEY_EN_F11
        ["Backspace", 0x50, 13, 1], //  67 KEY_EN_BACKSPACE
        ["]", 0x49, 13, 2], //  68 KEY_EN_RIGHT_BRACKET
        ["Right Shift", 0x4B, 13, 4], //  69 KEY_EN_RIGHT_SHIFT
        ["F12", 0x4F, 14, 0], //  70 KEY_EN_F12
        ["\\", 0x51, 14, 2], //  71 KEY_EN_ANSI_BACK_SLASH
        ["Enter", 0x52, 14, 3], //  72 KEY_EN_ANSI_ENTER
        ["Right Ctrl", 0x4C, 14, 5], //  73 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 0x53, 16, 0], //  74 KEY_EN_PRINT_SCREEN
        ["Insert", 0x54, 16, 1], //  75 KEY_EN_INSERT
        ["Del", 0x55, 16, 2], //  76 KEY_EN_DELETE
        ["Left Arrow", 0x56, 16, 5], //  77 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 0x57, 17, 0], //  78 KEY_EN_SCROLL_LOCK
        ["Home", 0x58, 17, 1], //  79 KEY_EN_HOME
        ["End", 0x59, 17, 2], //  80 KEY_EN_END
        ["Up Arrow", 0x5A, 17, 4], //  81 KEY_EN_UP_ARROW
        ["Down Arrow", 0x5B, 17, 5], //  82 KEY_EN_DOWN_ARROW
        ["Pause Break", 0x5C, 18, 0], //  83 KEY_EN_PAUSE_BREAK
        ["Page Up", 0x5D, 18, 1], //  84 KEY_EN_PAGE_UP
        ["Page Down", 0x5E, 18, 2], //  85 KEY_EN_PAGE_DOWN
        ["Right Arrow", 0x5F, 18, 5], //  86 KEY_EN_RIGHT_ARROW
        ["NumLock", 0x61, 20, 1], //  87 KEY_EN_NUMPAD_LOCK
        ["Num 7", 0x62, 20, 2], //  88 KEY_EN_NUMPAD_7
        ["Num 4", 0x63, 20, 3], //  89 KEY_EN_NUMPAD_4
        ["Num 1", 0x64, 20, 4], //  90 KEY_EN_NUMPAD_1
        ["Num 0", 0x65, 20, 5], //  91 KEY_EN_NUMPAD_0
        ["Num /", 0x67, 21, 1], //  92 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 0x68, 21, 2], //  93 KEY_EN_NUMPAD_8
        ["Num 5", 0x69, 21, 3], //  94 KEY_EN_NUMPAD_5
        ["Num 2", 0x6A, 21, 4], //  95 KEY_EN_NUMPAD_2
        ["Num *", 0x6C, 22, 1], //  96 KEY_EN_NUMPAD_TIMES
        ["Num 9", 0x6D, 22, 2], //  97 KEY_EN_NUMPAD_9
        ["Num 6", 0x6E, 22, 3], //  98 KEY_EN_NUMPAD_6
        ["Num 3", 0x6F, 22, 4], //  99 KEY_EN_NUMPAD_3
        ["Num .", 0x70, 22, 5], // 100 KEY_EN_NUMPAD_PERIOD
        ["Num -", 0x72, 23, 1], // 101 KEY_EN_NUMPAD_MINUS
        ["Num +", 0x73, 23, 2], // 102 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 0x75, 23, 4], // 103 KEY_EN_NUMPAD_ENTER
      ]),
    }),
  }),
  RoccatVulcanIILayouts: Object.freeze({
    UK: Object.freeze({
      width: 24,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0x02, 0, 0], //   0 KEY_EN_ESCAPE
        ["`", 0x03, 0, 1], //   1 KEY_EN_BACK_TICK
        ["Tab", 0x04, 0, 2], //   2 KEY_EN_TAB
        ["Caps Lock", 0x05, 0, 3], //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0x00, 0, 4], //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0x01, 0, 5], //   5 KEY_EN_LEFT_CONTROL
        ["1", 0x08, 1, 1], //   6 KEY_EN_1
        ["ISO_<", 0x06, 1, 4], //   7 KEY_EN_ISO_BACK_SLASH
        ["Left Win", 0x07, 1, 5], //   8 KEY_EN_LEFT_WINDOWS
        ["F1", 0x0D, 2, 0], //   9 KEY_EN_F1
        ["2", 0x0E, 2, 1], //  10 KEY_EN_2
        ["Q", 0x09, 2, 2], //  11 KEY_EN_Q
        ["A", 0x0A, 2, 3], //  12 KEY_EN_A
        ["Z", 0x0B, 2, 4], //  13 KEY_EN_Z
        ["Left Alt", 0x0C, 2, 5], //  14 KEY_EN_LEFT_ALT
        ["F2", 0x14, 3, 0], //  15 KEY_EN_F2
        ["3", 0x15, 3, 1], //  16 KEY_EN_3
        ["W", 0x0F, 3, 2], //  17 KEY_EN_W
        ["S", 0x10, 3, 3], //  18 KEY_EN_S
        ["X", 0x11, 3, 4], //  19 KEY_EN_X
        ["F3", 0x19, 4, 0], //  20 KEY_EN_F3
        ["4", 0x1A, 4, 1], //  21 KEY_EN_4
        ["E", 0x16, 4, 2], //  22 KEY_EN_E
        ["D", 0x17, 4, 3], //  23 KEY_EN_D
        ["C", 0x18, 4, 4], //  24 KEY_EN_C
        ["F4", 0x1E, 5, 0], //  25 KEY_EN_F4
        ["5", 0x1F, 5, 1], //  26 KEY_EN_5
        ["R", 0x1B, 5, 2], //  27 KEY_EN_R
        ["F", 0x1C, 5, 3], //  28 KEY_EN_F
        ["V", 0x1D, 5, 4], //  29 KEY_EN_V
        ["6", 0x24, 6, 1], //  30 KEY_EN_6
        ["T", 0x20, 6, 2], //  31 KEY_EN_T
        ["G", 0x21, 6, 3], //  32 KEY_EN_G
        ["B", 0x22, 6, 4], //  33 KEY_EN_B
        ["Space", 0x23, 6, 5], //  34 KEY_EN_SPACE
        ["F5", 0x28, 7, 0], //  35 KEY_EN_F5
        ["7", 0x29, 7, 1], //  36 KEY_EN_7
        ["Y", 0x25, 7, 2], //  37 KEY_EN_Y
        ["H", 0x26, 7, 3], //  38 KEY_EN_H
        ["N", 0x27, 7, 4], //  39 KEY_EN_N
        ["F6", 0x2F, 8, 0], //  40 KEY_EN_F6
        ["8", 0x30, 8, 1], //  41 KEY_EN_8
        ["U", 0x2A, 8, 2], //  42 KEY_EN_U
        ["J", 0x2B, 8, 3], //  43 KEY_EN_J
        ["M", 0x2C, 8, 4], //  44 KEY_EN_M
        ["F7", 0x35, 9, 0], //  45 KEY_EN_F7
        ["9", 0x36, 9, 1], //  46 KEY_EN_9
        ["I", 0x31, 9, 2], //  47 KEY_EN_I
        ["K", 0x32, 9, 3], //  48 KEY_EN_K
        [",", 0x33, 9, 4], //  49 KEY_EN_COMMA
        ["F8", 0x3B, 10, 0], //  50 KEY_EN_F8
        ["0", 0x3C, 10, 1], //  51 KEY_EN_0
        ["O", 0x37, 10, 2], //  52 KEY_EN_O
        ["L", 0x38, 10, 3], //  53 KEY_EN_L
        [".", 0x39, 10, 4], //  54 KEY_EN_PERIOD
        ["Right Alt", 0x3A, 10, 5], //  55 KEY_EN_RIGHT_ALT
        ["F9", 0x41, 11, 0], //  56 KEY_EN_F9
        ["-", 0x42, 11, 1], //  57 KEY_EN_MINUS
        ["P", 0x3D, 11, 2], //  58 KEY_EN_P
        [";", 0x3E, 11, 3], //  59 KEY_EN_SEMICOLON
        ["/", 0x3F, 11, 4], //  60 KEY_EN_FORWARD_SLASH
        ["Fn", 0x40, 11, 5], //  61 KEY_EN_RIGHT_FUNCTION
        ["F10", 0x47, 12, 0], //  62 KEY_EN_F10
        ["=", 0x48, 12, 1], //  63 KEY_EN_EQUALS
        ["[", 0x43, 12, 2], //  64 KEY_EN_LEFT_BRACKET
        ["'", 0x44, 12, 3], //  65 KEY_EN_QUOTE
        ["Menu", 0x46, 12, 5], //  66 KEY_EN_MENU
        ["F11", 0x4D, 13, 0], //  67 KEY_EN_F11
        ["Backspace", 0x50, 13, 1], //  68 KEY_EN_BACKSPACE
        ["]", 0x49, 13, 2], //  69 KEY_EN_RIGHT_BRACKET
        ["ISO_#", 0x4A, 13, 3], //  70 KEY_EN_POUND
        ["Right Shift", 0x4B, 13, 4], //  71 KEY_EN_RIGHT_SHIFT
        ["F12", 0x4F, 14, 0], //  72 KEY_EN_F12
        ["Enter", 0x52, 14, 3], //  73 KEY_EN_ISO_ENTER
        ["Right Ctrl", 0x4C, 14, 5], //  74 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 0x53, 16, 0], //  75 KEY_EN_PRINT_SCREEN
        ["Insert", 0x54, 16, 1], //  76 KEY_EN_INSERT
        ["Del", 0x55, 16, 2], //  77 KEY_EN_DELETE
        ["Left Arrow", 0x56, 16, 5], //  78 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 0x57, 17, 0], //  79 KEY_EN_SCROLL_LOCK
        ["Home", 0x58, 17, 1], //  80 KEY_EN_HOME
        ["End", 0x59, 17, 2], //  81 KEY_EN_END
        ["Up Arrow", 0x5A, 17, 4], //  82 KEY_EN_UP_ARROW
        ["Down Arrow", 0x5B, 17, 5], //  83 KEY_EN_DOWN_ARROW
        ["Pause Break", 0x5C, 18, 0], //  84 KEY_EN_PAUSE_BREAK
        ["Page Up", 0x5D, 18, 1], //  85 KEY_EN_PAGE_UP
        ["Page Down", 0x5E, 18, 2], //  86 KEY_EN_PAGE_DOWN
        ["Right Arrow", 0x5F, 18, 5], //  87 KEY_EN_RIGHT_ARROW
        ["NumLock", 0x61, 20, 1], //  88 KEY_EN_NUMPAD_LOCK
        ["Num 7", 0x62, 20, 2], //  89 KEY_EN_NUMPAD_7
        ["Num 4", 0x63, 20, 3], //  90 KEY_EN_NUMPAD_4
        ["Num 1", 0x64, 20, 4], //  91 KEY_EN_NUMPAD_1
        ["Num 0", 0x65, 20, 5], //  92 KEY_EN_NUMPAD_0
        ["Num /", 0x67, 21, 1], //  93 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 0x68, 21, 2], //  94 KEY_EN_NUMPAD_8
        ["Num 5", 0x69, 21, 3], //  95 KEY_EN_NUMPAD_5
        ["Num 2", 0x6A, 21, 4], //  96 KEY_EN_NUMPAD_2
        ["Num *", 0x6C, 22, 1], //  97 KEY_EN_NUMPAD_TIMES
        ["Num 9", 0x6D, 22, 2], //  98 KEY_EN_NUMPAD_9
        ["Num 6", 0x6E, 22, 3], //  99 KEY_EN_NUMPAD_6
        ["Num 3", 0x6F, 22, 4], // 100 KEY_EN_NUMPAD_3
        ["Num .", 0x70, 22, 5], // 101 KEY_EN_NUMPAD_PERIOD
        ["Num -", 0x72, 23, 1], // 102 KEY_EN_NUMPAD_MINUS
        ["Num +", 0x73, 23, 2], // 103 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 0x75, 23, 4], // 104 KEY_EN_NUMPAD_ENTER
        ["MediaPreviousTrack", 0x76, 20, 0], // 105 KEY_EN_MEDIA_PREVIOUS
        ["MediaPlayPause", 0x78, 21, 0], // 106 KEY_EN_MEDIA_PLAY_PAUSE
        ["MediaNextTrack", 0x79, 22, 0], // 107 KEY_EN_MEDIA_NEXT
      ]),
    }),
  }),
  RoccatVulcanIIMaxLayouts: Object.freeze({
    US: Object.freeze({
      width: 24,
      height: 13,
      leds: Object.freeze([
        ["Esc", 0x02, 0, 0], //   0 KEY_EN_ESCAPE
        ["`", 0x03, 0, 1], //   1 KEY_EN_BACK_TICK
        ["Tab", 0x04, 0, 2], //   2 KEY_EN_TAB
        ["Caps Lock", 0x05, 0, 3], //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0x00, 0, 4], //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0x01, 0, 5], //   5 KEY_EN_LEFT_CONTROL
        ["1", 0x08, 1, 1], //   6 KEY_EN_1
        ["LED 0x06", 0x06, null, null], //   7 KEY_EN_UNUSED
        ["Left Win", 0x07, 1, 5], //   8 KEY_EN_LEFT_WINDOWS
        ["F1", 0x0D, 2, 0], //   9 KEY_EN_F1
        ["2", 0x0E, 2, 1], //  10 KEY_EN_2
        ["Q", 0x09, 2, 2], //  11 KEY_EN_Q
        ["A", 0x0A, 2, 3], //  12 KEY_EN_A
        ["Z", 0x0B, 2, 4], //  13 KEY_EN_Z
        ["Left Alt", 0x0C, 2, 5], //  14 KEY_EN_LEFT_ALT
        ["F2", 0x14, 3, 0], //  15 KEY_EN_F2
        ["3", 0x15, 3, 1], //  16 KEY_EN_3
        ["W", 0x0F, 3, 2], //  17 KEY_EN_W
        ["S", 0x10, 3, 3], //  18 KEY_EN_S
        ["X", 0x11, 3, 4], //  19 KEY_EN_X
        ["F3", 0x19, 4, 0], //  20 KEY_EN_F3
        ["4", 0x1A, 4, 1], //  21 KEY_EN_4
        ["E", 0x16, 4, 2], //  22 KEY_EN_E
        ["D", 0x17, 4, 3], //  23 KEY_EN_D
        ["C", 0x18, 4, 4], //  24 KEY_EN_C
        ["F4", 0x1F, 5, 0], //  25 KEY_EN_F4
        ["5", 0x1E, 5, 1], //  26 KEY_EN_5
        ["R", 0x1B, 5, 2], //  27 KEY_EN_R
        ["F", 0x1C, 5, 3], //  28 KEY_EN_F
        ["V", 0x21, 5, 4], //  29 KEY_EN_V
        ["6", 0x24, 6, 1], //  30 KEY_EN_6
        ["T", 0x20, 6, 2], //  31 KEY_EN_T
        ["G", 0x1D, 6, 3], //  32 KEY_EN_G
        ["B", 0x22, 6, 4], //  33 KEY_EN_B
        ["Space", 0x28, 6, 5], //  34 KEY_EN_SPACE
        ["F5", 0x23, 7, 0], //  35 KEY_EN_F5
        ["7", 0x29, 7, 1], //  36 KEY_EN_7
        ["Y", 0x25, 7, 2], //  37 KEY_EN_Y
        ["H", 0x26, 7, 3], //  38 KEY_EN_H
        ["N", 0x27, 7, 4], //  39 KEY_EN_N
        ["F6", 0x2F, 8, 0], //  40 KEY_EN_F6
        ["8", 0x30, 8, 1], //  41 KEY_EN_8
        ["U", 0x3D, 8, 2], //  42 KEY_EN_U
        ["J", 0x2B, 8, 3], //  43 KEY_EN_J
        ["M", 0x2C, 8, 4], //  44 KEY_EN_M
        ["F7", 0x35, 9, 0], //  45 KEY_EN_F7
        ["9", 0x36, 9, 1], //  46 KEY_EN_9
        ["I", 0x31, 9, 2], //  47 KEY_EN_I
        ["K", 0x32, 9, 3], //  48 KEY_EN_K
        [",", 0x33, 9, 4], //  49 KEY_EN_COMMA
        ["F8", 0x3B, 10, 0], //  50 KEY_EN_F8
        ["0", 0x3C, 10, 1], //  51 KEY_EN_0
        ["O", 0x37, 10, 2], //  52 KEY_EN_O
        ["L", 0x38, 10, 3], //  53 KEY_EN_L
        [".", 0x39, 10, 4], //  54 KEY_EN_PERIOD
        ["Right Alt", 0x3A, 10, 5], //  55 KEY_EN_RIGHT_ALT
        ["F9", 0x41, 11, 0], //  56 KEY_EN_F9
        ["-", 0x42, 11, 1], //  57 KEY_EN_MINUS
        ["P", 0x2A, 11, 2], //  58 KEY_EN_P
        [";", 0x3E, 11, 3], //  59 KEY_EN_SEMICOLON
        ["/", 0x3F, 11, 4], //  60 KEY_EN_FORWARD_SLASH
        ["Fn", 0x40, 11, 5], //  61 KEY_EN_RIGHT_FUNCTION
        ["F10", 0x47, 12, 0], //  62 KEY_EN_F10
        ["=", 0x48, 12, 1], //  63 KEY_EN_EQUALS
        ["[", 0x43, 12, 2], //  64 KEY_EN_LEFT_BRACKET
        ["'", 0x44, 12, 3], //  65 KEY_EN_QUOTE
        ["Menu", 0x46, 12, 5], //  66 KEY_EN_MENU
        ["F11", 0x4D, 13, 0], //  67 KEY_EN_F11
        ["Backspace", 0x50, 13, 1], //  68 KEY_EN_BACKSPACE
        ["]", 0x49, 13, 2], //  69 KEY_EN_RIGHT_BRACKET
        ["\\", 0x51, 14, 2], //  70 KEY_EN_ANSI_BACK_SLASH
        ["Right Shift", 0x4B, 12, 4], //  71 KEY_EN_RIGHT_SHIFT
        ["F12", 0x4F, 14, 0], //  72 KEY_EN_F12
        ["Enter", 0x52, 13, 3], //  73 KEY_EN_ANSI_ENTER
        ["Right Ctrl", 0x4C, 13, 5], //  74 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 0x53, 16, 0], //  75 KEY_EN_PRINT_SCREEN
        ["Insert", 0x54, 16, 1], //  76 KEY_EN_INSERT
        ["Del", 0x55, 16, 2], //  77 KEY_EN_DELETE
        ["Left Arrow", 0x56, 16, 5], //  78 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 0x57, 17, 0], //  79 KEY_EN_SCROLL_LOCK
        ["Home", 0x58, 17, 1], //  80 KEY_EN_HOME
        ["End", 0x59, 17, 2], //  81 KEY_EN_END
        ["Up Arrow", 0x5A, 17, 4], //  82 KEY_EN_UP_ARROW
        ["Down Arrow", 0x5B, 17, 5], //  83 KEY_EN_DOWN_ARROW
        ["Pause Break", 0x5C, 18, 0], //  84 KEY_EN_PAUSE_BREAK
        ["Page Up", 0x5D, 18, 1], //  85 KEY_EN_PAGE_UP
        ["Page Down", 0x5E, 18, 2], //  86 KEY_EN_PAGE_DOWN
        ["Right Arrow", 0x5F, 18, 5], //  87 KEY_EN_RIGHT_ARROW
        ["NumLock", 0x60, 20, 1], //  88 KEY_EN_NUMPAD_LOCK
        ["Num 7", 0x61, 20, 2], //  89 KEY_EN_NUMPAD_7
        ["Num 4", 0x62, 20, 3], //  90 KEY_EN_NUMPAD_4
        ["Num 1", 0x6C, 20, 4], //  91 KEY_EN_NUMPAD_1
        ["Num 0", 0x64, 20, 5], //  92 KEY_EN_NUMPAD_0
        ["Num /", 0x65, 21, 1], //  93 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 0x66, 21, 2], //  94 KEY_EN_NUMPAD_8
        ["Num 5", 0x67, 21, 3], //  95 KEY_EN_NUMPAD_5
        ["Num 2", 0x68, 21, 4], //  96 KEY_EN_NUMPAD_2
        ["Num *", 0x69, 22, 1], //  97 KEY_EN_NUMPAD_TIMES
        ["Num 9", 0x6A, 22, 2], //  98 KEY_EN_NUMPAD_9
        ["Num 6", 0x6B, 22, 3], //  99 KEY_EN_NUMPAD_6
        ["Num 3", 0x63, 22, 4], // 100 KEY_EN_NUMPAD_3
        ["Num .", 0x6D, 22, 5], // 101 KEY_EN_NUMPAD_PERIOD
        ["Num -", 0x6E, 23, 1], // 102 KEY_EN_NUMPAD_MINUS
        ["Num +", 0x6F, 23, 2], // 103 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 0x71, 23, 4], // 104 KEY_EN_NUMPAD_ENTER
        ["MediaPreviousTrack", 0x72, 20, 0], // 105 KEY_EN_MEDIA_PREVIOUS
        ["MediaPlayPause", 0x73, 21, 0], // 106 KEY_EN_MEDIA_PLAY_PAUSE
        ["MediaNextTrack", 0x74, 22, 0], // 107 KEY_EN_MEDIA_NEXT
        ["F1 LED 2", 0x89, 0, 6], // 108 "Key: F1 LED 2"
        ["F2 LED 2", 0x7B, 1, 6], // 109 "Key: F2 LED 2"
        ["F3 LED 2", 0x81, 2, 6], // 110 "Key: F3 LED 2"
        ["F4 LED 2", 0x87, 3, 6], // 111 "Key: F4 LED 2"
        ["F5 LED 2", 0x8D, 4, 6], // 112 "Key: F5 LED 2"
        ["F6 LED 2", 0x76, 5, 6], // 113 "Key: F6 LED 2"
        ["F7 LED 2", 0x7C, 6, 6], // 114 "Key: F7 LED 2"
        ["F8 LED 2", 0x82, 7, 6], // 115 "Key: F8 LED 2"
        ["F9 LED 2", 0x88, 8, 6], // 116 "Key: F9 LED 2"
        ["F10 LED 2", 0x8E, 9, 6], // 117 "Key: F10 LED 2"
        ["F11 LED 2", 0x7A, 10, 6], // 118 "Key: F11 LED 2"
        ["F12 LED 2", 0x80, 11, 6], // 119 "Key: F12 LED 2"
        ["Print Screen LED 2", 0x86, 12, 6], // 120 "Key: Print Screen LED 2"
        ["Scroll Lock LED 2", 0x8C, 13, 6], // 121 "Key: Scroll Lock LED 2"
        ["Pause/Break LED 2", 0x92, 14, 6], // 122 "Key: Pause/Break LED 2"
        ["Insert LED 2", 0x75, 0, 7], // 123 "Key: Insert LED 2"
        ["Home LED 2", 0x7E, 1, 7], // 124 "Key: Home LED 2"
        ["Page Up LED 2", 0x84, 2, 7], // 125 "Key: Page Up LED 2"
        ["Num Lock LED 2", 0x8B, 0, 8], // 126 "Key: Num Lock LED 2"
        ["Delete LED 2", 0x78, 0, 9], // 127 "Key: Delete LED 2"
        ["End LED 2", 0x90, 1, 9], // 128 "Key: End LED 2"
        ["Page Down LED 2", 0x8A, 2, 9], // 129 "Key: Page Down LED 2"
        ["Caps Lock LED 2", 0x7D, 0, 10], // 130 "Key: Caps Lock LED 2"
        ["Left Windows LED 2", 0x83, 0, 11], // 131 "Key: Left Windows LED 2"
        ["Palm Rest LED 1", 0x96, 0, 12], // 132 "Palm Rest LED 1"
        ["Palm Rest LED 2", 0xAD, 1, 12], // 133 "Palm Rest LED 2"
        ["Palm Rest LED 3", 0xA7, 2, 12], // 134 "Palm Rest LED 3"
        ["Palm Rest LED 4", 0xA1, 3, 12], // 135 "Palm Rest LED 4"
        ["Palm Rest LED 5", 0x9B, 4, 12], // 136 "Palm Rest LED 5"
        ["Palm Rest LED 6", 0x95, 5, 12], // 137 "Palm Rest LED 6"
        ["Palm Rest LED 7", 0xAC, 6, 12], // 138 "Palm Rest LED 7"
        ["Palm Rest LED 8", 0xA6, 7, 12], // 139 "Palm Rest LED 8"
        ["Palm Rest LED 9", 0xA0, 8, 12], // 140 "Palm Rest LED 9"
        ["Palm Rest LED 10", 0x9A, 9, 12], // 141 "Palm Rest LED 10"
        ["Palm Rest LED 11", 0x94, 10, 12], // 142 "Palm Rest LED 11"
        ["Palm Rest LED 12", 0xAB, 11, 12], // 143 "Palm Rest LED 12"
        ["Palm Rest LED 13", 0xA5, 12, 12], // 144 "Palm Rest LED 13"
        ["Palm Rest LED 14", 0x9F, 13, 12], // 145 "Palm Rest LED 14"
        ["Palm Rest LED 15", 0x99, 14, 12], // 146 "Palm Rest LED 15"
        ["Palm Rest LED 16", 0x93, 15, 12], // 147 "Palm Rest LED 16"
      ]),
    }),
    JP: Object.freeze({
      width: 24,
      height: 13,
      leds: Object.freeze([
        ["Esc", 0x02, 0, 0], //   0 KEY_EN_ESCAPE
        ["E/J", 0x03, 0, 1], //   1 KEY_JP_EJ
        ["Tab", 0x04, 0, 2], //   2 KEY_EN_TAB
        ["Caps Lock", 0x05, 0, 3], //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0x00, 0, 4], //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0x01, 0, 5], //   5 KEY_EN_LEFT_CONTROL
        ["1", 0x08, 1, 1], //   6 KEY_EN_1
        ["F1", 0x0D, 2, 0], //   7 KEY_EN_F1
        ["2", 0x0E, 2, 1], //   8 KEY_EN_2
        ["Q", 0x09, 2, 2], //   9 KEY_EN_Q
        ["A", 0x0A, 2, 3], //  10 KEY_EN_A
        ["Z", 0x0B, 2, 4], //  11 KEY_EN_Z
        ["Left Win", 0x07, 2, 5], //  12 KEY_EN_LEFT_WINDOWS
        ["F2", 0x14, 3, 0], //  13 KEY_EN_F2
        ["3", 0x15, 3, 1], //  14 KEY_EN_3
        ["W", 0x0F, 3, 2], //  15 KEY_EN_W
        ["S", 0x10, 3, 3], //  16 KEY_EN_S
        ["X", 0x11, 3, 4], //  17 KEY_EN_X
        ["Left Alt", 0x0C, 3, 5], //  18 KEY_EN_LEFT_ALT
        ["F3", 0x19, 4, 0], //  19 KEY_EN_F3
        ["4", 0x1A, 4, 1], //  20 KEY_EN_4
        ["E", 0x16, 4, 2], //  21 KEY_EN_E
        ["D", 0x17, 4, 3], //  22 KEY_EN_D
        ["C", 0x18, 4, 4], //  23 KEY_EN_C
        ["無変換", 0x12, 4, 5], //  24 KEY_JP_MUHENKAN
        ["F4", 0x1F, 5, 0], //  25 KEY_EN_F4
        ["5", 0x1E, 5, 1], //  26 KEY_EN_5
        ["R", 0x1B, 5, 2], //  27 KEY_EN_R
        ["F", 0x1C, 5, 3], //  28 KEY_EN_F
        ["V", 0x21, 5, 4], //  29 KEY_EN_V
        ["6", 0x24, 6, 1], //  30 KEY_EN_6
        ["T", 0x20, 6, 2], //  31 KEY_EN_T
        ["G", 0x1D, 6, 3], //  32 KEY_EN_G
        ["B", 0x22, 6, 4], //  33 KEY_EN_B
        ["Space", 0x28, 6, 5], //  34 KEY_EN_SPACE
        ["F5", 0x23, 7, 0], //  35 KEY_EN_F5
        ["7", 0x29, 7, 1], //  36 KEY_EN_7
        ["Y", 0x25, 7, 2], //  37 KEY_EN_Y
        ["H", 0x26, 7, 3], //  38 KEY_EN_H
        ["N", 0x27, 7, 4], //  39 KEY_EN_N
        ["F6", 0x2F, 8, 0], //  40 KEY_EN_F6
        ["8", 0x30, 8, 1], //  41 KEY_EN_8
        ["U", 0x3D, 8, 2], //  42 KEY_EN_U
        ["J", 0x2B, 8, 3], //  43 KEY_EN_J
        ["M", 0x2C, 8, 4], //  44 KEY_EN_M
        ["F7", 0x35, 9, 0], //  45 KEY_EN_F7
        ["9", 0x36, 9, 1], //  46 KEY_EN_9
        ["I", 0x31, 9, 2], //  47 KEY_EN_I
        ["K", 0x32, 9, 3], //  48 KEY_EN_K
        [",", 0x33, 9, 4], //  49 KEY_EN_COMMA
        ["変換", 0x2D, 9, 5], //  50 KEY_JP_HENKAN
        ["F8", 0x3B, 10, 0], //  51 KEY_EN_F8
        ["0", 0x3C, 10, 1], //  52 KEY_EN_0
        ["O", 0x37, 10, 2], //  53 KEY_EN_O
        ["L", 0x38, 10, 3], //  54 KEY_EN_L
        [".", 0x39, 10, 4], //  55 KEY_EN_PERIOD
        ["かな", 0x34, 10, 5], //  56 KEY_JP_KANA
        ["F9", 0x41, 11, 0], //  57 KEY_EN_F9
        ["-", 0x42, 11, 1], //  58 KEY_EN_MINUS
        ["P", 0x2A, 11, 2], //  59 KEY_EN_P
        [";", 0x3E, 11, 3], //  60 KEY_EN_SEMICOLON
        ["/", 0x3F, 11, 4], //  61 KEY_EN_FORWARD_SLASH
        ["Fn", 0x40, 11, 5], //  62 KEY_EN_RIGHT_FUNCTION
        ["F10", 0x47, 12, 0], //  63 KEY_EN_F10
        ["^", 0x48, 12, 1], //  64 KEY_JP_CHEVRON
        ["@", 0x43, 12, 2], //  65 KEY_JP_AT
        [":", 0x44, 12, 3], //  66 KEY_JP_COLON
        ["\\", 0x45, 12, 4], //  67 KEY_EN_BACK_SLASH
        ["Menu", 0x46, 12, 5], //  68 KEY_EN_MENU
        ["F11", 0x4D, 13, 0], //  69 KEY_EN_F11
        ["¥", 0x4E, 13, 1], //  70 KEY_JP_YEN
        ["[", 0x49, 13, 2], //  71 KEY_EN_LEFT_BRACKET
        ["]", 0x4A, 13, 3], //  72 KEY_EN_RIGHT_BRACKET
        ["F12", 0x4F, 14, 0], //  73 KEY_EN_F12
        ["Backspace", 0x50, 14, 1], //  74 KEY_EN_BACKSPACE
        ["Enter", 0x52, 14, 2], //  75 KEY_EN_ISO_ENTER
        ["Right Shift", 0x4B, 14, 4], //  76 KEY_EN_RIGHT_SHIFT
        ["Right Ctrl", 0x4C, 14, 5], //  77 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 0x53, 16, 0], //  78 KEY_EN_PRINT_SCREEN
        ["Insert", 0x54, 16, 1], //  79 KEY_EN_INSERT
        ["Del", 0x55, 16, 2], //  80 KEY_EN_DELETE
        ["Left Arrow", 0x56, 16, 5], //  81 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 0x57, 17, 0], //  82 KEY_EN_SCROLL_LOCK
        ["Home", 0x58, 17, 1], //  83 KEY_EN_HOME
        ["End", 0x59, 17, 2], //  84 KEY_EN_END
        ["Up Arrow", 0x5A, 17, 4], //  85 KEY_EN_UP_ARROW
        ["Down Arrow", 0x5B, 17, 5], //  86 KEY_EN_DOWN_ARROW
        ["Pause Break", 0x5C, 18, 0], //  87 KEY_EN_PAUSE_BREAK
        ["Page Up", 0x5D, 18, 1], //  88 KEY_EN_PAGE_UP
        ["Page Down", 0x5E, 18, 2], //  89 KEY_EN_PAGE_DOWN
        ["Right Arrow", 0x5F, 18, 5], //  90 KEY_EN_RIGHT_ARROW
        ["NumLock", 0x60, 20, 1], //  91 KEY_EN_NUMPAD_LOCK
        ["Num 7", 0x61, 20, 2], //  92 KEY_EN_NUMPAD_7
        ["Num 4", 0x62, 20, 3], //  93 KEY_EN_NUMPAD_4
        ["Num 1", 0x6C, 20, 4], //  94 KEY_EN_NUMPAD_1
        ["Num 0", 0x64, 20, 5], //  95 KEY_EN_NUMPAD_0
        ["Num /", 0x65, 21, 1], //  96 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 0x66, 21, 2], //  97 KEY_EN_NUMPAD_8
        ["Num 5", 0x67, 21, 3], //  98 KEY_EN_NUMPAD_5
        ["Num 2", 0x68, 21, 4], //  99 KEY_EN_NUMPAD_2
        ["Num *", 0x69, 22, 1], // 100 KEY_EN_NUMPAD_TIMES
        ["Num 9", 0x6A, 22, 2], // 101 KEY_EN_NUMPAD_9
        ["Num 6", 0x6B, 22, 3], // 102 KEY_EN_NUMPAD_6
        ["Num 3", 0x63, 22, 4], // 103 KEY_EN_NUMPAD_3
        ["Num .", 0x6D, 22, 5], // 104 KEY_EN_NUMPAD_PERIOD
        ["Num -", 0x6E, 23, 1], // 105 KEY_EN_NUMPAD_MINUS
        ["Num +", 0x6F, 23, 2], // 106 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 0x71, 23, 4], // 107 KEY_EN_NUMPAD_ENTER
        ["MediaPreviousTrack", 0x72, 20, 0], // 108 KEY_EN_MEDIA_PREVIOUS
        ["MediaPlayPause", 0x73, 21, 0], // 109 KEY_EN_MEDIA_PLAY_PAUSE
        ["MediaNextTrack", 0x74, 22, 0], // 110 KEY_EN_MEDIA_NEXT
        ["F1 LED 2", 0x89, 0, 6], // 111 "Key: F1 LED 2"
        ["F2 LED 2", 0x7B, 1, 6], // 112 "Key: F2 LED 2"
        ["F3 LED 2", 0x81, 2, 6], // 113 "Key: F3 LED 2"
        ["F4 LED 2", 0x87, 3, 6], // 114 "Key: F4 LED 2"
        ["F5 LED 2", 0x8D, 4, 6], // 115 "Key: F5 LED 2"
        ["F6 LED 2", 0x76, 5, 6], // 116 "Key: F6 LED 2"
        ["F7 LED 2", 0x7C, 6, 6], // 117 "Key: F7 LED 2"
        ["F8 LED 2", 0x82, 7, 6], // 118 "Key: F8 LED 2"
        ["F9 LED 2", 0x88, 8, 6], // 119 "Key: F9 LED 2"
        ["F10 LED 2", 0x8E, 9, 6], // 120 "Key: F10 LED 2"
        ["F11 LED 2", 0x7A, 10, 6], // 121 "Key: F11 LED 2"
        ["F12 LED 2", 0x80, 11, 6], // 122 "Key: F12 LED 2"
        ["Print Screen LED 2", 0x86, 12, 6], // 123 "Key: Print Screen LED 2"
        ["Scroll Lock LED 2", 0x8C, 13, 6], // 124 "Key: Scroll Lock LED 2"
        ["Pause/Break LED 2", 0x92, 14, 6], // 125 "Key: Pause/Break LED 2"
        ["Insert LED 2", 0x75, 0, 7], // 126 "Key: Insert LED 2"
        ["Home LED 2", 0x7E, 1, 7], // 127 "Key: Home LED 2"
        ["Page Up LED 2", 0x84, 2, 7], // 128 "Key: Page Up LED 2"
        ["Num Lock LED 2", 0x8B, 0, 8], // 129 "Key: Num Lock LED 2"
        ["Delete LED 2", 0x78, 0, 9], // 130 "Key: Delete LED 2"
        ["End LED 2", 0x90, 1, 9], // 131 "Key: End LED 2"
        ["Page Down LED 2", 0x8A, 2, 9], // 132 "Key: Page Down LED 2"
        ["Caps Lock LED 2", 0x7D, 0, 10], // 133 "Key: Caps Lock LED 2"
        ["Left Windows LED 2", 0x83, 0, 11], // 134 "Key: Left Windows LED 2"
        ["Palm Rest LED 1", 0x96, 0, 12], // 135 "Palm Rest LED 1"
        ["Palm Rest LED 2", 0xAD, 1, 12], // 136 "Palm Rest LED 2"
        ["Palm Rest LED 3", 0xA7, 2, 12], // 137 "Palm Rest LED 3"
        ["Palm Rest LED 4", 0xA1, 3, 12], // 138 "Palm Rest LED 4"
        ["Palm Rest LED 5", 0x9B, 4, 12], // 139 "Palm Rest LED 5"
        ["Palm Rest LED 6", 0x95, 5, 12], // 140 "Palm Rest LED 6"
        ["Palm Rest LED 7", 0xAC, 6, 12], // 141 "Palm Rest LED 7"
        ["Palm Rest LED 8", 0xA6, 7, 12], // 142 "Palm Rest LED 8"
        ["Palm Rest LED 9", 0xA0, 8, 12], // 143 "Palm Rest LED 9"
        ["Palm Rest LED 10", 0x9A, 9, 12], // 144 "Palm Rest LED 10"
        ["Palm Rest LED 11", 0x94, 10, 12], // 145 "Palm Rest LED 11"
        ["Palm Rest LED 12", 0xAB, 11, 12], // 146 "Palm Rest LED 12"
        ["Palm Rest LED 13", 0xA5, 12, 12], // 147 "Palm Rest LED 13"
        ["Palm Rest LED 14", 0x9F, 13, 12], // 148 "Palm Rest LED 14"
        ["Palm Rest LED 15", 0x99, 14, 12], // 149 "Palm Rest LED 15"
        ["Palm Rest LED 16", 0x93, 15, 12], // 150 "Palm Rest LED 16"
      ]),
    }),
  }),
  RoccatVulcanTKLLayouts: Object.freeze({
    UK: Object.freeze({
      width: 19,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0x02, 0, 0], //   0 KEY_EN_ESCAPE
        ["`", 0x03, 0, 1], //   1 KEY_EN_BACK_TICK
        ["Tab", 0x04, 0, 2], //   2 KEY_EN_TAB
        ["Caps Lock", 0x05, 0, 3], //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0x00, 0, 4], //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0x01, 0, 5], //   5 KEY_EN_LEFT_CONTROL
        ["1", 0x08, 1, 1], //   6 KEY_EN_1
        ["ISO_<", 0x06, 1, 4], //   7 KEY_EN_ISO_BACK_SLASH
        ["Left Win", 0x07, 1, 5], //   8 KEY_EN_LEFT_WINDOWS
        ["F1", 0x0D, 2, 0], //   9 KEY_EN_F1
        ["2", 0x0E, 2, 1], //  10 KEY_EN_2
        ["Q", 0x09, 2, 2], //  11 KEY_EN_Q
        ["A", 0x0A, 2, 3], //  12 KEY_EN_A
        ["Z", 0x0B, 2, 4], //  13 KEY_EN_Z
        ["Left Alt", 0x0C, 2, 5], //  14 KEY_EN_LEFT_ALT
        ["F2", 0x14, 3, 0], //  15 KEY_EN_F2
        ["3", 0x15, 3, 1], //  16 KEY_EN_3
        ["W", 0x0F, 3, 2], //  17 KEY_EN_W
        ["S", 0x10, 3, 3], //  18 KEY_EN_S
        ["X", 0x11, 3, 4], //  19 KEY_EN_X
        ["F3", 0x19, 4, 0], //  20 KEY_EN_F3
        ["4", 0x1A, 4, 1], //  21 KEY_EN_4
        ["E", 0x16, 4, 2], //  22 KEY_EN_E
        ["D", 0x17, 4, 3], //  23 KEY_EN_D
        ["C", 0x18, 4, 4], //  24 KEY_EN_C
        ["F4", 0x1E, 5, 0], //  25 KEY_EN_F4
        ["5", 0x1F, 5, 1], //  26 KEY_EN_5
        ["R", 0x1B, 5, 2], //  27 KEY_EN_R
        ["F", 0x1C, 5, 3], //  28 KEY_EN_F
        ["V", 0x1D, 5, 4], //  29 KEY_EN_V
        ["6", 0x24, 6, 1], //  30 KEY_EN_6
        ["T", 0x20, 6, 2], //  31 KEY_EN_T
        ["G", 0x21, 6, 3], //  32 KEY_EN_G
        ["B", 0x22, 6, 4], //  33 KEY_EN_B
        ["Space", 0x23, 6, 5], //  34 KEY_EN_SPACE
        ["F5", 0x28, 7, 0], //  35 KEY_EN_F5
        ["7", 0x29, 7, 1], //  36 KEY_EN_7
        ["Y", 0x25, 7, 2], //  37 KEY_EN_Y
        ["H", 0x26, 7, 3], //  38 KEY_EN_H
        ["N", 0x27, 7, 4], //  39 KEY_EN_N
        ["F6", 0x2F, 8, 0], //  40 KEY_EN_F6
        ["8", 0x30, 8, 1], //  41 KEY_EN_8
        ["U", 0x2A, 8, 2], //  42 KEY_EN_U
        ["J", 0x2B, 8, 3], //  43 KEY_EN_J
        ["M", 0x2C, 8, 4], //  44 KEY_EN_M
        ["F7", 0x35, 9, 0], //  45 KEY_EN_F7
        ["9", 0x36, 9, 1], //  46 KEY_EN_9
        ["I", 0x31, 9, 2], //  47 KEY_EN_I
        ["K", 0x32, 9, 3], //  48 KEY_EN_K
        [",", 0x33, 9, 4], //  49 KEY_EN_COMMA
        ["F8", 0x3B, 10, 0], //  50 KEY_EN_F8
        ["0", 0x3C, 10, 1], //  51 KEY_EN_0
        ["O", 0x37, 10, 2], //  52 KEY_EN_O
        ["L", 0x38, 10, 3], //  53 KEY_EN_L
        [".", 0x39, 10, 4], //  54 KEY_EN_PERIOD
        ["Right Alt", 0x3A, 10, 5], //  55 KEY_EN_RIGHT_ALT
        ["F9", 0x41, 11, 0], //  56 KEY_EN_F9
        ["-", 0x42, 11, 1], //  57 KEY_EN_MINUS
        ["P", 0x3D, 11, 2], //  58 KEY_EN_P
        [";", 0x3E, 11, 3], //  59 KEY_EN_SEMICOLON
        ["/", 0x3F, 11, 4], //  60 KEY_EN_FORWARD_SLASH
        ["Fn", 0x40, 11, 5], //  61 KEY_EN_RIGHT_FUNCTION
        ["F10", 0x47, 12, 0], //  62 KEY_EN_F10
        ["=", 0x48, 12, 1], //  63 KEY_EN_EQUALS
        ["[", 0x43, 12, 2], //  64 KEY_EN_LEFT_BRACKET
        ["'", 0x44, 12, 3], //  65 KEY_EN_QUOTE
        ["Menu", 0x46, 12, 5], //  66 KEY_EN_MENU
        ["F11", 0x4D, 13, 0], //  67 KEY_EN_F11
        ["Backspace", 0x50, 13, 1], //  68 KEY_EN_BACKSPACE
        ["]", 0x49, 13, 2], //  69 KEY_EN_RIGHT_BRACKET
        ["ISO_#", 0x4A, 13, 3], //  70 KEY_EN_POUND
        ["Right Shift", 0x4B, 13, 4], //  71 KEY_EN_RIGHT_SHIFT
        ["F12", 0x4F, 14, 0], //  72 KEY_EN_F12
        ["Enter", 0x52, 14, 3], //  73 KEY_EN_ISO_ENTER
        ["Right Ctrl", 0x4C, 14, 5], //  74 KEY_EN_RIGHT_CONTROL
        ["AudioMute", 0x5C, 16, 0], //  75 KEY_EN_MEDIA_MUTE
        ["Insert", 0x54, 16, 1], //  76 KEY_EN_INSERT
        ["Del", 0x55, 16, 2], //  77 KEY_EN_DELETE
        ["Left Arrow", 0x56, 16, 5], //  78 KEY_EN_LEFT_ARROW
        ["Home", 0x58, 17, 1], //  79 KEY_EN_HOME
        ["End", 0x59, 17, 2], //  80 KEY_EN_END
        ["Up Arrow", 0x5A, 17, 4], //  81 KEY_EN_UP_ARROW
        ["Down Arrow", 0x5B, 17, 5], //  82 KEY_EN_DOWN_ARROW
        ["Page Up", 0x5D, 18, 1], //  83 KEY_EN_PAGE_UP
        ["Page Down", 0x5E, 18, 2], //  84 KEY_EN_PAGE_DOWN
        ["Right Arrow", 0x5F, 18, 5], //  85 KEY_EN_RIGHT_ARROW
      ]),
    }),
    US: Object.freeze({
      width: 19,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0x02, 0, 0], //   0 KEY_EN_ESCAPE
        ["`", 0x03, 0, 1], //   1 KEY_EN_BACK_TICK
        ["Tab", 0x04, 0, 2], //   2 KEY_EN_TAB
        ["Caps Lock", 0x05, 0, 3], //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0x00, 0, 4], //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0x01, 0, 5], //   5 KEY_EN_LEFT_CONTROL
        ["1", 0x08, 1, 1], //   6 KEY_EN_1
        ["Left Win", 0x07, 1, 5], //   7 KEY_EN_LEFT_WINDOWS
        ["F1", 0x0D, 2, 0], //   8 KEY_EN_F1
        ["2", 0x0E, 2, 1], //   9 KEY_EN_2
        ["Q", 0x09, 2, 2], //  10 KEY_EN_Q
        ["A", 0x0A, 2, 3], //  11 KEY_EN_A
        ["Z", 0x0B, 2, 4], //  12 KEY_EN_Z
        ["Left Alt", 0x0C, 2, 5], //  13 KEY_EN_LEFT_ALT
        ["F2", 0x14, 3, 0], //  14 KEY_EN_F2
        ["3", 0x15, 3, 1], //  15 KEY_EN_3
        ["W", 0x0F, 3, 2], //  16 KEY_EN_W
        ["S", 0x10, 3, 3], //  17 KEY_EN_S
        ["X", 0x11, 3, 4], //  18 KEY_EN_X
        ["F3", 0x19, 4, 0], //  19 KEY_EN_F3
        ["4", 0x1A, 4, 1], //  20 KEY_EN_4
        ["E", 0x16, 4, 2], //  21 KEY_EN_E
        ["D", 0x17, 4, 3], //  22 KEY_EN_D
        ["C", 0x18, 4, 4], //  23 KEY_EN_C
        ["F4", 0x1E, 5, 0], //  24 KEY_EN_F4
        ["5", 0x1F, 5, 1], //  25 KEY_EN_5
        ["R", 0x1B, 5, 2], //  26 KEY_EN_R
        ["F", 0x1C, 5, 3], //  27 KEY_EN_F
        ["V", 0x1D, 5, 4], //  28 KEY_EN_V
        ["6", 0x24, 6, 1], //  29 KEY_EN_6
        ["T", 0x20, 6, 2], //  30 KEY_EN_T
        ["G", 0x21, 6, 3], //  31 KEY_EN_G
        ["B", 0x22, 6, 4], //  32 KEY_EN_B
        ["Space", 0x23, 6, 5], //  33 KEY_EN_SPACE
        ["F5", 0x28, 7, 0], //  34 KEY_EN_F5
        ["7", 0x29, 7, 1], //  35 KEY_EN_7
        ["Y", 0x25, 7, 2], //  36 KEY_EN_Y
        ["H", 0x26, 7, 3], //  37 KEY_EN_H
        ["N", 0x27, 7, 4], //  38 KEY_EN_N
        ["F6", 0x2F, 8, 0], //  39 KEY_EN_F6
        ["8", 0x30, 8, 1], //  40 KEY_EN_8
        ["U", 0x2A, 8, 2], //  41 KEY_EN_U
        ["J", 0x2B, 8, 3], //  42 KEY_EN_J
        ["M", 0x2C, 8, 4], //  43 KEY_EN_M
        ["F7", 0x35, 9, 0], //  44 KEY_EN_F7
        ["9", 0x36, 9, 1], //  45 KEY_EN_9
        ["I", 0x31, 9, 2], //  46 KEY_EN_I
        ["K", 0x32, 9, 3], //  47 KEY_EN_K
        [",", 0x33, 9, 4], //  48 KEY_EN_COMMA
        ["F8", 0x3B, 10, 0], //  49 KEY_EN_F8
        ["0", 0x3C, 10, 1], //  50 KEY_EN_0
        ["O", 0x37, 10, 2], //  51 KEY_EN_O
        ["L", 0x38, 10, 3], //  52 KEY_EN_L
        [".", 0x39, 10, 4], //  53 KEY_EN_PERIOD
        ["Right Alt", 0x3A, 10, 5], //  54 KEY_EN_RIGHT_ALT
        ["F9", 0x41, 11, 0], //  55 KEY_EN_F9
        ["-", 0x42, 11, 1], //  56 KEY_EN_MINUS
        ["P", 0x3D, 11, 2], //  57 KEY_EN_P
        [";", 0x3E, 11, 3], //  58 KEY_EN_SEMICOLON
        ["/", 0x3F, 11, 4], //  59 KEY_EN_FORWARD_SLASH
        ["Fn", 0x40, 11, 5], //  60 KEY_EN_RIGHT_FUNCTION
        ["F10", 0x47, 12, 0], //  61 KEY_EN_F10
        ["=", 0x48, 12, 1], //  62 KEY_EN_EQUALS
        ["[", 0x43, 12, 2], //  63 KEY_EN_LEFT_BRACKET
        ["'", 0x44, 12, 3], //  64 KEY_EN_QUOTE
        ["Menu", 0x46, 12, 5], //  65 KEY_EN_MENU
        ["F11", 0x4D, 13, 0], //  66 KEY_EN_F11
        ["Backspace", 0x50, 13, 1], //  67 KEY_EN_BACKSPACE
        ["]", 0x49, 13, 2], //  68 KEY_EN_RIGHT_BRACKET
        ["Right Shift", 0x4B, 13, 4], //  69 KEY_EN_RIGHT_SHIFT
        ["F12", 0x4F, 14, 0], //  70 KEY_EN_F12
        ["\\", 0x51, 14, 2], //  71 KEY_EN_ANSI_BACK_SLASH
        ["Enter", 0x52, 14, 3], //  72 KEY_EN_ANSI_ENTER
        ["Right Ctrl", 0x4C, 14, 5], //  73 KEY_EN_RIGHT_CONTROL
        ["AudioMute", 0x5C, 16, 0], //  74 KEY_EN_MEDIA_MUTE
        ["Insert", 0x54, 16, 1], //  75 KEY_EN_INSERT
        ["Del", 0x55, 16, 2], //  76 KEY_EN_DELETE
        ["Left Arrow", 0x56, 16, 5], //  77 KEY_EN_LEFT_ARROW
        ["Home", 0x58, 17, 1], //  78 KEY_EN_HOME
        ["End", 0x59, 17, 2], //  79 KEY_EN_END
        ["Up Arrow", 0x5A, 17, 4], //  80 KEY_EN_UP_ARROW
        ["Down Arrow", 0x5B, 17, 5], //  81 KEY_EN_DOWN_ARROW
        ["Page Up", 0x5D, 18, 1], //  82 KEY_EN_PAGE_UP
        ["Page Down", 0x5E, 18, 2], //  83 KEY_EN_PAGE_DOWN
        ["Right Arrow", 0x5F, 18, 5], //  84 KEY_EN_RIGHT_ARROW
      ]),
    }),
  }),
  TurtleBeachVulcanIITKLProLayouts: Object.freeze({
    UK: Object.freeze({
      width: 19,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0x00, 0, 0], //   0 KEY_EN_ESCAPE
        ["F1", 0x01, 2, 0], //   1 KEY_EN_F1
        ["F2", 0x02, 3, 0], //   2 KEY_EN_F2
        ["F3", 0x03, 4, 0], //   3 KEY_EN_F3
        ["F4", 0x04, 5, 0], //   4 KEY_EN_F4
        ["F5", 0x05, 7, 0], //   5 KEY_EN_F5
        ["F6", 0x06, 8, 0], //   6 KEY_EN_F6
        ["F7", 0x07, 9, 0], //   7 KEY_EN_F7
        ["F8", 0x08, 10, 0], //   8 KEY_EN_F8
        ["F9", 0x09, 11, 0], //   9 KEY_EN_F9
        ["F10", 0x0A, 12, 0], //  10 KEY_EN_F10
        ["F11", 0x0B, 13, 0], //  11 KEY_EN_F11
        ["F12", 0x0C, 14, 0], //  12 KEY_EN_F12
        ["`", 0x0D, 0, 1], //  13 KEY_EN_BACK_TICK
        ["1", 0x0E, 1, 1], //  14 KEY_EN_1
        ["2", 0x0F, 2, 1], //  15 KEY_EN_2
        ["3", 0x10, 3, 1], //  16 KEY_EN_3
        ["4", 0x11, 4, 1], //  17 KEY_EN_4
        ["5", 0x12, 5, 1], //  18 KEY_EN_5
        ["6", 0x13, 6, 1], //  19 KEY_EN_6
        ["7", 0x14, 7, 1], //  20 KEY_EN_7
        ["8", 0x15, 8, 1], //  21 KEY_EN_8
        ["9", 0x16, 9, 1], //  22 KEY_EN_9
        ["0", 0x17, 10, 1], //  23 KEY_EN_0
        ["-", 0x18, 11, 1], //  24 KEY_EN_MINUS
        ["=", 0x19, 12, 1], //  25 KEY_EN_EQUALS
        ["Backspace", 0x1B, 13, 1], //  26 KEY_EN_BACKSPACE
        ["Tab", 0x1C, 0, 2], //  27 KEY_EN_TAB
        ["Q", 0x1D, 2, 2], //  28 KEY_EN_Q
        ["W", 0x1E, 3, 2], //  29 KEY_EN_W
        ["E", 0x1F, 4, 2], //  30 KEY_EN_E
        ["R", 0x20, 5, 2], //  31 KEY_EN_R
        ["T", 0x21, 6, 2], //  32 KEY_EN_T
        ["Y", 0x22, 7, 2], //  33 KEY_EN_Y
        ["U", 0x23, 8, 2], //  34 KEY_EN_U
        ["I", 0x24, 9, 2], //  35 KEY_EN_I
        ["O", 0x25, 10, 2], //  36 KEY_EN_O
        ["P", 0x26, 11, 2], //  37 KEY_EN_P
        ["[", 0x27, 12, 2], //  38 KEY_EN_LEFT_BRACKET
        ["]", 0x28, 13, 2], //  39 KEY_EN_RIGHT_BRACKET
        ["Caps Lock", 0x2A, 0, 3], //  40 KEY_EN_CAPS_LOCK
        ["A", 0x2B, 2, 3], //  41 KEY_EN_A
        ["S", 0x2C, 3, 3], //  42 KEY_EN_S
        ["D", 0x2D, 4, 3], //  43 KEY_EN_D
        ["F", 0x2E, 5, 3], //  44 KEY_EN_F
        ["G", 0x2F, 6, 3], //  45 KEY_EN_G
        ["H", 0x30, 7, 3], //  46 KEY_EN_H
        ["J", 0x31, 8, 3], //  47 KEY_EN_J
        ["K", 0x32, 9, 3], //  48 KEY_EN_K
        ["L", 0x33, 10, 3], //  49 KEY_EN_L
        [";", 0x34, 11, 3], //  50 KEY_EN_SEMICOLON
        ["'", 0x35, 12, 3], //  51 KEY_EN_QUOTE
        ["ISO_#", 0x36, 13, 3], //  52 KEY_EN_POUND
        ["Enter", 0x37, 14, 3], //  53 KEY_EN_ISO_ENTER
        ["Left Shift", 0x38, 0, 4], //  54 KEY_EN_LEFT_SHIFT
        ["ISO_<", 0x39, 1, 4], //  55 KEY_EN_ISO_BACK_SLASH
        ["Z", 0x3A, 2, 4], //  56 KEY_EN_Y
        ["X", 0x3B, 3, 4], //  57 KEY_EN_X
        ["C", 0x3C, 4, 4], //  58 KEY_EN_C
        ["V", 0x3D, 5, 4], //  59 KEY_EN_V
        ["B", 0x3E, 6, 4], //  60 KEY_EN_B
        ["N", 0x3F, 7, 4], //  61 KEY_EN_N
        ["M", 0x40, 8, 4], //  62 KEY_EN_M
        [",", 0x41, 9, 4], //  63 KEY_EN_COMMA
        [".", 0x42, 10, 4], //  64 KEY_EN_PERIOD
        ["/", 0x43, 11, 4], //  65 KEY_EN_FORWARD_SLASH
        ["Right Shift", 0x45, 13, 4], //  66 KEY_EN_RIGHT_SHIFT
        ["Left Ctrl", 0x46, 0, 5], //  67 KEY_EN_LEFT_CONTROL
        ["Left Win", 0x47, 1, 5], //  68 KEY_EN_LEFT_WINDOWS
        ["Left Alt", 0x48, 2, 5], //  69 KEY_EN_LEFT_ALT
        ["Space", 0x4A, 6, 5], //  70 KEY_EN_SPACE
        ["Right Alt", 0x4D, 11, 5], //  71 KEY_EN_RIGHT_ALT
        ["Fn", 0x4E, 12, 5], //  72 KEY_EN_RIGHT_FUNCTION
        ["Menu", 0x4F, 13, 5], //  73 KEY_EN_MENU
        ["Right Ctrl", 0x50, 14, 5], //  74 KEY_EN_RIGHT_CONTROL
        ["Up Arrow", 0x51, 17, 4], //  75 KEY_EN_UP_ARROW
        ["Left Arrow", 0x52, 16, 5], //  76 KEY_EN_LEFT_ARROW
        ["Down Arrow", 0x53, 17, 5], //  77 KEY_EN_DOWN_ARROW
        ["Right Arrow", 0x54, 18, 5], //  78 KEY_EN_RIGHT_ARROW
        ["Insert", 0x5A, 16, 1], //  79 KEY_EN_INSERT
        ["Home", 0x59, 17, 1], //  80 KEY_EN_HOME
        ["Page Up", 0x58, 18, 1], //  81 KEY_EN_PAGE_UP
        ["Del", 0x55, 16, 2], //  82 KEY_EN_DELETE
        ["End", 0x56, 17, 2], //  83 KEY_EN_END
        ["Page Down", 0x57, 18, 2], //  84 KEY_EN_PAGE_DOWN
      ]),
    }),
  }),
  RoccatVulcanIIMiniLayouts: Object.freeze({
    UK: Object.freeze({
      width: 15,
      height: 5,
      leds: Object.freeze([
        ["Esc", 0x00, 0, 0], //   0 KEY_EN_ESCAPE
        ["Tab", 0x01, 0, 1], //   1 KEY_EN_TAB
        ["Caps Lock", 0x02, 0, 2], //   2 KEY_EN_CAPS_LOCK
        ["Left Shift", 0x03, 0, 3], //   3 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0x04, 0, 4], //   4 KEY_EN_LEFT_CONTROL
        ["ISO_<", 0x05, 1, 3], //   5 KEY_EN_ISO_BACK_SLASH
        ["1", 0x06, 1, 0], //   6 KEY_EN_1
        ["Q", 0x07, 1, 1], //   7 KEY_EN_Q
        ["A", 0x08, 1, 2], //   8 KEY_EN_A
        ["Z", 0x09, 2, 3], //   9 KEY_EN_Z
        ["Left Win", 0x0A, 1, 4], //  10 KEY_EN_LEFT_WINDOWS
        ["Left Alt", 0x0B, 2, 4], //  11 KEY_EN_LEFT_ALT
        ["2", 0x0C, 2, 0], //  12 KEY_EN_2
        ["3", 0x0D, 3, 0], //  13 KEY_EN_3
        ["W", 0x0E, 2, 1], //  14 KEY_EN_W
        ["S", 0x0F, 2, 2], //  15 KEY_EN_S
        ["X", 0x10, 3, 3], //  16 KEY_EN_X
        ["4", 0x11, 4, 0], //  17 KEY_EN_4
        ["E", 0x12, 3, 1], //  18 KEY_EN_E
        ["R", 0x13, 4, 1], //  19 KEY_EN_R
        ["D", 0x14, 3, 2], //  20 KEY_EN_D
        ["C", 0x15, 4, 3], //  21 KEY_EN_C
        ["5", 0x16, 5, 0], //  22 KEY_EN_5
        ["T", 0x17, 5, 1], //  23 KEY_EN_T
        ["F", 0x18, 4, 2], //  24 KEY_EN_F
        ["G", 0x19, 5, 2], //  25 KEY_EN_G
        ["V", 0x1A, 5, 3], //  26 KEY_EN_V
        ["B", 0x1B, 6, 3], //  27 KEY_EN_B
        ["6", 0x1C, 6, 0], //  28 KEY_EN_6
        ["7", 0x1D, 7, 0], //  29 KEY_EN_7
        ["Y", 0x1E, 6, 1], //  30 KEY_EN_Y
        ["H", 0x1F, 6, 2], //  31 KEY_EN_H
        ["N", 0x20, 7, 3], //  32 KEY_EN_N
        ["Space", 0x21, 5, 4], //  33 KEY_EN_SPACE
        ["8", 0x22, 8, 0], //  34 KEY_EN_8
        ["I", 0x23, 8, 1], //  35 KEY_EN_I
        ["U", 0x24, 7, 1], //  36 KEY_EN_U
        ["J", 0x25, 7, 2], //  37 KEY_EN_J
        ["M", 0x26, 8, 3], //  38 KEY_EN_M
        ["9", 0x27, 9, 0], //  39 KEY_EN_9
        ["O", 0x28, 9, 1], //  40 KEY_EN_O
        ["K", 0x29, 8, 2], //  41 KEY_EN_K
        [",", 0x2A, 9, 3], //  42 KEY_EN_COMMA
        ["0", 0x2B, 10, 0], //  43 KEY_EN_0
        ["P", 0x2C, 10, 1], //  44 KEY_EN_P
        ["L", 0x2D, 9, 2], //  45 KEY_EN_L
        [";", 0x2E, 10, 2], //  46 KEY_EN_SEMICOLON
        [".", 0x2F, 10, 3], //  47 KEY_EN_PERIOD
        ["Right Alt", 0x30, 9, 4], //  48 KEY_EN_RIGHT_ALT
        ["-", 0x31, 11, 0], //  49 KEY_EN_MINUS
        ["[", 0x32, 11, 1], //  50 KEY_EN_LEFT_BRACKET
        ["'", 0x33, 11, 2], //  51 KEY_EN_QUOTE
        ["/", 0x34, 11, 3], //  52 KEY_EN_FORWARD_SLASH
        ["Fn", 0x35, 10, 4], //  53 KEY_EN_RIGHT_FUNCTION
        ["Right Ctrl", 0x36, 11, 4], //  54 KEY_EN_RIGHT_CONTROL
        ["=", 0x37, 12, 0], //  55 KEY_EN_EQUALS
        ["]", 0x38, 12, 1], //  56 KEY_EN_RIGHT_BRACKET
        ["LED 0x39", 0x39, 12, 2], //  57 KEY_EN_UNUSED
        ["Right Shift", 0x3A, 12, 3], //  58 KEY_EN_RIGHT_SHIFT
        ["Left Arrow", 0x3B, 12, 4], //  59 KEY_EN_LEFT_ARROW
        ["Backspace", 0x3C, 13, 0], //  60 KEY_EN_BACKSPACE
        ["Enter", 0x3D, 13, 1], //  61 KEY_EN_ISO_ENTER
        ["Up Arrow", 0x3E, 13, 3], //  62 KEY_EN_UP_ARROW
        ["Down Arrow", 0x3F, 13, 4], //  63 KEY_EN_DOWN_ARROW
        ["Del", 0x40, 14, 0], //  64 KEY_EN_DELETE
        ["Home", 0x41, 14, 1], //  65 KEY_EN_HOME
        ["Page Up", 0x42, 14, 2], //  66 KEY_EN_PAGE_UP
        ["Page Down", 0x43, 14, 3], //  67 KEY_EN_PAGE_DOWN
        ["Right Arrow", 0x44, 14, 4], //  68 KEY_EN_RIGHT_ARROW
      ]),
    }),
    US: Object.freeze({
      width: 15,
      height: 5,
      leds: Object.freeze([
        ["Esc", 0x00, 0, 0], //   0 KEY_EN_ESCAPE
        ["Tab", 0x01, 0, 1], //   1 KEY_EN_TAB
        ["Caps Lock", 0x02, 0, 2], //   2 KEY_EN_CAPS_LOCK
        ["Left Shift", 0x03, 0, 3], //   3 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0x04, 0, 4], //   4 KEY_EN_LEFT_CONTROL
        ["ISO_<", 0x05, 1, 3], //   5 KEY_EN_ISO_BACK_SLASH
        ["1", 0x06, 1, 0], //   6 KEY_EN_1
        ["Q", 0x07, 1, 1], //   7 KEY_EN_Q
        ["A", 0x08, 1, 2], //   8 KEY_EN_A
        ["Z", 0x09, 2, 3], //   9 KEY_EN_Z
        ["Left Win", 0x0A, 1, 4], //  10 KEY_EN_LEFT_WINDOWS
        ["Left Alt", 0x0B, 2, 4], //  11 KEY_EN_LEFT_ALT
        ["2", 0x0C, 2, 0], //  12 KEY_EN_2
        ["3", 0x0D, 3, 0], //  13 KEY_EN_3
        ["W", 0x0E, 2, 1], //  14 KEY_EN_W
        ["S", 0x0F, 2, 2], //  15 KEY_EN_S
        ["X", 0x10, 3, 3], //  16 KEY_EN_X
        ["4", 0x11, 4, 0], //  17 KEY_EN_4
        ["E", 0x12, 3, 1], //  18 KEY_EN_E
        ["R", 0x13, 4, 1], //  19 KEY_EN_R
        ["D", 0x14, 3, 2], //  20 KEY_EN_D
        ["C", 0x15, 4, 3], //  21 KEY_EN_C
        ["5", 0x16, 5, 0], //  22 KEY_EN_5
        ["T", 0x17, 5, 1], //  23 KEY_EN_T
        ["F", 0x18, 4, 2], //  24 KEY_EN_F
        ["G", 0x19, 5, 2], //  25 KEY_EN_G
        ["V", 0x1A, 5, 3], //  26 KEY_EN_V
        ["B", 0x1B, 6, 3], //  27 KEY_EN_B
        ["6", 0x1C, 6, 0], //  28 KEY_EN_6
        ["7", 0x1D, 7, 0], //  29 KEY_EN_7
        ["Y", 0x1E, 6, 1], //  30 KEY_EN_Y
        ["H", 0x1F, 6, 2], //  31 KEY_EN_H
        ["N", 0x20, 7, 3], //  32 KEY_EN_N
        ["Space", 0x21, 5, 4], //  33 KEY_EN_SPACE
        ["8", 0x22, 8, 0], //  34 KEY_EN_8
        ["I", 0x23, 8, 1], //  35 KEY_EN_I
        ["U", 0x24, 7, 1], //  36 KEY_EN_U
        ["J", 0x25, 7, 2], //  37 KEY_EN_J
        ["M", 0x26, 8, 3], //  38 KEY_EN_M
        ["9", 0x27, 9, 0], //  39 KEY_EN_9
        ["O", 0x28, 9, 1], //  40 KEY_EN_O
        ["K", 0x29, 8, 2], //  41 KEY_EN_K
        [",", 0x2A, 9, 3], //  42 KEY_EN_COMMA
        ["0", 0x2B, 10, 0], //  43 KEY_EN_0
        ["P", 0x2C, 10, 1], //  44 KEY_EN_P
        ["L", 0x2D, 9, 2], //  45 KEY_EN_L
        [";", 0x2E, 10, 2], //  46 KEY_EN_SEMICOLON
        [".", 0x2F, 10, 3], //  47 KEY_EN_PERIOD
        ["Right Alt", 0x30, 9, 4], //  48 KEY_EN_RIGHT_ALT
        ["-", 0x31, 11, 0], //  49 KEY_EN_MINUS
        ["[", 0x32, 11, 1], //  50 KEY_EN_LEFT_BRACKET
        ["'", 0x33, 11, 2], //  51 KEY_EN_QUOTE
        ["/", 0x34, 11, 3], //  52 KEY_EN_FORWARD_SLASH
        ["Fn", 0x35, 10, 4], //  53 KEY_EN_RIGHT_FUNCTION
        ["Right Ctrl", 0x36, 11, 4], //  54 KEY_EN_RIGHT_CONTROL
        ["=", 0x37, 12, 0], //  55 KEY_EN_EQUALS
        ["]", 0x38, 12, 1], //  56 KEY_EN_RIGHT_BRACKET
        ["LED 0x39", 0x39, 12, 2], //  57 KEY_EN_UNUSED
        ["Right Shift", 0x3A, 12, 3], //  58 KEY_EN_RIGHT_SHIFT
        ["Left Arrow", 0x3B, 12, 4], //  59 KEY_EN_LEFT_ARROW
        ["Backspace", 0x3C, 13, 0], //  60 KEY_EN_BACKSPACE
        ["Enter", 0x3D, 13, 1], //  61 KEY_EN_ANSI_ENTER
        ["Up Arrow", 0x3E, 13, 3], //  62 KEY_EN_UP_ARROW
        ["Down Arrow", 0x3F, 13, 4], //  63 KEY_EN_DOWN_ARROW
        ["Del", 0x40, 14, 0], //  64 KEY_EN_DELETE
        ["Home", 0x41, 14, 1], //  65 KEY_EN_HOME
        ["Page Up", 0x42, 14, 2], //  66 KEY_EN_PAGE_UP
        ["Page Down", 0x43, 14, 3], //  67 KEY_EN_PAGE_DOWN
        ["Right Arrow", 0x44, 14, 4], //  68 KEY_EN_RIGHT_ARROW
      ]),
    }),
  }),
  RoccatMagmaLayouts: Object.freeze({
    US: Object.freeze({
      width: 5,
      height: 1,
      leds: Object.freeze([
        ["Keyboard LED 1", 0x00, 0, 0], //   0 "Keyboard LED 1"
        ["Keyboard LED 2", 0x01, 1, 0], //   1 "Keyboard LED 2"
        ["Keyboard LED 3", 0x02, 2, 0], //   2 "Keyboard LED 3"
        ["Keyboard LED 4", 0x03, 3, 0], //   3 "Keyboard LED 4"
        ["Keyboard LED 5", 0x04, 4, 0], //   4 "Keyboard LED 5"
      ]),
    }),
  }),
});
