// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: Logitech G813 / G815 and G915 / G915 TKL per-key layouts, as data. Generated
// from the OpenRGB tables; do not edit by hand.
// Adapted from OpenRGB Controllers/LogitechController/LogitechG815Controller/
// RGBController_LogitechG815.cpp (matrix_map, led_names) and LogitechG915Controller/
// RGBController_LogitechG915.cpp (matrix_map, matrix_map_tkl, led_names) (GPL-2.0-or-later) at
// commit d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: Cheerpipe (20 Mar 2021; both RGBController files).
//
// Per LED, in OpenRGB's LED order (the order of led_names):
//   name         the key, in the key names HueDeck plugins use (docs/host-api/hid-usb.md section 6);
//                the comment keeps OpenRGB's KEY_EN_* constant or label;
//   column, row  the LED's cell in OpenRGB's matrix map, so keys sit where they are on the keyboard;
//   code         the key code DeviceUpdateLEDs() puts in the 0x1F / 0x6F frames: led_names' index,
//                shifted by its zone (keyboard -0x03, modifiers -0x78, G-keys +0xB3, logo +0xD1,
//                media and indicator keys unchanged).
// The G915 TKL uses the first 95 LEDs of the G915 list.

export const G815_LAYOUT = Object.freeze({
  name: "G813 / G815",
  width: 27,
  height: 7,
  // [name, column, row, code] per LED, in OpenRGB's LED order.
  leds: Object.freeze([
    ["A", 4, 4, 0x01],            //   0 KEY_EN_A
    ["B", 9, 5, 0x02],            //   1 KEY_EN_B
    ["C", 6, 5, 0x03],            //   2 KEY_EN_C
    ["D", 6, 4, 0x04],            //   3 KEY_EN_D
    ["E", 6, 3, 0x05],            //   4 KEY_EN_E
    ["F", 7, 4, 0x06],            //   5 KEY_EN_F
    ["G", 9, 4, 0x07],            //   6 KEY_EN_G
    ["H", 10, 4, 0x08],           //   7 KEY_EN_H
    ["I", 12, 3, 0x09],           //   8 KEY_EN_I
    ["J", 11, 4, 0x0A],           //   9 KEY_EN_J
    ["K", 12, 4, 0x0B],           //  10 KEY_EN_K
    ["L", 13, 4, 0x0C],           //  11 KEY_EN_L
    ["M", 12, 5, 0x0D],           //  12 KEY_EN_M
    ["N", 11, 5, 0x0E],           //  13 KEY_EN_N
    ["O", 13, 3, 0x0F],           //  14 KEY_EN_O
    ["P", 14, 3, 0x10],           //  15 KEY_EN_P
    ["Q", 4, 3, 0x11],            //  16 KEY_EN_Q
    ["R", 7, 3, 0x12],            //  17 KEY_EN_R
    ["S", 5, 4, 0x13],            //  18 KEY_EN_S
    ["T", 9, 3, 0x14],            //  19 KEY_EN_T
    ["U", 11, 3, 0x15],           //  20 KEY_EN_U
    ["V", 7, 5, 0x16],            //  21 KEY_EN_V
    ["W", 5, 3, 0x17],            //  22 KEY_EN_W
    ["X", 5, 5, 0x18],            //  23 KEY_EN_X
    ["Y", 10, 3, 0x19],           //  24 KEY_EN_Y
    ["Z", 4, 5, 0x1A],            //  25 KEY_EN_Z
    ["1", 3, 2, 0x1B],            //  26 KEY_EN_1
    ["2", 4, 2, 0x1C],            //  27 KEY_EN_2
    ["3", 5, 2, 0x1D],            //  28 KEY_EN_3
    ["4", 6, 2, 0x1E],            //  29 KEY_EN_4
    ["5", 7, 2, 0x1F],            //  30 KEY_EN_5
    ["6", 8, 2, 0x20],            //  31 KEY_EN_6
    ["7", 9, 2, 0x21],            //  32 KEY_EN_7
    ["8", 10, 2, 0x22],           //  33 KEY_EN_8
    ["9", 11, 2, 0x23],           //  34 KEY_EN_9
    ["0", 12, 2, 0x24],           //  35 KEY_EN_0
    ["Enter", 17, 4, 0x25],       //  36 KEY_EN_ANSI_ENTER
    ["Esc", 2, 1, 0x26],          //  37 KEY_EN_ESCAPE
    ["Backspace", 16, 2, 0x27],   //  38 KEY_EN_BACKSPACE
    ["Tab", 2, 3, 0x28],          //  39 KEY_EN_TAB
    ["Space", 9, 6, 0x29],        //  40 KEY_EN_SPACE
    ["-", 14, 2, 0x2A],           //  41 KEY_EN_MINUS
    ["=", 15, 2, 0x2B],           //  42 KEY_EN_EQUALS
    ["[", 15, 3, 0x2C],           //  43 KEY_EN_LEFT_BRACKET
    ["]", 16, 3, 0x2D],           //  44 KEY_EN_RIGHT_BRACKET
    ["\\", 17, 3, 0x2E],          //  45 KEY_EN_ANSI_BACK_SLASH
    ["ISO_#", 16, 4, 0x2F],       //  46 KEY_EN_POUND
    [";", 14, 4, 0x30],           //  47 KEY_EN_SEMICOLON
    ["'", 15, 4, 0x31],           //  48 KEY_EN_QUOTE
    ["`", 2, 2, 0x32],            //  49 KEY_EN_BACK_TICK
    [",", 13, 5, 0x33],           //  50 KEY_EN_COMMA
    [".", 14, 5, 0x34],           //  51 KEY_EN_PERIOD
    ["/", 15, 5, 0x35],           //  52 KEY_EN_FORWARD_SLASH
    ["Caps Lock", 2, 4, 0x36],    //  53 KEY_EN_CAPS_LOCK
    ["F1", 4, 1, 0x37],           //  54 KEY_EN_F1
    ["F2", 5, 1, 0x38],           //  55 KEY_EN_F2
    ["F3", 6, 1, 0x39],           //  56 KEY_EN_F3
    ["F4", 7, 1, 0x3A],           //  57 KEY_EN_F4
    ["F5", 9, 1, 0x3B],           //  58 KEY_EN_F5
    ["F6", 10, 1, 0x3C],          //  59 KEY_EN_F6
    ["F7", 11, 1, 0x3D],          //  60 KEY_EN_F7
    ["F8", 12, 1, 0x3E],          //  61 KEY_EN_F8
    ["F9", 14, 1, 0x3F],          //  62 KEY_EN_F9
    ["F10", 15, 1, 0x40],         //  63 KEY_EN_F10
    ["F11", 16, 1, 0x41],         //  64 KEY_EN_F11
    ["F12", 17, 1, 0x42],         //  65 KEY_EN_F12
    ["Print Screen", 19, 1, 0x43],//  66 KEY_EN_PRINT_SCREEN
    ["Scroll Lock", 20, 1, 0x44], //  67 KEY_EN_SCROLL_LOCK
    ["Pause Break", 21, 1, 0x45], //  68 KEY_EN_PAUSE_BREAK
    ["Insert", 19, 2, 0x46],      //  69 KEY_EN_INSERT
    ["Home", 20, 2, 0x47],        //  70 KEY_EN_HOME
    ["Page Up", 21, 2, 0x48],     //  71 KEY_EN_PAGE_UP
    ["Del", 19, 3, 0x49],         //  72 KEY_EN_DELETE
    ["End", 20, 3, 0x4A],         //  73 KEY_EN_END
    ["Page Down", 21, 3, 0x4B],   //  74 KEY_EN_PAGE_DOWN
    ["Right Arrow", 21, 6, 0x4C], //  75 KEY_EN_RIGHT_ARROW
    ["Left Arrow", 19, 6, 0x4D],  //  76 KEY_EN_LEFT_ARROW
    ["Down Arrow", 20, 6, 0x4E],  //  77 KEY_EN_DOWN_ARROW
    ["Up Arrow", 20, 5, 0x4F],    //  78 KEY_EN_UP_ARROW
    ["NumLock", 23, 2, 0x50],     //  79 KEY_EN_NUMPAD_LOCK
    ["Num /", 24, 2, 0x51],       //  80 KEY_EN_NUMPAD_DIVIDE
    ["Num *", 25, 2, 0x52],       //  81 KEY_EN_NUMPAD_TIMES
    ["Num -", 26, 2, 0x53],       //  82 KEY_EN_NUMPAD_MINUS
    ["Num +", 26, 3, 0x54],       //  83 KEY_EN_NUMPAD_PLUS
    ["Num Enter", 26, 5, 0x55],   //  84 KEY_EN_NUMPAD_ENTER
    ["Num 1", 23, 5, 0x56],       //  85 KEY_EN_NUMPAD_1
    ["Num 2", 24, 5, 0x57],       //  86 KEY_EN_NUMPAD_2
    ["Num 3", 25, 5, 0x58],       //  87 KEY_EN_NUMPAD_3
    ["Num 4", 23, 4, 0x59],       //  88 KEY_EN_NUMPAD_4
    ["Num 5", 24, 4, 0x5A],       //  89 KEY_EN_NUMPAD_5
    ["Num 6", 25, 4, 0x5B],       //  90 KEY_EN_NUMPAD_6
    ["Num 7", 23, 3, 0x5C],       //  91 KEY_EN_NUMPAD_7
    ["Num 8", 24, 3, 0x5D],       //  92 KEY_EN_NUMPAD_8
    ["Num 9", 25, 3, 0x5E],       //  93 KEY_EN_NUMPAD_9
    ["Num 0", 23, 6, 0x5F],       //  94 KEY_EN_NUMPAD_0
    ["Num .", 25, 6, 0x60],       //  95 KEY_EN_NUMPAD_PERIOD
    ["ISO_<", 3, 5, 0x61],        //  96 KEY_EN_ISO_BACK_SLASH
    ["Menu", 16, 6, 0x62],        //  97 KEY_EN_MENU
    ["Left Ctrl", 2, 6, 0x68],    //  98 KEY_EN_LEFT_CONTROL
    ["Left Shift", 2, 5, 0x69],   //  99 KEY_EN_LEFT_SHIFT
    ["Left Alt", 4, 6, 0x6A],     // 100 KEY_EN_LEFT_ALT
    ["Left Win", 3, 6, 0x6B],     // 101 KEY_EN_LEFT_WINDOWS
    ["Right Ctrl", 17, 6, 0x6C],  // 102 KEY_EN_RIGHT_CONTROL
    ["Right Shift", 16, 5, 0x6D], // 103 KEY_EN_RIGHT_SHIFT
    ["Right Alt", 14, 6, 0x6E],   // 104 KEY_EN_RIGHT_ALT
    ["Right Win", 15, 6, 0x6F],   // 105 KEY_EN_RIGHT_WINDOWS
    ["MediaPreviousTrack", 23, 1, 0x9E],// 106 KEY_EN_MEDIA_PREVIOUS
    ["MediaPlayPause", 24, 1, 0x9B],// 107 KEY_EN_MEDIA_PLAY_PAUSE
    ["MediaNextTrack", 25, 1, 0x9D],// 108 KEY_EN_MEDIA_NEXT
    ["AudioMute", 26, 1, 0x9C],   // 109 KEY_EN_MEDIA_MUTE
    ["Logo", 0, 0, 0xD2],         // 110 "Logo"
    ["Lighting", 10, 0, 0x99],    // 111 "Lighting"
    ["G1", 0, 2, 0xB4],           // 112 "Key: G1"
    ["G2", 0, 3, 0xB5],           // 113 "Key: G2"
    ["G3", 0, 4, 0xB6],           // 114 "Key: G3"
    ["G4", 0, 5, 0xB7],           // 115 "Key: G4"
    ["G5", 0, 6, 0xB8],           // 116 "Key: G5"
  ]),
});

