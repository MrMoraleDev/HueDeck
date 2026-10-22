// SPDX-License-Identifier: GPL-3.0-or-later
//
// HueDeck plugin: EVision V2 keyboard layouts (the BYGG CSB ICL01 320F:5041 and the Endorfy
// Omnis 3299:0012), as data. Generated from the OpenRGB tables; do not edit by hand.
// Adapted from OpenRGB Controllers/EVisionKeyboardController/EVisionV2KeyboardController/
// EVisionV2KeyboardController.cpp (evisionv2_map, endorfy_map) and
// RGBController_EVisionV2Keyboard.cpp (evisionv2_matrix, endorfy_matrix, led_evisionv2,
// led_endorfy) (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: Le Philousophe (25 Dec 2022)
//
// Per LED, in OpenRGB's LED order (the order of led_evisionv2 / led_endorfy):
//   name    the key, in the key names HueDeck plugins use (docs/host-api/hid-usb.md section 6);
//           the comment keeps OpenRGB's KEY_EN_* constant;
//   column, row  the LED's cell in OpenRGB's 21 x 6 matrix map (EVISION_V2_MATRIX_WIDTH x
//           EVISION_V2_MATRIX_HEIGHT), so keys sit where they are on the keyboard;
//   slot    keyvalue_map[led]: the LED's colour goes to bytes 3 * slot .. 3 * slot + 2 of the
//           direct-mode colour buffer (R, G, B).
// The EVision V2 slots are column-major (slot = 6 * column + row); slot 6 is the blank cell
// between Esc and F1 (BLANK_SPACE in EVisionV2KeyboardController.cpp). The Endorfy slots are
// row-major (21 per row), so there slot 6 is F5.