export const G915_LAYOUT = Object.freeze({
  name: "G915",
  width: 27,
  height: 7,
  // [name, column, row, code] per LED, in OpenRGB's LED order.
  leds: Object.freeze([
    ["A", 4, 4, 0x01],            //   0 KEY_EN_A
    ["B", 9, 5, 0x02],            //   1 KEY_EN_B
    ["C", 6, 5, 0x03],            //   2 KEY_EN_C
    ["D", 6, 4, 0x04],            //   3 KEY_EN_D
    ["E", 6, 3, 0x05],            //   4 KEY_EN_E
    ["F", 7, 4, 0x06],            //   5 KEY_EN_F
    ["G", 9, 4, 0x07],            //   6 KEY_EN_G
    ["H", 10, 4, 0x08],           //   7 KEY_EN_H
    ["I", 12, 3, 0x09],           //   8 KEY_EN_I
    ["J", 11, 4, 0x0A],           //   9 KEY_EN_J
    ["K", 12, 4, 0x0B],           //  10 KEY_EN_K
    ["L", 13, 4, 0x0C],           //  11 KEY_EN_L
    ["M", 12, 5, 0x0D],           //  12 KEY_EN_M
    ["N", 11, 5, 0x0E],           //  13 KEY_EN_N
    ["O", 13, 3, 0x0F],           //  14 KEY_EN_O
    ["P", 14, 3, 0x10],           //  15 KEY_EN_P
    ["Q", 4, 3, 0x11],            //  16 KEY_EN_Q
    ["R", 7, 3, 0x12],            //  17 KEY_EN_R
    ["S", 5, 4, 0x13],            //  18 KEY_EN_S
    ["T", 9, 3, 0x14],            //  19 KEY_EN_T
    ["U", 11, 3, 0x15],           //  20 KEY_EN_U
    ["V", 7, 5, 0x16],            //  21 KEY_EN_V
    ["W", 5, 3, 0x17],            //  22 KEY_EN_W
    ["X", 5, 5, 0x18],            //  23 KEY_EN_X
    ["Y", 10, 3, 0x19],           //  24 KEY_EN_Y
    ["Z", 4, 5, 0x1A],            //  25 KEY_EN_Z
    ["1", 3, 2, 0x1B],            //  26 KEY_EN_1
    ["2", 4, 2, 0x1C],            //  27 KEY_EN_2
    ["3", 5, 2, 0x1D],            //  28 KEY_EN_3
    ["4", 6, 2, 0x1E],            //  29 KEY_EN_4
    ["5", 7, 2, 0x1F],            //  30 KEY_EN_5
    ["6", 8, 2, 0x20],            //  31 KEY_EN_6
    ["7", 9, 2, 0x21],            //  32 KEY_EN_7
    ["8", 10, 2, 0x22],           //  33 KEY_EN_8
    ["9", 11, 2, 0x23],           //  34 KEY_EN_9
    ["0", 12, 2, 0x24],           //  35 KEY_EN_0
    ["Enter", 17, 4, 0x25],       //  36 KEY_EN_ANSI_ENTER
    ["Esc", 2, 1, 0x26],          //  37 KEY_EN_ESCAPE
    ["Backspace", 16, 2, 0x27],   //  38 KEY_EN_BACKSPACE
    ["Tab", 2, 3, 0x28],          //  39 KEY_EN_TAB
    ["Space", 9, 6, 0x29],        //  40 KEY_EN_SPACE
    ["-", 14, 2, 0x2A],           //  41 KEY_EN_MINUS
    ["=", 15, 2, 0x2B],           //  42 KEY_EN_EQUALS
    ["[", 15, 3, 0x2C],           //  43 KEY_EN_LEFT_BRACKET
    ["]", 16, 3, 0x2D],           //  44 KEY_EN_RIGHT_BRACKET
    ["\\", 17, 3, 0x2E],          //  45 KEY_EN_ANSI_BACK_SLASH
    ["ISO_#", 16, 4, 0x2F],       //  46 KEY_EN_POUND
    [";", 14, 4, 0x30],           //  47 KEY_EN_SEMICOLON
    ["'", 15, 4, 0x31],           //  48 KEY_EN_QUOTE
    ["`", 2, 2, 0x32],            //  49 KEY_EN_BACK_TICK
    [",", 13, 5, 0x33],           //  50 KEY_EN_COMMA
    [".", 14, 5, 0x34],           //  51 KEY_EN_PERIOD
    ["/", 15, 5, 0x35],           //  52 KEY_EN_FORWARD_SLASH
    ["Caps Lock", 2, 4, 0x36],    //  53 KEY_EN_CAPS_LOCK
    ["F1", 4, 1, 0x37],           //  54 KEY_EN_F1
    ["F2", 5, 1, 0x38],           //  55 KEY_EN_F2
    ["F3", 6, 1, 0x39],           //  56 KEY_EN_F3
    ["F4", 7, 1, 0x3A],           //  57 KEY_EN_F4
    ["F5", 9, 1, 0x3B],           //  58 KEY_EN_F5
    ["F6", 10, 1, 0x3C],          //  59 KEY_EN_F6
    ["F7", 11, 1, 0x3D],          //  60 KEY_EN_F7
    ["F8", 12, 1, 0x3E],          //  61 KEY_EN_F8
    ["F9", 14, 1, 0x3F],          //  62 KEY_EN_F9
    ["F10", 15, 1, 0x40],         //  63 KEY_EN_F10
    ["F11", 16, 1, 0x41],         //  64 KEY_EN_F11
    ["F12", 17, 1, 0x42],         //  65 KEY_EN_F12
    ["Print Screen", 19, 1, 0x43],//  66 KEY_EN_PRINT_SCREEN
    ["Scroll Lock", 20, 1, 0x44], //  67 KEY_EN_SCROLL_LOCK
    ["Pause Break", 21, 1, 0x45], //  68 KEY_EN_PAUSE_BREAK
    ["Insert", 19, 2, 0x46],      //  69 KEY_EN_INSERT
    ["Home", 20, 2, 0x47],        //  70 KEY_EN_HOME
    ["Page Up", 21, 2, 0x48],     //  71 KEY_EN_PAGE_UP
    ["Del", 19, 3, 0x49],         //  72 KEY_EN_DELETE
    ["End", 20, 3, 0x4A],         //  73 KEY_EN_END
    ["Page Down", 21, 3, 0x4B],   //  74 KEY_EN_PAGE_DOWN
    ["Right Arrow", 21, 6, 0x4C], //  75 KEY_EN_RIGHT_ARROW
    ["Left Arrow", 19, 6, 0x4D],  //  76 KEY_EN_LEFT_ARROW
    ["Down Arrow", 20, 6, 0x4E],  //  77 KEY_EN_DOWN_ARROW
    ["Up Arrow", 20, 5, 0x4F],    //  78 KEY_EN_UP_ARROW
    ["ISO_<", 3, 5, 0x61],        //  79 KEY_EN_ISO_BACK_SLASH
    ["Menu", 16, 6, 0x62],        //  80 KEY_EN_MENU
    ["Left Ctrl", 2, 6, 0x68],    //  81 KEY_EN_LEFT_CONTROL
    ["Left Shift", 2, 5, 0x69],   //  82 KEY_EN_LEFT_SHIFT
    ["Left Alt", 4, 6, 0x6A],     //  83 KEY_EN_LEFT_ALT
    ["Left Win", 3, 6, 0x6B],     //  84 KEY_EN_LEFT_WINDOWS
    ["Right Ctrl", 17, 6, 0x6C],  //  85 KEY_EN_RIGHT_CONTROL
    ["Right Shift", 16, 5, 0x6D], //  86 KEY_EN_RIGHT_SHIFT
    ["Right Alt", 14, 6, 0x6E],   //  87 KEY_EN_RIGHT_ALT
    ["Right Win", 15, 6, 0x6F],   //  88 KEY_EN_RIGHT_WINDOWS
    ["MediaPreviousTrack", 23, 1, 0x9E],//  89 KEY_EN_MEDIA_PREVIOUS
    ["MediaPlayPause", 24, 1, 0x9B],//  90 KEY_EN_MEDIA_PLAY_PAUSE
    ["MediaNextTrack", 25, 1, 0x9D],//  91 KEY_EN_MEDIA_NEXT
    ["AudioMute", 26, 1, 0x9C],   //  92 KEY_EN_MEDIA_MUTE
    ["Logo", 0, 0, 0xD2],         //  93 "Logo"
    ["Brightness", 10, 0, 0x99],  //  94 "Key: Brightness"
    ["NumLock", 23, 2, 0x50],     //  95 KEY_EN_NUMPAD_LOCK
    ["Num /", 24, 2, 0x51],       //  96 KEY_EN_NUMPAD_DIVIDE
    ["Num *", 25, 2, 0x52],       //  97 KEY_EN_NUMPAD_TIMES
    ["Num -", 26, 2, 0x53],       //  98 KEY_EN_NUMPAD_MINUS
    ["Num +", 26, 3, 0x54],       //  99 KEY_EN_NUMPAD_PLUS
    ["Num Enter", 26, 5, 0x55],   // 100 KEY_EN_NUMPAD_ENTER
    ["Num 1", 23, 5, 0x56],       // 101 KEY_EN_NUMPAD_1
    ["Num 2", 24, 5, 0x57],       // 102 KEY_EN_NUMPAD_2
    ["Num 3", 25, 5, 0x58],       // 103 KEY_EN_NUMPAD_3
    ["Num 4", 23, 4, 0x59],       // 104 KEY_EN_NUMPAD_4
    ["Num 5", 24, 4, 0x5A],       // 105 KEY_EN_NUMPAD_5
    ["Num 6", 25, 4, 0x5B],       // 106 KEY_EN_NUMPAD_6
    ["Num 7", 23, 3, 0x5C],       // 107 KEY_EN_NUMPAD_7
    ["Num 8", 24, 3, 0x5D],       // 108 KEY_EN_NUMPAD_8
    ["Num 9", 25, 3, 0x5E],       // 109 KEY_EN_NUMPAD_9
    ["Num 0", 23, 6, 0x5F],       // 110 KEY_EN_NUMPAD_0
    ["Num .", 25, 6, 0x60],       // 111 KEY_EN_NUMPAD_PERIOD
    ["G1", 0, 2, 0xB4],           // 112 "Key: G1"
    ["G2", 0, 3, 0xB5],           // 113 "Key: G2"
    ["G3", 0, 4, 0xB6],           // 114 "Key: G3"
    ["G4", 0, 5, 0xB7],           // 115 "Key: G4"
    ["G5", 0, 6, 0xB8],           // 116 "Key: G5"
  ]),
});

export const G915_TKL_LAYOUT = Object.freeze({
  name: "G915 TKL",
  width: 20,
  height: 7,
  // [name, column, row, code] per LED, in OpenRGB's LED order.
  leds: Object.freeze([
    ["A", 2, 4, 0x01],            //   0 KEY_EN_A
    ["B", 7, 5, 0x02],            //   1 KEY_EN_B
    ["C", 4, 5, 0x03],            //   2 KEY_EN_C
    ["D", 4, 4, 0x04],            //   3 KEY_EN_D
    ["E", 4, 3, 0x05],            //   4 KEY_EN_E
    ["F", 5, 4, 0x06],            //   5 KEY_EN_F
    ["G", 7, 4, 0x07],            //   6 KEY_EN_G
    ["H", 8, 4, 0x08],            //   7 KEY_EN_H
    ["I", 10, 3, 0x09],           //   8 KEY_EN_I
    ["J", 9, 4, 0x0A],            //   9 KEY_EN_J
    ["K", 10, 4, 0x0B],           //  10 KEY_EN_K
    ["L", 11, 4, 0x0C],           //  11 KEY_EN_L
    ["M", 10, 5, 0x0D],           //  12 KEY_EN_M
    ["N", 9, 5, 0x0E],            //  13 KEY_EN_N
    ["O", 11, 3, 0x0F],           //  14 KEY_EN_O
    ["P", 12, 3, 0x10],           //  15 KEY_EN_P
    ["Q", 2, 3, 0x11],            //  16 KEY_EN_Q
    ["R", 5, 3, 0x12],            //  17 KEY_EN_R
    ["S", 3, 4, 0x13],            //  18 KEY_EN_S
    ["T", 7, 3, 0x14],            //  19 KEY_EN_T
    ["U", 9, 3, 0x15],            //  20 KEY_EN_U
    ["V", 5, 5, 0x16],            //  21 KEY_EN_V
    ["W", 3, 3, 0x17],            //  22 KEY_EN_W
    ["X", 3, 5, 0x18],            //  23 KEY_EN_X
    ["Y", 8, 3, 0x19],            //  24 KEY_EN_Y
    ["Z", 2, 5, 0x1A],            //  25 KEY_EN_Z
    ["1", 1, 2, 0x1B],            //  26 KEY_EN_1
    ["2", 2, 2, 0x1C],            //  27 KEY_EN_2
    ["3", 3, 2, 0x1D],            //  28 KEY_EN_3
    ["4", 4, 2, 0x1E],            //  29 KEY_EN_4
    ["5", 5, 2, 0x1F],            //  30 KEY_EN_5
    ["6", 6, 2, 0x20],            //  31 KEY_EN_6
    ["7", 7, 2, 0x21],            //  32 KEY_EN_7
    ["8", 8, 2, 0x22],            //  33 KEY_EN_8
    ["9", 9, 2, 0x23],            //  34 KEY_EN_9
    ["0", 10, 2, 0x24],           //  35 KEY_EN_0
    ["Enter", 15, 4, 0x25],       //  36 KEY_EN_ANSI_ENTER
    ["Esc", 0, 1, 0x26],          //  37 KEY_EN_ESCAPE
    ["Backspace", 14, 2, 0x27],   //  38 KEY_EN_BACKSPACE
    ["Tab", 0, 3, 0x28],          //  39 KEY_EN_TAB
    ["Space", 7, 6, 0x29],        //  40 KEY_EN_SPACE
    ["-", 12, 2, 0x2A],           //  41 KEY_EN_MINUS
    ["=", 13, 2, 0x2B],           //  42 KEY_EN_EQUALS
    ["[", 13, 3, 0x2C],           //  43 KEY_EN_LEFT_BRACKET
    ["]", 14, 3, 0x2D],           //  44 KEY_EN_RIGHT_BRACKET
    ["\\", 15, 3, 0x2E],          //  45 KEY_EN_ANSI_BACK_SLASH
    ["ISO_#", 14, 4, 0x2F],       //  46 KEY_EN_POUND
    [";", 12, 4, 0x30],           //  47 KEY_EN_SEMICOLON
    ["'", 13, 4, 0x31],           //  48 KEY_EN_QUOTE
    ["`", 0, 2, 0x32],            //  49 KEY_EN_BACK_TICK
    [",", 11, 5, 0x33],           //  50 KEY_EN_COMMA
    [".", 12, 5, 0x34],           //  51 KEY_EN_PERIOD
    ["/", 13, 5, 0x35],           //  52 KEY_EN_FORWARD_SLASH
    ["Caps Lock", 0, 4, 0x36],    //  53 KEY_EN_CAPS_LOCK
    ["F1", 2, 1, 0x37],           //  54 KEY_EN_F1
    ["F2", 3, 1, 0x38],           //  55 KEY_EN_F2
    ["F3", 4, 1, 0x39],           //  56 KEY_EN_F3
    ["F4", 5, 1, 0x3A],           //  57 KEY_EN_F4
    ["F5", 7, 1, 0x3B],           //  58 KEY_EN_F5
    ["F6", 8, 1, 0x3C],           //  59 KEY_EN_F6
    ["F7", 9, 1, 0x3D],           //  60 KEY_EN_F7
    ["F8", 10, 1, 0x3E],          //  61 KEY_EN_F8
    ["F9", 12, 1, 0x3F],          //  62 KEY_EN_F9
    ["F10", 13, 1, 0x40],         //  63 KEY_EN_F10
    ["F11", 14, 1, 0x41],         //  64 KEY_EN_F11
    ["F12", 15, 1, 0x42],         //  65 KEY_EN_F12
    ["Print Screen", 17, 1, 0x43],//  66 KEY_EN_PRINT_SCREEN
    ["Scroll Lock", 18, 1, 0x44], //  67 KEY_EN_SCROLL_LOCK
    ["Pause Break", 19, 1, 0x45], //  68 KEY_EN_PAUSE_BREAK
    ["Insert", 17, 2, 0x46],      //  69 KEY_EN_INSERT
    ["Home", 18, 2, 0x47],        //  70 KEY_EN_HOME
    ["Page Up", 19, 2, 0x48],     //  71 KEY_EN_PAGE_UP
    ["Del", 17, 3, 0x49],         //  72 KEY_EN_DELETE
    ["End", 18, 3, 0x4A],         //  73 KEY_EN_END
    ["Page Down", 19, 3, 0x4B],   //  74 KEY_EN_PAGE_DOWN
    ["Right Arrow", 19, 6, 0x4C], //  75 KEY_EN_RIGHT_ARROW
    ["Left Arrow", 17, 6, 0x4D],  //  76 KEY_EN_LEFT_ARROW
    ["Down Arrow", 18, 6, 0x4E],  //  77 KEY_EN_DOWN_ARROW
    ["Up Arrow", 18, 5, 0x4F],    //  78 KEY_EN_UP_ARROW
    ["ISO_<", 1, 5, 0x61],        //  79 KEY_EN_ISO_BACK_SLASH
    ["Menu", 14, 6, 0x62],        //  80 KEY_EN_MENU
    ["Left Ctrl", 0, 6, 0x68],    //  81 KEY_EN_LEFT_CONTROL
    ["Left Shift", 0, 5, 0x69],   //  82 KEY_EN_LEFT_SHIFT
    ["Left Alt", 2, 6, 0x6A],     //  83 KEY_EN_LEFT_ALT
    ["Left Win", 1, 6, 0x6B],     //  84 KEY_EN_LEFT_WINDOWS
    ["Right Ctrl", 15, 6, 0x6C],  //  85 KEY_EN_RIGHT_CONTROL
    ["Right Shift", 14, 5, 0x6D], //  86 KEY_EN_RIGHT_SHIFT
    ["Right Alt", 12, 6, 0x6E],   //  87 KEY_EN_RIGHT_ALT
    ["Right Win", 13, 6, 0x6F],   //  88 KEY_EN_RIGHT_WINDOWS
    ["MediaPreviousTrack", 12, 0, 0x9E],//  89 KEY_EN_MEDIA_PREVIOUS
    ["MediaPlayPause", 13, 0, 0x9B],//  90 KEY_EN_MEDIA_PLAY_PAUSE
    ["MediaNextTrack", 14, 0, 0x9D],//  91 KEY_EN_MEDIA_NEXT
    ["AudioMute", 15, 0, 0x9C],   //  92 KEY_EN_MEDIA_MUTE
    ["Logo", 0, 0, 0xD2],         //  93 "Logo"
    ["Brightness", 5, 0, 0x99],   //  94 "Key: Brightness"
  ]),
});