export const EVISION_V2_LAYOUT = Object.freeze({
  name: "EVision V2",
  width: 21,
  height: 6,
  // [name, column, row, slot] per LED, in OpenRGB's LED order; slot = keyvalue_map[led].
  leds: Object.freeze([
    ["Esc", 0, 0, 0],               //   0 KEY_EN_ESCAPE
    ["F1", 2, 0, 12],               //   1 KEY_EN_F1
    ["F2", 3, 0, 18],               //   2 KEY_EN_F2
    ["F3", 4, 0, 24],               //   3 KEY_EN_F3
    ["F4", 5, 0, 30],               //   4 KEY_EN_F4
    ["F5", 6, 0, 36],               //   5 KEY_EN_F5
    ["F6", 7, 0, 42],               //   6 KEY_EN_F6
    ["F7", 8, 0, 48],               //   7 KEY_EN_F7
    ["F8", 9, 0, 54],               //   8 KEY_EN_F8
    ["F9", 10, 0, 60],              //   9 KEY_EN_F9
    ["F10", 11, 0, 66],             //  10 KEY_EN_F10
    ["F11", 12, 0, 72],             //  11 KEY_EN_F11
    ["F12", 13, 0, 78],             //  12 KEY_EN_F12
    ["Print Screen", 14, 0, 84],    //  13 KEY_EN_PRINT_SCREEN
    ["Scroll Lock", 15, 0, 90],     //  14 KEY_EN_SCROLL_LOCK
    ["Pause Break", 16, 0, 96],     //  15 KEY_EN_PAUSE_BREAK
    ["`", 0, 1, 1],                 //  16 KEY_EN_BACK_TICK
    ["1", 1, 1, 7],                 //  17 KEY_EN_1
    ["2", 2, 1, 13],                //  18 KEY_EN_2
    ["3", 3, 1, 19],                //  19 KEY_EN_3
    ["4", 4, 1, 25],                //  20 KEY_EN_4
    ["5", 5, 1, 31],                //  21 KEY_EN_5
    ["6", 6, 1, 37],                //  22 KEY_EN_6
    ["7", 7, 1, 43],                //  23 KEY_EN_7
    ["8", 8, 1, 49],                //  24 KEY_EN_8
    ["9", 9, 1, 55],                //  25 KEY_EN_9
    ["0", 10, 1, 61],               //  26 KEY_EN_0
    ["-", 11, 1, 67],               //  27 KEY_EN_MINUS
    ["=", 12, 1, 73],               //  28 KEY_EN_EQUALS
    ["Backspace", 13, 1, 79],       //  29 KEY_EN_BACKSPACE
    ["Insert", 14, 1, 85],          //  30 KEY_EN_INSERT
    ["Home", 15, 1, 91],            //  31 KEY_EN_HOME
    ["Page Up", 16, 1, 97],         //  32 KEY_EN_PAGE_UP
    ["NumLock", 17, 1, 103],        //  33 KEY_EN_NUMPAD_LOCK
    ["Num /", 18, 1, 109],          //  34 KEY_EN_NUMPAD_DIVIDE
    ["Num *", 19, 1, 115],          //  35 KEY_EN_NUMPAD_TIMES
    ["Num -", 20, 1, 121],          //  36 KEY_EN_NUMPAD_MINUS
    ["Tab", 0, 2, 2],               //  37 KEY_EN_TAB
    ["Q", 1, 2, 8],                 //  38 KEY_EN_Q
    ["W", 2, 2, 14],                //  39 KEY_EN_W
    ["E", 3, 2, 20],                //  40 KEY_EN_E
    ["R", 4, 2, 26],                //  41 KEY_EN_R
    ["T", 5, 2, 32],                //  42 KEY_EN_T
    ["Y", 6, 2, 38],                //  43 KEY_EN_Y
    ["U", 7, 2, 44],                //  44 KEY_EN_U
    ["I", 8, 2, 50],                //  45 KEY_EN_I
    ["O", 9, 2, 56],                //  46 KEY_EN_O
    ["P", 10, 2, 62],               //  47 KEY_EN_P
    ["[", 11, 2, 68],               //  48 KEY_EN_LEFT_BRACKET
    ["]", 12, 2, 74],               //  49 KEY_EN_RIGHT_BRACKET
    ["\\", 13, 2, 80],              //  50 KEY_EN_ANSI_BACK_SLASH
    ["Del", 14, 2, 86],             //  51 KEY_EN_DELETE
    ["End", 15, 2, 92],             //  52 KEY_EN_END
    ["Page Down", 16, 2, 98],       //  53 KEY_EN_PAGE_DOWN
    ["Num 7", 17, 2, 104],          //  54 KEY_EN_NUMPAD_7
    ["Num 8", 18, 2, 110],          //  55 KEY_EN_NUMPAD_8
    ["Num 9", 19, 2, 116],          //  56 KEY_EN_NUMPAD_9
    ["Num +", 20, 2, 122],          //  57 KEY_EN_NUMPAD_PLUS
    ["Caps Lock", 0, 3, 3],         //  58 KEY_EN_CAPS_LOCK
    ["A", 1, 3, 9],                 //  59 KEY_EN_A
    ["S", 2, 3, 15],                //  60 KEY_EN_S
    ["D", 3, 3, 21],                //  61 KEY_EN_D
    ["F", 4, 3, 27],                //  62 KEY_EN_F
    ["G", 5, 3, 33],                //  63 KEY_EN_G
    ["H", 6, 3, 39],                //  64 KEY_EN_H
    ["J", 7, 3, 45],                //  65 KEY_EN_J
    ["K", 8, 3, 51],                //  66 KEY_EN_K
    ["L", 9, 3, 57],                //  67 KEY_EN_L
    [";", 10, 3, 63],               //  68 KEY_EN_SEMICOLON
    ["'", 11, 3, 69],               //  69 KEY_EN_QUOTE
    ["ISO_#", 12, 3, 75],           //  70 KEY_EN_POUND
    ["Enter", 13, 3, 81],           //  71 KEY_EN_ISO_ENTER
    ["Num 4", 17, 3, 105],          //  72 KEY_EN_NUMPAD_4
    ["Num 5", 18, 3, 111],          //  73 KEY_EN_NUMPAD_5
    ["Num 6", 19, 3, 117],          //  74 KEY_EN_NUMPAD_6
    ["Left Shift", 0, 4, 4],        //  75 KEY_EN_LEFT_SHIFT
    ["ISO_<", 1, 4, 10],            //  76 KEY_EN_ISO_BACK_SLASH
    ["Z", 2, 4, 16],                //  77 KEY_EN_Z
    ["X", 3, 4, 22],                //  78 KEY_EN_X
    ["C", 4, 4, 28],                //  79 KEY_EN_C
    ["V", 5, 4, 34],                //  80 KEY_EN_V
    ["B", 6, 4, 40],                //  81 KEY_EN_B
    ["N", 7, 4, 46],                //  82 KEY_EN_N
    ["M", 8, 4, 52],                //  83 KEY_EN_M
    [",", 9, 4, 58],                //  84 KEY_EN_COMMA
    [".", 10, 4, 64],               //  85 KEY_EN_PERIOD
    ["/", 11, 4, 70],               //  86 KEY_EN_FORWARD_SLASH
    ["Right Shift", 13, 4, 82],     //  87 KEY_EN_RIGHT_SHIFT
    ["Up Arrow", 15, 4, 94],        //  88 KEY_EN_UP_ARROW
    ["Num 1", 17, 4, 106],          //  89 KEY_EN_NUMPAD_1
    ["Num 2", 18, 4, 112],          //  90 KEY_EN_NUMPAD_2
    ["Num 3", 19, 4, 118],          //  91 KEY_EN_NUMPAD_3
    ["Num Enter", 20, 4, 124],      //  92 KEY_EN_NUMPAD_ENTER
    ["Left Ctrl", 0, 5, 5],         //  93 KEY_EN_LEFT_CONTROL
    ["Left Win", 1, 5, 11],         //  94 KEY_EN_LEFT_WINDOWS
    ["Left Alt", 2, 5, 17],         //  95 KEY_EN_LEFT_ALT
    ["Space", 6, 5, 41],            //  96 KEY_EN_SPACE
    ["Right Alt", 10, 5, 65],       //  97 KEY_EN_RIGHT_ALT
    ["Right Win", 11, 5, 71],       //  98 KEY_EN_RIGHT_WINDOWS
    ["Fn", 12, 5, 77],              //  99 KEY_EN_RIGHT_FUNCTION
    ["Right Ctrl", 13, 5, 83],      // 100 KEY_EN_RIGHT_CONTROL
    ["Left Arrow", 14, 5, 89],      // 101 KEY_EN_LEFT_ARROW
    ["Down Arrow", 15, 5, 95],      // 102 KEY_EN_DOWN_ARROW
    ["Right Arrow", 16, 5, 101],    // 103 KEY_EN_RIGHT_ARROW
    ["Num 0", 18, 5, 113],          // 104 KEY_EN_NUMPAD_0
    ["Num .", 19, 5, 119],          // 105 KEY_EN_NUMPAD_PERIOD
  ]),
});

export const ENDORFY_LAYOUT = Object.freeze({
  name: "Endorfy",
  width: 21,
  height: 6,
  // [name, column, row, slot] per LED, in OpenRGB's LED order; slot = keyvalue_map[led].
  leds: Object.freeze([
    ["Esc", 0, 0, 0],               //   0 KEY_EN_ESCAPE
    ["F1", 2, 0, 2],                //   1 KEY_EN_F1
    ["F2", 3, 0, 3],                //   2 KEY_EN_F2
    ["F3", 4, 0, 4],                //   3 KEY_EN_F3
    ["F4", 5, 0, 5],                //   4 KEY_EN_F4
    ["F5", 6, 0, 6],                //   5 KEY_EN_F5
    ["F6", 7, 0, 7],                //   6 KEY_EN_F6
    ["F7", 8, 0, 8],                //   7 KEY_EN_F7
    ["F8", 9, 0, 9],                //   8 KEY_EN_F8
    ["F9", 10, 0, 10],              //   9 KEY_EN_F9
    ["F10", 11, 0, 11],             //  10 KEY_EN_F10
    ["F11", 12, 0, 12],             //  11 KEY_EN_F11
    ["F12", 13, 0, 13],             //  12 KEY_EN_F12
    ["Print Screen", 14, 0, 14],    //  13 KEY_EN_PRINT_SCREEN
    ["Scroll Lock", 15, 0, 15],     //  14 KEY_EN_SCROLL_LOCK
    ["Pause Break", 16, 0, 16],     //  15 KEY_EN_PAUSE_BREAK
    ["`", 0, 1, 21],                //  16 KEY_EN_BACK_TICK
    ["1", 1, 1, 22],                //  17 KEY_EN_1
    ["2", 2, 1, 23],                //  18 KEY_EN_2
    ["3", 3, 1, 24],                //  19 KEY_EN_3
    ["4", 4, 1, 25],                //  20 KEY_EN_4
    ["5", 5, 1, 26],                //  21 KEY_EN_5
    ["6", 6, 1, 27],                //  22 KEY_EN_6
    ["7", 7, 1, 28],                //  23 KEY_EN_7
    ["8", 8, 1, 29],                //  24 KEY_EN_8
    ["9", 9, 1, 30],                //  25 KEY_EN_9
    ["0", 10, 1, 31],               //  26 KEY_EN_0
    ["-", 11, 1, 32],               //  27 KEY_EN_MINUS
    ["=", 12, 1, 33],               //  28 KEY_EN_EQUALS
    ["Backspace", 13, 1, 34],       //  29 KEY_EN_BACKSPACE
    ["Insert", 14, 1, 35],          //  30 KEY_EN_INSERT
    ["Home", 15, 1, 36],            //  31 KEY_EN_HOME
    ["Page Up", 16, 1, 37],         //  32 KEY_EN_PAGE_UP
    ["NumLock", 17, 1, 38],         //  33 KEY_EN_NUMPAD_LOCK
    ["Num /", 18, 1, 39],           //  34 KEY_EN_NUMPAD_DIVIDE
    ["Num *", 19, 1, 40],           //  35 KEY_EN_NUMPAD_TIMES
    ["Num -", 20, 1, 41],           //  36 KEY_EN_NUMPAD_MINUS
    ["Tab", 0, 2, 42],              //  37 KEY_EN_TAB
    ["Q", 1, 2, 43],                //  38 KEY_EN_Q
    ["W", 2, 2, 44],                //  39 KEY_EN_W
    ["E", 3, 2, 45],                //  40 KEY_EN_E
    ["R", 4, 2, 46],                //  41 KEY_EN_R
    ["T", 5, 2, 47],                //  42 KEY_EN_T
    ["Y", 6, 2, 48],                //  43 KEY_EN_Y
    ["U", 7, 2, 49],                //  44 KEY_EN_U
    ["I", 8, 2, 50],                //  45 KEY_EN_I
    ["O", 9, 2, 51],                //  46 KEY_EN_O
    ["P", 10, 2, 52],               //  47 KEY_EN_P
    ["[", 11, 2, 53],               //  48 KEY_EN_LEFT_BRACKET
    ["]", 12, 2, 54],               //  49 KEY_EN_RIGHT_BRACKET
    ["\\", 13, 2, 55],              //  50 KEY_EN_ANSI_BACK_SLASH
    ["Del", 14, 2, 56],             //  51 KEY_EN_DELETE
    ["End", 15, 2, 57],             //  52 KEY_EN_END
    ["Page Down", 16, 2, 58],       //  53 KEY_EN_PAGE_DOWN
    ["Num 7", 17, 2, 59],           //  54 KEY_EN_NUMPAD_7
    ["Num 8", 18, 2, 60],           //  55 KEY_EN_NUMPAD_8
    ["Num 9", 19, 2, 61],           //  56 KEY_EN_NUMPAD_9
    ["Num +", 20, 2, 62],           //  57 KEY_EN_NUMPAD_PLUS
    ["Caps Lock", 0, 3, 63],        //  58 KEY_EN_CAPS_LOCK
    ["A", 1, 3, 64],                //  59 KEY_EN_A
    ["S", 2, 3, 65],                //  60 KEY_EN_S
    ["D", 3, 3, 66],                //  61 KEY_EN_D
    ["F", 4, 3, 67],                //  62 KEY_EN_F
    ["G", 5, 3, 68],                //  63 KEY_EN_G
    ["H", 6, 3, 69],                //  64 KEY_EN_H
    ["J", 7, 3, 70],                //  65 KEY_EN_J
    ["K", 8, 3, 71],                //  66 KEY_EN_K
    ["L", 9, 3, 72],                //  67 KEY_EN_L
    [";", 10, 3, 73],               //  68 KEY_EN_SEMICOLON
    ["'", 11, 3, 74],               //  69 KEY_EN_QUOTE
    ["Enter", 13, 3, 76],           //  70 KEY_EN_ANSI_ENTER
    ["Num 4", 17, 3, 80],           //  71 KEY_EN_NUMPAD_4
    ["Num 5", 18, 3, 81],           //  72 KEY_EN_NUMPAD_5
    ["Num 6", 19, 3, 82],           //  73 KEY_EN_NUMPAD_6
    ["Left Shift", 0, 4, 84],       //  74 KEY_EN_LEFT_SHIFT
    ["Z", 2, 4, 86],                //  75 KEY_EN_Z
    ["X", 3, 4, 87],                //  76 KEY_EN_X
    ["C", 4, 4, 88],                //  77 KEY_EN_C
    ["V", 5, 4, 89],                //  78 KEY_EN_V
    ["B", 6, 4, 90],                //  79 KEY_EN_B
    ["N", 7, 4, 91],                //  80 KEY_EN_N
    ["M", 8, 4, 92],                //  81 KEY_EN_M
    [",", 9, 4, 93],                //  82 KEY_EN_COMMA
    [".", 10, 4, 94],               //  83 KEY_EN_PERIOD
    ["/", 11, 4, 95],               //  84 KEY_EN_FORWARD_SLASH
    ["Right Shift", 12, 4, 97],     //  85 KEY_EN_RIGHT_SHIFT
    ["Up Arrow", 15, 4, 99],        //  86 KEY_EN_UP_ARROW
    ["Num 1", 17, 4, 101],          //  87 KEY_EN_NUMPAD_1
    ["Num 2", 18, 4, 102],          //  88 KEY_EN_NUMPAD_2
    ["Num 3", 19, 4, 103],          //  89 KEY_EN_NUMPAD_3
    ["Num Enter", 20, 4, 104],      //  90 KEY_EN_NUMPAD_ENTER
    ["Left Ctrl", 0, 5, 105],       //  91 KEY_EN_LEFT_CONTROL
    ["Left Win", 1, 5, 106],        //  92 KEY_EN_LEFT_WINDOWS
    ["Left Alt", 2, 5, 107],        //  93 KEY_EN_LEFT_ALT
    ["Space", 6, 5, 111],           //  94 KEY_EN_SPACE
    ["Right Alt", 10, 5, 115],      //  95 KEY_EN_RIGHT_ALT
    ["Fn", 11, 5, 116],             //  96 KEY_EN_RIGHT_FUNCTION
    ["Menu", 12, 5, 117],           //  97 KEY_EN_MENU
    ["Right Ctrl", 13, 5, 118],     //  98 KEY_EN_RIGHT_CONTROL
    ["Left Arrow", 14, 5, 119],     //  99 KEY_EN_LEFT_ARROW
    ["Down Arrow", 15, 5, 120],     // 100 KEY_EN_DOWN_ARROW
    ["Right Arrow", 16, 5, 121],    // 101 KEY_EN_RIGHT_ARROW
    ["Num 0", 17, 5, 123],          // 102 KEY_EN_NUMPAD_0
    ["Num .", 19, 5, 124],          // 103 KEY_EN_NUMPAD_PERIOD
  ]),
});
