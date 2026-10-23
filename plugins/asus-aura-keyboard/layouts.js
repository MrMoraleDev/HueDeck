// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: ASUS Aura keyboard layouts, as data. Generated from the OpenRGB tables; do not
// edit by hand.
// Adapted from OpenRGB Controllers/AsusAuraUSBController/AsusAuraTUFKeyboardController/
// AsusAuraTUFKeyboardLayouts.h ("Mola19  02 Nov 2021") and AsusAuraKeyboardController/
// RGBController_AsusAuraKeyboard.cpp ("Adam Honse (CalcProgrammer1)  19 Mar 2020": scope_tkl_matrix_map,
// default_tkl_led_names), and RGBController/RGBControllerKeyNames.cpp ("Chris M (Dr_No)  25 Jan 2022")
// for key names Lightloom has no name of its own for (GPL-2.0-or-later) at commit
// d4798c120659ec5ba2e664a9e87bedff8d36f880, https://gitlab.com/CalcProgrammer1/OpenRGB
//
// Per layout: width x height = OpenRGB's matrix map (cols x rows); leds = [name, column, row, id]
// in OpenRGB's LED order, where (column, row) is the matrix cell holding the LED's index and id is
// the LED value the keyboard is sent (led_value.id). The comment keeps OpenRGB's key constant.

/* eslint-disable */

// TUF-protocol keyboards: layout maps keyed by OpenRGB's layout number (ASUS_TUF_K7_LAYOUT_*).
export const TUF_LAYOUTS = Object.freeze({
  AsusTUFK7Layouts: Object.freeze({
    // ASUS_TUF_K7_LAYOUT_UK, 105 LEDs, matrix ASUS_TUF_K7_LAYOUT_KEYS_ISO
    4: Object.freeze({
      width: 24,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["1", 1, 1, 0x11],                //   6 KEY_EN_1
        ["ISO_<", 1, 4, 0x0C],            //   7 KEY_EN_ISO_BACK_SLASH
        ["Left Win", 1, 5, 0x0D],         //   8 KEY_EN_LEFT_WINDOWS
        ["F1", 2, 0, 0x18],               //   9 KEY_EN_F1
        ["2", 2, 1, 0x19],                //  10 KEY_EN_2
        ["Q", 2, 2, 0x12],                //  11 KEY_EN_Q
        ["A", 2, 3, 0x13],                //  12 KEY_EN_A
        ["Z", 2, 4, 0x14],                //  13 KEY_EN_Z
        ["Left Alt", 2, 5, 0x15],         //  14 KEY_EN_LEFT_ALT
        ["F2", 3, 0, 0x20],               //  15 KEY_EN_F2
        ["3", 3, 1, 0x21],                //  16 KEY_EN_3
        ["W", 3, 2, 0x1A],                //  17 KEY_EN_W
        ["S", 3, 3, 0x1B],                //  18 KEY_EN_S
        ["X", 3, 4, 0x1C],                //  19 KEY_EN_X
        ["F3", 4, 0, 0x28],               //  20 KEY_EN_F3
        ["4", 4, 1, 0x29],                //  21 KEY_EN_4
        ["E", 4, 2, 0x22],                //  22 KEY_EN_E
        ["D", 4, 3, 0x23],                //  23 KEY_EN_D
        ["C", 4, 4, 0x24],                //  24 KEY_EN_C
        ["F4", 5, 0, 0x30],               //  25 KEY_EN_F4
        ["5", 5, 1, 0x31],                //  26 KEY_EN_5
        ["R", 5, 2, 0x2A],                //  27 KEY_EN_R
        ["F", 5, 3, 0x2B],                //  28 KEY_EN_F
        ["V", 5, 4, 0x2C],                //  29 KEY_EN_V
        ["6", 6, 1, 0x39],                //  30 KEY_EN_6
        ["T", 6, 2, 0x32],                //  31 KEY_EN_T
        ["G", 6, 3, 0x33],                //  32 KEY_EN_G
        ["B", 6, 4, 0x34],                //  33 KEY_EN_B
        ["Space", 6, 5, 0x35],            //  34 KEY_EN_SPACE
        ["F5", 7, 0, 0x40],               //  35 KEY_EN_F5
        ["7", 7, 1, 0x41],                //  36 KEY_EN_7
        ["Y", 7, 2, 0x3A],                //  37 KEY_EN_Y
        ["H", 7, 3, 0x3B],                //  38 KEY_EN_H
        ["N", 7, 4, 0x3C],                //  39 KEY_EN_N
        ["F6", 8, 0, 0x48],               //  40 KEY_EN_F6
        ["8", 8, 1, 0x49],                //  41 KEY_EN_8
        ["U", 8, 2, 0x42],                //  42 KEY_EN_U
        ["J", 8, 3, 0x43],                //  43 KEY_EN_J
        ["M", 8, 4, 0x44],                //  44 KEY_EN_M
        ["F7", 9, 0, 0x50],               //  45 KEY_EN_F7
        ["9", 9, 1, 0x51],                //  46 KEY_EN_9
        ["I", 9, 2, 0x4A],                //  47 KEY_EN_I
        ["K", 9, 3, 0x4B],                //  48 KEY_EN_K
        [",", 9, 4, 0x4C],                //  49 KEY_EN_COMMA
        ["F8", 10, 0, 0x58],              //  50 KEY_EN_F8
        ["0", 10, 1, 0x59],               //  51 KEY_EN_0
        ["O", 10, 2, 0x52],               //  52 KEY_EN_O
        ["L", 10, 3, 0x53],               //  53 KEY_EN_L
        [".", 10, 4, 0x54],               //  54 KEY_EN_PERIOD
        ["Right Alt", 10, 5, 0x4D],       //  55 KEY_EN_RIGHT_ALT
        ["F9", 11, 0, 0x60],              //  56 KEY_EN_F9
        ["-", 11, 1, 0x61],               //  57 KEY_EN_MINUS
        ["P", 11, 2, 0x5A],               //  58 KEY_EN_P
        [";", 11, 3, 0x5B],               //  59 KEY_EN_SEMICOLON
        ["/", 11, 4, 0x5C],               //  60 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 5, 0x5D],              //  61 KEY_EN_RIGHT_FUNCTION
        ["F10", 12, 0, 0x68],             //  62 KEY_EN_F10
        ["=", 12, 1, 0x69],               //  63 KEY_EN_EQUALS
        ["[", 12, 2, 0x62],               //  64 KEY_EN_LEFT_BRACKET
        ["'", 12, 3, 0x63],               //  65 KEY_EN_QUOTE
        ["Menu", 12, 5, 0x65],            //  66 KEY_EN_MENU
        ["F11", 13, 0, 0x70],             //  67 KEY_EN_F11
        ["Backspace", 13, 1, 0x79],       //  68 KEY_EN_BACKSPACE
        ["]", 13, 2, 0x6A],               //  69 KEY_EN_RIGHT_BRACKET
        ["ISO_#", 13, 3, 0x6B],           //  70 KEY_EN_POUND
        ["Right Shift", 13, 4, 0x7C],     //  71 KEY_EN_RIGHT_SHIFT
        ["F12", 14, 0, 0x78],             //  72 KEY_EN_F12
        ["Enter", 14, 3, 0x7B],           //  73 KEY_EN_ISO_ENTER
        ["Right Ctrl", 14, 5, 0x7D],      //  74 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 16, 0, 0x80],    //  75 KEY_EN_PRINT_SCREEN
        ["Insert", 16, 1, 0x81],          //  76 KEY_EN_INSERT
        ["Del", 16, 2, 0x82],             //  77 KEY_EN_DELETE
        ["Left Arrow", 16, 5, 0x85],      //  78 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 17, 0, 0x88],     //  79 KEY_EN_SCROLL_LOCK
        ["Home", 17, 1, 0x89],            //  80 KEY_EN_HOME
        ["End", 17, 2, 0x8A],             //  81 KEY_EN_END
        ["Up Arrow", 17, 4, 0x8C],        //  82 KEY_EN_UP_ARROW
        ["Down Arrow", 17, 5, 0x8D],      //  83 KEY_EN_DOWN_ARROW
        ["Pause Break", 18, 0, 0x90],     //  84 KEY_EN_PAUSE_BREAK
        ["Page Up", 18, 1, 0x91],         //  85 KEY_EN_PAGE_UP
        ["Page Down", 18, 2, 0x92],       //  86 KEY_EN_PAGE_DOWN
        ["Right Arrow", 18, 5, 0x95],     //  87 KEY_EN_RIGHT_ARROW
        ["NumLock", 20, 1, 0x99],         //  88 KEY_EN_NUMPAD_LOCK
        ["Num 7", 20, 2, 0x9A],           //  89 KEY_EN_NUMPAD_7
        ["Num 4", 20, 3, 0x9B],           //  90 KEY_EN_NUMPAD_4
        ["Num 1", 20, 4, 0x9C],           //  91 KEY_EN_NUMPAD_1
        ["Num 0", 20, 5, 0x9D],           //  92 KEY_EN_NUMPAD_0
        ["Num /", 21, 1, 0xA1],           //  93 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 21, 2, 0xA2],           //  94 KEY_EN_NUMPAD_8
        ["Num 5", 21, 3, 0xA3],           //  95 KEY_EN_NUMPAD_5
        ["Num 2", 21, 4, 0xA4],           //  96 KEY_EN_NUMPAD_2
        ["Num *", 22, 1, 0xA9],           //  97 KEY_EN_NUMPAD_TIMES
        ["Num 9", 22, 2, 0xAA],           //  98 KEY_EN_NUMPAD_9
        ["Num 6", 22, 3, 0xAB],           //  99 KEY_EN_NUMPAD_6
        ["Num 3", 22, 4, 0xAC],           // 100 KEY_EN_NUMPAD_3
        ["Num .", 22, 5, 0xAD],           // 101 KEY_EN_NUMPAD_PERIOD
        ["Num -", 23, 1, 0xB1],           // 102 KEY_EN_NUMPAD_MINUS
        ["Num +", 23, 2, 0xB2],           // 103 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 23, 4, 0xB4],       // 104 KEY_EN_NUMPAD_ENTER
      ]),
    }),
    // ASUS_TUF_K7_LAYOUT_US, 104 LEDs, matrix ASUS_TUF_K7_LAYOUT_KEYS_ANSI
    17: Object.freeze({
      width: 24,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["1", 1, 1, 0x11],                //   6 KEY_EN_1
        ["Left Win", 1, 5, 0x0D],         //   7 KEY_EN_LEFT_WINDOWS
        ["F1", 2, 0, 0x18],               //   8 KEY_EN_F1
        ["2", 2, 1, 0x19],                //   9 KEY_EN_2
        ["Q", 2, 2, 0x12],                //  10 KEY_EN_Q
        ["A", 2, 3, 0x13],                //  11 KEY_EN_A
        ["Z", 2, 4, 0x14],                //  12 KEY_EN_Z
        ["Left Alt", 2, 5, 0x15],         //  13 KEY_EN_LEFT_ALT
        ["F2", 3, 0, 0x20],               //  14 KEY_EN_F2
        ["3", 3, 1, 0x21],                //  15 KEY_EN_3
        ["W", 3, 2, 0x1A],                //  16 KEY_EN_W
        ["S", 3, 3, 0x1B],                //  17 KEY_EN_S
        ["X", 3, 4, 0x1C],                //  18 KEY_EN_X
        ["F3", 4, 0, 0x28],               //  19 KEY_EN_F3
        ["4", 4, 1, 0x29],                //  20 KEY_EN_4
        ["E", 4, 2, 0x22],                //  21 KEY_EN_E
        ["D", 4, 3, 0x23],                //  22 KEY_EN_D
        ["C", 4, 4, 0x24],                //  23 KEY_EN_C
        ["F4", 5, 0, 0x30],               //  24 KEY_EN_F4
        ["5", 5, 1, 0x31],                //  25 KEY_EN_5
        ["R", 5, 2, 0x2A],                //  26 KEY_EN_R
        ["F", 5, 3, 0x2B],                //  27 KEY_EN_F
        ["V", 5, 4, 0x2C],                //  28 KEY_EN_V
        ["6", 6, 1, 0x39],                //  29 KEY_EN_6
        ["T", 6, 2, 0x32],                //  30 KEY_EN_T
        ["G", 6, 3, 0x33],                //  31 KEY_EN_G
        ["B", 6, 4, 0x34],                //  32 KEY_EN_B
        ["Space", 6, 5, 0x35],            //  33 KEY_EN_SPACE
        ["F5", 7, 0, 0x40],               //  34 KEY_EN_F5
        ["7", 7, 1, 0x41],                //  35 KEY_EN_7
        ["Y", 7, 2, 0x3A],                //  36 KEY_EN_Y
        ["H", 7, 3, 0x3B],                //  37 KEY_EN_H
        ["N", 7, 4, 0x3C],                //  38 KEY_EN_N
        ["F6", 8, 0, 0x48],               //  39 KEY_EN_F6
        ["8", 8, 1, 0x49],                //  40 KEY_EN_8
        ["U", 8, 2, 0x42],                //  41 KEY_EN_U
        ["J", 8, 3, 0x43],                //  42 KEY_EN_J
        ["M", 8, 4, 0x44],                //  43 KEY_EN_M
        ["F7", 9, 0, 0x50],               //  44 KEY_EN_F7
        ["9", 9, 1, 0x51],                //  45 KEY_EN_9
        ["I", 9, 2, 0x4A],                //  46 KEY_EN_I
        ["K", 9, 3, 0x4B],                //  47 KEY_EN_K
        [",", 9, 4, 0x4C],                //  48 KEY_EN_COMMA
        ["F8", 10, 0, 0x58],              //  49 KEY_EN_F8
        ["0", 10, 1, 0x59],               //  50 KEY_EN_0
        ["O", 10, 2, 0x52],               //  51 KEY_EN_O
        ["L", 10, 3, 0x53],               //  52 KEY_EN_L
        [".", 10, 4, 0x54],               //  53 KEY_EN_PERIOD
        ["Right Alt", 10, 5, 0x4D],       //  54 KEY_EN_RIGHT_ALT
        ["F9", 11, 0, 0x60],              //  55 KEY_EN_F9
        ["-", 11, 1, 0x61],               //  56 KEY_EN_MINUS
        ["P", 11, 2, 0x5A],               //  57 KEY_EN_P
        [";", 11, 3, 0x5B],               //  58 KEY_EN_SEMICOLON
        ["/", 11, 4, 0x5C],               //  59 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 5, 0x5D],              //  60 KEY_EN_RIGHT_FUNCTION
        ["F10", 12, 0, 0x68],             //  61 KEY_EN_F10
        ["=", 12, 1, 0x69],               //  62 KEY_EN_EQUALS
        ["[", 12, 2, 0x62],               //  63 KEY_EN_LEFT_BRACKET
        ["'", 12, 3, 0x63],               //  64 KEY_EN_QUOTE
        ["Menu", 12, 5, 0x65],            //  65 KEY_EN_MENU
        ["F11", 13, 0, 0x70],             //  66 KEY_EN_F11
        ["Backspace", 13, 1, 0x79],       //  67 KEY_EN_BACKSPACE
        ["]", 13, 2, 0x6A],               //  68 KEY_EN_RIGHT_BRACKET
        ["Right Shift", 13, 4, 0x7C],     //  69 KEY_EN_RIGHT_SHIFT
        ["F12", 14, 0, 0x78],             //  70 KEY_EN_F12
        ["\\", 14, 2, 0x7A],              //  71 KEY_EN_ANSI_BACK_SLASH
        ["Enter", 14, 3, 0x7B],           //  72 KEY_EN_ANSI_ENTER
        ["Right Ctrl", 14, 5, 0x7D],      //  73 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 16, 0, 0x80],    //  74 KEY_EN_PRINT_SCREEN
        ["Insert", 16, 1, 0x81],          //  75 KEY_EN_INSERT
        ["Del", 16, 2, 0x82],             //  76 KEY_EN_DELETE
        ["Left Arrow", 16, 5, 0x85],      //  77 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 17, 0, 0x88],     //  78 KEY_EN_SCROLL_LOCK
        ["Home", 17, 1, 0x89],            //  79 KEY_EN_HOME
        ["End", 17, 2, 0x8A],             //  80 KEY_EN_END
        ["Up Arrow", 17, 4, 0x8C],        //  81 KEY_EN_UP_ARROW
        ["Down Arrow", 17, 5, 0x8D],      //  82 KEY_EN_DOWN_ARROW
        ["Pause Break", 18, 0, 0x90],     //  83 KEY_EN_PAUSE_BREAK
        ["Page Up", 18, 1, 0x91],         //  84 KEY_EN_PAGE_UP
        ["Page Down", 18, 2, 0x92],       //  85 KEY_EN_PAGE_DOWN
        ["Right Arrow", 18, 5, 0x95],     //  86 KEY_EN_RIGHT_ARROW
        ["NumLock", 20, 1, 0x99],         //  87 KEY_EN_NUMPAD_LOCK
        ["Num 7", 20, 2, 0x9A],           //  88 KEY_EN_NUMPAD_7
        ["Num 4", 20, 3, 0x9B],           //  89 KEY_EN_NUMPAD_4
        ["Num 1", 20, 4, 0x9C],           //  90 KEY_EN_NUMPAD_1
        ["Num 0", 20, 5, 0x9D],           //  91 KEY_EN_NUMPAD_0
        ["Num /", 21, 1, 0xA1],           //  92 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 21, 2, 0xA2],           //  93 KEY_EN_NUMPAD_8
        ["Num 5", 21, 3, 0xA3],           //  94 KEY_EN_NUMPAD_5
        ["Num 2", 21, 4, 0xA4],           //  95 KEY_EN_NUMPAD_2
        ["Num *", 22, 1, 0xA9],           //  96 KEY_EN_NUMPAD_TIMES
        ["Num 9", 22, 2, 0xAA],           //  97 KEY_EN_NUMPAD_9
        ["Num 6", 22, 3, 0xAB],           //  98 KEY_EN_NUMPAD_6
        ["Num 3", 22, 4, 0xAC],           //  99 KEY_EN_NUMPAD_3
        ["Num .", 22, 5, 0xAD],           // 100 KEY_EN_NUMPAD_PERIOD
        ["Num -", 23, 1, 0xB1],           // 101 KEY_EN_NUMPAD_MINUS
        ["Num +", 23, 2, 0xB2],           // 102 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 23, 4, 0xB4],       // 103 KEY_EN_NUMPAD_ENTER
      ]),
    }),
  }),
  AsusROGAzothLayouts: Object.freeze({
    // ASUS_TUF_K7_LAYOUT_UK, 82 LEDs, matrix ASUS_ROG_AZOTH_LAYOUT_KEYS_UK
    4: Object.freeze({
      width: 16,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["F1", 1, 0, 0x08],               //   6 KEY_EN_F1
        ["1", 1, 1, 0x09],                //   7 KEY_EN_1
        ["ISO_<", 1, 4, 0x0C],            //   8 KEY_EN_ISO_BACK_SLASH
        ["Left Win", 1, 5, 0x0D],         //   9 KEY_EN_LEFT_WINDOWS
        ["F2", 2, 0, 0x10],               //  10 KEY_EN_F2
        ["2", 2, 1, 0x11],                //  11 KEY_EN_2
        ["Q", 2, 2, 0x0A],                //  12 KEY_EN_Q
        ["A", 2, 3, 0x0B],                //  13 KEY_EN_A
        ["Z", 2, 4, 0x14],                //  14 KEY_EN_Z
        ["Left Alt", 2, 5, 0x15],         //  15 KEY_EN_LEFT_ALT
        ["F3", 3, 0, 0x18],               //  16 KEY_EN_F3
        ["3", 3, 1, 0x19],                //  17 KEY_EN_3
        ["W", 3, 2, 0x12],                //  18 KEY_EN_W
        ["S", 3, 3, 0x13],                //  19 KEY_EN_S
        ["X", 3, 4, 0x1C],                //  20 KEY_EN_X
        ["F4", 4, 0, 0x20],               //  21 KEY_EN_F4
        ["4", 4, 1, 0x21],                //  22 KEY_EN_4
        ["E", 4, 2, 0x1A],                //  23 KEY_EN_E
        ["D", 4, 3, 0x1B],                //  24 KEY_EN_D
        ["C", 4, 4, 0x24],                //  25 KEY_EN_C
        ["F5", 5, 0, 0x28],               //  26 KEY_EN_F5
        ["5", 5, 1, 0x29],                //  27 KEY_EN_5
        ["R", 5, 2, 0x22],                //  28 KEY_EN_R
        ["F", 5, 3, 0x23],                //  29 KEY_EN_F
        ["V", 5, 4, 0x2C],                //  30 KEY_EN_V
        ["F6", 6, 0, 0x30],               //  31 KEY_EN_F6
        ["6", 6, 1, 0x31],                //  32 KEY_EN_6
        ["T", 6, 2, 0x2A],                //  33 KEY_EN_T
        ["G", 6, 3, 0x2B],                //  34 KEY_EN_G
        ["B", 6, 4, 0x34],                //  35 KEY_EN_B
        ["Space", 6, 5, 0x35],            //  36 KEY_EN_SPACE
        ["F7", 7, 0, 0x38],               //  37 KEY_EN_F7
        ["7", 7, 1, 0x39],                //  38 KEY_EN_7
        ["Y", 7, 2, 0x32],                //  39 KEY_EN_Y
        ["H", 7, 3, 0x33],                //  40 KEY_EN_H
        ["N", 7, 4, 0x3C],                //  41 KEY_EN_N
        ["F8", 8, 0, 0x40],               //  42 KEY_EN_F8
        ["8", 8, 1, 0x41],                //  43 KEY_EN_8
        ["U", 8, 2, 0x3A],                //  44 KEY_EN_U
        ["J", 8, 3, 0x3B],                //  45 KEY_EN_J
        ["M", 8, 4, 0x44],                //  46 KEY_EN_M
        ["F9", 9, 0, 0x48],               //  47 KEY_EN_F9
        ["9", 9, 1, 0x49],                //  48 KEY_EN_9
        ["I", 9, 2, 0x42],                //  49 KEY_EN_I
        ["K", 9, 3, 0x43],                //  50 KEY_EN_K
        [",", 9, 4, 0x4C],                //  51 KEY_EN_COMMA
        ["F10", 10, 0, 0x50],             //  52 KEY_EN_F10
        ["0", 10, 1, 0x51],               //  53 KEY_EN_0
        ["O", 10, 2, 0x4A],               //  54 KEY_EN_O
        ["L", 10, 3, 0x4B],               //  55 KEY_EN_L
        [".", 10, 4, 0x54],               //  56 KEY_EN_PERIOD
        ["Right Alt", 10, 5, 0x55],       //  57 KEY_EN_RIGHT_ALT
        ["F11", 11, 0, 0x58],             //  58 KEY_EN_F11
        ["-", 11, 1, 0x59],               //  59 KEY_EN_MINUS
        ["P", 11, 2, 0x52],               //  60 KEY_EN_P
        [";", 11, 3, 0x53],               //  61 KEY_EN_SEMICOLON
        ["/", 11, 4, 0x5C],               //  62 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 5, 0x5D],              //  63 KEY_EN_RIGHT_FUNCTION
        ["F12", 12, 0, 0x60],             //  64 KEY_EN_F12
        ["=", 12, 1, 0x61],               //  65 KEY_EN_EQUALS
        ["[", 12, 2, 0x5A],               //  66 KEY_EN_LEFT_BRACKET
        ["'", 12, 3, 0x5B],               //  67 KEY_EN_QUOTE
        ["Right Shift", 12, 4, 0x64],     //  68 KEY_EN_RIGHT_SHIFT
        ["Right Ctrl", 12, 5, 0x65],      //  69 KEY_EN_RIGHT_CONTROL
        ["Backspace", 13, 1, 0x69],       //  70 KEY_EN_BACKSPACE
        ["]", 13, 2, 0x62],               //  71 KEY_EN_RIGHT_BRACKET
        ["ISO_#", 13, 3, 0x6B],           //  72 KEY_EN_POUND
        ["Left Arrow", 13, 5, 0x6D],      //  73 KEY_EN_LEFT_ARROW
        ["Enter", 14, 3, 0x6A],           //  74 KEY_EN_ISO_ENTER
        ["Up Arrow", 14, 4, 0x74],        //  75 KEY_EN_UP_ARROW
        ["Down Arrow", 14, 5, 0x75],      //  76 KEY_EN_DOWN_ARROW
        ["Insert", 15, 1, 0x79],          //  77 KEY_EN_INSERT
        ["Del", 15, 2, 0x7A],             //  78 KEY_EN_DELETE
        ["Page Up", 15, 3, 0x7B],         //  79 KEY_EN_PAGE_UP
        ["Page Down", 15, 4, 0x7C],       //  80 KEY_EN_PAGE_DOWN
        ["Right Arrow", 15, 5, 0x7D],     //  81 KEY_EN_RIGHT_ARROW
      ]),
    }),
    // ASUS_TUF_K7_LAYOUT_US, 81 LEDs, matrix ASUS_ROG_AZOTH_LAYOUT_KEYS_US
    17: Object.freeze({
      width: 16,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["F1", 1, 0, 0x08],               //   6 KEY_EN_F1
        ["1", 1, 1, 0x09],                //   7 KEY_EN_1
        ["Left Win", 1, 5, 0x0D],         //   8 KEY_EN_LEFT_WINDOWS
        ["F2", 2, 0, 0x10],               //   9 KEY_EN_F2
        ["2", 2, 1, 0x11],                //  10 KEY_EN_2
        ["Q", 2, 2, 0x0A],                //  11 KEY_EN_Q
        ["A", 2, 3, 0x0B],                //  12 KEY_EN_A
        ["Z", 2, 4, 0x14],                //  13 KEY_EN_Z
        ["Left Alt", 2, 5, 0x15],         //  14 KEY_EN_LEFT_ALT
        ["F3", 3, 0, 0x18],               //  15 KEY_EN_F3
        ["3", 3, 1, 0x19],                //  16 KEY_EN_3
        ["W", 3, 2, 0x12],                //  17 KEY_EN_W
        ["S", 3, 3, 0x13],                //  18 KEY_EN_S
        ["X", 3, 4, 0x1C],                //  19 KEY_EN_X
        ["F4", 4, 0, 0x20],               //  20 KEY_EN_F4
        ["4", 4, 1, 0x21],                //  21 KEY_EN_4
        ["E", 4, 2, 0x1A],                //  22 KEY_EN_E
        ["D", 4, 3, 0x1B],                //  23 KEY_EN_D
        ["C", 4, 4, 0x24],                //  24 KEY_EN_C
        ["F5", 5, 0, 0x28],               //  25 KEY_EN_F5
        ["5", 5, 1, 0x29],                //  26 KEY_EN_5
        ["R", 5, 2, 0x22],                //  27 KEY_EN_R
        ["F", 5, 3, 0x23],                //  28 KEY_EN_F
        ["V", 5, 4, 0x2C],                //  29 KEY_EN_V
        ["F6", 6, 0, 0x30],               //  30 KEY_EN_F6
        ["6", 6, 1, 0x31],                //  31 KEY_EN_6
        ["T", 6, 2, 0x2A],                //  32 KEY_EN_T
        ["G", 6, 3, 0x2B],                //  33 KEY_EN_G
        ["B", 6, 4, 0x34],                //  34 KEY_EN_B
        ["Space", 6, 5, 0x35],            //  35 KEY_EN_SPACE
        ["F7", 7, 0, 0x38],               //  36 KEY_EN_F7
        ["7", 7, 1, 0x39],                //  37 KEY_EN_7
        ["Y", 7, 2, 0x32],                //  38 KEY_EN_Y
        ["H", 7, 3, 0x33],                //  39 KEY_EN_H
        ["N", 7, 4, 0x3C],                //  40 KEY_EN_N
        ["F8", 8, 0, 0x40],               //  41 KEY_EN_F8
        ["8", 8, 1, 0x41],                //  42 KEY_EN_8
        ["U", 8, 2, 0x3A],                //  43 KEY_EN_U
        ["J", 8, 3, 0x3B],                //  44 KEY_EN_J
        ["M", 8, 4, 0x44],                //  45 KEY_EN_M
        ["F9", 9, 0, 0x48],               //  46 KEY_EN_F9
        ["9", 9, 1, 0x49],                //  47 KEY_EN_9
        ["I", 9, 2, 0x42],                //  48 KEY_EN_I
        ["K", 9, 3, 0x43],                //  49 KEY_EN_K
        [",", 9, 4, 0x4C],                //  50 KEY_EN_COMMA
        ["F10", 10, 0, 0x50],             //  51 KEY_EN_F10
        ["0", 10, 1, 0x51],               //  52 KEY_EN_0
        ["O", 10, 2, 0x4A],               //  53 KEY_EN_O
        ["L", 10, 3, 0x4B],               //  54 KEY_EN_L
        [".", 10, 4, 0x54],               //  55 KEY_EN_PERIOD
        ["Right Alt", 10, 5, 0x55],       //  56 KEY_EN_RIGHT_ALT
        ["F11", 11, 0, 0x58],             //  57 KEY_EN_F11
        ["-", 11, 1, 0x59],               //  58 KEY_EN_MINUS
        ["P", 11, 2, 0x52],               //  59 KEY_EN_P
        [";", 11, 3, 0x53],               //  60 KEY_EN_SEMICOLON
        ["/", 11, 4, 0x5C],               //  61 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 5, 0x5D],              //  62 KEY_EN_RIGHT_FUNCTION
        ["F12", 12, 0, 0x60],             //  63 KEY_EN_F12
        ["=", 12, 1, 0x61],               //  64 KEY_EN_EQUALS
        ["[", 12, 2, 0x5A],               //  65 KEY_EN_LEFT_BRACKET
        ["'", 12, 3, 0x5B],               //  66 KEY_EN_QUOTE
        ["Right Shift", 12, 4, 0x64],     //  67 KEY_EN_RIGHT_SHIFT
        ["Right Ctrl", 12, 5, 0x65],      //  68 KEY_EN_RIGHT_CONTROL
        ["Backspace", 13, 1, 0x69],       //  69 KEY_EN_BACKSPACE
        ["]", 13, 2, 0x62],               //  70 KEY_EN_RIGHT_BRACKET
        ["Left Arrow", 13, 5, 0x6D],      //  71 KEY_EN_LEFT_ARROW
        ["\\", 14, 2, 0x6A],              //  72 KEY_EN_ANSI_BACK_SLASH
        ["Enter", 14, 3, 0x6B],           //  73 KEY_EN_ANSI_ENTER
        ["Up Arrow", 14, 4, 0x74],        //  74 KEY_EN_UP_ARROW
        ["Down Arrow", 14, 5, 0x75],      //  75 KEY_EN_DOWN_ARROW
        ["Insert", 15, 1, 0x79],          //  76 KEY_EN_INSERT
        ["Del", 15, 2, 0x7A],             //  77 KEY_EN_DELETE
        ["Page Up", 15, 3, 0x7B],         //  78 KEY_EN_PAGE_UP
        ["Page Down", 15, 4, 0x7C],       //  79 KEY_EN_PAGE_DOWN
        ["Right Arrow", 15, 5, 0x7D],     //  80 KEY_EN_RIGHT_ARROW
      ]),
    }),
  }),
  AsusROGStrixScopeLayouts: Object.freeze({
    // ASUS_TUF_K7_LAYOUT_UK, 107 LEDs, matrix ASUS_ROG_STRIX_SCOPE_LAYOUT_KEYS_ISO
    4: Object.freeze({
      width: 24,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["1", 1, 1, 0x11],                //   6 KEY_EN_1
        ["ISO_<", 1, 4, 0x0C],            //   7 KEY_EN_ISO_BACK_SLASH
        ["Left Win", 1, 5, 0x15],         //   8 KEY_EN_LEFT_WINDOWS
        ["F1", 2, 0, 0x18],               //   9 KEY_EN_F1
        ["2", 2, 1, 0x19],                //  10 KEY_EN_2
        ["Q", 2, 2, 0x12],                //  11 KEY_EN_Q
        ["A", 2, 3, 0x13],                //  12 KEY_EN_A
        ["Z", 2, 4, 0x14],                //  13 KEY_EN_Z
        ["Left Alt", 2, 5, 0x1D],         //  14 KEY_EN_LEFT_ALT
        ["F2", 3, 0, 0x20],               //  15 KEY_EN_F2
        ["3", 3, 1, 0x21],                //  16 KEY_EN_3
        ["W", 3, 2, 0x1A],                //  17 KEY_EN_W
        ["S", 3, 3, 0x1B],                //  18 KEY_EN_S
        ["X", 3, 4, 0x1C],                //  19 KEY_EN_X
        ["F3", 4, 0, 0x28],               //  20 KEY_EN_F3
        ["4", 4, 1, 0x29],                //  21 KEY_EN_4
        ["E", 4, 2, 0x22],                //  22 KEY_EN_E
        ["D", 4, 3, 0x23],                //  23 KEY_EN_D
        ["C", 4, 4, 0x24],                //  24 KEY_EN_C
        ["F4", 5, 0, 0x30],               //  25 KEY_EN_F4
        ["5", 5, 1, 0x31],                //  26 KEY_EN_5
        ["R", 5, 2, 0x2A],                //  27 KEY_EN_R
        ["F", 5, 3, 0x2B],                //  28 KEY_EN_F
        ["V", 5, 4, 0x2C],                //  29 KEY_EN_V
        ["6", 6, 1, 0x39],                //  30 KEY_EN_6
        ["T", 6, 2, 0x32],                //  31 KEY_EN_T
        ["G", 6, 3, 0x33],                //  32 KEY_EN_G
        ["B", 6, 4, 0x34],                //  33 KEY_EN_B
        ["Space", 6, 5, 0x35],            //  34 KEY_EN_SPACE
        ["F5", 7, 0, 0x40],               //  35 KEY_EN_F5
        ["7", 7, 1, 0x41],                //  36 KEY_EN_7
        ["Y", 7, 2, 0x3A],                //  37 KEY_EN_Y
        ["H", 7, 3, 0x3B],                //  38 KEY_EN_H
        ["N", 7, 4, 0x3C],                //  39 KEY_EN_N
        ["F6", 8, 0, 0x48],               //  40 KEY_EN_F6
        ["8", 8, 1, 0x49],                //  41 KEY_EN_8
        ["U", 8, 2, 0x42],                //  42 KEY_EN_U
        ["J", 8, 3, 0x43],                //  43 KEY_EN_J
        ["M", 8, 4, 0x44],                //  44 KEY_EN_M
        ["F7", 9, 0, 0x50],               //  45 KEY_EN_F7
        ["9", 9, 1, 0x51],                //  46 KEY_EN_9
        ["I", 9, 2, 0x4A],                //  47 KEY_EN_I
        ["K", 9, 3, 0x4B],                //  48 KEY_EN_K
        [",", 9, 4, 0x4C],                //  49 KEY_EN_COMMA
        ["F8", 10, 0, 0x58],              //  50 KEY_EN_F8
        ["0", 10, 1, 0x59],               //  51 KEY_EN_0
        ["O", 10, 2, 0x52],               //  52 KEY_EN_O
        ["L", 10, 3, 0x53],               //  53 KEY_EN_L
        [".", 10, 4, 0x54],               //  54 KEY_EN_PERIOD
        ["Right Alt", 10, 5, 0x4D],       //  55 KEY_EN_RIGHT_ALT
        ["F9", 11, 0, 0x60],              //  56 KEY_EN_F9
        ["-", 11, 1, 0x61],               //  57 KEY_EN_MINUS
        ["P", 11, 2, 0x5A],               //  58 KEY_EN_P
        [";", 11, 3, 0x5B],               //  59 KEY_EN_SEMICOLON
        ["/", 11, 4, 0x5C],               //  60 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 5, 0x5D],              //  61 KEY_EN_RIGHT_FUNCTION
        ["F10", 12, 0, 0x68],             //  62 KEY_EN_F10
        ["=", 12, 1, 0x69],               //  63 KEY_EN_EQUALS
        ["[", 12, 2, 0x62],               //  64 KEY_EN_LEFT_BRACKET
        ["'", 12, 3, 0x63],               //  65 KEY_EN_QUOTE
        ["Menu", 12, 5, 0x65],            //  66 KEY_EN_MENU
        ["F11", 13, 0, 0x70],             //  67 KEY_EN_F11
        ["Backspace", 13, 1, 0x79],       //  68 KEY_EN_BACKSPACE
        ["]", 13, 2, 0x6A],               //  69 KEY_EN_RIGHT_BRACKET
        ["ISO_#", 13, 3, 0x6B],           //  70 KEY_EN_POUND
        ["Right Shift", 13, 4, 0x7C],     //  71 KEY_EN_RIGHT_SHIFT
        ["F12", 14, 0, 0x78],             //  72 KEY_EN_F12
        ["Enter", 14, 3, 0x7B],           //  73 KEY_EN_ISO_ENTER
        ["Right Ctrl", 14, 5, 0x7D],      //  74 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 16, 0, 0x80],    //  75 KEY_EN_PRINT_SCREEN
        ["Insert", 16, 1, 0x81],          //  76 KEY_EN_INSERT
        ["Del", 16, 2, 0x82],             //  77 KEY_EN_DELETE
        ["Left Arrow", 16, 5, 0x85],      //  78 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 17, 0, 0x88],     //  79 KEY_EN_SCROLL_LOCK
        ["Home", 17, 1, 0x89],            //  80 KEY_EN_HOME
        ["End", 17, 2, 0x8A],             //  81 KEY_EN_END
        ["Up Arrow", 17, 4, 0x8C],        //  82 KEY_EN_UP_ARROW
        ["Down Arrow", 17, 5, 0x8D],      //  83 KEY_EN_DOWN_ARROW
        ["Pause Break", 18, 0, 0x90],     //  84 KEY_EN_PAUSE_BREAK
        ["Page Up", 18, 1, 0x91],         //  85 KEY_EN_PAGE_UP
        ["Page Down", 18, 2, 0x92],       //  86 KEY_EN_PAGE_DOWN
        ["Right Arrow", 18, 5, 0x95],     //  87 KEY_EN_RIGHT_ARROW
        ["NumLock", 20, 1, 0x99],         //  88 KEY_EN_NUMPAD_LOCK
        ["Num 7", 20, 2, 0x9A],           //  89 KEY_EN_NUMPAD_7
        ["Num 4", 20, 3, 0x9B],           //  90 KEY_EN_NUMPAD_4
        ["Num 1", 20, 4, 0x9C],           //  91 KEY_EN_NUMPAD_1
        ["Num 0", 20, 5, 0x9D],           //  92 KEY_EN_NUMPAD_0
        ["Num /", 21, 1, 0xA1],           //  93 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 21, 2, 0xA2],           //  94 KEY_EN_NUMPAD_8
        ["Num 5", 21, 3, 0xA3],           //  95 KEY_EN_NUMPAD_5
        ["Num 2", 21, 4, 0xA4],           //  96 KEY_EN_NUMPAD_2
        ["Num *", 22, 1, 0xA9],           //  97 KEY_EN_NUMPAD_TIMES
        ["Num 9", 22, 2, 0xAA],           //  98 KEY_EN_NUMPAD_9
        ["Num 6", 22, 3, 0xAB],           //  99 KEY_EN_NUMPAD_6
        ["Num 3", 22, 4, 0xAC],           // 100 KEY_EN_NUMPAD_3
        ["Num .", 22, 5, 0xAD],           // 101 KEY_EN_NUMPAD_PERIOD
        ["Num -", 23, 1, 0xB1],           // 102 KEY_EN_NUMPAD_MINUS
        ["Num +", 23, 2, 0xB2],           // 103 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 23, 4, 0xB4],       // 104 KEY_EN_NUMPAD_ENTER
        ["Logo 1", 22, 0, 0xA8],          // 105 Logo 1
        ["Logo 2", 23, 0, 0xB0],          // 106 Logo 2
      ]),
    }),
    // ASUS_TUF_K7_LAYOUT_US, 106 LEDs, matrix ASUS_ROG_STRIX_SCOPE_LAYOUT_KEYS_ANSI
    17: Object.freeze({
      width: 24,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["1", 1, 1, 0x11],                //   6 KEY_EN_1
        ["Left Win", 1, 5, 0x15],         //   7 KEY_EN_LEFT_WINDOWS
        ["F1", 2, 0, 0x18],               //   8 KEY_EN_F1
        ["2", 2, 1, 0x19],                //   9 KEY_EN_2
        ["Q", 2, 2, 0x12],                //  10 KEY_EN_Q
        ["A", 2, 3, 0x13],                //  11 KEY_EN_A
        ["Z", 2, 4, 0x14],                //  12 KEY_EN_Z
        ["Left Alt", 2, 5, 0x1D],         //  13 KEY_EN_LEFT_ALT
        ["F2", 3, 0, 0x20],               //  14 KEY_EN_F2
        ["3", 3, 1, 0x21],                //  15 KEY_EN_3
        ["W", 3, 2, 0x1A],                //  16 KEY_EN_W
        ["S", 3, 3, 0x1B],                //  17 KEY_EN_S
        ["X", 3, 4, 0x1C],                //  18 KEY_EN_X
        ["F3", 4, 0, 0x28],               //  19 KEY_EN_F3
        ["4", 4, 1, 0x29],                //  20 KEY_EN_4
        ["E", 4, 2, 0x22],                //  21 KEY_EN_E
        ["D", 4, 3, 0x23],                //  22 KEY_EN_D
        ["C", 4, 4, 0x24],                //  23 KEY_EN_C
        ["F4", 5, 0, 0x30],               //  24 KEY_EN_F4
        ["5", 5, 1, 0x31],                //  25 KEY_EN_5
        ["R", 5, 2, 0x2A],                //  26 KEY_EN_R
        ["F", 5, 3, 0x2B],                //  27 KEY_EN_F
        ["V", 5, 4, 0x2C],                //  28 KEY_EN_V
        ["6", 6, 1, 0x39],                //  29 KEY_EN_6
        ["T", 6, 2, 0x32],                //  30 KEY_EN_T
        ["G", 6, 3, 0x33],                //  31 KEY_EN_G
        ["B", 6, 4, 0x34],                //  32 KEY_EN_B
        ["Space", 6, 5, 0x35],            //  33 KEY_EN_SPACE
        ["F5", 7, 0, 0x40],               //  34 KEY_EN_F5
        ["7", 7, 1, 0x41],                //  35 KEY_EN_7
        ["Y", 7, 2, 0x3A],                //  36 KEY_EN_Y
        ["H", 7, 3, 0x3B],                //  37 KEY_EN_H
        ["N", 7, 4, 0x3C],                //  38 KEY_EN_N
        ["F6", 8, 0, 0x48],               //  39 KEY_EN_F6
        ["8", 8, 1, 0x49],                //  40 KEY_EN_8
        ["U", 8, 2, 0x42],                //  41 KEY_EN_U
        ["J", 8, 3, 0x43],                //  42 KEY_EN_J
        ["M", 8, 4, 0x44],                //  43 KEY_EN_M
        ["F7", 9, 0, 0x50],               //  44 KEY_EN_F7
        ["9", 9, 1, 0x51],                //  45 KEY_EN_9
        ["I", 9, 2, 0x4A],                //  46 KEY_EN_I
        ["K", 9, 3, 0x4B],                //  47 KEY_EN_K
        [",", 9, 4, 0x4C],                //  48 KEY_EN_COMMA
        ["F8", 10, 0, 0x58],              //  49 KEY_EN_F8
        ["0", 10, 1, 0x59],               //  50 KEY_EN_0
        ["O", 10, 2, 0x52],               //  51 KEY_EN_O
        ["L", 10, 3, 0x53],               //  52 KEY_EN_L
        [".", 10, 4, 0x54],               //  53 KEY_EN_PERIOD
        ["Right Alt", 10, 5, 0x4D],       //  54 KEY_EN_RIGHT_ALT
        ["F9", 11, 0, 0x60],              //  55 KEY_EN_F9
        ["-", 11, 1, 0x61],               //  56 KEY_EN_MINUS
        ["P", 11, 2, 0x5A],               //  57 KEY_EN_P
        [";", 11, 3, 0x5B],               //  58 KEY_EN_SEMICOLON
        ["/", 11, 4, 0x5C],               //  59 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 5, 0x5D],              //  60 KEY_EN_RIGHT_FUNCTION
        ["F10", 12, 0, 0x68],             //  61 KEY_EN_F10
        ["=", 12, 1, 0x69],               //  62 KEY_EN_EQUALS
        ["[", 12, 2, 0x62],               //  63 KEY_EN_LEFT_BRACKET
        ["'", 12, 3, 0x63],               //  64 KEY_EN_QUOTE
        ["Menu", 12, 5, 0x65],            //  65 KEY_EN_MENU
        ["F11", 13, 0, 0x70],             //  66 KEY_EN_F11
        ["Backspace", 13, 1, 0x79],       //  67 KEY_EN_BACKSPACE
        ["]", 13, 2, 0x6A],               //  68 KEY_EN_RIGHT_BRACKET
        ["Right Shift", 13, 4, 0x7C],     //  69 KEY_EN_RIGHT_SHIFT
        ["F12", 14, 0, 0x78],             //  70 KEY_EN_F12
        ["\\", 14, 2, 0x7A],              //  71 KEY_EN_ANSI_BACK_SLASH
        ["Enter", 14, 3, 0x7B],           //  72 KEY_EN_ANSI_ENTER
        ["Right Ctrl", 14, 5, 0x7D],      //  73 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 16, 0, 0x80],    //  74 KEY_EN_PRINT_SCREEN
        ["Insert", 16, 1, 0x81],          //  75 KEY_EN_INSERT
        ["Del", 16, 2, 0x82],             //  76 KEY_EN_DELETE
        ["Left Arrow", 16, 5, 0x85],      //  77 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 17, 0, 0x88],     //  78 KEY_EN_SCROLL_LOCK
        ["Home", 17, 1, 0x89],            //  79 KEY_EN_HOME
        ["End", 17, 2, 0x8A],             //  80 KEY_EN_END
        ["Up Arrow", 17, 4, 0x8C],        //  81 KEY_EN_UP_ARROW
        ["Down Arrow", 17, 5, 0x8D],      //  82 KEY_EN_DOWN_ARROW
        ["Pause Break", 18, 0, 0x90],     //  83 KEY_EN_PAUSE_BREAK
        ["Page Up", 18, 1, 0x91],         //  84 KEY_EN_PAGE_UP
        ["Page Down", 18, 2, 0x92],       //  85 KEY_EN_PAGE_DOWN
        ["Right Arrow", 18, 5, 0x95],     //  86 KEY_EN_RIGHT_ARROW
        ["NumLock", 20, 1, 0x99],         //  87 KEY_EN_NUMPAD_LOCK
        ["Num 7", 20, 2, 0x9A],           //  88 KEY_EN_NUMPAD_7
        ["Num 4", 20, 3, 0x9B],           //  89 KEY_EN_NUMPAD_4
        ["Num 1", 20, 4, 0x9C],           //  90 KEY_EN_NUMPAD_1
        ["Num 0", 20, 5, 0x9D],           //  91 KEY_EN_NUMPAD_0
        ["Num /", 21, 1, 0xA1],           //  92 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 21, 2, 0xA2],           //  93 KEY_EN_NUMPAD_8
        ["Num 5", 21, 3, 0xA3],           //  94 KEY_EN_NUMPAD_5
        ["Num 2", 21, 4, 0xA4],           //  95 KEY_EN_NUMPAD_2
        ["Num *", 22, 1, 0xA9],           //  96 KEY_EN_NUMPAD_TIMES
        ["Num 9", 22, 2, 0xAA],           //  97 KEY_EN_NUMPAD_9
        ["Num 6", 22, 3, 0xAB],           //  98 KEY_EN_NUMPAD_6
        ["Num 3", 22, 4, 0xAC],           //  99 KEY_EN_NUMPAD_3
        ["Num .", 22, 5, 0xAD],           // 100 KEY_EN_NUMPAD_PERIOD
        ["Num -", 23, 1, 0xB1],           // 101 KEY_EN_NUMPAD_MINUS
        ["Num +", 23, 2, 0xB2],           // 102 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 23, 4, 0xB4],       // 103 KEY_EN_NUMPAD_ENTER
        ["Logo 1", 22, 0, 0xA8],          // 104 Logo 1
        ["Logo 2", 23, 0, 0xB0],          // 105 Logo 2
      ]),
    }),
  }),
  AsusROGStrixScopeIILayouts: Object.freeze({
    // ASUS_TUF_K7_LAYOUT_UK, 108 LEDs, matrix ASUS_ROG_STRIX_SCOPE_II_LAYOUT_KEYS_ISO
    4: Object.freeze({
      width: 24,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["1", 1, 1, 0x11],                //   6 KEY_EN_1
        ["ISO_<", 1, 4, 0x0C],            //   7 KEY_EN_ISO_BACK_SLASH
        ["Left Win", 1, 5, 0x0D],         //   8 KEY_EN_LEFT_WINDOWS
        ["F1", 2, 0, 0x18],               //   9 KEY_EN_F1
        ["2", 2, 1, 0x19],                //  10 KEY_EN_2
        ["Q", 2, 2, 0x12],                //  11 KEY_EN_Q
        ["A", 2, 3, 0x13],                //  12 KEY_EN_A
        ["Z", 2, 4, 0x14],                //  13 KEY_EN_Z
        ["Left Alt", 2, 5, 0x15],         //  14 KEY_EN_LEFT_ALT
        ["F2", 3, 0, 0x20],               //  15 KEY_EN_F2
        ["3", 3, 1, 0x21],                //  16 KEY_EN_3
        ["W", 3, 2, 0x1A],                //  17 KEY_EN_W
        ["S", 3, 3, 0x1B],                //  18 KEY_EN_S
        ["X", 3, 4, 0x1C],                //  19 KEY_EN_X
        ["F3", 4, 0, 0x28],               //  20 KEY_EN_F3
        ["4", 4, 1, 0x29],                //  21 KEY_EN_4
        ["E", 4, 2, 0x22],                //  22 KEY_EN_E
        ["D", 4, 3, 0x23],                //  23 KEY_EN_D
        ["C", 4, 4, 0x24],                //  24 KEY_EN_C
        ["F4", 5, 0, 0x30],               //  25 KEY_EN_F4
        ["5", 5, 1, 0x31],                //  26 KEY_EN_5
        ["R", 5, 2, 0x2A],                //  27 KEY_EN_R
        ["F", 5, 3, 0x2B],                //  28 KEY_EN_F
        ["V", 5, 4, 0x2C],                //  29 KEY_EN_V
        ["Space", 5, 5, 0x2D],            //  30 KEY_EN_SPACE
        ["6", 6, 1, 0x39],                //  31 KEY_EN_6
        ["T", 6, 2, 0x32],                //  32 KEY_EN_T
        ["G", 6, 3, 0x33],                //  33 KEY_EN_G
        ["B", 6, 4, 0x34],                //  34 KEY_EN_B
        ["Space", 6, 5, 0x35],            //  35 KEY_EN_SPACE
        ["F5", 7, 0, 0x40],               //  36 KEY_EN_F5
        ["7", 7, 1, 0x41],                //  37 KEY_EN_7
        ["Y", 7, 2, 0x3A],                //  38 KEY_EN_Y
        ["H", 7, 3, 0x3B],                //  39 KEY_EN_H
        ["N", 7, 4, 0x3C],                //  40 KEY_EN_N
        ["Space", 7, 5, 0x3D],            //  41 KEY_EN_SPACE
        ["F6", 8, 0, 0x48],               //  42 KEY_EN_F6
        ["8", 8, 1, 0x49],                //  43 KEY_EN_8
        ["U", 8, 2, 0x42],                //  44 KEY_EN_U
        ["J", 8, 3, 0x43],                //  45 KEY_EN_J
        ["M", 8, 4, 0x44],                //  46 KEY_EN_M
        ["F7", 9, 0, 0x50],               //  47 KEY_EN_F7
        ["9", 9, 1, 0x51],                //  48 KEY_EN_9
        ["I", 9, 2, 0x4A],                //  49 KEY_EN_I
        ["K", 9, 3, 0x4B],                //  50 KEY_EN_K
        [",", 9, 4, 0x4C],                //  51 KEY_EN_COMMA
        ["F8", 10, 0, 0x58],              //  52 KEY_EN_F8
        ["0", 10, 1, 0x59],               //  53 KEY_EN_0
        ["O", 10, 2, 0x52],               //  54 KEY_EN_O
        ["L", 10, 3, 0x53],               //  55 KEY_EN_L
        [".", 10, 4, 0x54],               //  56 KEY_EN_PERIOD
        ["Right Alt", 10, 5, 0x4D],       //  57 KEY_EN_RIGHT_ALT
        ["F9", 11, 0, 0x60],              //  58 KEY_EN_F9
        ["-", 11, 1, 0x61],               //  59 KEY_EN_MINUS
        ["P", 11, 2, 0x5A],               //  60 KEY_EN_P
        [";", 11, 3, 0x5B],               //  61 KEY_EN_SEMICOLON
        ["/", 11, 4, 0x5C],               //  62 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 5, 0x5D],              //  63 KEY_EN_RIGHT_FUNCTION
        ["F10", 12, 0, 0x68],             //  64 KEY_EN_F10
        ["=", 12, 1, 0x69],               //  65 KEY_EN_EQUALS
        ["[", 12, 2, 0x62],               //  66 KEY_EN_LEFT_BRACKET
        ["'", 12, 3, 0x63],               //  67 KEY_EN_QUOTE
        ["Menu", 12, 5, 0x65],            //  68 KEY_EN_MENU
        ["F11", 13, 0, 0x70],             //  69 KEY_EN_F11
        ["Backspace", 13, 1, 0x79],       //  70 KEY_EN_BACKSPACE
        ["]", 13, 2, 0x6A],               //  71 KEY_EN_RIGHT_BRACKET
        ["ISO_#", 13, 3, 0x6B],           //  72 KEY_EN_POUND
        ["Right Shift", 13, 4, 0x7C],     //  73 KEY_EN_RIGHT_SHIFT
        ["F12", 14, 0, 0x78],             //  74 KEY_EN_F12
        ["Enter", 14, 3, 0x7B],           //  75 KEY_EN_ISO_ENTER
        ["Right Ctrl", 14, 5, 0x7D],      //  76 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 16, 0, 0x80],    //  77 KEY_EN_PRINT_SCREEN
        ["Insert", 16, 1, 0x81],          //  78 KEY_EN_INSERT
        ["Del", 16, 2, 0x82],             //  79 KEY_EN_DELETE
        ["Left Arrow", 16, 5, 0x85],      //  80 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 17, 0, 0x88],     //  81 KEY_EN_SCROLL_LOCK
        ["Home", 17, 1, 0x89],            //  82 KEY_EN_HOME
        ["End", 17, 2, 0x8A],             //  83 KEY_EN_END
        ["Up Arrow", 17, 4, 0x8C],        //  84 KEY_EN_UP_ARROW
        ["Down Arrow", 17, 5, 0x8D],      //  85 KEY_EN_DOWN_ARROW
        ["Pause Break", 18, 0, 0x90],     //  86 KEY_EN_PAUSE_BREAK
        ["Page Up", 18, 1, 0x91],         //  87 KEY_EN_PAGE_UP
        ["Page Down", 18, 2, 0x92],       //  88 KEY_EN_PAGE_DOWN
        ["Right Arrow", 18, 5, 0x95],     //  89 KEY_EN_RIGHT_ARROW
        ["NumLock", 20, 1, 0x99],         //  90 KEY_EN_NUMPAD_LOCK
        ["Num 7", 20, 2, 0x9A],           //  91 KEY_EN_NUMPAD_7
        ["Num 4", 20, 3, 0x9B],           //  92 KEY_EN_NUMPAD_4
        ["Num 1", 20, 4, 0x9C],           //  93 KEY_EN_NUMPAD_1
        ["Num 0", 20, 5, 0x9D],           //  94 KEY_EN_NUMPAD_0
        ["Logo", 21, 0, 0xA0],            //  95 Logo
        ["Num /", 21, 1, 0xA1],           //  96 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 21, 2, 0xA2],           //  97 KEY_EN_NUMPAD_8
        ["Num 5", 21, 3, 0xA3],           //  98 KEY_EN_NUMPAD_5
        ["Num 2", 21, 4, 0xA4],           //  99 KEY_EN_NUMPAD_2
        ["Num *", 22, 1, 0xA9],           // 100 KEY_EN_NUMPAD_TIMES
        ["Num 9", 22, 2, 0xAA],           // 101 KEY_EN_NUMPAD_9
        ["Num 6", 22, 3, 0xAB],           // 102 KEY_EN_NUMPAD_6
        ["Num 3", 22, 4, 0xAC],           // 103 KEY_EN_NUMPAD_3
        ["Num .", 22, 5, 0xAD],           // 104 KEY_EN_NUMPAD_PERIOD
        ["Num -", 23, 1, 0xB1],           // 105 KEY_EN_NUMPAD_MINUS
        ["Num +", 23, 2, 0xB2],           // 106 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 23, 4, 0xB4],       // 107 KEY_EN_NUMPAD_ENTER
      ]),
    }),
    // ASUS_TUF_K7_LAYOUT_US, 107 LEDs, matrix ASUS_ROG_STRIX_SCOPE_II_LAYOUT_KEYS_ANSI
    17: Object.freeze({
      width: 24,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["1", 1, 1, 0x11],                //   6 KEY_EN_1
        ["Left Win", 1, 5, 0x0D],         //   7 KEY_EN_LEFT_WINDOWS
        ["F1", 2, 0, 0x18],               //   8 KEY_EN_F1
        ["2", 2, 1, 0x19],                //   9 KEY_EN_2
        ["Q", 2, 2, 0x12],                //  10 KEY_EN_Q
        ["A", 2, 3, 0x13],                //  11 KEY_EN_A
        ["Z", 2, 4, 0x14],                //  12 KEY_EN_Z
        ["Left Alt", 2, 5, 0x15],         //  13 KEY_EN_LEFT_ALT
        ["F2", 3, 0, 0x20],               //  14 KEY_EN_F2
        ["3", 3, 1, 0x21],                //  15 KEY_EN_3
        ["W", 3, 2, 0x1A],                //  16 KEY_EN_W
        ["S", 3, 3, 0x1B],                //  17 KEY_EN_S
        ["X", 3, 4, 0x1C],                //  18 KEY_EN_X
        ["F3", 4, 0, 0x28],               //  19 KEY_EN_F3
        ["4", 4, 1, 0x29],                //  20 KEY_EN_4
        ["E", 4, 2, 0x22],                //  21 KEY_EN_E
        ["D", 4, 3, 0x23],                //  22 KEY_EN_D
        ["C", 4, 4, 0x24],                //  23 KEY_EN_C
        ["F4", 5, 0, 0x30],               //  24 KEY_EN_F4
        ["5", 5, 1, 0x31],                //  25 KEY_EN_5
        ["R", 5, 2, 0x2A],                //  26 KEY_EN_R
        ["F", 5, 3, 0x2B],                //  27 KEY_EN_F
        ["V", 5, 4, 0x2C],                //  28 KEY_EN_V
        ["Space", 5, 5, 0x2D],            //  29 KEY_EN_SPACE
        ["6", 6, 1, 0x39],                //  30 KEY_EN_6
        ["T", 6, 2, 0x32],                //  31 KEY_EN_T
        ["G", 6, 3, 0x33],                //  32 KEY_EN_G
        ["B", 6, 4, 0x34],                //  33 KEY_EN_B
        ["Space", 6, 5, 0x35],            //  34 KEY_EN_SPACE
        ["F5", 7, 0, 0x40],               //  35 KEY_EN_F5
        ["7", 7, 1, 0x41],                //  36 KEY_EN_7
        ["Y", 7, 2, 0x3A],                //  37 KEY_EN_Y
        ["H", 7, 3, 0x3B],                //  38 KEY_EN_H
        ["N", 7, 4, 0x3C],                //  39 KEY_EN_N
        ["Space", 7, 5, 0x3D],            //  40 KEY_EN_SPACE
        ["F6", 8, 0, 0x48],               //  41 KEY_EN_F6
        ["8", 8, 1, 0x49],                //  42 KEY_EN_8
        ["U", 8, 2, 0x42],                //  43 KEY_EN_U
        ["J", 8, 3, 0x43],                //  44 KEY_EN_J
        ["M", 8, 4, 0x44],                //  45 KEY_EN_M
        ["F7", 9, 0, 0x50],               //  46 KEY_EN_F7
        ["9", 9, 1, 0x51],                //  47 KEY_EN_9
        ["I", 9, 2, 0x4A],                //  48 KEY_EN_I
        ["K", 9, 3, 0x4B],                //  49 KEY_EN_K
        [",", 9, 4, 0x4C],                //  50 KEY_EN_COMMA
        ["F8", 10, 0, 0x58],              //  51 KEY_EN_F8
        ["0", 10, 1, 0x59],               //  52 KEY_EN_0
        ["O", 10, 2, 0x52],               //  53 KEY_EN_O
        ["L", 10, 3, 0x53],               //  54 KEY_EN_L
        [".", 10, 4, 0x54],               //  55 KEY_EN_PERIOD
        ["Right Alt", 10, 5, 0x4D],       //  56 KEY_EN_RIGHT_ALT
        ["F9", 11, 0, 0x60],              //  57 KEY_EN_F9
        ["-", 11, 1, 0x61],               //  58 KEY_EN_MINUS
        ["P", 11, 2, 0x5A],               //  59 KEY_EN_P
        [";", 11, 3, 0x5B],               //  60 KEY_EN_SEMICOLON
        ["/", 11, 4, 0x5C],               //  61 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 5, 0x5D],              //  62 KEY_EN_RIGHT_FUNCTION
        ["F10", 12, 0, 0x68],             //  63 KEY_EN_F10
        ["=", 12, 1, 0x69],               //  64 KEY_EN_EQUALS
        ["[", 12, 2, 0x62],               //  65 KEY_EN_LEFT_BRACKET
        ["'", 12, 3, 0x63],               //  66 KEY_EN_QUOTE
        ["Menu", 12, 5, 0x65],            //  67 KEY_EN_MENU
        ["F11", 13, 0, 0x70],             //  68 KEY_EN_F11
        ["Backspace", 13, 1, 0x79],       //  69 KEY_EN_BACKSPACE
        ["]", 13, 2, 0x6A],               //  70 KEY_EN_RIGHT_BRACKET
        ["Right Shift", 13, 4, 0x7C],     //  71 KEY_EN_RIGHT_SHIFT
        ["F12", 14, 0, 0x78],             //  72 KEY_EN_F12
        ["\\", 14, 2, 0x7A],              //  73 KEY_EN_ANSI_BACK_SLASH
        ["Enter", 14, 3, 0x7B],           //  74 KEY_EN_ANSI_ENTER
        ["Right Ctrl", 14, 5, 0x7D],      //  75 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 16, 0, 0x80],    //  76 KEY_EN_PRINT_SCREEN
        ["Insert", 16, 1, 0x81],          //  77 KEY_EN_INSERT
        ["Del", 16, 2, 0x82],             //  78 KEY_EN_DELETE
        ["Left Arrow", 16, 5, 0x85],      //  79 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 17, 0, 0x88],     //  80 KEY_EN_SCROLL_LOCK
        ["Home", 17, 1, 0x89],            //  81 KEY_EN_HOME
        ["End", 17, 2, 0x8A],             //  82 KEY_EN_END
        ["Up Arrow", 17, 4, 0x8C],        //  83 KEY_EN_UP_ARROW
        ["Down Arrow", 17, 5, 0x8D],      //  84 KEY_EN_DOWN_ARROW
        ["Pause Break", 18, 0, 0x90],     //  85 KEY_EN_PAUSE_BREAK
        ["Page Up", 18, 1, 0x91],         //  86 KEY_EN_PAGE_UP
        ["Page Down", 18, 2, 0x92],       //  87 KEY_EN_PAGE_DOWN
        ["Right Arrow", 18, 5, 0x95],     //  88 KEY_EN_RIGHT_ARROW
        ["NumLock", 20, 1, 0x99],         //  89 KEY_EN_NUMPAD_LOCK
        ["Num 7", 20, 2, 0x9A],           //  90 KEY_EN_NUMPAD_7
        ["Num 4", 20, 3, 0x9B],           //  91 KEY_EN_NUMPAD_4
        ["Num 1", 20, 4, 0x9C],           //  92 KEY_EN_NUMPAD_1
        ["Num 0", 20, 5, 0x9D],           //  93 KEY_EN_NUMPAD_0
        ["Logo", 21, 0, 0xA0],            //  94 Logo
        ["Num /", 21, 1, 0xA1],           //  95 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 21, 2, 0xA2],           //  96 KEY_EN_NUMPAD_8
        ["Num 5", 21, 3, 0xA3],           //  97 KEY_EN_NUMPAD_5
        ["Num 2", 21, 4, 0xA4],           //  98 KEY_EN_NUMPAD_2
        ["Num *", 22, 1, 0xA9],           //  99 KEY_EN_NUMPAD_TIMES
        ["Num 9", 22, 2, 0xAA],           // 100 KEY_EN_NUMPAD_9
        ["Num 6", 22, 3, 0xAB],           // 101 KEY_EN_NUMPAD_6
        ["Num 3", 22, 4, 0xAC],           // 102 KEY_EN_NUMPAD_3
        ["Num .", 22, 5, 0xAD],           // 103 KEY_EN_NUMPAD_PERIOD
        ["Num -", 23, 1, 0xB1],           // 104 KEY_EN_NUMPAD_MINUS
        ["Num +", 23, 2, 0xB2],           // 105 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 23, 4, 0xB4],       // 106 KEY_EN_NUMPAD_ENTER
      ]),
    }),
  }),
  AsusROGStrixScopeII96RxWirelessLayouts: Object.freeze({
    // ASUS_TUF_K7_LAYOUT_UK, 104 LEDs, matrix ASUS_ROG_STRIX_SCOPE_II_96_RX_WIRELESS_LAYOUT_KEYS_ISO
    4: Object.freeze({
      width: 19,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["F1", 1, 0, 0x08],               //   6 KEY_EN_F1
        ["1", 1, 1, 0x09],                //   7 KEY_EN_1
        ["Q", 1, 2, 0x0A],                //   8 KEY_EN_Q
        ["A", 1, 3, 0x0B],                //   9 KEY_EN_A
        ["ISO_<", 1, 4, 0x0C],            //  10 KEY_EN_ISO_BACK_SLASH
        ["Left Win", 1, 5, 0x0D],         //  11 KEY_EN_LEFT_WINDOWS
        ["F2", 2, 0, 0x10],               //  12 KEY_EN_F2
        ["2", 2, 1, 0x11],                //  13 KEY_EN_2
        ["W", 2, 2, 0x12],                //  14 KEY_EN_W
        ["S", 2, 3, 0x13],                //  15 KEY_EN_S
        ["Z", 2, 4, 0x14],                //  16 KEY_EN_Z
        ["Left Alt", 2, 5, 0x15],         //  17 KEY_EN_LEFT_ALT
        ["F3", 3, 0, 0x18],               //  18 KEY_EN_F3
        ["3", 3, 1, 0x19],                //  19 KEY_EN_3
        ["E", 3, 2, 0x1A],                //  20 KEY_EN_E
        ["D", 3, 3, 0x1B],                //  21 KEY_EN_D
        ["X", 3, 4, 0x1C],                //  22 KEY_EN_X
        ["F4", 4, 0, 0x20],               //  23 KEY_EN_F4
        ["4", 4, 1, 0x21],                //  24 KEY_EN_4
        ["R", 4, 2, 0x22],                //  25 KEY_EN_R
        ["F", 4, 3, 0x23],                //  26 KEY_EN_F
        ["C", 4, 4, 0x24],                //  27 KEY_EN_C
        ["F5", 5, 0, 0x28],               //  28 KEY_EN_F5
        ["5", 5, 1, 0x29],                //  29 KEY_EN_5
        ["T", 5, 2, 0x2A],                //  30 KEY_EN_T
        ["G", 5, 3, 0x2B],                //  31 KEY_EN_G
        ["V", 5, 4, 0x2C],                //  32 KEY_EN_V
        ["Space", 5, 5, 0x2D],            //  33 KEY_EN_SPACE
        ["F6", 6, 0, 0x30],               //  34 KEY_EN_F6
        ["6", 6, 1, 0x31],                //  35 KEY_EN_6
        ["Y", 6, 2, 0x32],                //  36 KEY_EN_Y
        ["H", 6, 3, 0x33],                //  37 KEY_EN_H
        ["B", 6, 4, 0x34],                //  38 KEY_EN_B
        ["Space", 6, 5, 0x35],            //  39 KEY_EN_SPACE
        ["F7", 7, 0, 0x38],               //  40 KEY_EN_F7
        ["7", 7, 1, 0x39],                //  41 KEY_EN_7
        ["U", 7, 2, 0x3A],                //  42 KEY_EN_U
        ["J", 7, 3, 0x3B],                //  43 KEY_EN_J
        ["N", 7, 4, 0x3C],                //  44 KEY_EN_N
        ["Space", 7, 5, 0x3D],            //  45 KEY_EN_SPACE
        ["F8", 8, 0, 0x40],               //  46 KEY_EN_F8
        ["8", 8, 1, 0x41],                //  47 KEY_EN_8
        ["I", 8, 2, 0x42],                //  48 KEY_EN_I
        ["K", 8, 3, 0x43],                //  49 KEY_EN_K
        ["M", 8, 4, 0x44],                //  50 KEY_EN_M
        ["F9", 9, 0, 0x48],               //  51 KEY_EN_F9
        ["9", 9, 1, 0x49],                //  52 KEY_EN_9
        ["O", 9, 2, 0x4A],                //  53 KEY_EN_O
        ["L", 9, 3, 0x4B],                //  54 KEY_EN_L
        [",", 9, 4, 0x4C],                //  55 KEY_EN_COMMA
        ["F10", 10, 0, 0x50],             //  56 KEY_EN_F10
        ["0", 10, 1, 0x51],               //  57 KEY_EN_0
        ["P", 10, 2, 0x52],               //  58 KEY_EN_P
        [";", 10, 3, 0x53],               //  59 KEY_EN_SEMICOLON
        [".", 10, 4, 0x54],               //  60 KEY_EN_PERIOD
        ["Right Alt", 10, 5, 0x55],       //  61 KEY_EN_RIGHT_ALT
        ["F11", 11, 0, 0x58],             //  62 KEY_EN_F11
        ["-", 11, 1, 0x59],               //  63 KEY_EN_MINUS
        ["[", 11, 2, 0x5A],               //  64 KEY_EN_LEFT_BRACKET
        ["'", 11, 3, 0x5B],               //  65 KEY_EN_QUOTE
        ["/", 11, 4, 0x5C],               //  66 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 5, 0x5D],              //  67 KEY_EN_RIGHT_FUNCTION
        ["F12", 12, 0, 0x60],             //  68 KEY_EN_F12
        ["=", 12, 1, 0x61],               //  69 KEY_EN_EQUALS
        ["]", 12, 2, 0x62],               //  70 KEY_EN_RIGHT_BRACKET
        ["ISO_#", 12, 3, 0x63],           //  71 KEY_EN_POUND
        ["Right Shift", 12, 4, 0x64],     //  72 KEY_EN_RIGHT_SHIFT
        ["Right Ctrl", 12, 5, 0x65],      //  73 KEY_EN_RIGHT_CONTROL
        ["Insert", 13, 0, 0x68],          //  74 KEY_EN_INSERT
        ["Backspace", 13, 1, 0x69],       //  75 KEY_EN_BACKSPACE
        ["Enter", 13, 3, 0x6B],           //  76 KEY_EN_ISO_ENTER
        ["Left Arrow", 13, 5, 0x6D],      //  77 KEY_EN_LEFT_ARROW
        ["Del", 14, 0, 0x70],             //  78 KEY_EN_DELETE
        ["Up Arrow", 14, 4, 0x74],        //  79 KEY_EN_UP_ARROW
        ["Down Arrow", 14, 5, 0x75],      //  80 KEY_EN_DOWN_ARROW
        ["Page Up", 15, 0, 0x78],         //  81 KEY_EN_PAGE_UP
        ["NumLock", 15, 1, 0x79],         //  82 KEY_EN_NUMPAD_LOCK
        ["Num 7", 15, 2, 0x7A],           //  83 KEY_EN_NUMPAD_7
        ["Num 4", 15, 3, 0x7B],           //  84 KEY_EN_NUMPAD_4
        ["Num 1", 15, 4, 0x7C],           //  85 KEY_EN_NUMPAD_1
        ["Right Arrow", 15, 5, 0x7D],     //  86 KEY_EN_RIGHT_ARROW
        ["Page Down", 16, 0, 0x80],       //  87 KEY_EN_PAGE_DOWN
        ["Num /", 16, 1, 0x81],           //  88 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 16, 2, 0x82],           //  89 KEY_EN_NUMPAD_8
        ["Num 5", 16, 3, 0x83],           //  90 KEY_EN_NUMPAD_5
        ["Num 2", 16, 4, 0x84],           //  91 KEY_EN_NUMPAD_2
        ["Num 0", 16, 5, 0x85],           //  92 KEY_EN_NUMPAD_0
        ["Num *", 17, 1, 0x89],           //  93 KEY_EN_NUMPAD_TIMES
        ["Num 9", 17, 2, 0x8A],           //  94 KEY_EN_NUMPAD_9
        ["Num 6", 17, 3, 0x8B],           //  95 KEY_EN_NUMPAD_6
        ["Num 3", 17, 4, 0x8C],           //  96 KEY_EN_NUMPAD_3
        ["Num .", 17, 5, 0x8D],           //  97 KEY_EN_NUMPAD_PERIOD
        ["Logo", 17, 0, 0x88],            //  98 Logo
        ["Num -", 18, 1, 0x91],           //  99 KEY_EN_NUMPAD_MINUS
        ["Num +", 18, 2, 0x92],           // 100 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 18, 4, 0x94],       // 101 KEY_EN_NUMPAD_ENTER
        ["Space", 4, 5, 0x25],            // 102 KEY_EN_SPACE
        ["Space", 8, 5, 0x45],            // 103 KEY_EN_SPACE
      ]),
    }),
  }),
  AsusROGStrixScopeII96WirelessLayouts: Object.freeze({
    // ASUS_TUF_K7_LAYOUT_UK, 102 LEDs, matrix ASUS_ROG_STRIX_SCOPE_II_96_WIRELESS_LAYOUT_KEYS_ISO
    4: Object.freeze({
      width: 19,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["F1", 1, 0, 0x08],               //   6 KEY_EN_F1
        ["1", 1, 1, 0x09],                //   7 KEY_EN_1
        ["Q", 1, 2, 0x0A],                //   8 KEY_EN_Q
        ["A", 1, 3, 0x0B],                //   9 KEY_EN_A
        ["ISO_<", 1, 4, 0x0C],            //  10 KEY_EN_ISO_BACK_SLASH
        ["Left Win", 1, 5, 0x0D],         //  11 KEY_EN_LEFT_WINDOWS
        ["F2", 2, 0, 0x10],               //  12 KEY_EN_F2
        ["2", 2, 1, 0x11],                //  13 KEY_EN_2
        ["W", 2, 2, 0x12],                //  14 KEY_EN_W
        ["S", 2, 3, 0x13],                //  15 KEY_EN_S
        ["Z", 2, 4, 0x14],                //  16 KEY_EN_Z
        ["Left Alt", 2, 5, 0x15],         //  17 KEY_EN_LEFT_ALT
        ["F3", 3, 0, 0x18],               //  18 KEY_EN_F3
        ["3", 3, 1, 0x19],                //  19 KEY_EN_3
        ["E", 3, 2, 0x1A],                //  20 KEY_EN_E
        ["D", 3, 3, 0x1B],                //  21 KEY_EN_D
        ["X", 3, 4, 0x1C],                //  22 KEY_EN_X
        ["F4", 4, 0, 0x20],               //  23 KEY_EN_F4
        ["4", 4, 1, 0x21],                //  24 KEY_EN_4
        ["R", 4, 2, 0x22],                //  25 KEY_EN_R
        ["F", 4, 3, 0x23],                //  26 KEY_EN_F
        ["C", 4, 4, 0x24],                //  27 KEY_EN_C
        ["F5", 5, 0, 0x28],               //  28 KEY_EN_F5
        ["5", 5, 1, 0x29],                //  29 KEY_EN_5
        ["T", 5, 2, 0x2A],                //  30 KEY_EN_T
        ["G", 5, 3, 0x2B],                //  31 KEY_EN_G
        ["V", 5, 4, 0x2C],                //  32 KEY_EN_V
        ["Space", 5, 5, 0x2D],            //  33 KEY_EN_SPACE
        ["F6", 6, 0, 0x30],               //  34 KEY_EN_F6
        ["6", 6, 1, 0x31],                //  35 KEY_EN_6
        ["Y", 6, 2, 0x32],                //  36 KEY_EN_Y
        ["H", 6, 3, 0x33],                //  37 KEY_EN_H
        ["B", 6, 4, 0x34],                //  38 KEY_EN_B
        ["Space", 6, 5, 0x35],            //  39 KEY_EN_SPACE
        ["F7", 7, 0, 0x38],               //  40 KEY_EN_F7
        ["7", 7, 1, 0x39],                //  41 KEY_EN_7
        ["U", 7, 2, 0x3A],                //  42 KEY_EN_U
        ["J", 7, 3, 0x3B],                //  43 KEY_EN_J
        ["N", 7, 4, 0x3C],                //  44 KEY_EN_N
        ["Space", 7, 5, 0x3D],            //  45 KEY_EN_SPACE
        ["F8", 8, 0, 0x40],               //  46 KEY_EN_F8
        ["8", 8, 1, 0x41],                //  47 KEY_EN_8
        ["I", 8, 2, 0x42],                //  48 KEY_EN_I
        ["K", 8, 3, 0x43],                //  49 KEY_EN_K
        ["M", 8, 4, 0x44],                //  50 KEY_EN_M
        ["F9", 9, 0, 0x48],               //  51 KEY_EN_F9
        ["9", 9, 1, 0x49],                //  52 KEY_EN_9
        ["O", 9, 2, 0x4A],                //  53 KEY_EN_O
        ["L", 9, 3, 0x4B],                //  54 KEY_EN_L
        [",", 9, 4, 0x4C],                //  55 KEY_EN_COMMA
        ["F10", 10, 0, 0x50],             //  56 KEY_EN_F10
        ["0", 10, 1, 0x51],               //  57 KEY_EN_0
        ["P", 10, 2, 0x52],               //  58 KEY_EN_P
        [";", 10, 3, 0x53],               //  59 KEY_EN_SEMICOLON
        [".", 10, 4, 0x54],               //  60 KEY_EN_PERIOD
        ["Right Alt", 10, 5, 0x55],       //  61 KEY_EN_RIGHT_ALT
        ["F11", 11, 0, 0x58],             //  62 KEY_EN_F11
        ["-", 11, 1, 0x59],               //  63 KEY_EN_MINUS
        ["[", 11, 2, 0x5A],               //  64 KEY_EN_LEFT_BRACKET
        ["'", 11, 3, 0x5B],               //  65 KEY_EN_QUOTE
        ["/", 11, 4, 0x5C],               //  66 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 5, 0x5D],              //  67 KEY_EN_RIGHT_FUNCTION
        ["F12", 12, 0, 0x60],             //  68 KEY_EN_F12
        ["=", 12, 1, 0x61],               //  69 KEY_EN_EQUALS
        ["]", 12, 2, 0x62],               //  70 KEY_EN_RIGHT_BRACKET
        ["ISO_#", 12, 3, 0x63],           //  71 KEY_EN_POUND
        ["Right Shift", 12, 4, 0x64],     //  72 KEY_EN_RIGHT_SHIFT
        ["Right Ctrl", 12, 5, 0x65],      //  73 KEY_EN_RIGHT_CONTROL
        ["Insert", 13, 0, 0x68],          //  74 KEY_EN_INSERT
        ["Backspace", 13, 1, 0x69],       //  75 KEY_EN_BACKSPACE
        ["Enter", 13, 3, 0x6B],           //  76 KEY_EN_ISO_ENTER
        ["Left Arrow", 13, 5, 0x6D],      //  77 KEY_EN_LEFT_ARROW
        ["Del", 14, 0, 0x70],             //  78 KEY_EN_DELETE
        ["Up Arrow", 14, 4, 0x74],        //  79 KEY_EN_UP_ARROW
        ["Down Arrow", 14, 5, 0x75],      //  80 KEY_EN_DOWN_ARROW
        ["Page Up", 15, 0, 0x78],         //  81 KEY_EN_PAGE_UP
        ["NumLock", 15, 1, 0x79],         //  82 KEY_EN_NUMPAD_LOCK
        ["Num 7", 15, 2, 0x7A],           //  83 KEY_EN_NUMPAD_7
        ["Num 4", 15, 3, 0x7B],           //  84 KEY_EN_NUMPAD_4
        ["Num 1", 15, 4, 0x7C],           //  85 KEY_EN_NUMPAD_1
        ["Right Arrow", 15, 5, 0x7D],     //  86 KEY_EN_RIGHT_ARROW
        ["Page Down", 16, 0, 0x80],       //  87 KEY_EN_PAGE_DOWN
        ["Num /", 16, 1, 0x81],           //  88 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 16, 2, 0x82],           //  89 KEY_EN_NUMPAD_8
        ["Num 5", 16, 3, 0x83],           //  90 KEY_EN_NUMPAD_5
        ["Num 2", 16, 4, 0x84],           //  91 KEY_EN_NUMPAD_2
        ["Num 0", 16, 5, 0x85],           //  92 KEY_EN_NUMPAD_0
        ["Num *", 17, 1, 0x89],           //  93 KEY_EN_NUMPAD_TIMES
        ["Num 9", 17, 2, 0x8A],           //  94 KEY_EN_NUMPAD_9
        ["Num 6", 17, 3, 0x8B],           //  95 KEY_EN_NUMPAD_6
        ["Num 3", 17, 4, 0x8C],           //  96 KEY_EN_NUMPAD_3
        ["Num .", 17, 5, 0x8D],           //  97 KEY_EN_NUMPAD_PERIOD
        ["Logo", 18, 0, 0x88],            //  98 Logo
        ["Num -", 18, 1, 0x91],           //  99 KEY_EN_NUMPAD_MINUS
        ["Num +", 18, 2, 0x92],           // 100 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 18, 4, 0x94],       // 101 KEY_EN_NUMPAD_ENTER
      ]),
    }),
    // ASUS_TUF_K7_LAYOUT_NO, 102 LEDs, matrix ASUS_ROG_STRIX_SCOPE_II_96_WIRELESS_LAYOUT_KEYS_ISO
    11: Object.freeze({
      width: 19,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["\u00a7 \u00bd", 0, 1, 0x01],    //   1 KEY_NORD_HALF
        ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["F1", 1, 0, 0x08],               //   6 KEY_EN_F1
        ["1", 1, 1, 0x09],                //   7 KEY_EN_1
        ["Q", 1, 2, 0x0A],                //   8 KEY_EN_Q
        ["A", 1, 3, 0x0B],                //   9 KEY_EN_A
        ["< >", 1, 4, 0x0C],              //  10 KEY_NORD_ANGLE_BRACKET
        ["Left Win", 1, 5, 0x0D],         //  11 KEY_EN_LEFT_WINDOWS
        ["F2", 2, 0, 0x10],               //  12 KEY_EN_F2
        ["2", 2, 1, 0x11],                //  13 KEY_EN_2
        ["W", 2, 2, 0x12],                //  14 KEY_EN_W
        ["S", 2, 3, 0x13],                //  15 KEY_EN_S
        ["Z", 2, 4, 0x14],                //  16 KEY_EN_Z
        ["Left Alt", 2, 5, 0x15],         //  17 KEY_EN_LEFT_ALT
        ["F3", 3, 0, 0x18],               //  18 KEY_EN_F3
        ["3", 3, 1, 0x19],                //  19 KEY_EN_3
        ["E", 3, 2, 0x1A],                //  20 KEY_EN_E
        ["D", 3, 3, 0x1B],                //  21 KEY_EN_D
        ["X", 3, 4, 0x1C],                //  22 KEY_EN_X
        ["F4", 4, 0, 0x20],               //  23 KEY_EN_F4
        ["4", 4, 1, 0x21],                //  24 KEY_EN_4
        ["R", 4, 2, 0x22],                //  25 KEY_EN_R
        ["F", 4, 3, 0x23],                //  26 KEY_EN_F
        ["C", 4, 4, 0x24],                //  27 KEY_EN_C
        ["F5", 5, 0, 0x28],               //  28 KEY_EN_F5
        ["5", 5, 1, 0x29],                //  29 KEY_EN_5
        ["T", 5, 2, 0x2A],                //  30 KEY_EN_T
        ["G", 5, 3, 0x2B],                //  31 KEY_EN_G
        ["V", 5, 4, 0x2C],                //  32 KEY_EN_V
        ["Space", 5, 5, 0x2D],            //  33 KEY_EN_SPACE
        ["F6", 6, 0, 0x30],               //  34 KEY_EN_F6
        ["6", 6, 1, 0x31],                //  35 KEY_EN_6
        ["Y", 6, 2, 0x32],                //  36 KEY_EN_Y
        ["H", 6, 3, 0x33],                //  37 KEY_EN_H
        ["B", 6, 4, 0x34],                //  38 KEY_EN_B
        ["Space", 6, 5, 0x35],            //  39 KEY_EN_SPACE
        ["F7", 7, 0, 0x38],               //  40 KEY_EN_F7
        ["7", 7, 1, 0x39],                //  41 KEY_EN_7
        ["U", 7, 2, 0x3A],                //  42 KEY_EN_U
        ["J", 7, 3, 0x3B],                //  43 KEY_EN_J
        ["N", 7, 4, 0x3C],                //  44 KEY_EN_N
        ["Space", 7, 5, 0x3D],            //  45 KEY_EN_SPACE
        ["F8", 8, 0, 0x40],               //  46 KEY_EN_F8
        ["8", 8, 1, 0x41],                //  47 KEY_EN_8
        ["I", 8, 2, 0x42],                //  48 KEY_EN_I
        ["K", 8, 3, 0x43],                //  49 KEY_EN_K
        ["M", 8, 4, 0x44],                //  50 KEY_EN_M
        ["F9", 9, 0, 0x48],               //  51 KEY_EN_F9
        ["9", 9, 1, 0x49],                //  52 KEY_EN_9
        ["O", 9, 2, 0x4A],                //  53 KEY_EN_O
        ["L", 9, 3, 0x4B],                //  54 KEY_EN_L
        [",", 9, 4, 0x4C],                //  55 KEY_EN_COMMA
        ["F10", 10, 0, 0x50],             //  56 KEY_EN_F10
        ["0", 10, 1, 0x51],               //  57 KEY_EN_0
        ["P", 10, 2, 0x52],               //  58 KEY_EN_P
        ["\u00d6 \u00c6", 10, 3, 0x53],   //  59 KEY_NORD_O_AE
        [".", 10, 4, 0x54],               //  60 KEY_EN_PERIOD
        ["Right Alt", 10, 5, 0x55],       //  61 KEY_EN_RIGHT_ALT
        ["F11", 11, 0, 0x58],             //  62 KEY_EN_F11
        ["+ ?", 11, 1, 0x59],             //  63 KEY_NORD_PLUS_QUESTION
        ["\u00c5", 11, 2, 0x5A],          //  64 KEY_NORD_AAL
        ["\u00c4 \u00d8", 11, 3, 0x5B],   //  65 KEY_NORD_A_OE
        ["- _", 11, 4, 0x5C],             //  66 KEY_NORD_HYPHEN
        ["Fn", 11, 5, 0x5D],              //  67 KEY_EN_RIGHT_FUNCTION
        ["F12", 12, 0, 0x60],             //  68 KEY_EN_F12
        ["\u00b4 `", 12, 1, 0x61],        //  69 KEY_NORD_ACUTE_GRAVE
        ["\u00a8 ^", 12, 2, 0x62],        //  70 KEY_NORD_DOTS_CARET
        ["' *", 12, 3, 0x63],             //  71 KEY_NORD_QUOTE
        ["Right Shift", 12, 4, 0x64],     //  72 KEY_EN_RIGHT_SHIFT
        ["Right Ctrl", 12, 5, 0x65],      //  73 KEY_EN_RIGHT_CONTROL
        ["Insert", 13, 0, 0x68],          //  74 KEY_EN_INSERT
        ["Backspace", 13, 1, 0x69],       //  75 KEY_EN_BACKSPACE
        ["Enter", 13, 3, 0x6B],           //  76 KEY_EN_ISO_ENTER
        ["Left Arrow", 13, 5, 0x6D],      //  77 KEY_EN_LEFT_ARROW
        ["Del", 14, 0, 0x70],             //  78 KEY_EN_DELETE
        ["Up Arrow", 14, 4, 0x74],        //  79 KEY_EN_UP_ARROW
        ["Down Arrow", 14, 5, 0x75],      //  80 KEY_EN_DOWN_ARROW
        ["Page Up", 15, 0, 0x78],         //  81 KEY_EN_PAGE_UP
        ["NumLock", 15, 1, 0x79],         //  82 KEY_EN_NUMPAD_LOCK
        ["Num 7", 15, 2, 0x7A],           //  83 KEY_EN_NUMPAD_7
        ["Num 4", 15, 3, 0x7B],           //  84 KEY_EN_NUMPAD_4
        ["Num 1", 15, 4, 0x7C],           //  85 KEY_EN_NUMPAD_1
        ["Right Arrow", 15, 5, 0x7D],     //  86 KEY_EN_RIGHT_ARROW
        ["Page Down", 16, 0, 0x80],       //  87 KEY_EN_PAGE_DOWN
        ["Num /", 16, 1, 0x81],           //  88 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 16, 2, 0x82],           //  89 KEY_EN_NUMPAD_8
        ["Num 5", 16, 3, 0x83],           //  90 KEY_EN_NUMPAD_5
        ["Num 2", 16, 4, 0x84],           //  91 KEY_EN_NUMPAD_2
        ["Num 0", 16, 5, 0x85],           //  92 KEY_EN_NUMPAD_0
        ["Num *", 17, 1, 0x89],           //  93 KEY_EN_NUMPAD_TIMES
        ["Num 9", 17, 2, 0x8A],           //  94 KEY_EN_NUMPAD_9
        ["Num 6", 17, 3, 0x8B],           //  95 KEY_EN_NUMPAD_6
        ["Num 3", 17, 4, 0x8C],           //  96 KEY_EN_NUMPAD_3
        ["Num .", 17, 5, 0x8D],           //  97 KEY_EN_NUMPAD_PERIOD
        ["Logo", 18, 0, 0x88],            //  98 Logo
        ["Num -", 18, 1, 0x91],           //  99 KEY_EN_NUMPAD_MINUS
        ["Num +", 18, 2, 0x92],           // 100 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 18, 4, 0x94],       // 101 KEY_EN_NUMPAD_ENTER
      ]),
    }),
    // ASUS_TUF_K7_LAYOUT_US, 101 LEDs, matrix ASUS_ROG_STRIX_SCOPE_II_96_WIRELESS_LAYOUT_KEYS_ANSI
    17: Object.freeze({
      width: 19,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["F1", 1, 0, 0x08],               //   6 KEY_EN_F1
        ["1", 1, 1, 0x09],                //   7 KEY_EN_1
        ["Q", 1, 2, 0x0A],                //   8 KEY_EN_Q
        ["A", 1, 3, 0x0B],                //   9 KEY_EN_A
        ["Left Win", 1, 5, 0x0D],         //  10 KEY_EN_LEFT_WINDOWS
        ["F2", 2, 0, 0x10],               //  11 KEY_EN_F2
        ["2", 2, 1, 0x11],                //  12 KEY_EN_2
        ["W", 2, 2, 0x12],                //  13 KEY_EN_W
        ["S", 2, 3, 0x13],                //  14 KEY_EN_S
        ["Z", 2, 4, 0x14],                //  15 KEY_EN_Z
        ["Left Alt", 2, 5, 0x15],         //  16 KEY_EN_LEFT_ALT
        ["F3", 3, 0, 0x18],               //  17 KEY_EN_F3
        ["3", 3, 1, 0x19],                //  18 KEY_EN_3
        ["E", 3, 2, 0x1A],                //  19 KEY_EN_E
        ["D", 3, 3, 0x1B],                //  20 KEY_EN_D
        ["X", 3, 4, 0x1C],                //  21 KEY_EN_X
        ["F4", 4, 0, 0x20],               //  22 KEY_EN_F4
        ["4", 4, 1, 0x21],                //  23 KEY_EN_4
        ["R", 4, 2, 0x22],                //  24 KEY_EN_R
        ["F", 4, 3, 0x23],                //  25 KEY_EN_F
        ["C", 4, 4, 0x24],                //  26 KEY_EN_C
        ["F5", 5, 0, 0x28],               //  27 KEY_EN_F5
        ["5", 5, 1, 0x29],                //  28 KEY_EN_5
        ["T", 5, 2, 0x2A],                //  29 KEY_EN_T
        ["G", 5, 3, 0x2B],                //  30 KEY_EN_G
        ["V", 5, 4, 0x2C],                //  31 KEY_EN_V
        ["Space", 5, 5, 0x2D],            //  32 KEY_EN_SPACE
        ["F6", 6, 0, 0x30],               //  33 KEY_EN_F6
        ["6", 6, 1, 0x31],                //  34 KEY_EN_6
        ["Y", 6, 2, 0x32],                //  35 KEY_EN_Y
        ["H", 6, 3, 0x33],                //  36 KEY_EN_H
        ["B", 6, 4, 0x34],                //  37 KEY_EN_B
        ["Space", 6, 5, 0x35],            //  38 KEY_EN_SPACE
        ["F7", 7, 0, 0x38],               //  39 KEY_EN_F7
        ["7", 7, 1, 0x39],                //  40 KEY_EN_7
        ["U", 7, 2, 0x3A],                //  41 KEY_EN_U
        ["J", 7, 3, 0x3B],                //  42 KEY_EN_J
        ["N", 7, 4, 0x3C],                //  43 KEY_EN_N
        ["Space", 7, 5, 0x3D],            //  44 KEY_EN_SPACE
        ["F8", 8, 0, 0x40],               //  45 KEY_EN_F8
        ["8", 8, 1, 0x41],                //  46 KEY_EN_8
        ["I", 8, 2, 0x42],                //  47 KEY_EN_I
        ["K", 8, 3, 0x43],                //  48 KEY_EN_K
        ["M", 8, 4, 0x44],                //  49 KEY_EN_M
        ["F9", 9, 0, 0x48],               //  50 KEY_EN_F9
        ["9", 9, 1, 0x49],                //  51 KEY_EN_9
        ["O", 9, 2, 0x4A],                //  52 KEY_EN_O
        ["L", 9, 3, 0x4B],                //  53 KEY_EN_L
        [",", 9, 4, 0x4C],                //  54 KEY_EN_COMMA
        ["F10", 10, 0, 0x50],             //  55 KEY_EN_F10
        ["0", 10, 1, 0x51],               //  56 KEY_EN_0
        ["P", 10, 2, 0x52],               //  57 KEY_EN_P
        [";", 10, 3, 0x53],               //  58 KEY_EN_SEMICOLON
        [".", 10, 4, 0x54],               //  59 KEY_EN_PERIOD
        ["Right Alt", 10, 5, 0x55],       //  60 KEY_EN_RIGHT_ALT
        ["F11", 11, 0, 0x58],             //  61 KEY_EN_F11
        ["-", 11, 1, 0x59],               //  62 KEY_EN_MINUS
        ["[", 11, 2, 0x5A],               //  63 KEY_EN_LEFT_BRACKET
        ["'", 11, 3, 0x5B],               //  64 KEY_EN_QUOTE
        ["/", 11, 4, 0x5C],               //  65 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 5, 0x5D],              //  66 KEY_EN_RIGHT_FUNCTION
        ["F12", 12, 0, 0x60],             //  67 KEY_EN_F12
        ["=", 12, 1, 0x61],               //  68 KEY_EN_EQUALS
        ["]", 12, 2, 0x62],               //  69 KEY_EN_RIGHT_BRACKET
        ["Right Shift", 12, 4, 0x64],     //  70 KEY_EN_RIGHT_SHIFT
        ["Right Ctrl", 12, 5, 0x65],      //  71 KEY_EN_RIGHT_CONTROL
        ["Insert", 13, 0, 0x68],          //  72 KEY_EN_INSERT
        ["Backspace", 13, 1, 0x69],       //  73 KEY_EN_BACKSPACE
        ["\\", 13, 2, 0x6A],              //  74 KEY_EN_ANSI_BACK_SLASH
        ["Enter", 13, 3, 0x6B],           //  75 KEY_EN_ANSI_ENTER
        ["Left Arrow", 13, 5, 0x6D],      //  76 KEY_EN_LEFT_ARROW
        ["Del", 14, 0, 0x70],             //  77 KEY_EN_DELETE
        ["Up Arrow", 14, 4, 0x74],        //  78 KEY_EN_UP_ARROW
        ["Down Arrow", 14, 5, 0x75],      //  79 KEY_EN_DOWN_ARROW
        ["Page Up", 15, 0, 0x78],         //  80 KEY_EN_PAGE_UP
        ["NumLock", 15, 1, 0x79],         //  81 KEY_EN_NUMPAD_LOCK
        ["Num 7", 15, 2, 0x7A],           //  82 KEY_EN_NUMPAD_7
        ["Num 4", 15, 3, 0x7B],           //  83 KEY_EN_NUMPAD_4
        ["Num 1", 15, 4, 0x7C],           //  84 KEY_EN_NUMPAD_1
        ["Right Arrow", 15, 5, 0x7D],     //  85 KEY_EN_RIGHT_ARROW
        ["Page Down", 16, 0, 0x80],       //  86 KEY_EN_PAGE_DOWN
        ["Num /", 16, 1, 0x81],           //  87 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 16, 2, 0x82],           //  88 KEY_EN_NUMPAD_8
        ["Num 5", 16, 3, 0x83],           //  89 KEY_EN_NUMPAD_5
        ["Num 2", 16, 4, 0x84],           //  90 KEY_EN_NUMPAD_2
        ["Num 0", 16, 5, 0x85],           //  91 KEY_EN_NUMPAD_0
        ["Num *", 17, 1, 0x89],           //  92 KEY_EN_NUMPAD_TIMES
        ["Num 9", 17, 2, 0x8A],           //  93 KEY_EN_NUMPAD_9
        ["Num 6", 17, 3, 0x8B],           //  94 KEY_EN_NUMPAD_6
        ["Num 3", 17, 4, 0x8C],           //  95 KEY_EN_NUMPAD_3
        ["Num .", 17, 5, 0x8D],           //  96 KEY_EN_NUMPAD_PERIOD
        ["Logo", 18, 0, 0x88],            //  97 Logo
        ["Num -", 18, 1, 0x91],           //  98 KEY_EN_NUMPAD_MINUS
        ["Num +", 18, 2, 0x92],           //  99 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 18, 4, 0x94],       // 100 KEY_EN_NUMPAD_ENTER
      ]),
    }),
  }),
  AsusROGStrixFlareLayouts: Object.freeze({
    // ASUS_TUF_K7_LAYOUT_UK, 108 LEDs, matrix ASUS_ROG_STRIX_FLARE_LAYOUT_KEYS_ISO
    4: Object.freeze({
      width: 26,
      height: 6,
      leds: Object.freeze([
        ["Esc", 1, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 1, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 1, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 1, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 1, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 1, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["1", 2, 1, 0x11],                //   6 KEY_EN_1
        ["ISO_<", 2, 4, 0x0C],            //   7 KEY_EN_ISO_BACK_SLASH
        ["Left Win", 2, 5, 0x0D],         //   8 KEY_EN_LEFT_WINDOWS
        ["F1", 3, 0, 0x18],               //   9 KEY_EN_F1
        ["2", 3, 1, 0x19],                //  10 KEY_EN_2
        ["Q", 3, 2, 0x12],                //  11 KEY_EN_Q
        ["A", 3, 3, 0x13],                //  12 KEY_EN_A
        ["Z", 3, 4, 0x14],                //  13 KEY_EN_Z
        ["Left Alt", 3, 5, 0x15],         //  14 KEY_EN_LEFT_ALT
        ["F2", 4, 0, 0x20],               //  15 KEY_EN_F2
        ["3", 4, 1, 0x21],                //  16 KEY_EN_3
        ["W", 4, 2, 0x1A],                //  17 KEY_EN_W
        ["S", 4, 3, 0x1B],                //  18 KEY_EN_S
        ["X", 4, 4, 0x1C],                //  19 KEY_EN_X
        ["F3", 5, 0, 0x28],               //  20 KEY_EN_F3
        ["4", 5, 1, 0x29],                //  21 KEY_EN_4
        ["E", 5, 2, 0x22],                //  22 KEY_EN_E
        ["D", 5, 3, 0x23],                //  23 KEY_EN_D
        ["C", 5, 4, 0x24],                //  24 KEY_EN_C
        ["F4", 6, 0, 0x30],               //  25 KEY_EN_F4
        ["5", 6, 1, 0x31],                //  26 KEY_EN_5
        ["R", 6, 2, 0x2A],                //  27 KEY_EN_R
        ["F", 6, 3, 0x2B],                //  28 KEY_EN_F
        ["V", 6, 4, 0x2C],                //  29 KEY_EN_V
        ["6", 7, 1, 0x39],                //  30 KEY_EN_6
        ["T", 7, 2, 0x32],                //  31 KEY_EN_T
        ["G", 7, 3, 0x33],                //  32 KEY_EN_G
        ["B", 7, 4, 0x34],                //  33 KEY_EN_B
        ["Space", 7, 5, 0x35],            //  34 KEY_EN_SPACE
        ["F5", 8, 0, 0x40],               //  35 KEY_EN_F5
        ["7", 8, 1, 0x41],                //  36 KEY_EN_7
        ["Y", 8, 2, 0x3A],                //  37 KEY_EN_Y
        ["H", 8, 3, 0x3B],                //  38 KEY_EN_H
        ["N", 8, 4, 0x3C],                //  39 KEY_EN_N
        ["F6", 9, 0, 0x48],               //  40 KEY_EN_F6
        ["8", 9, 1, 0x49],                //  41 KEY_EN_8
        ["U", 9, 2, 0x42],                //  42 KEY_EN_U
        ["J", 9, 3, 0x43],                //  43 KEY_EN_J
        ["M", 9, 4, 0x44],                //  44 KEY_EN_M
        ["F7", 10, 0, 0x50],              //  45 KEY_EN_F7
        ["9", 10, 1, 0x51],               //  46 KEY_EN_9
        ["I", 10, 2, 0x4A],               //  47 KEY_EN_I
        ["K", 10, 3, 0x4B],               //  48 KEY_EN_K
        [",", 10, 4, 0x4C],               //  49 KEY_EN_COMMA
        ["F8", 11, 0, 0x58],              //  50 KEY_EN_F8
        ["0", 11, 1, 0x59],               //  51 KEY_EN_0
        ["O", 11, 2, 0x52],               //  52 KEY_EN_O
        ["L", 11, 3, 0x53],               //  53 KEY_EN_L
        [".", 11, 4, 0x54],               //  54 KEY_EN_PERIOD
        ["Right Alt", 11, 5, 0x4D],       //  55 KEY_EN_RIGHT_ALT
        ["F9", 12, 0, 0x60],              //  56 KEY_EN_F9
        ["-", 12, 1, 0x61],               //  57 KEY_EN_MINUS
        ["P", 12, 2, 0x5A],               //  58 KEY_EN_P
        [";", 12, 3, 0x5B],               //  59 KEY_EN_SEMICOLON
        ["/", 12, 4, 0x5C],               //  60 KEY_EN_FORWARD_SLASH
        ["Fn", 12, 5, 0x5D],              //  61 KEY_EN_RIGHT_FUNCTION
        ["F10", 13, 0, 0x68],             //  62 KEY_EN_F10
        ["=", 13, 1, 0x69],               //  63 KEY_EN_EQUALS
        ["[", 13, 2, 0x62],               //  64 KEY_EN_LEFT_BRACKET
        ["'", 13, 3, 0x63],               //  65 KEY_EN_QUOTE
        ["Menu", 13, 5, 0x65],            //  66 KEY_EN_MENU
        ["F11", 14, 0, 0x70],             //  67 KEY_EN_F11
        ["Backspace", 14, 1, 0x79],       //  68 KEY_EN_BACKSPACE
        ["]", 14, 2, 0x6A],               //  69 KEY_EN_RIGHT_BRACKET
        ["ISO_#", 14, 3, 0x6B],           //  70 KEY_EN_POUND
        ["Right Shift", 14, 4, 0x7C],     //  71 KEY_EN_RIGHT_SHIFT
        ["F12", 15, 0, 0x78],             //  72 KEY_EN_F12
        ["Enter", 15, 3, 0x7B],           //  73 KEY_EN_ISO_ENTER
        ["Right Ctrl", 15, 5, 0x7D],      //  74 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 17, 0, 0x80],    //  75 KEY_EN_PRINT_SCREEN
        ["Insert", 17, 1, 0x81],          //  76 KEY_EN_INSERT
        ["Del", 17, 2, 0x82],             //  77 KEY_EN_DELETE
        ["Left Arrow", 17, 5, 0x85],      //  78 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 18, 0, 0x88],     //  79 KEY_EN_SCROLL_LOCK
        ["Home", 18, 1, 0x89],            //  80 KEY_EN_HOME
        ["End", 18, 2, 0x8A],             //  81 KEY_EN_END
        ["Up Arrow", 18, 4, 0x8C],        //  82 KEY_EN_UP_ARROW
        ["Down Arrow", 18, 5, 0x8D],      //  83 KEY_EN_DOWN_ARROW
        ["Pause Break", 19, 0, 0x90],     //  84 KEY_EN_PAUSE_BREAK
        ["Page Up", 19, 1, 0x91],         //  85 KEY_EN_PAGE_UP
        ["Page Down", 19, 2, 0x92],       //  86 KEY_EN_PAGE_DOWN
        ["Right Arrow", 19, 5, 0x95],     //  87 KEY_EN_RIGHT_ARROW
        ["NumLock", 21, 1, 0x99],         //  88 KEY_EN_NUMPAD_LOCK
        ["Num 7", 21, 2, 0x9A],           //  89 KEY_EN_NUMPAD_7
        ["Num 4", 21, 3, 0x9B],           //  90 KEY_EN_NUMPAD_4
        ["Num 1", 21, 4, 0x9C],           //  91 KEY_EN_NUMPAD_1
        ["Num 0", 21, 5, 0x9D],           //  92 KEY_EN_NUMPAD_0
        ["Num /", 22, 1, 0xA1],           //  93 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 22, 2, 0xA2],           //  94 KEY_EN_NUMPAD_8
        ["Num 5", 22, 3, 0xA3],           //  95 KEY_EN_NUMPAD_5
        ["Num 2", 22, 4, 0xA4],           //  96 KEY_EN_NUMPAD_2
        ["Num *", 23, 1, 0xA9],           //  97 KEY_EN_NUMPAD_TIMES
        ["Num 9", 23, 2, 0xAA],           //  98 KEY_EN_NUMPAD_9
        ["Num 6", 23, 3, 0xAB],           //  99 KEY_EN_NUMPAD_6
        ["Num 3", 23, 4, 0xAC],           // 100 KEY_EN_NUMPAD_3
        ["Num .", 23, 5, 0xAD],           // 101 KEY_EN_NUMPAD_PERIOD
        ["Num -", 24, 1, 0xB1],           // 102 KEY_EN_NUMPAD_MINUS
        ["Num +", 24, 2, 0xB2],           // 103 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 24, 4, 0xB4],       // 104 KEY_EN_NUMPAD_ENTER
        ["Logo", 21, 0, 0xB8],            // 105 Logo
        ["Underglow left", 0, 3, 0xB9],   // 106 Underglow left
        ["Underglow right", 25, 3, 0xBA], // 107 Underglow right
      ]),
    }),
    // ASUS_TUF_K7_LAYOUT_US, 107 LEDs, matrix ASUS_ROG_STRIX_FLARE_LAYOUT_KEYS_ANSI
    17: Object.freeze({
      width: 26,
      height: 6,
      leds: Object.freeze([
        ["Esc", 1, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 1, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 1, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 1, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 1, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 1, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["1", 2, 1, 0x11],                //   6 KEY_EN_1
        ["Left Win", 2, 5, 0x0D],         //   7 KEY_EN_LEFT_WINDOWS
        ["F1", 3, 0, 0x18],               //   8 KEY_EN_F1
        ["2", 3, 1, 0x19],                //   9 KEY_EN_2
        ["Q", 3, 2, 0x12],                //  10 KEY_EN_Q
        ["A", 3, 3, 0x13],                //  11 KEY_EN_A
        ["Z", 3, 4, 0x14],                //  12 KEY_EN_Z
        ["Left Alt", 3, 5, 0x15],         //  13 KEY_EN_LEFT_ALT
        ["F2", 4, 0, 0x20],               //  14 KEY_EN_F2
        ["3", 4, 1, 0x21],                //  15 KEY_EN_3
        ["W", 4, 2, 0x1A],                //  16 KEY_EN_W
        ["S", 4, 3, 0x1B],                //  17 KEY_EN_S
        ["X", 4, 4, 0x1C],                //  18 KEY_EN_X
        ["F3", 5, 0, 0x28],               //  19 KEY_EN_F3
        ["4", 5, 1, 0x29],                //  20 KEY_EN_4
        ["E", 5, 2, 0x22],                //  21 KEY_EN_E
        ["D", 5, 3, 0x23],                //  22 KEY_EN_D
        ["C", 5, 4, 0x24],                //  23 KEY_EN_C
        ["F4", 6, 0, 0x30],               //  24 KEY_EN_F4
        ["5", 6, 1, 0x31],                //  25 KEY_EN_5
        ["R", 6, 2, 0x2A],                //  26 KEY_EN_R
        ["F", 6, 3, 0x2B],                //  27 KEY_EN_F
        ["V", 6, 4, 0x2C],                //  28 KEY_EN_V
        ["6", 7, 1, 0x39],                //  29 KEY_EN_6
        ["T", 7, 2, 0x32],                //  30 KEY_EN_T
        ["G", 7, 3, 0x33],                //  31 KEY_EN_G
        ["B", 7, 4, 0x34],                //  32 KEY_EN_B
        ["Space", 7, 5, 0x35],            //  33 KEY_EN_SPACE
        ["F5", 8, 0, 0x40],               //  34 KEY_EN_F5
        ["7", 8, 1, 0x41],                //  35 KEY_EN_7
        ["Y", 8, 2, 0x3A],                //  36 KEY_EN_Y
        ["H", 8, 3, 0x3B],                //  37 KEY_EN_H
        ["N", 8, 4, 0x3C],                //  38 KEY_EN_N
        ["F6", 9, 0, 0x48],               //  39 KEY_EN_F6
        ["8", 9, 1, 0x49],                //  40 KEY_EN_8
        ["U", 9, 2, 0x42],                //  41 KEY_EN_U
        ["J", 9, 3, 0x43],                //  42 KEY_EN_J
        ["M", 9, 4, 0x44],                //  43 KEY_EN_M
        ["F7", 10, 0, 0x50],              //  44 KEY_EN_F7
        ["9", 10, 1, 0x51],               //  45 KEY_EN_9
        ["I", 10, 2, 0x4A],               //  46 KEY_EN_I
        ["K", 10, 3, 0x4B],               //  47 KEY_EN_K
        [",", 10, 4, 0x4C],               //  48 KEY_EN_COMMA
        ["F8", 11, 0, 0x58],              //  49 KEY_EN_F8
        ["0", 11, 1, 0x59],               //  50 KEY_EN_0
        ["O", 11, 2, 0x52],               //  51 KEY_EN_O
        ["L", 11, 3, 0x53],               //  52 KEY_EN_L
        [".", 11, 4, 0x54],               //  53 KEY_EN_PERIOD
        ["Right Alt", 11, 5, 0x4D],       //  54 KEY_EN_RIGHT_ALT
        ["F9", 12, 0, 0x60],              //  55 KEY_EN_F9
        ["-", 12, 1, 0x61],               //  56 KEY_EN_MINUS
        ["P", 12, 2, 0x5A],               //  57 KEY_EN_P
        [";", 12, 3, 0x5B],               //  58 KEY_EN_SEMICOLON
        ["/", 12, 4, 0x5C],               //  59 KEY_EN_FORWARD_SLASH
        ["Fn", 12, 5, 0x5D],              //  60 KEY_EN_RIGHT_FUNCTION
        ["F10", 13, 0, 0x68],             //  61 KEY_EN_F10
        ["=", 13, 1, 0x69],               //  62 KEY_EN_EQUALS
        ["[", 13, 2, 0x62],               //  63 KEY_EN_LEFT_BRACKET
        ["'", 13, 3, 0x63],               //  64 KEY_EN_QUOTE
        ["Menu", 13, 5, 0x65],            //  65 KEY_EN_MENU
        ["F11", 14, 0, 0x70],             //  66 KEY_EN_F11
        ["Backspace", 14, 1, 0x79],       //  67 KEY_EN_BACKSPACE
        ["]", 14, 2, 0x6A],               //  68 KEY_EN_RIGHT_BRACKET
        ["Right Shift", 14, 4, 0x7C],     //  69 KEY_EN_RIGHT_SHIFT
        ["F12", 15, 0, 0x78],             //  70 KEY_EN_F12
        ["\\", 15, 2, 0x7A],              //  71 KEY_EN_ANSI_BACK_SLASH
        ["Enter", 15, 3, 0x7B],           //  72 KEY_EN_ANSI_ENTER
        ["Right Ctrl", 15, 5, 0x7D],      //  73 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 17, 0, 0x80],    //  74 KEY_EN_PRINT_SCREEN
        ["Insert", 17, 1, 0x81],          //  75 KEY_EN_INSERT
        ["Del", 17, 2, 0x82],             //  76 KEY_EN_DELETE
        ["Left Arrow", 17, 5, 0x85],      //  77 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 18, 0, 0x88],     //  78 KEY_EN_SCROLL_LOCK
        ["Home", 18, 1, 0x89],            //  79 KEY_EN_HOME
        ["End", 18, 2, 0x8A],             //  80 KEY_EN_END
        ["Up Arrow", 18, 4, 0x8C],        //  81 KEY_EN_UP_ARROW
        ["Down Arrow", 18, 5, 0x8D],      //  82 KEY_EN_DOWN_ARROW
        ["Pause Break", 19, 0, 0x90],     //  83 KEY_EN_PAUSE_BREAK
        ["Page Up", 19, 1, 0x91],         //  84 KEY_EN_PAGE_UP
        ["Page Down", 19, 2, 0x92],       //  85 KEY_EN_PAGE_DOWN
        ["Right Arrow", 19, 5, 0x95],     //  86 KEY_EN_RIGHT_ARROW
        ["NumLock", 21, 1, 0x99],         //  87 KEY_EN_NUMPAD_LOCK
        ["Num 7", 21, 2, 0x9A],           //  88 KEY_EN_NUMPAD_7
        ["Num 4", 21, 3, 0x9B],           //  89 KEY_EN_NUMPAD_4
        ["Num 1", 21, 4, 0x9C],           //  90 KEY_EN_NUMPAD_1
        ["Num 0", 21, 5, 0x9D],           //  91 KEY_EN_NUMPAD_0
        ["Num /", 22, 1, 0xA1],           //  92 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 22, 2, 0xA2],           //  93 KEY_EN_NUMPAD_8
        ["Num 5", 22, 3, 0xA3],           //  94 KEY_EN_NUMPAD_5
        ["Num 2", 22, 4, 0xA4],           //  95 KEY_EN_NUMPAD_2
        ["Num *", 23, 1, 0xA9],           //  96 KEY_EN_NUMPAD_TIMES
        ["Num 9", 23, 2, 0xAA],           //  97 KEY_EN_NUMPAD_9
        ["Num 6", 23, 3, 0xAB],           //  98 KEY_EN_NUMPAD_6
        ["Num 3", 23, 4, 0xAC],           //  99 KEY_EN_NUMPAD_3
        ["Num .", 23, 5, 0xAD],           // 100 KEY_EN_NUMPAD_PERIOD
        ["Num -", 24, 1, 0xB1],           // 101 KEY_EN_NUMPAD_MINUS
        ["Num +", 24, 2, 0xB2],           // 102 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 24, 4, 0xB4],       // 103 KEY_EN_NUMPAD_ENTER
        ["Logo", 21, 0, 0xB8],            // 104 Logo
        ["Underglow left", 0, 3, 0xB9],   // 105 Underglow left
        ["Underglow right", 25, 3, 0xBA], // 106 Underglow right
      ]),
    }),
  }),
  AsusROGStrixFlareIILayouts: Object.freeze({
    // ASUS_TUF_K7_LAYOUT_UK, 107 LEDs, matrix ASUS_ROG_STRIX_FLARE_II_LAYOUT_KEYS_ISO
    4: Object.freeze({
      width: 30,
      height: 7,
      leds: Object.freeze([
        ["Esc", 0, 1, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 2, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 3, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 4, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 5, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 6, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["1", 1, 2, 0x11],                //   6 KEY_EN_1
        ["ISO_<", 1, 5, 0x0C],            //   7 KEY_EN_ISO_BACK_SLASH
        ["Left Win", 1, 6, 0x15],         //   8 KEY_EN_LEFT_WINDOWS
        ["F1", 2, 1, 0x18],               //   9 KEY_EN_F1
        ["2", 2, 2, 0x19],                //  10 KEY_EN_2
        ["Q", 2, 3, 0x12],                //  11 KEY_EN_Q
        ["A", 2, 4, 0x13],                //  12 KEY_EN_A
        ["Z", 2, 5, 0x14],                //  13 KEY_EN_Z
        ["Left Alt", 2, 6, 0x1D],         //  14 KEY_EN_LEFT_ALT
        ["F2", 3, 1, 0x20],               //  15 KEY_EN_F2
        ["3", 3, 2, 0x21],                //  16 KEY_EN_3
        ["W", 3, 3, 0x1A],                //  17 KEY_EN_W
        ["S", 3, 4, 0x1B],                //  18 KEY_EN_S
        ["X", 3, 5, 0x1C],                //  19 KEY_EN_X
        ["F3", 4, 1, 0x28],               //  20 KEY_EN_F3
        ["4", 4, 2, 0x29],                //  21 KEY_EN_4
        ["E", 4, 3, 0x22],                //  22 KEY_EN_E
        ["D", 4, 4, 0x23],                //  23 KEY_EN_D
        ["C", 4, 5, 0x24],                //  24 KEY_EN_C
        ["F4", 5, 1, 0x30],               //  25 KEY_EN_F4
        ["5", 5, 2, 0x31],                //  26 KEY_EN_5
        ["R", 5, 3, 0x2A],                //  27 KEY_EN_R
        ["F", 5, 4, 0x2B],                //  28 KEY_EN_F
        ["V", 5, 5, 0x2C],                //  29 KEY_EN_V
        ["6", 6, 2, 0x39],                //  30 KEY_EN_6
        ["T", 6, 3, 0x32],                //  31 KEY_EN_T
        ["G", 6, 4, 0x33],                //  32 KEY_EN_G
        ["B", 6, 5, 0x34],                //  33 KEY_EN_B
        ["Space", 6, 6, 0x35],            //  34 KEY_EN_SPACE
        ["F5", 7, 1, 0x40],               //  35 KEY_EN_F5
        ["7", 7, 2, 0x41],                //  36 KEY_EN_7
        ["Y", 7, 3, 0x3A],                //  37 KEY_EN_Y
        ["H", 7, 4, 0x3B],                //  38 KEY_EN_H
        ["N", 7, 5, 0x3C],                //  39 KEY_EN_N
        ["F6", 8, 1, 0x48],               //  40 KEY_EN_F6
        ["8", 8, 2, 0x49],                //  41 KEY_EN_8
        ["U", 8, 3, 0x42],                //  42 KEY_EN_U
        ["J", 8, 4, 0x43],                //  43 KEY_EN_J
        ["M", 8, 5, 0x44],                //  44 KEY_EN_M
        ["F7", 9, 1, 0x50],               //  45 KEY_EN_F7
        ["9", 9, 2, 0x51],                //  46 KEY_EN_9
        ["I", 9, 3, 0x4A],                //  47 KEY_EN_I
        ["K", 9, 4, 0x4B],                //  48 KEY_EN_K
        [",", 9, 5, 0x4C],                //  49 KEY_EN_COMMA
        ["F8", 10, 1, 0x58],              //  50 KEY_EN_F8
        ["0", 10, 2, 0x59],               //  51 KEY_EN_0
        ["O", 10, 3, 0x52],               //  52 KEY_EN_O
        ["L", 10, 4, 0x53],               //  53 KEY_EN_L
        [".", 10, 5, 0x54],               //  54 KEY_EN_PERIOD
        ["Right Alt", 10, 6, 0x4D],       //  55 KEY_EN_RIGHT_ALT
        ["F9", 11, 1, 0x60],              //  56 KEY_EN_F9
        ["-", 11, 2, 0x61],               //  57 KEY_EN_MINUS
        ["P", 11, 3, 0x5A],               //  58 KEY_EN_P
        [";", 11, 4, 0x5B],               //  59 KEY_EN_SEMICOLON
        ["/", 11, 5, 0x5C],               //  60 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 6, 0x5D],              //  61 KEY_EN_RIGHT_FUNCTION
        ["F10", 12, 1, 0x68],             //  62 KEY_EN_F10
        ["=", 12, 2, 0x69],               //  63 KEY_EN_EQUALS
        ["[", 12, 3, 0x62],               //  64 KEY_EN_LEFT_BRACKET
        ["'", 12, 4, 0x63],               //  65 KEY_EN_QUOTE
        ["Menu", 12, 6, 0x65],            //  66 KEY_EN_MENU
        ["F11", 13, 1, 0x70],             //  67 KEY_EN_F11
        ["Backspace", 13, 2, 0x79],       //  68 KEY_EN_BACKSPACE
        ["]", 13, 3, 0x6A],               //  69 KEY_EN_RIGHT_BRACKET
        ["ISO_#", 13, 4, 0x6B],           //  70 KEY_EN_POUND
        ["Right Shift", 13, 5, 0x7C],     //  71 KEY_EN_RIGHT_SHIFT
        ["F12", 14, 1, 0x78],             //  72 KEY_EN_F12
        ["Enter", 14, 4, 0x7B],           //  73 KEY_EN_ISO_ENTER
        ["Right Ctrl", 14, 6, 0x7D],      //  74 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 19, 1, 0x80],    //  75 KEY_EN_PRINT_SCREEN
        ["Insert", 19, 2, 0x81],          //  76 KEY_EN_INSERT
        ["Del", 19, 3, 0x82],             //  77 KEY_EN_DELETE
        ["Left Arrow", 19, 6, 0x85],      //  78 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 20, 1, 0x88],     //  79 KEY_EN_SCROLL_LOCK
        ["Home", 20, 2, 0x89],            //  80 KEY_EN_HOME
        ["End", 20, 3, 0x8A],             //  81 KEY_EN_END
        ["Up Arrow", 20, 5, 0x8C],        //  82 KEY_EN_UP_ARROW
        ["Down Arrow", 20, 6, 0x8D],      //  83 KEY_EN_DOWN_ARROW
        ["Pause Break", 21, 1, 0x90],     //  84 KEY_EN_PAUSE_BREAK
        ["Page Up", 21, 2, 0x91],         //  85 KEY_EN_PAGE_UP
        ["Page Down", 21, 3, 0x92],       //  86 KEY_EN_PAGE_DOWN
        ["Right Arrow", 21, 6, 0x95],     //  87 KEY_EN_RIGHT_ARROW
        ["NumLock", 26, 2, 0x99],         //  88 KEY_EN_NUMPAD_LOCK
        ["Num 7", 26, 3, 0x9A],           //  89 KEY_EN_NUMPAD_7
        ["Num 4", 26, 4, 0x9B],           //  90 KEY_EN_NUMPAD_4
        ["Num 1", 26, 5, 0x9C],           //  91 KEY_EN_NUMPAD_1
        ["Num 0", 26, 6, 0x9D],           //  92 KEY_EN_NUMPAD_0
        ["Num /", 27, 2, 0xA1],           //  93 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 27, 3, 0xA2],           //  94 KEY_EN_NUMPAD_8
        ["Num 5", 27, 4, 0xA3],           //  95 KEY_EN_NUMPAD_5
        ["Num 2", 27, 5, 0xA4],           //  96 KEY_EN_NUMPAD_2
        ["Num *", 28, 2, 0xA9],           //  97 KEY_EN_NUMPAD_TIMES
        ["Num 9", 28, 3, 0xAA],           //  98 KEY_EN_NUMPAD_9
        ["Num 6", 28, 4, 0xAB],           //  99 KEY_EN_NUMPAD_6
        ["Num 3", 28, 5, 0xAC],           // 100 KEY_EN_NUMPAD_3
        ["Num .", 28, 6, 0xAD],           // 101 KEY_EN_NUMPAD_PERIOD
        ["Num -", 29, 2, 0xB1],           // 102 KEY_EN_NUMPAD_MINUS
        ["Num +", 29, 3, 0xB2],           // 103 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 29, 5, 0xB4],       // 104 KEY_EN_NUMPAD_ENTER
        ["Logo 1", 26, 0, 0xB0],          // 105 Logo 1
        ["Logo 2", 27, 0, 0xA8],          // 106 Logo 2
      ]),
    }),
    // ASUS_TUF_K7_LAYOUT_US, 106 LEDs, matrix ASUS_ROG_STRIX_FLARE_II_LAYOUT_KEYS_ANSI
    17: Object.freeze({
      width: 30,
      height: 7,
      leds: Object.freeze([
        ["Esc", 0, 1, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 2, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 3, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 4, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 5, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 6, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["1", 1, 2, 0x11],                //   6 KEY_EN_1
        ["Left Win", 1, 6, 0x15],         //   7 KEY_EN_LEFT_WINDOWS
        ["F1", 2, 1, 0x18],               //   8 KEY_EN_F1
        ["2", 2, 2, 0x19],                //   9 KEY_EN_2
        ["Q", 2, 3, 0x12],                //  10 KEY_EN_Q
        ["A", 2, 4, 0x13],                //  11 KEY_EN_A
        ["Z", 2, 5, 0x14],                //  12 KEY_EN_Z
        ["Left Alt", 2, 6, 0x1D],         //  13 KEY_EN_LEFT_ALT
        ["F2", 3, 1, 0x20],               //  14 KEY_EN_F2
        ["3", 3, 2, 0x21],                //  15 KEY_EN_3
        ["W", 3, 3, 0x1A],                //  16 KEY_EN_W
        ["S", 3, 4, 0x1B],                //  17 KEY_EN_S
        ["X", 3, 5, 0x1C],                //  18 KEY_EN_X
        ["F3", 4, 1, 0x28],               //  19 KEY_EN_F3
        ["4", 4, 2, 0x29],                //  20 KEY_EN_4
        ["E", 4, 3, 0x22],                //  21 KEY_EN_E
        ["D", 4, 4, 0x23],                //  22 KEY_EN_D
        ["C", 4, 5, 0x24],                //  23 KEY_EN_C
        ["F4", 5, 1, 0x30],               //  24 KEY_EN_F4
        ["5", 5, 2, 0x31],                //  25 KEY_EN_5
        ["R", 5, 3, 0x2A],                //  26 KEY_EN_R
        ["F", 5, 4, 0x2B],                //  27 KEY_EN_F
        ["V", 5, 5, 0x2C],                //  28 KEY_EN_V
        ["6", 6, 2, 0x39],                //  29 KEY_EN_6
        ["T", 6, 3, 0x32],                //  30 KEY_EN_T
        ["G", 6, 4, 0x33],                //  31 KEY_EN_G
        ["B", 6, 5, 0x34],                //  32 KEY_EN_B
        ["Space", 6, 6, 0x35],            //  33 KEY_EN_SPACE
        ["F5", 7, 1, 0x40],               //  34 KEY_EN_F5
        ["7", 7, 2, 0x41],                //  35 KEY_EN_7
        ["Y", 7, 3, 0x3A],                //  36 KEY_EN_Y
        ["H", 7, 4, 0x3B],                //  37 KEY_EN_H
        ["N", 7, 5, 0x3C],                //  38 KEY_EN_N
        ["F6", 8, 1, 0x48],               //  39 KEY_EN_F6
        ["8", 8, 2, 0x49],                //  40 KEY_EN_8
        ["U", 8, 3, 0x42],                //  41 KEY_EN_U
        ["J", 8, 4, 0x43],                //  42 KEY_EN_J
        ["M", 8, 5, 0x44],                //  43 KEY_EN_M
        ["F7", 9, 1, 0x50],               //  44 KEY_EN_F7
        ["9", 9, 2, 0x51],                //  45 KEY_EN_9
        ["I", 9, 3, 0x4A],                //  46 KEY_EN_I
        ["K", 9, 4, 0x4B],                //  47 KEY_EN_K
        [",", 9, 5, 0x4C],                //  48 KEY_EN_COMMA
        ["F8", 10, 1, 0x58],              //  49 KEY_EN_F8
        ["0", 10, 2, 0x59],               //  50 KEY_EN_0
        ["O", 10, 3, 0x52],               //  51 KEY_EN_O
        ["L", 10, 4, 0x53],               //  52 KEY_EN_L
        [".", 10, 5, 0x54],               //  53 KEY_EN_PERIOD
        ["Right Alt", 10, 6, 0x4D],       //  54 KEY_EN_RIGHT_ALT
        ["F9", 11, 1, 0x60],              //  55 KEY_EN_F9
        ["-", 11, 2, 0x61],               //  56 KEY_EN_MINUS
        ["P", 11, 3, 0x5A],               //  57 KEY_EN_P
        [";", 11, 4, 0x5B],               //  58 KEY_EN_SEMICOLON
        ["/", 11, 5, 0x5C],               //  59 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 6, 0x5D],              //  60 KEY_EN_RIGHT_FUNCTION
        ["F10", 12, 1, 0x68],             //  61 KEY_EN_F10
        ["=", 12, 2, 0x69],               //  62 KEY_EN_EQUALS
        ["[", 12, 3, 0x62],               //  63 KEY_EN_LEFT_BRACKET
        ["'", 12, 4, 0x63],               //  64 KEY_EN_QUOTE
        ["Menu", 12, 6, 0x65],            //  65 KEY_EN_MENU
        ["F11", 13, 1, 0x70],             //  66 KEY_EN_F11
        ["Backspace", 13, 2, 0x79],       //  67 KEY_EN_BACKSPACE
        ["]", 13, 3, 0x6A],               //  68 KEY_EN_RIGHT_BRACKET
        ["Right Shift", 13, 5, 0x7C],     //  69 KEY_EN_RIGHT_SHIFT
        ["F12", 14, 1, 0x78],             //  70 KEY_EN_F12
        ["\\", 14, 3, 0x7A],              //  71 KEY_EN_ANSI_BACK_SLASH
        ["Enter", 14, 4, 0x7B],           //  72 KEY_EN_ANSI_ENTER
        ["Right Ctrl", 14, 6, 0x7D],      //  73 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 19, 1, 0x80],    //  74 KEY_EN_PRINT_SCREEN
        ["Insert", 19, 2, 0x81],          //  75 KEY_EN_INSERT
        ["Del", 19, 3, 0x82],             //  76 KEY_EN_DELETE
        ["Left Arrow", 19, 6, 0x85],      //  77 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 20, 1, 0x88],     //  78 KEY_EN_SCROLL_LOCK
        ["Home", 20, 2, 0x89],            //  79 KEY_EN_HOME
        ["End", 20, 3, 0x8A],             //  80 KEY_EN_END
        ["Up Arrow", 20, 5, 0x8C],        //  81 KEY_EN_UP_ARROW
        ["Down Arrow", 20, 6, 0x8D],      //  82 KEY_EN_DOWN_ARROW
        ["Pause Break", 21, 1, 0x90],     //  83 KEY_EN_PAUSE_BREAK
        ["Page Up", 21, 2, 0x91],         //  84 KEY_EN_PAGE_UP
        ["Page Down", 21, 3, 0x92],       //  85 KEY_EN_PAGE_DOWN
        ["Right Arrow", 21, 6, 0x95],     //  86 KEY_EN_RIGHT_ARROW
        ["NumLock", 26, 2, 0x99],         //  87 KEY_EN_NUMPAD_LOCK
        ["Num 7", 26, 3, 0x9A],           //  88 KEY_EN_NUMPAD_7
        ["Num 4", 26, 4, 0x9B],           //  89 KEY_EN_NUMPAD_4
        ["Num 1", 26, 5, 0x9C],           //  90 KEY_EN_NUMPAD_1
        ["Num 0", 26, 6, 0x9D],           //  91 KEY_EN_NUMPAD_0
        ["Num /", 27, 2, 0xA1],           //  92 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 27, 3, 0xA2],           //  93 KEY_EN_NUMPAD_8
        ["Num 5", 27, 4, 0xA3],           //  94 KEY_EN_NUMPAD_5
        ["Num 2", 27, 5, 0xA4],           //  95 KEY_EN_NUMPAD_2
        ["Num *", 28, 2, 0xA9],           //  96 KEY_EN_NUMPAD_TIMES
        ["Num 9", 28, 3, 0xAA],           //  97 KEY_EN_NUMPAD_9
        ["Num 6", 28, 4, 0xAB],           //  98 KEY_EN_NUMPAD_6
        ["Num 3", 28, 5, 0xAC],           //  99 KEY_EN_NUMPAD_3
        ["Num .", 28, 6, 0xAD],           // 100 KEY_EN_NUMPAD_PERIOD
        ["Num -", 29, 2, 0xB1],           // 101 KEY_EN_NUMPAD_MINUS
        ["Num +", 29, 3, 0xB2],           // 102 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 29, 5, 0xB4],       // 103 KEY_EN_NUMPAD_ENTER
        ["Logo 1", 26, 0, 0xB0],          // 104 Logo 1
        ["Logo 2", 27, 0, 0xA8],          // 105 Logo 2
      ]),
    }),
  }),
  AsusROGStrixFlareIIAnimateLayouts: Object.freeze({
    // ASUS_TUF_K7_LAYOUT_UK, 135 LEDs, matrix ASUS_ROG_STRIX_FLARE_II_ANIMATE_LAYOUT_KEYS_ISO
    4: Object.freeze({
      width: 30,
      height: 7,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["1", 1, 1, 0x11],                //   6 KEY_EN_1
        ["ISO_<", 1, 4, 0x0C],            //   7 KEY_EN_ISO_BACK_SLASH
        ["Left Win", 1, 5, 0x15],         //   8 KEY_EN_LEFT_WINDOWS
        ["F1", 2, 0, 0x18],               //   9 KEY_EN_F1
        ["2", 2, 1, 0x19],                //  10 KEY_EN_2
        ["Q", 2, 2, 0x12],                //  11 KEY_EN_Q
        ["A", 2, 3, 0x13],                //  12 KEY_EN_A
        ["Z", 2, 4, 0x14],                //  13 KEY_EN_Z
        ["Left Alt", 2, 5, 0x1D],         //  14 KEY_EN_LEFT_ALT
        ["F2", 3, 0, 0x20],               //  15 KEY_EN_F2
        ["3", 3, 1, 0x21],                //  16 KEY_EN_3
        ["W", 3, 2, 0x1A],                //  17 KEY_EN_W
        ["S", 3, 3, 0x1B],                //  18 KEY_EN_S
        ["X", 3, 4, 0x1C],                //  19 KEY_EN_X
        ["F3", 4, 0, 0x28],               //  20 KEY_EN_F3
        ["4", 4, 1, 0x29],                //  21 KEY_EN_4
        ["E", 4, 2, 0x22],                //  22 KEY_EN_E
        ["D", 4, 3, 0x23],                //  23 KEY_EN_D
        ["C", 4, 4, 0x24],                //  24 KEY_EN_C
        ["F4", 5, 0, 0x30],               //  25 KEY_EN_F4
        ["5", 5, 1, 0x31],                //  26 KEY_EN_5
        ["R", 5, 2, 0x2A],                //  27 KEY_EN_R
        ["F", 5, 3, 0x2B],                //  28 KEY_EN_F
        ["V", 5, 4, 0x2C],                //  29 KEY_EN_V
        ["6", 6, 1, 0x39],                //  30 KEY_EN_6
        ["T", 6, 2, 0x32],                //  31 KEY_EN_T
        ["G", 6, 3, 0x33],                //  32 KEY_EN_G
        ["B", 6, 4, 0x34],                //  33 KEY_EN_B
        ["Space", 6, 5, 0x35],            //  34 KEY_EN_SPACE
        ["F5", 7, 0, 0x40],               //  35 KEY_EN_F5
        ["7", 7, 1, 0x41],                //  36 KEY_EN_7
        ["Y", 7, 2, 0x3A],                //  37 KEY_EN_Y
        ["H", 7, 3, 0x3B],                //  38 KEY_EN_H
        ["N", 7, 4, 0x3C],                //  39 KEY_EN_N
        ["F6", 8, 0, 0x48],               //  40 KEY_EN_F6
        ["8", 8, 1, 0x49],                //  41 KEY_EN_8
        ["U", 8, 2, 0x42],                //  42 KEY_EN_U
        ["J", 8, 3, 0x43],                //  43 KEY_EN_J
        ["M", 8, 4, 0x44],                //  44 KEY_EN_M
        ["F7", 9, 0, 0x50],               //  45 KEY_EN_F7
        ["9", 9, 1, 0x51],                //  46 KEY_EN_9
        ["I", 9, 2, 0x4A],                //  47 KEY_EN_I
        ["K", 9, 3, 0x4B],                //  48 KEY_EN_K
        [",", 9, 4, 0x4C],                //  49 KEY_EN_COMMA
        ["F8", 10, 0, 0x58],              //  50 KEY_EN_F8
        ["0", 10, 1, 0x59],               //  51 KEY_EN_0
        ["O", 10, 2, 0x52],               //  52 KEY_EN_O
        ["L", 10, 3, 0x53],               //  53 KEY_EN_L
        [".", 10, 4, 0x54],               //  54 KEY_EN_PERIOD
        ["Right Alt", 10, 5, 0x4D],       //  55 KEY_EN_RIGHT_ALT
        ["F9", 11, 0, 0x60],              //  56 KEY_EN_F9
        ["-", 11, 1, 0x61],               //  57 KEY_EN_MINUS
        ["P", 11, 2, 0x5A],               //  58 KEY_EN_P
        [";", 11, 3, 0x5B],               //  59 KEY_EN_SEMICOLON
        ["/", 11, 4, 0x5C],               //  60 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 5, 0x5D],              //  61 KEY_EN_RIGHT_FUNCTION
        ["F10", 12, 0, 0x68],             //  62 KEY_EN_F10
        ["=", 12, 1, 0x69],               //  63 KEY_EN_EQUALS
        ["[", 12, 2, 0x62],               //  64 KEY_EN_LEFT_BRACKET
        ["'", 12, 3, 0x63],               //  65 KEY_EN_QUOTE
        ["Menu", 12, 5, 0x65],            //  66 KEY_EN_MENU
        ["F11", 13, 0, 0x70],             //  67 KEY_EN_F11
        ["Backspace", 13, 1, 0x79],       //  68 KEY_EN_BACKSPACE
        ["]", 13, 2, 0x6A],               //  69 KEY_EN_RIGHT_BRACKET
        ["ISO_#", 13, 3, 0x6B],           //  70 KEY_EN_POUND
        ["Right Shift", 13, 4, 0x7C],     //  71 KEY_EN_RIGHT_SHIFT
        ["F12", 14, 0, 0x78],             //  72 KEY_EN_F12
        ["Enter", 14, 3, 0x7B],           //  73 KEY_EN_ISO_ENTER
        ["Right Ctrl", 14, 5, 0x7D],      //  74 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 19, 0, 0x80],    //  75 KEY_EN_PRINT_SCREEN
        ["Insert", 19, 1, 0x81],          //  76 KEY_EN_INSERT
        ["Del", 19, 2, 0x82],             //  77 KEY_EN_DELETE
        ["Left Arrow", 19, 5, 0x85],      //  78 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 20, 0, 0x88],     //  79 KEY_EN_SCROLL_LOCK
        ["Home", 20, 1, 0x89],            //  80 KEY_EN_HOME
        ["End", 20, 2, 0x8A],             //  81 KEY_EN_END
        ["Up Arrow", 20, 4, 0x8C],        //  82 KEY_EN_UP_ARROW
        ["Down Arrow", 20, 5, 0x8D],      //  83 KEY_EN_DOWN_ARROW
        ["Pause Break", 21, 0, 0x90],     //  84 KEY_EN_PAUSE_BREAK
        ["Page Up", 21, 1, 0x91],         //  85 KEY_EN_PAGE_UP
        ["Page Down", 21, 2, 0x92],       //  86 KEY_EN_PAGE_DOWN
        ["Right Arrow", 21, 5, 0x95],     //  87 KEY_EN_RIGHT_ARROW
        ["NumLock", 26, 1, 0x99],         //  88 KEY_EN_NUMPAD_LOCK
        ["Num 7", 26, 2, 0x9A],           //  89 KEY_EN_NUMPAD_7
        ["Num 4", 26, 3, 0x9B],           //  90 KEY_EN_NUMPAD_4
        ["Num 1", 26, 4, 0x9C],           //  91 KEY_EN_NUMPAD_1
        ["Num 0", 26, 5, 0x9D],           //  92 KEY_EN_NUMPAD_0
        ["Num /", 27, 1, 0xA1],           //  93 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 27, 2, 0xA2],           //  94 KEY_EN_NUMPAD_8
        ["Num 5", 27, 3, 0xA3],           //  95 KEY_EN_NUMPAD_5
        ["Num 2", 27, 4, 0xA4],           //  96 KEY_EN_NUMPAD_2
        ["Num *", 28, 1, 0xA9],           //  97 KEY_EN_NUMPAD_TIMES
        ["Num 9", 28, 2, 0xAA],           //  98 KEY_EN_NUMPAD_9
        ["Num 6", 28, 3, 0xAB],           //  99 KEY_EN_NUMPAD_6
        ["Num 3", 28, 4, 0xAC],           // 100 KEY_EN_NUMPAD_3
        ["Num .", 28, 5, 0xAD],           // 101 KEY_EN_NUMPAD_PERIOD
        ["Num -", 29, 1, 0xB1],           // 102 KEY_EN_NUMPAD_MINUS
        ["Num +", 29, 2, 0xB2],           // 103 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 29, 4, 0xB4],       // 104 KEY_EN_NUMPAD_ENTER
        ["Underglow LED 1", 0, 6, 0x06],  // 105 Underglow LED 1
        ["Underglow LED 2", 1, 6, 0x0E],  // 106 Underglow LED 2
        ["Underglow LED 3", 2, 6, 0x16],  // 107 Underglow LED 3
        ["Underglow LED 4", 3, 6, 0x1E],  // 108 Underglow LED 4
        ["Underglow LED 5", 4, 6, 0x26],  // 109 Underglow LED 5
        ["Underglow LED 6", 5, 6, 0x2E],  // 110 Underglow LED 6
        ["Underglow LED 7", 6, 6, 0x36],  // 111 Underglow LED 7
        ["Underglow LED 8", 7, 6, 0x3E],  // 112 Underglow LED 8
        ["Underglow LED 9", 8, 6, 0x46],  // 113 Underglow LED 9
        ["Underglow LED 10", 9, 6, 0x4E], // 114 Underglow LED 10
        ["Underglow LED 11", 10, 6, 0x56],// 115 Underglow LED 11
        ["Underglow LED 12", 11, 6, 0x5E],// 116 Underglow LED 12
        ["Underglow LED 13", 12, 6, 0x66],// 117 Underglow LED 13
        ["Underglow LED 14", 13, 6, 0x6E],// 118 Underglow LED 14
        ["Underglow LED 15", 14, 6, 0x76],// 119 Underglow LED 15
        ["Underglow LED 16", 15, 6, 0x7E],// 120 Underglow LED 16
        ["Underglow LED 17", 16, 6, 0x86],// 121 Underglow LED 17
        ["Underglow LED 18", 17, 6, 0x8E],// 122 Underglow LED 18
        ["Underglow LED 19", 18, 6, 0x96],// 123 Underglow LED 19
        ["Underglow LED 20", 19, 6, 0x9E],// 124 Underglow LED 20
        ["Underglow LED 21", 20, 6, 0xA6],// 125 Underglow LED 21
        ["Underglow LED 22", 21, 6, 0xAE],// 126 Underglow LED 22
        ["Underglow LED 23", 22, 6, 0xB6],// 127 Underglow LED 23
        ["Underglow LED 24", 23, 6, 0xBE],// 128 Underglow LED 24
        ["Underglow LED 25", 24, 6, 0xC6],// 129 Underglow LED 25
        ["Underglow LED 26", 25, 6, 0xCE],// 130 Underglow LED 26
        ["Underglow LED 27", 26, 6, 0xD6],// 131 Underglow LED 27
        ["Underglow LED 28", 27, 6, 0xDE],// 132 Underglow LED 28
        ["Underglow LED 29", 28, 6, 0xE6],// 133 Underglow LED 29
        ["Underglow LED 30", 29, 6, 0xEE],// 134 Underglow LED 30
      ]),
    }),
    // ASUS_TUF_K7_LAYOUT_US, 134 LEDs, matrix ASUS_ROG_STRIX_FLARE_II_ANIMATE_LAYOUT_KEYS_ANSI
    17: Object.freeze({
      width: 30,
      height: 7,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["1", 1, 1, 0x11],                //   6 KEY_EN_1
        ["Left Win", 1, 5, 0x15],         //   7 KEY_EN_LEFT_WINDOWS
        ["F1", 2, 0, 0x18],               //   8 KEY_EN_F1
        ["2", 2, 1, 0x19],                //   9 KEY_EN_2
        ["Q", 2, 2, 0x12],                //  10 KEY_EN_Q
        ["A", 2, 3, 0x13],                //  11 KEY_EN_A
        ["Z", 2, 4, 0x14],                //  12 KEY_EN_Z
        ["Left Alt", 2, 5, 0x1D],         //  13 KEY_EN_LEFT_ALT
        ["F2", 3, 0, 0x20],               //  14 KEY_EN_F2
        ["3", 3, 1, 0x21],                //  15 KEY_EN_3
        ["W", 3, 2, 0x1A],                //  16 KEY_EN_W
        ["S", 3, 3, 0x1B],                //  17 KEY_EN_S
        ["X", 3, 4, 0x1C],                //  18 KEY_EN_X
        ["F3", 4, 0, 0x28],               //  19 KEY_EN_F3
        ["4", 4, 1, 0x29],                //  20 KEY_EN_4
        ["E", 4, 2, 0x22],                //  21 KEY_EN_E
        ["D", 4, 3, 0x23],                //  22 KEY_EN_D
        ["C", 4, 4, 0x24],                //  23 KEY_EN_C
        ["F4", 5, 0, 0x30],               //  24 KEY_EN_F4
        ["5", 5, 1, 0x31],                //  25 KEY_EN_5
        ["R", 5, 2, 0x2A],                //  26 KEY_EN_R
        ["F", 5, 3, 0x2B],                //  27 KEY_EN_F
        ["V", 5, 4, 0x2C],                //  28 KEY_EN_V
        ["6", 6, 1, 0x39],                //  29 KEY_EN_6
        ["T", 6, 2, 0x32],                //  30 KEY_EN_T
        ["G", 6, 3, 0x33],                //  31 KEY_EN_G
        ["B", 6, 4, 0x34],                //  32 KEY_EN_B
        ["Space", 6, 5, 0x35],            //  33 KEY_EN_SPACE
        ["F5", 7, 0, 0x40],               //  34 KEY_EN_F5
        ["7", 7, 1, 0x41],                //  35 KEY_EN_7
        ["Y", 7, 2, 0x3A],                //  36 KEY_EN_Y
        ["H", 7, 3, 0x3B],                //  37 KEY_EN_H
        ["N", 7, 4, 0x3C],                //  38 KEY_EN_N
        ["F6", 8, 0, 0x48],               //  39 KEY_EN_F6
        ["8", 8, 1, 0x49],                //  40 KEY_EN_8
        ["U", 8, 2, 0x42],                //  41 KEY_EN_U
        ["J", 8, 3, 0x43],                //  42 KEY_EN_J
        ["M", 8, 4, 0x44],                //  43 KEY_EN_M
        ["F7", 9, 0, 0x50],               //  44 KEY_EN_F7
        ["9", 9, 1, 0x51],                //  45 KEY_EN_9
        ["I", 9, 2, 0x4A],                //  46 KEY_EN_I
        ["K", 9, 3, 0x4B],                //  47 KEY_EN_K
        [",", 9, 4, 0x4C],                //  48 KEY_EN_COMMA
        ["F8", 10, 0, 0x58],              //  49 KEY_EN_F8
        ["0", 10, 1, 0x59],               //  50 KEY_EN_0
        ["O", 10, 2, 0x52],               //  51 KEY_EN_O
        ["L", 10, 3, 0x53],               //  52 KEY_EN_L
        [".", 10, 4, 0x54],               //  53 KEY_EN_PERIOD
        ["Right Alt", 10, 5, 0x4D],       //  54 KEY_EN_RIGHT_ALT
        ["F9", 11, 0, 0x60],              //  55 KEY_EN_F9
        ["-", 11, 1, 0x61],               //  56 KEY_EN_MINUS
        ["P", 11, 2, 0x5A],               //  57 KEY_EN_P
        [";", 11, 3, 0x5B],               //  58 KEY_EN_SEMICOLON
        ["/", 11, 4, 0x5C],               //  59 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 5, 0x5D],              //  60 KEY_EN_RIGHT_FUNCTION
        ["F10", 12, 0, 0x68],             //  61 KEY_EN_F10
        ["=", 12, 1, 0x69],               //  62 KEY_EN_EQUALS
        ["[", 12, 2, 0x62],               //  63 KEY_EN_LEFT_BRACKET
        ["'", 12, 3, 0x63],               //  64 KEY_EN_QUOTE
        ["Menu", 12, 5, 0x65],            //  65 KEY_EN_MENU
        ["F11", 13, 0, 0x70],             //  66 KEY_EN_F11
        ["Backspace", 13, 1, 0x79],       //  67 KEY_EN_BACKSPACE
        ["]", 13, 2, 0x6A],               //  68 KEY_EN_RIGHT_BRACKET
        ["Right Shift", 13, 4, 0x7C],     //  69 KEY_EN_RIGHT_SHIFT
        ["F12", 14, 0, 0x78],             //  70 KEY_EN_F12
        ["\\", 14, 2, 0x7A],              //  71 KEY_EN_ANSI_BACK_SLASH
        ["Enter", 14, 3, 0x7B],           //  72 KEY_EN_ANSI_ENTER
        ["Right Ctrl", 14, 5, 0x7D],      //  73 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 19, 0, 0x80],    //  74 KEY_EN_PRINT_SCREEN
        ["Insert", 19, 1, 0x81],          //  75 KEY_EN_INSERT
        ["Del", 19, 2, 0x82],             //  76 KEY_EN_DELETE
        ["Left Arrow", 19, 5, 0x85],      //  77 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 20, 0, 0x88],     //  78 KEY_EN_SCROLL_LOCK
        ["Home", 20, 1, 0x89],            //  79 KEY_EN_HOME
        ["End", 20, 2, 0x8A],             //  80 KEY_EN_END
        ["Up Arrow", 20, 4, 0x8C],        //  81 KEY_EN_UP_ARROW
        ["Down Arrow", 20, 5, 0x8D],      //  82 KEY_EN_DOWN_ARROW
        ["Pause Break", 21, 0, 0x90],     //  83 KEY_EN_PAUSE_BREAK
        ["Page Up", 21, 1, 0x91],         //  84 KEY_EN_PAGE_UP
        ["Page Down", 21, 2, 0x92],       //  85 KEY_EN_PAGE_DOWN
        ["Right Arrow", 21, 5, 0x95],     //  86 KEY_EN_RIGHT_ARROW
        ["NumLock", 26, 1, 0x99],         //  87 KEY_EN_NUMPAD_LOCK
        ["Num 7", 26, 2, 0x9A],           //  88 KEY_EN_NUMPAD_7
        ["Num 4", 26, 3, 0x9B],           //  89 KEY_EN_NUMPAD_4
        ["Num 1", 26, 4, 0x9C],           //  90 KEY_EN_NUMPAD_1
        ["Num 0", 26, 5, 0x9D],           //  91 KEY_EN_NUMPAD_0
        ["Num /", 27, 1, 0xA1],           //  92 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 27, 2, 0xA2],           //  93 KEY_EN_NUMPAD_8
        ["Num 5", 27, 3, 0xA3],           //  94 KEY_EN_NUMPAD_5
        ["Num 2", 27, 4, 0xA4],           //  95 KEY_EN_NUMPAD_2
        ["Num *", 28, 1, 0xA9],           //  96 KEY_EN_NUMPAD_TIMES
        ["Num 9", 28, 2, 0xAA],           //  97 KEY_EN_NUMPAD_9
        ["Num 6", 28, 3, 0xAB],           //  98 KEY_EN_NUMPAD_6
        ["Num 3", 28, 4, 0xAC],           //  99 KEY_EN_NUMPAD_3
        ["Num .", 28, 5, 0xAD],           // 100 KEY_EN_NUMPAD_PERIOD
        ["Num -", 29, 1, 0xB1],           // 101 KEY_EN_NUMPAD_MINUS
        ["Num +", 29, 2, 0xB2],           // 102 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 29, 4, 0xB4],       // 103 KEY_EN_NUMPAD_ENTER
        ["Underglow LED 1", 0, 6, 0x06],  // 104 Underglow LED 1
        ["Underglow LED 2", 1, 6, 0x0E],  // 105 Underglow LED 2
        ["Underglow LED 3", 2, 6, 0x16],  // 106 Underglow LED 3
        ["Underglow LED 4", 3, 6, 0x1E],  // 107 Underglow LED 4
        ["Underglow LED 5", 4, 6, 0x26],  // 108 Underglow LED 5
        ["Underglow LED 6", 5, 6, 0x2E],  // 109 Underglow LED 6
        ["Underglow LED 7", 6, 6, 0x36],  // 110 Underglow LED 7
        ["Underglow LED 8", 7, 6, 0x3E],  // 111 Underglow LED 8
        ["Underglow LED 9", 8, 6, 0x46],  // 112 Underglow LED 9
        ["Underglow LED 10", 9, 6, 0x4E], // 113 Underglow LED 10
        ["Underglow LED 11", 10, 6, 0x56],// 114 Underglow LED 11
        ["Underglow LED 12", 11, 6, 0x5E],// 115 Underglow LED 12
        ["Underglow LED 13", 12, 6, 0x66],// 116 Underglow LED 13
        ["Underglow LED 14", 13, 6, 0x6E],// 117 Underglow LED 14
        ["Underglow LED 15", 14, 6, 0x76],// 118 Underglow LED 15
        ["Underglow LED 16", 15, 6, 0x7E],// 119 Underglow LED 16
        ["Underglow LED 17", 16, 6, 0x86],// 120 Underglow LED 17
        ["Underglow LED 18", 17, 6, 0x8E],// 121 Underglow LED 18
        ["Underglow LED 19", 18, 6, 0x96],// 122 Underglow LED 19
        ["Underglow LED 20", 19, 6, 0x9E],// 123 Underglow LED 20
        ["Underglow LED 21", 20, 6, 0xA6],// 124 Underglow LED 21
        ["Underglow LED 22", 21, 6, 0xAE],// 125 Underglow LED 22
        ["Underglow LED 23", 22, 6, 0xB6],// 126 Underglow LED 23
        ["Underglow LED 24", 23, 6, 0xBE],// 127 Underglow LED 24
        ["Underglow LED 25", 24, 6, 0xC6],// 128 Underglow LED 25
        ["Underglow LED 26", 25, 6, 0xCE],// 129 Underglow LED 26
        ["Underglow LED 27", 26, 6, 0xD6],// 130 Underglow LED 27
        ["Underglow LED 28", 27, 6, 0xDE],// 131 Underglow LED 28
        ["Underglow LED 29", 28, 6, 0xE6],// 132 Underglow LED 29
        ["Underglow LED 30", 29, 6, 0xEE],// 133 Underglow LED 30
      ]),
    }),
  }),
  AsusFalchionLayouts: Object.freeze({
    // ASUS_TUF_K7_LAYOUT_UK, 69 LEDs, matrix ASUS_FALCHION_LAYOUT_KEYS_ISO
    4: Object.freeze({
      width: 16,
      height: 5,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["Tab", 0, 1, 0x01],              //   1 KEY_EN_TAB
        ["Caps Lock", 0, 2, 0x02],        //   2 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 3, 0x03],       //   3 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 4, 0x04],        //   4 KEY_EN_LEFT_CONTROL
        ["1", 1, 0, 0x08],                //   5 KEY_EN_1
        ["ISO_<", 1, 3, 0x0B],            //   6 KEY_EN_ISO_BACK_SLASH
        ["Left Win", 1, 4, 0x0C],         //   7 KEY_EN_LEFT_WINDOWS
        ["2", 2, 0, 0x10],                //   8 KEY_EN_2
        ["Q", 2, 1, 0x09],                //   9 KEY_EN_Q
        ["A", 2, 2, 0x0A],                //  10 KEY_EN_A
        ["Z", 2, 3, 0x13],                //  11 KEY_EN_Z
        ["Left Alt", 2, 4, 0x14],         //  12 KEY_EN_LEFT_ALT
        ["3", 3, 0, 0x18],                //  13 KEY_EN_3
        ["W", 3, 1, 0x11],                //  14 KEY_EN_W
        ["S", 3, 2, 0x12],                //  15 KEY_EN_S
        ["X", 3, 3, 0x1B],                //  16 KEY_EN_X
        ["4", 4, 0, 0x20],                //  17 KEY_EN_4
        ["E", 4, 1, 0x19],                //  18 KEY_EN_E
        ["D", 4, 2, 0x1A],                //  19 KEY_EN_D
        ["C", 4, 3, 0x23],                //  20 KEY_EN_C
        ["5", 5, 0, 0x28],                //  21 KEY_EN_5
        ["R", 5, 1, 0x21],                //  22 KEY_EN_R
        ["F", 5, 2, 0x22],                //  23 KEY_EN_F
        ["V", 5, 3, 0x2B],                //  24 KEY_EN_V
        ["6", 6, 0, 0x30],                //  25 KEY_EN_6
        ["T", 6, 1, 0x29],                //  26 KEY_EN_T
        ["G", 6, 2, 0x2A],                //  27 KEY_EN_G
        ["B", 6, 3, 0x33],                //  28 KEY_EN_B
        ["Space", 6, 4, 0x34],            //  29 KEY_EN_SPACE
        ["7", 7, 0, 0x38],                //  30 KEY_EN_7
        ["Y", 7, 1, 0x31],                //  31 KEY_EN_Y
        ["H", 7, 2, 0x32],                //  32 KEY_EN_H
        ["N", 7, 3, 0x3B],                //  33 KEY_EN_N
        ["8", 8, 0, 0x40],                //  34 KEY_EN_8
        ["U", 8, 1, 0x39],                //  35 KEY_EN_U
        ["J", 8, 2, 0x3A],                //  36 KEY_EN_J
        ["M", 8, 3, 0x43],                //  37 KEY_EN_M
        ["9", 9, 0, 0x48],                //  38 KEY_EN_9
        ["I", 9, 1, 0x41],                //  39 KEY_EN_I
        ["K", 9, 2, 0x42],                //  40 KEY_EN_K
        [",", 9, 3, 0x4B],                //  41 KEY_EN_COMMA
        ["0", 10, 0, 0x50],               //  42 KEY_EN_0
        ["O", 10, 1, 0x49],               //  43 KEY_EN_O
        ["L", 10, 2, 0x4A],               //  44 KEY_EN_L
        [".", 10, 3, 0x53],               //  45 KEY_EN_PERIOD
        ["Right Alt", 10, 4, 0x4C],       //  46 KEY_EN_RIGHT_ALT
        ["-", 11, 0, 0x58],               //  47 KEY_EN_MINUS
        ["P", 11, 1, 0x51],               //  48 KEY_EN_P
        [";", 11, 2, 0x52],               //  49 KEY_EN_SEMICOLON
        ["/", 11, 3, 0x5B],               //  50 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 4, 0x54],              //  51 KEY_EN_RIGHT_FUNCTION
        ["=", 12, 0, 0x60],               //  52 KEY_EN_EQUALS
        ["[", 12, 1, 0x59],               //  53 KEY_EN_LEFT_BRACKET
        ["'", 12, 2, 0x5A],               //  54 KEY_EN_QUOTE
        ["Right Shift", 12, 3, 0x63],     //  55 KEY_EN_RIGHT_SHIFT
        ["Right Ctrl", 12, 4, 0x5C],      //  56 KEY_EN_RIGHT_CONTROL
        ["Backspace", 13, 0, 0x68],       //  57 KEY_EN_BACKSPACE
        ["]", 13, 1, 0x61],               //  58 KEY_EN_RIGHT_BRACKET
        ["ISO_#", 13, 2, 0x62],           //  59 KEY_EN_POUND
        ["Left Arrow", 13, 4, 0x64],      //  60 KEY_EN_LEFT_ARROW
        ["Enter", 14, 2, 0x6A],           //  61 KEY_EN_ISO_ENTER
        ["Up Arrow", 14, 3, 0x6B],        //  62 KEY_EN_UP_ARROW
        ["Down Arrow", 14, 4, 0x6C],      //  63 KEY_EN_DOWN_ARROW
        ["Insert", 15, 0, 0x70],          //  64 KEY_EN_INSERT
        ["Del", 15, 1, 0x71],             //  65 KEY_EN_DELETE
        ["Page Up", 15, 2, 0x72],         //  66 KEY_EN_PAGE_UP
        ["Page Down", 15, 3, 0x73],       //  67 KEY_EN_PAGE_DOWN
        ["Right Arrow", 15, 4, 0x74],     //  68 KEY_EN_RIGHT_ARROW
      ]),
    }),
    // ASUS_TUF_K7_LAYOUT_US, 68 LEDs, matrix ASUS_FALCHION_LAYOUT_KEYS_ANSI
    17: Object.freeze({
      width: 16,
      height: 5,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["Tab", 0, 1, 0x01],              //   1 KEY_EN_TAB
        ["Caps Lock", 0, 2, 0x02],        //   2 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 3, 0x03],       //   3 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 4, 0x04],        //   4 KEY_EN_LEFT_CONTROL
        ["1", 1, 0, 0x08],                //   5 KEY_EN_1
        ["Left Win", 1, 4, 0x0C],         //   6 KEY_EN_LEFT_WINDOWS
        ["2", 2, 0, 0x10],                //   7 KEY_EN_2
        ["Q", 2, 1, 0x09],                //   8 KEY_EN_Q
        ["A", 2, 2, 0x0A],                //   9 KEY_EN_A
        ["Z", 2, 3, 0x13],                //  10 KEY_EN_Z
        ["Left Alt", 2, 4, 0x14],         //  11 KEY_EN_LEFT_ALT
        ["3", 3, 0, 0x18],                //  12 KEY_EN_3
        ["W", 3, 1, 0x11],                //  13 KEY_EN_W
        ["S", 3, 2, 0x12],                //  14 KEY_EN_S
        ["X", 3, 3, 0x1B],                //  15 KEY_EN_X
        ["4", 4, 0, 0x20],                //  16 KEY_EN_4
        ["E", 4, 1, 0x19],                //  17 KEY_EN_E
        ["D", 4, 2, 0x1A],                //  18 KEY_EN_D
        ["C", 4, 3, 0x23],                //  19 KEY_EN_C
        ["5", 5, 0, 0x28],                //  20 KEY_EN_5
        ["R", 5, 1, 0x21],                //  21 KEY_EN_R
        ["F", 5, 2, 0x22],                //  22 KEY_EN_F
        ["V", 5, 3, 0x2B],                //  23 KEY_EN_V
        ["6", 6, 0, 0x30],                //  24 KEY_EN_6
        ["T", 6, 1, 0x29],                //  25 KEY_EN_T
        ["G", 6, 2, 0x2A],                //  26 KEY_EN_G
        ["B", 6, 3, 0x33],                //  27 KEY_EN_B
        ["Space", 6, 4, 0x34],            //  28 KEY_EN_SPACE
        ["7", 7, 0, 0x38],                //  29 KEY_EN_7
        ["Y", 7, 1, 0x31],                //  30 KEY_EN_Y
        ["H", 7, 2, 0x32],                //  31 KEY_EN_H
        ["N", 7, 3, 0x3B],                //  32 KEY_EN_N
        ["8", 8, 0, 0x40],                //  33 KEY_EN_8
        ["U", 8, 1, 0x39],                //  34 KEY_EN_U
        ["J", 8, 2, 0x3A],                //  35 KEY_EN_J
        ["M", 8, 3, 0x43],                //  36 KEY_EN_M
        ["9", 9, 0, 0x48],                //  37 KEY_EN_9
        ["I", 9, 1, 0x41],                //  38 KEY_EN_I
        ["K", 9, 2, 0x42],                //  39 KEY_EN_K
        [",", 9, 3, 0x4B],                //  40 KEY_EN_COMMA
        ["0", 10, 0, 0x50],               //  41 KEY_EN_0
        ["O", 10, 1, 0x49],               //  42 KEY_EN_O
        ["L", 10, 2, 0x4A],               //  43 KEY_EN_L
        [".", 10, 3, 0x53],               //  44 KEY_EN_PERIOD
        ["Right Alt", 10, 4, 0x4C],       //  45 KEY_EN_RIGHT_ALT
        ["-", 11, 0, 0x58],               //  46 KEY_EN_MINUS
        ["P", 11, 1, 0x51],               //  47 KEY_EN_P
        [";", 11, 2, 0x52],               //  48 KEY_EN_SEMICOLON
        ["/", 11, 3, 0x5B],               //  49 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 4, 0x54],              //  50 KEY_EN_RIGHT_FUNCTION
        ["=", 12, 0, 0x60],               //  51 KEY_EN_EQUALS
        ["[", 12, 1, 0x59],               //  52 KEY_EN_LEFT_BRACKET
        ["'", 12, 2, 0x5A],               //  53 KEY_EN_QUOTE
        ["Right Shift", 12, 3, 0x63],     //  54 KEY_EN_RIGHT_SHIFT
        ["Right Ctrl", 12, 4, 0x5C],      //  55 KEY_EN_RIGHT_CONTROL
        ["Backspace", 13, 0, 0x68],       //  56 KEY_EN_BACKSPACE
        ["]", 13, 1, 0x61],               //  57 KEY_EN_RIGHT_BRACKET
        ["Left Arrow", 13, 4, 0x64],      //  58 KEY_EN_LEFT_ARROW
        ["\\", 14, 1, 0x69],              //  59 KEY_EN_ANSI_BACK_SLASH
        ["Enter", 14, 2, 0x6A],           //  60 KEY_EN_ANSI_ENTER
        ["Up Arrow", 14, 3, 0x6B],        //  61 KEY_EN_UP_ARROW
        ["Down Arrow", 14, 4, 0x6C],      //  62 KEY_EN_DOWN_ARROW
        ["Insert", 15, 0, 0x70],          //  63 KEY_EN_INSERT
        ["Del", 15, 1, 0x71],             //  64 KEY_EN_DELETE
        ["Page Up", 15, 2, 0x72],         //  65 KEY_EN_PAGE_UP
        ["Page Down", 15, 3, 0x73],       //  66 KEY_EN_PAGE_DOWN
        ["Right Arrow", 15, 4, 0x74],     //  67 KEY_EN_RIGHT_ARROW
      ]),
    }),
  }),
  AsusClaymoreNoNumpadLayouts: Object.freeze({
    // ASUS_TUF_K7_LAYOUT_UK, 89 LEDs, matrix ASUS_CLAYMORE_NO_NUMPAD_LAYOUT_KEYS_ISO
    4: Object.freeze({
      width: 19,
      height: 7,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["1", 1, 1, 0x09],                //   6 KEY_EN_1
        ["ISO_<", 1, 4, 0x0C],            //   7 KEY_EN_ISO_BACK_SLASH
        ["Left Win", 1, 5, 0x0D],         //   8 KEY_EN_LEFT_WINDOWS
        ["F1", 2, 0, 0x10],               //   9 KEY_EN_F1
        ["2", 2, 1, 0x11],                //  10 KEY_EN_2
        ["Q", 2, 2, 0x0A],                //  11 KEY_EN_Q
        ["A", 2, 3, 0x0B],                //  12 KEY_EN_A
        ["Z", 2, 4, 0x14],                //  13 KEY_EN_Z
        ["Left Alt", 2, 5, 0x15],         //  14 KEY_EN_LEFT_ALT
        ["F2", 3, 0, 0x18],               //  15 KEY_EN_F2
        ["3", 3, 1, 0x19],                //  16 KEY_EN_3
        ["W", 3, 2, 0x12],                //  17 KEY_EN_W
        ["S", 3, 3, 0x13],                //  18 KEY_EN_S
        ["X", 3, 4, 0x1C],                //  19 KEY_EN_X
        ["F3", 4, 0, 0x20],               //  20 KEY_EN_F3
        ["4", 4, 1, 0x21],                //  21 KEY_EN_4
        ["E", 4, 2, 0x1A],                //  22 KEY_EN_E
        ["D", 4, 3, 0x1B],                //  23 KEY_EN_D
        ["C", 4, 4, 0x24],                //  24 KEY_EN_C
        ["F4", 5, 0, 0x28],               //  25 KEY_EN_F4
        ["5", 5, 1, 0x29],                //  26 KEY_EN_5
        ["R", 5, 2, 0x22],                //  27 KEY_EN_R
        ["F", 5, 3, 0x23],                //  28 KEY_EN_F
        ["V", 5, 4, 0x2C],                //  29 KEY_EN_V
        ["6", 6, 1, 0x31],                //  30 KEY_EN_6
        ["T", 6, 2, 0x2A],                //  31 KEY_EN_T
        ["G", 6, 3, 0x2B],                //  32 KEY_EN_G
        ["B", 6, 4, 0x34],                //  33 KEY_EN_B
        ["Space", 6, 5, 0x25],            //  34 KEY_EN_SPACE
        ["F5", 7, 0, 0x38],               //  35 KEY_EN_F5
        ["7", 7, 1, 0x39],                //  36 KEY_EN_7
        ["Y", 7, 2, 0x32],                //  37 KEY_EN_Y
        ["H", 7, 3, 0x33],                //  38 KEY_EN_H
        ["N", 7, 4, 0x3C],                //  39 KEY_EN_N
        ["F6", 8, 0, 0x40],               //  40 KEY_EN_F6
        ["8", 8, 1, 0x41],                //  41 KEY_EN_8
        ["U", 8, 2, 0x3A],                //  42 KEY_EN_U
        ["J", 8, 3, 0x3B],                //  43 KEY_EN_J
        ["M", 8, 4, 0x44],                //  44 KEY_EN_M
        ["Logo", 8, 6, 0x45],             //  45 Logo
        ["F7", 9, 0, 0x48],               //  46 KEY_EN_F7
        ["9", 9, 1, 0x49],                //  47 KEY_EN_9
        ["I", 9, 2, 0x42],                //  48 KEY_EN_I
        ["K", 9, 3, 0x43],                //  49 KEY_EN_K
        [",", 9, 4, 0x4C],                //  50 KEY_EN_COMMA
        ["F8", 10, 0, 0x50],              //  51 KEY_EN_F8
        ["0", 10, 1, 0x51],               //  52 KEY_EN_0
        ["O", 10, 2, 0x4A],               //  53 KEY_EN_O
        ["L", 10, 3, 0x4B],               //  54 KEY_EN_L
        [".", 10, 4, 0x54],               //  55 KEY_EN_PERIOD
        ["Right Alt", 10, 5, 0x4D],       //  56 KEY_EN_RIGHT_ALT
        ["F9", 11, 0, 0x60],              //  57 KEY_EN_F9
        ["-", 11, 1, 0x59],               //  58 KEY_EN_MINUS
        ["P", 11, 2, 0x52],               //  59 KEY_EN_P
        [";", 11, 3, 0x53],               //  60 KEY_EN_SEMICOLON
        ["/", 11, 4, 0x5C],               //  61 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 5, 0x55],              //  62 KEY_EN_RIGHT_FUNCTION
        ["F10", 12, 0, 0x68],             //  63 KEY_EN_F10
        ["=", 12, 1, 0x61],               //  64 KEY_EN_EQUALS
        ["[", 12, 2, 0x5A],               //  65 KEY_EN_LEFT_BRACKET
        ["'", 12, 3, 0x5B],               //  66 KEY_EN_QUOTE
        ["Menu", 12, 5, 0x5D],            //  67 KEY_EN_MENU
        ["F11", 13, 0, 0x70],             //  68 KEY_EN_F11
        ["Backspace", 13, 1, 0x69],       //  69 KEY_EN_BACKSPACE
        ["]", 13, 2, 0x62],               //  70 KEY_EN_RIGHT_BRACKET
        ["ISO_#", 13, 3, 0x63],           //  71 KEY_EN_POUND
        ["Right Shift", 13, 4, 0x6C],     //  72 KEY_EN_RIGHT_SHIFT
        ["F12", 14, 0, 0x78],             //  73 KEY_EN_F12
        ["Enter", 14, 3, 0x6B],           //  74 KEY_EN_ISO_ENTER
        ["Right Ctrl", 14, 5, 0x6D],      //  75 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 16, 0, 0x80],    //  76 KEY_EN_PRINT_SCREEN
        ["Insert", 16, 1, 0x81],          //  77 KEY_EN_INSERT
        ["Del", 16, 2, 0x82],             //  78 KEY_EN_DELETE
        ["Left Arrow", 16, 5, 0x85],      //  79 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 17, 0, 0x88],     //  80 KEY_EN_SCROLL_LOCK
        ["Home", 17, 1, 0x89],            //  81 KEY_EN_HOME
        ["End", 17, 2, 0x8A],             //  82 KEY_EN_END
        ["Up Arrow", 17, 4, 0x8C],        //  83 KEY_EN_UP_ARROW
        ["Down Arrow", 17, 5, 0x8D],      //  84 KEY_EN_DOWN_ARROW
        ["Pause Break", 18, 0, 0x90],     //  85 KEY_EN_PAUSE_BREAK
        ["Page Up", 18, 1, 0x91],         //  86 KEY_EN_PAGE_UP
        ["Page Down", 18, 2, 0x92],       //  87 KEY_EN_PAGE_DOWN
        ["Right Arrow", 18, 5, 0x95],     //  88 KEY_EN_RIGHT_ARROW
      ]),
    }),
    // ASUS_TUF_K7_LAYOUT_US, 88 LEDs, matrix ASUS_CLAYMORE_NO_NUMPAD_LAYOUT_KEYS_ANSI
    17: Object.freeze({
      width: 19,
      height: 7,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["1", 1, 1, 0x09],                //   6 KEY_EN_1
        ["Left Win", 1, 5, 0x0D],         //   7 KEY_EN_LEFT_WINDOWS
        ["F1", 2, 0, 0x10],               //   8 KEY_EN_F1
        ["2", 2, 1, 0x11],                //   9 KEY_EN_2
        ["Q", 2, 2, 0x0A],                //  10 KEY_EN_Q
        ["A", 2, 3, 0x0B],                //  11 KEY_EN_A
        ["Z", 2, 4, 0x0C],                //  12 KEY_EN_Z
        ["Left Alt", 2, 5, 0x15],         //  13 KEY_EN_LEFT_ALT
        ["F2", 3, 0, 0x18],               //  14 KEY_EN_F2
        ["3", 3, 1, 0x19],                //  15 KEY_EN_3
        ["W", 3, 2, 0x12],                //  16 KEY_EN_W
        ["S", 3, 3, 0x13],                //  17 KEY_EN_S
        ["X", 3, 4, 0x14],                //  18 KEY_EN_X
        ["F3", 4, 0, 0x20],               //  19 KEY_EN_F3
        ["4", 4, 1, 0x21],                //  20 KEY_EN_4
        ["E", 4, 2, 0x1A],                //  21 KEY_EN_E
        ["D", 4, 3, 0x1B],                //  22 KEY_EN_D
        ["C", 4, 4, 0x1C],                //  23 KEY_EN_C
        ["F4", 5, 0, 0x28],               //  24 KEY_EN_F4
        ["5", 5, 1, 0x29],                //  25 KEY_EN_5
        ["R", 5, 2, 0x22],                //  26 KEY_EN_R
        ["F", 5, 3, 0x23],                //  27 KEY_EN_F
        ["V", 5, 4, 0x24],                //  28 KEY_EN_V
        ["6", 6, 1, 0x31],                //  29 KEY_EN_6
        ["T", 6, 2, 0x2A],                //  30 KEY_EN_T
        ["G", 6, 3, 0x2B],                //  31 KEY_EN_G
        ["B", 6, 4, 0x2C],                //  32 KEY_EN_B
        ["Space", 6, 5, 0x25],            //  33 KEY_EN_SPACE
        ["F5", 7, 0, 0x38],               //  34 KEY_EN_F5
        ["7", 7, 1, 0x39],                //  35 KEY_EN_7
        ["Y", 7, 2, 0x32],                //  36 KEY_EN_Y
        ["H", 7, 3, 0x33],                //  37 KEY_EN_H
        ["N", 7, 4, 0x34],                //  38 KEY_EN_N
        ["F6", 8, 0, 0x40],               //  39 KEY_EN_F6
        ["8", 8, 1, 0x41],                //  40 KEY_EN_8
        ["U", 8, 2, 0x3A],                //  41 KEY_EN_U
        ["J", 8, 3, 0x3B],                //  42 KEY_EN_J
        ["M", 8, 4, 0x3C],                //  43 KEY_EN_M
        ["Logo", 8, 6, 0x3D],             //  44 Logo
        ["F7", 9, 0, 0x48],               //  45 KEY_EN_F7
        ["9", 9, 1, 0x49],                //  46 KEY_EN_9
        ["I", 9, 2, 0x42],                //  47 KEY_EN_I
        ["K", 9, 3, 0x43],                //  48 KEY_EN_K
        [",", 9, 4, 0x44],                //  49 KEY_EN_COMMA
        ["F8", 10, 0, 0x50],              //  50 KEY_EN_F8
        ["0", 10, 1, 0x51],               //  51 KEY_EN_0
        ["O", 10, 2, 0x4A],               //  52 KEY_EN_O
        ["L", 10, 3, 0x4B],               //  53 KEY_EN_L
        [".", 10, 4, 0x4C],               //  54 KEY_EN_PERIOD
        ["Right Alt", 10, 5, 0x4D],       //  55 KEY_EN_RIGHT_ALT
        ["F9", 11, 0, 0x60],              //  56 KEY_EN_F9
        ["-", 11, 1, 0x59],               //  57 KEY_EN_MINUS
        ["P", 11, 2, 0x52],               //  58 KEY_EN_P
        [";", 11, 3, 0x53],               //  59 KEY_EN_SEMICOLON
        ["/", 11, 4, 0x54],               //  60 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 5, 0x55],              //  61 KEY_EN_RIGHT_FUNCTION
        ["F10", 12, 0, 0x68],             //  62 KEY_EN_F10
        ["=", 12, 1, 0x61],               //  63 KEY_EN_EQUALS
        ["[", 12, 2, 0x5A],               //  64 KEY_EN_LEFT_BRACKET
        ["'", 12, 3, 0x5B],               //  65 KEY_EN_QUOTE
        ["Menu", 12, 5, 0x5D],            //  66 KEY_EN_MENU
        ["F11", 13, 0, 0x70],             //  67 KEY_EN_F11
        ["Backspace", 13, 1, 0x69],       //  68 KEY_EN_BACKSPACE
        ["]", 13, 2, 0x62],               //  69 KEY_EN_RIGHT_BRACKET
        ["Right Shift", 13, 4, 0x6C],     //  70 KEY_EN_RIGHT_SHIFT
        ["F12", 14, 0, 0x78],             //  71 KEY_EN_F12
        ["\\", 14, 2, 0x6A],              //  72 KEY_EN_ANSI_BACK_SLASH
        ["Enter", 14, 3, 0x6B],           //  73 KEY_EN_ANSI_ENTER
        ["Right Ctrl", 14, 5, 0x6D],      //  74 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 16, 0, 0x80],    //  75 KEY_EN_PRINT_SCREEN
        ["Insert", 16, 1, 0x81],          //  76 KEY_EN_INSERT
        ["Del", 16, 2, 0x82],             //  77 KEY_EN_DELETE
        ["Left Arrow", 16, 5, 0x85],      //  78 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 17, 0, 0x88],     //  79 KEY_EN_SCROLL_LOCK
        ["Home", 17, 1, 0x89],            //  80 KEY_EN_HOME
        ["End", 17, 2, 0x8A],             //  81 KEY_EN_END
        ["Up Arrow", 17, 4, 0x8C],        //  82 KEY_EN_UP_ARROW
        ["Down Arrow", 17, 5, 0x8D],      //  83 KEY_EN_DOWN_ARROW
        ["Pause Break", 18, 0, 0x90],     //  84 KEY_EN_PAUSE_BREAK
        ["Page Up", 18, 1, 0x91],         //  85 KEY_EN_PAGE_UP
        ["Page Down", 18, 2, 0x92],       //  86 KEY_EN_PAGE_DOWN
        ["Right Arrow", 18, 5, 0x95],     //  87 KEY_EN_RIGHT_ARROW
      ]),
    }),
  }),
  AsusClaymoreNumpadRightLayouts: Object.freeze({
    // ASUS_TUF_K7_LAYOUT_UK, 106 LEDs, matrix ASUS_CLAYMORE_NUMPAD_RIGHT_LAYOUT_KEYS_ISO
    4: Object.freeze({
      width: 24,
      height: 7,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["1", 1, 1, 0x09],                //   6 KEY_EN_1
        ["ISO_<", 1, 4, 0x0C],            //   7 KEY_EN_ISO_BACK_SLASH
        ["Left Win", 1, 5, 0x0D],         //   8 KEY_EN_LEFT_WINDOWS
        ["F1", 2, 0, 0x10],               //   9 KEY_EN_F1
        ["2", 2, 1, 0x11],                //  10 KEY_EN_2
        ["Q", 2, 2, 0x0A],                //  11 KEY_EN_Q
        ["A", 2, 3, 0x0B],                //  12 KEY_EN_A
        ["Z", 2, 4, 0x14],                //  13 KEY_EN_Z
        ["Left Alt", 2, 5, 0x15],         //  14 KEY_EN_LEFT_ALT
        ["F2", 3, 0, 0x18],               //  15 KEY_EN_F2
        ["3", 3, 1, 0x19],                //  16 KEY_EN_3
        ["W", 3, 2, 0x12],                //  17 KEY_EN_W
        ["S", 3, 3, 0x13],                //  18 KEY_EN_S
        ["X", 3, 4, 0x1C],                //  19 KEY_EN_X
        ["F3", 4, 0, 0x20],               //  20 KEY_EN_F3
        ["4", 4, 1, 0x21],                //  21 KEY_EN_4
        ["E", 4, 2, 0x1A],                //  22 KEY_EN_E
        ["D", 4, 3, 0x1B],                //  23 KEY_EN_D
        ["C", 4, 4, 0x24],                //  24 KEY_EN_C
        ["F4", 5, 0, 0x28],               //  25 KEY_EN_F4
        ["5", 5, 1, 0x29],                //  26 KEY_EN_5
        ["R", 5, 2, 0x22],                //  27 KEY_EN_R
        ["F", 5, 3, 0x23],                //  28 KEY_EN_F
        ["V", 5, 4, 0x2C],                //  29 KEY_EN_V
        ["6", 6, 1, 0x31],                //  30 KEY_EN_6
        ["T", 6, 2, 0x2A],                //  31 KEY_EN_T
        ["G", 6, 3, 0x2B],                //  32 KEY_EN_G
        ["B", 6, 4, 0x34],                //  33 KEY_EN_B
        ["Space", 6, 5, 0x25],            //  34 KEY_EN_SPACE
        ["F5", 7, 0, 0x38],               //  35 KEY_EN_F5
        ["7", 7, 1, 0x39],                //  36 KEY_EN_7
        ["Y", 7, 2, 0x32],                //  37 KEY_EN_Y
        ["H", 7, 3, 0x33],                //  38 KEY_EN_H
        ["N", 7, 4, 0x3C],                //  39 KEY_EN_N
        ["F6", 8, 0, 0x40],               //  40 KEY_EN_F6
        ["8", 8, 1, 0x41],                //  41 KEY_EN_8
        ["U", 8, 2, 0x3A],                //  42 KEY_EN_U
        ["J", 8, 3, 0x3B],                //  43 KEY_EN_J
        ["M", 8, 4, 0x44],                //  44 KEY_EN_M
        ["Logo", 8, 6, 0x45],             //  45 Logo
        ["F7", 9, 0, 0x48],               //  46 KEY_EN_F7
        ["9", 9, 1, 0x49],                //  47 KEY_EN_9
        ["I", 9, 2, 0x42],                //  48 KEY_EN_I
        ["K", 9, 3, 0x43],                //  49 KEY_EN_K
        [",", 9, 4, 0x4C],                //  50 KEY_EN_COMMA
        ["F8", 10, 0, 0x50],              //  51 KEY_EN_F8
        ["0", 10, 1, 0x51],               //  52 KEY_EN_0
        ["O", 10, 2, 0x4A],               //  53 KEY_EN_O
        ["L", 10, 3, 0x4B],               //  54 KEY_EN_L
        [".", 10, 4, 0x54],               //  55 KEY_EN_PERIOD
        ["Right Alt", 10, 5, 0x4D],       //  56 KEY_EN_RIGHT_ALT
        ["F9", 11, 0, 0x60],              //  57 KEY_EN_F9
        ["-", 11, 1, 0x59],               //  58 KEY_EN_MINUS
        ["P", 11, 2, 0x52],               //  59 KEY_EN_P
        [";", 11, 3, 0x53],               //  60 KEY_EN_SEMICOLON
        ["/", 11, 4, 0x5C],               //  61 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 5, 0x55],              //  62 KEY_EN_RIGHT_FUNCTION
        ["F10", 12, 0, 0x68],             //  63 KEY_EN_F10
        ["=", 12, 1, 0x61],               //  64 KEY_EN_EQUALS
        ["[", 12, 2, 0x5A],               //  65 KEY_EN_LEFT_BRACKET
        ["'", 12, 3, 0x5B],               //  66 KEY_EN_QUOTE
        ["Menu", 12, 5, 0x5D],            //  67 KEY_EN_MENU
        ["F11", 13, 0, 0x70],             //  68 KEY_EN_F11
        ["Backspace", 13, 1, 0x69],       //  69 KEY_EN_BACKSPACE
        ["]", 13, 2, 0x62],               //  70 KEY_EN_RIGHT_BRACKET
        ["ISO_#", 13, 3, 0x63],           //  71 KEY_EN_POUND
        ["Right Shift", 13, 4, 0x6C],     //  72 KEY_EN_RIGHT_SHIFT
        ["F12", 14, 0, 0x78],             //  73 KEY_EN_F12
        ["Enter", 14, 3, 0x6B],           //  74 KEY_EN_ISO_ENTER
        ["Right Ctrl", 14, 5, 0x6D],      //  75 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 16, 0, 0x80],    //  76 KEY_EN_PRINT_SCREEN
        ["Insert", 16, 1, 0x81],          //  77 KEY_EN_INSERT
        ["Del", 16, 2, 0x82],             //  78 KEY_EN_DELETE
        ["Left Arrow", 16, 5, 0x85],      //  79 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 17, 0, 0x88],     //  80 KEY_EN_SCROLL_LOCK
        ["Home", 17, 1, 0x89],            //  81 KEY_EN_HOME
        ["End", 17, 2, 0x8A],             //  82 KEY_EN_END
        ["Up Arrow", 17, 4, 0x8C],        //  83 KEY_EN_UP_ARROW
        ["Down Arrow", 17, 5, 0x8D],      //  84 KEY_EN_DOWN_ARROW
        ["Pause Break", 18, 0, 0x90],     //  85 KEY_EN_PAUSE_BREAK
        ["Page Up", 18, 1, 0x91],         //  86 KEY_EN_PAGE_UP
        ["Page Down", 18, 2, 0x92],       //  87 KEY_EN_PAGE_DOWN
        ["Right Arrow", 18, 5, 0x95],     //  88 KEY_EN_RIGHT_ARROW
        ["NumLock", 20, 1, 0x99],         //  89 KEY_EN_NUMPAD_LOCK
        ["Num 7", 20, 2, 0x9A],           //  90 KEY_EN_NUMPAD_7
        ["Num 4", 20, 3, 0x9B],           //  91 KEY_EN_NUMPAD_4
        ["Num 1", 20, 4, 0x9C],           //  92 KEY_EN_NUMPAD_1
        ["Num 0", 20, 5, 0x9D],           //  93 KEY_EN_NUMPAD_0
        ["Num /", 21, 1, 0xA1],           //  94 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 21, 2, 0xA2],           //  95 KEY_EN_NUMPAD_8
        ["Num 5", 21, 3, 0xA3],           //  96 KEY_EN_NUMPAD_5
        ["Num 2", 21, 4, 0xA4],           //  97 KEY_EN_NUMPAD_2
        ["Num *", 22, 1, 0xA9],           //  98 KEY_EN_NUMPAD_TIMES
        ["Num 9", 22, 2, 0xAA],           //  99 KEY_EN_NUMPAD_9
        ["Num 6", 22, 3, 0xAB],           // 100 KEY_EN_NUMPAD_6
        ["Num 3", 22, 4, 0xAC],           // 101 KEY_EN_NUMPAD_3
        ["Num .", 22, 5, 0xAD],           // 102 KEY_EN_NUMPAD_PERIOD
        ["Num -", 23, 1, 0xB1],           // 103 KEY_EN_NUMPAD_MINUS
        ["Num +", 23, 2, 0xB2],           // 104 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 23, 4, 0xB4],       // 105 KEY_EN_NUMPAD_ENTER
      ]),
    }),
    // ASUS_TUF_K7_LAYOUT_US, 105 LEDs, matrix ASUS_CLAYMORE_NUMPAD_RIGHT_LAYOUT_KEYS_ANSI
    17: Object.freeze({
      width: 24,
      height: 7,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["1", 1, 1, 0x09],                //   6 KEY_EN_1
        ["Left Win", 1, 5, 0x0D],         //   7 KEY_EN_LEFT_WINDOWS
        ["F1", 2, 0, 0x10],               //   8 KEY_EN_F1
        ["2", 2, 1, 0x11],                //   9 KEY_EN_2
        ["Q", 2, 2, 0x0A],                //  10 KEY_EN_Q
        ["A", 2, 3, 0x0B],                //  11 KEY_EN_A
        ["Z", 2, 4, 0x0C],                //  12 KEY_EN_Z
        ["Left Alt", 2, 5, 0x15],         //  13 KEY_EN_LEFT_ALT
        ["F2", 3, 0, 0x18],               //  14 KEY_EN_F2
        ["3", 3, 1, 0x19],                //  15 KEY_EN_3
        ["W", 3, 2, 0x12],                //  16 KEY_EN_W
        ["S", 3, 3, 0x13],                //  17 KEY_EN_S
        ["X", 3, 4, 0x14],                //  18 KEY_EN_X
        ["F3", 4, 0, 0x20],               //  19 KEY_EN_F3
        ["4", 4, 1, 0x21],                //  20 KEY_EN_4
        ["E", 4, 2, 0x1A],                //  21 KEY_EN_E
        ["D", 4, 3, 0x1B],                //  22 KEY_EN_D
        ["C", 4, 4, 0x1C],                //  23 KEY_EN_C
        ["F4", 5, 0, 0x28],               //  24 KEY_EN_F4
        ["5", 5, 1, 0x29],                //  25 KEY_EN_5
        ["R", 5, 2, 0x22],                //  26 KEY_EN_R
        ["F", 5, 3, 0x23],                //  27 KEY_EN_F
        ["V", 5, 4, 0x24],                //  28 KEY_EN_V
        ["6", 6, 1, 0x31],                //  29 KEY_EN_6
        ["T", 6, 2, 0x2A],                //  30 KEY_EN_T
        ["G", 6, 3, 0x2B],                //  31 KEY_EN_G
        ["B", 6, 4, 0x2C],                //  32 KEY_EN_B
        ["Space", 6, 5, 0x25],            //  33 KEY_EN_SPACE
        ["F5", 7, 0, 0x38],               //  34 KEY_EN_F5
        ["7", 7, 1, 0x39],                //  35 KEY_EN_7
        ["Y", 7, 2, 0x32],                //  36 KEY_EN_Y
        ["H", 7, 3, 0x33],                //  37 KEY_EN_H
        ["N", 7, 4, 0x34],                //  38 KEY_EN_N
        ["F6", 8, 0, 0x40],               //  39 KEY_EN_F6
        ["8", 8, 1, 0x41],                //  40 KEY_EN_8
        ["U", 8, 2, 0x3A],                //  41 KEY_EN_U
        ["J", 8, 3, 0x3B],                //  42 KEY_EN_J
        ["M", 8, 4, 0x3C],                //  43 KEY_EN_M
        ["Logo", 8, 6, 0x3D],             //  44 Logo
        ["F7", 9, 0, 0x48],               //  45 KEY_EN_F7
        ["9", 9, 1, 0x49],                //  46 KEY_EN_9
        ["I", 9, 2, 0x42],                //  47 KEY_EN_I
        ["K", 9, 3, 0x43],                //  48 KEY_EN_K
        [",", 9, 4, 0x44],                //  49 KEY_EN_COMMA
        ["F8", 10, 0, 0x50],              //  50 KEY_EN_F8
        ["0", 10, 1, 0x51],               //  51 KEY_EN_0
        ["O", 10, 2, 0x4A],               //  52 KEY_EN_O
        ["L", 10, 3, 0x4B],               //  53 KEY_EN_L
        [".", 10, 4, 0x4C],               //  54 KEY_EN_PERIOD
        ["Right Alt", 10, 5, 0x4D],       //  55 KEY_EN_RIGHT_ALT
        ["F9", 11, 0, 0x60],              //  56 KEY_EN_F9
        ["-", 11, 1, 0x59],               //  57 KEY_EN_MINUS
        ["P", 11, 2, 0x52],               //  58 KEY_EN_P
        [";", 11, 3, 0x53],               //  59 KEY_EN_SEMICOLON
        ["/", 11, 4, 0x54],               //  60 KEY_EN_FORWARD_SLASH
        ["Fn", 11, 5, 0x55],              //  61 KEY_EN_RIGHT_FUNCTION
        ["F10", 12, 0, 0x68],             //  62 KEY_EN_F10
        ["=", 12, 1, 0x61],               //  63 KEY_EN_EQUALS
        ["[", 12, 2, 0x5A],               //  64 KEY_EN_LEFT_BRACKET
        ["'", 12, 3, 0x5B],               //  65 KEY_EN_QUOTE
        ["Menu", 12, 5, 0x5D],            //  66 KEY_EN_MENU
        ["F11", 13, 0, 0x70],             //  67 KEY_EN_F11
        ["Backspace", 13, 1, 0x69],       //  68 KEY_EN_BACKSPACE
        ["]", 13, 2, 0x62],               //  69 KEY_EN_RIGHT_BRACKET
        ["Right Shift", 13, 4, 0x6C],     //  70 KEY_EN_RIGHT_SHIFT
        ["F12", 14, 0, 0x78],             //  71 KEY_EN_F12
        ["\\", 14, 2, 0x6A],              //  72 KEY_EN_ANSI_BACK_SLASH
        ["Enter", 14, 3, 0x6B],           //  73 KEY_EN_ANSI_ENTER
        ["Right Ctrl", 14, 5, 0x6D],      //  74 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 16, 0, 0x80],    //  75 KEY_EN_PRINT_SCREEN
        ["Insert", 16, 1, 0x81],          //  76 KEY_EN_INSERT
        ["Del", 16, 2, 0x82],             //  77 KEY_EN_DELETE
        ["Left Arrow", 16, 5, 0x85],      //  78 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 17, 0, 0x88],     //  79 KEY_EN_SCROLL_LOCK
        ["Home", 17, 1, 0x89],            //  80 KEY_EN_HOME
        ["End", 17, 2, 0x8A],             //  81 KEY_EN_END
        ["Up Arrow", 17, 4, 0x8C],        //  82 KEY_EN_UP_ARROW
        ["Down Arrow", 17, 5, 0x8D],      //  83 KEY_EN_DOWN_ARROW
        ["Pause Break", 18, 0, 0x90],     //  84 KEY_EN_PAUSE_BREAK
        ["Page Up", 18, 1, 0x91],         //  85 KEY_EN_PAGE_UP
        ["Page Down", 18, 2, 0x92],       //  86 KEY_EN_PAGE_DOWN
        ["Right Arrow", 18, 5, 0x95],     //  87 KEY_EN_RIGHT_ARROW
        ["NumLock", 20, 1, 0x99],         //  88 KEY_EN_NUMPAD_LOCK
        ["Num 7", 20, 2, 0x9A],           //  89 KEY_EN_NUMPAD_7
        ["Num 4", 20, 3, 0x9B],           //  90 KEY_EN_NUMPAD_4
        ["Num 1", 20, 4, 0x9C],           //  91 KEY_EN_NUMPAD_1
        ["Num 0", 20, 5, 0x9D],           //  92 KEY_EN_NUMPAD_0
        ["Num /", 21, 1, 0xA1],           //  93 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 21, 2, 0xA2],           //  94 KEY_EN_NUMPAD_8
        ["Num 5", 21, 3, 0xA3],           //  95 KEY_EN_NUMPAD_5
        ["Num 2", 21, 4, 0xA4],           //  96 KEY_EN_NUMPAD_2
        ["Num *", 22, 1, 0xA9],           //  97 KEY_EN_NUMPAD_TIMES
        ["Num 9", 22, 2, 0xAA],           //  98 KEY_EN_NUMPAD_9
        ["Num 6", 22, 3, 0xAB],           //  99 KEY_EN_NUMPAD_6
        ["Num 3", 22, 4, 0xAC],           // 100 KEY_EN_NUMPAD_3
        ["Num .", 22, 5, 0xAD],           // 101 KEY_EN_NUMPAD_PERIOD
        ["Num -", 23, 1, 0xB1],           // 102 KEY_EN_NUMPAD_MINUS
        ["Num +", 23, 2, 0xB2],           // 103 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 23, 4, 0xB4],       // 104 KEY_EN_NUMPAD_ENTER
      ]),
    }),
  }),
  AsusClaymoreNumpadLeftLayouts: Object.freeze({
    // ASUS_TUF_K7_LAYOUT_UK, 106 LEDs, matrix ASUS_CLAYMORE_NUMPAD_LEFT_LAYOUT_KEYS_ISO
    4: Object.freeze({
      width: 24,
      height: 7,
      leds: Object.freeze([
        ["NumLock", 0, 1, 0x01],          //   0 KEY_EN_NUMPAD_LOCK
        ["Num 7", 0, 2, 0x02],            //   1 KEY_EN_NUMPAD_7
        ["Num 4", 0, 3, 0x03],            //   2 KEY_EN_NUMPAD_4
        ["Num 1", 0, 4, 0x04],            //   3 KEY_EN_NUMPAD_1
        ["Num 0", 0, 5, 0x05],            //   4 KEY_EN_NUMPAD_0
        ["Num /", 1, 1, 0x09],            //   5 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 1, 2, 0x0A],            //   6 KEY_EN_NUMPAD_8
        ["Num 5", 1, 3, 0x0B],            //   7 KEY_EN_NUMPAD_5
        ["Num 2", 1, 4, 0x0C],            //   8 KEY_EN_NUMPAD_2
        ["Num *", 2, 1, 0x11],            //   9 KEY_EN_NUMPAD_TIMES
        ["Num 9", 2, 2, 0x12],            //  10 KEY_EN_NUMPAD_9
        ["Num 6", 2, 3, 0x13],            //  11 KEY_EN_NUMPAD_6
        ["Num 3", 2, 4, 0x14],            //  12 KEY_EN_NUMPAD_3
        ["Num .", 2, 5, 0x15],            //  13 KEY_EN_NUMPAD_PERIOD
        ["Num -", 3, 1, 0x19],            //  14 KEY_EN_NUMPAD_MINUS
        ["Num +", 3, 2, 0x1A],            //  15 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 3, 4, 0x1C],        //  16 KEY_EN_NUMPAD_ENTER
        ["Esc", 5, 0, 0x20],              //  17 KEY_EN_ESCAPE
        ["`", 5, 1, 0x21],                //  18 KEY_EN_BACK_TICK
        ["Tab", 5, 2, 0x22],              //  19 KEY_EN_TAB
        ["Caps Lock", 5, 3, 0x23],        //  20 KEY_EN_CAPS_LOCK
        ["Left Shift", 5, 4, 0x24],       //  21 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 5, 5, 0x25],        //  22 KEY_EN_LEFT_CONTROL
        ["1", 6, 1, 0x29],                //  23 KEY_EN_1
        ["ISO_<", 6, 4, 0x2C],            //  24 KEY_EN_ISO_BACK_SLASH
        ["Left Win", 6, 5, 0x2D],         //  25 KEY_EN_LEFT_WINDOWS
        ["F1", 7, 0, 0x30],               //  26 KEY_EN_F1
        ["2", 7, 1, 0x31],                //  27 KEY_EN_2
        ["Q", 7, 2, 0x2A],                //  28 KEY_EN_Q
        ["A", 7, 3, 0x2B],                //  29 KEY_EN_A
        ["Z", 7, 4, 0x34],                //  30 KEY_EN_Z
        ["Left Alt", 7, 5, 0x35],         //  31 KEY_EN_LEFT_ALT
        ["F2", 8, 0, 0x38],               //  32 KEY_EN_F2
        ["3", 8, 1, 0x39],                //  33 KEY_EN_3
        ["W", 8, 2, 0x32],                //  34 KEY_EN_W
        ["S", 8, 3, 0x33],                //  35 KEY_EN_S
        ["X", 8, 4, 0x3C],                //  36 KEY_EN_X
        ["F3", 9, 0, 0x40],               //  37 KEY_EN_F3
        ["4", 9, 1, 0x41],                //  38 KEY_EN_4
        ["E", 9, 2, 0x3A],                //  39 KEY_EN_E
        ["D", 9, 3, 0x3B],                //  40 KEY_EN_D
        ["C", 9, 4, 0x44],                //  41 KEY_EN_C
        ["F4", 10, 0, 0x48],              //  42 KEY_EN_F4
        ["5", 10, 1, 0x49],               //  43 KEY_EN_5
        ["R", 10, 2, 0x42],               //  44 KEY_EN_R
        ["F", 10, 3, 0x43],               //  45 KEY_EN_F
        ["V", 10, 4, 0x4C],               //  46 KEY_EN_V
        ["6", 11, 1, 0x51],               //  47 KEY_EN_6
        ["T", 11, 2, 0x4A],               //  48 KEY_EN_T
        ["G", 11, 3, 0x4B],               //  49 KEY_EN_G
        ["B", 11, 4, 0x54],               //  50 KEY_EN_B
        ["Space", 11, 5, 0x45],           //  51 KEY_EN_SPACE
        ["F5", 12, 0, 0x58],              //  52 KEY_EN_F5
        ["7", 12, 1, 0x59],               //  53 KEY_EN_7
        ["Y", 12, 2, 0x52],               //  54 KEY_EN_Y
        ["H", 12, 3, 0x53],               //  55 KEY_EN_H
        ["N", 12, 4, 0x5C],               //  56 KEY_EN_N
        ["F6", 13, 0, 0x60],              //  57 KEY_EN_F6
        ["8", 13, 1, 0x61],               //  58 KEY_EN_8
        ["U", 13, 2, 0x5A],               //  59 KEY_EN_U
        ["J", 13, 3, 0x5B],               //  60 KEY_EN_J
        ["M", 13, 4, 0x64],               //  61 KEY_EN_M
        ["Logo", 13, 6, 0x65],            //  62 Logo
        ["F7", 14, 0, 0x68],              //  63 KEY_EN_F7
        ["9", 14, 1, 0x69],               //  64 KEY_EN_9
        ["I", 14, 2, 0x62],               //  65 KEY_EN_I
        ["K", 14, 3, 0x63],               //  66 KEY_EN_K
        [",", 14, 4, 0x6C],               //  67 KEY_EN_COMMA
        ["F8", 15, 0, 0x70],              //  68 KEY_EN_F8
        ["0", 15, 1, 0x71],               //  69 KEY_EN_0
        ["O", 15, 2, 0x6A],               //  70 KEY_EN_O
        ["L", 15, 3, 0x6B],               //  71 KEY_EN_L
        [".", 15, 4, 0x74],               //  72 KEY_EN_PERIOD
        ["Right Alt", 15, 5, 0x6D],       //  73 KEY_EN_RIGHT_ALT
        ["F9", 16, 0, 0x80],              //  74 KEY_EN_F9
        ["-", 16, 1, 0x79],               //  75 KEY_EN_MINUS
        ["P", 16, 2, 0x72],               //  76 KEY_EN_P
        [";", 16, 3, 0x73],               //  77 KEY_EN_SEMICOLON
        ["/", 16, 4, 0x7C],               //  78 KEY_EN_FORWARD_SLASH
        ["Fn", 16, 5, 0x75],              //  79 KEY_EN_RIGHT_FUNCTION
        ["F10", 17, 0, 0x88],             //  80 KEY_EN_F10
        ["=", 17, 1, 0x81],               //  81 KEY_EN_EQUALS
        ["[", 17, 2, 0x7A],               //  82 KEY_EN_LEFT_BRACKET
        ["'", 17, 3, 0x7B],               //  83 KEY_EN_QUOTE
        ["Menu", 17, 5, 0x7D],            //  84 KEY_EN_MENU
        ["F11", 18, 0, 0x90],             //  85 KEY_EN_F11
        ["Backspace", 18, 1, 0x89],       //  86 KEY_EN_BACKSPACE
        ["]", 18, 2, 0x82],               //  87 KEY_EN_RIGHT_BRACKET
        ["ISO_#", 18, 3, 0x83],           //  88 KEY_EN_POUND
        ["Right Shift", 18, 4, 0x8C],     //  89 KEY_EN_RIGHT_SHIFT
        ["F12", 19, 0, 0x98],             //  90 KEY_EN_F12
        ["Enter", 19, 3, 0x8B],           //  91 KEY_EN_ISO_ENTER
        ["Right Ctrl", 19, 5, 0x8D],      //  92 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 21, 0, 0xA0],    //  93 KEY_EN_PRINT_SCREEN
        ["Insert", 21, 1, 0xA1],          //  94 KEY_EN_INSERT
        ["Del", 21, 2, 0xA2],             //  95 KEY_EN_DELETE
        ["Left Arrow", 21, 5, 0xA5],      //  96 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 22, 0, 0xA8],     //  97 KEY_EN_SCROLL_LOCK
        ["Home", 22, 1, 0xA9],            //  98 KEY_EN_HOME
        ["End", 22, 2, 0xAA],             //  99 KEY_EN_END
        ["Up Arrow", 22, 4, 0xAC],        // 100 KEY_EN_UP_ARROW
        ["Down Arrow", 22, 5, 0xAD],      // 101 KEY_EN_DOWN_ARROW
        ["Pause Break", 23, 0, 0xB0],     // 102 KEY_EN_PAUSE_BREAK
        ["Page Up", 23, 1, 0xB1],         // 103 KEY_EN_PAGE_UP
        ["Page Down", 23, 2, 0xB2],       // 104 KEY_EN_PAGE_DOWN
        ["Right Arrow", 23, 5, 0xB5],     // 105 KEY_EN_RIGHT_ARROW
      ]),
    }),
    // ASUS_TUF_K7_LAYOUT_US, 105 LEDs, matrix ASUS_CLAYMORE_NUMPAD_LEFT_LAYOUT_KEYS_ANSI
    17: Object.freeze({
      width: 24,
      height: 7,
      leds: Object.freeze([
        ["NumLock", 0, 1, 0x01],          //   0 KEY_EN_NUMPAD_LOCK
        ["Num 7", 0, 2, 0x02],            //   1 KEY_EN_NUMPAD_7
        ["Num 4", 0, 3, 0x03],            //   2 KEY_EN_NUMPAD_4
        ["Num 1", 0, 4, 0x04],            //   3 KEY_EN_NUMPAD_1
        ["Num 0", 0, 5, 0x05],            //   4 KEY_EN_NUMPAD_0
        ["Num /", 1, 1, 0x09],            //   5 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 1, 2, 0x0A],            //   6 KEY_EN_NUMPAD_8
        ["Num 5", 1, 3, 0x0B],            //   7 KEY_EN_NUMPAD_5
        ["Num 2", 1, 4, 0x0C],            //   8 KEY_EN_NUMPAD_2
        ["Num *", 2, 1, 0x11],            //   9 KEY_EN_NUMPAD_TIMES
        ["Num 9", 2, 2, 0x12],            //  10 KEY_EN_NUMPAD_9
        ["Num 6", 2, 3, 0x13],            //  11 KEY_EN_NUMPAD_6
        ["Num 3", 2, 4, 0x14],            //  12 KEY_EN_NUMPAD_3
        ["Num .", 2, 5, 0x15],            //  13 KEY_EN_NUMPAD_PERIOD
        ["Num -", 3, 1, 0x19],            //  14 KEY_EN_NUMPAD_MINUS
        ["Num +", 3, 2, 0x1A],            //  15 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 3, 4, 0x1C],        //  16 KEY_EN_NUMPAD_ENTER
        ["Esc", 5, 0, 0x20],              //  17 KEY_EN_ESCAPE
        ["`", 5, 1, 0x21],                //  18 KEY_EN_BACK_TICK
        ["Tab", 5, 2, 0x22],              //  19 KEY_EN_TAB
        ["Caps Lock", 5, 3, 0x23],        //  20 KEY_EN_CAPS_LOCK
        ["Left Shift", 5, 4, 0x24],       //  21 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 5, 5, 0x25],        //  22 KEY_EN_LEFT_CONTROL
        ["1", 6, 1, 0x29],                //  23 KEY_EN_1
        ["Left Win", 6, 5, 0x2D],         //  24 KEY_EN_LEFT_WINDOWS
        ["F1", 7, 0, 0x30],               //  25 KEY_EN_F1
        ["2", 7, 1, 0x31],                //  26 KEY_EN_2
        ["Q", 7, 2, 0x2A],                //  27 KEY_EN_Q
        ["A", 7, 3, 0x2B],                //  28 KEY_EN_A
        ["Z", 7, 4, 0x2C],                //  29 KEY_EN_Z
        ["Left Alt", 7, 5, 0x35],         //  30 KEY_EN_LEFT_ALT
        ["F2", 8, 0, 0x38],               //  31 KEY_EN_F2
        ["3", 8, 1, 0x39],                //  32 KEY_EN_3
        ["W", 8, 2, 0x32],                //  33 KEY_EN_W
        ["S", 8, 3, 0x33],                //  34 KEY_EN_S
        ["X", 8, 4, 0x34],                //  35 KEY_EN_X
        ["F3", 9, 0, 0x40],               //  36 KEY_EN_F3
        ["4", 9, 1, 0x41],                //  37 KEY_EN_4
        ["E", 9, 2, 0x3A],                //  38 KEY_EN_E
        ["D", 9, 3, 0x3B],                //  39 KEY_EN_D
        ["C", 9, 4, 0x3C],                //  40 KEY_EN_C
        ["F4", 10, 0, 0x48],              //  41 KEY_EN_F4
        ["5", 10, 1, 0x49],               //  42 KEY_EN_5
        ["R", 10, 2, 0x42],               //  43 KEY_EN_R
        ["F", 10, 3, 0x43],               //  44 KEY_EN_F
        ["V", 10, 4, 0x44],               //  45 KEY_EN_V
        ["6", 11, 1, 0x51],               //  46 KEY_EN_6
        ["T", 11, 2, 0x4A],               //  47 KEY_EN_T
        ["G", 11, 3, 0x4B],               //  48 KEY_EN_G
        ["B", 11, 4, 0x4C],               //  49 KEY_EN_B
        ["Space", 11, 5, 0x45],           //  50 KEY_EN_SPACE
        ["F5", 12, 0, 0x58],              //  51 KEY_EN_F5
        ["7", 12, 1, 0x59],               //  52 KEY_EN_7
        ["Y", 12, 2, 0x52],               //  53 KEY_EN_Y
        ["H", 12, 3, 0x53],               //  54 KEY_EN_H
        ["N", 12, 4, 0x54],               //  55 KEY_EN_N
        ["F6", 13, 0, 0x60],              //  56 KEY_EN_F6
        ["8", 13, 1, 0x61],               //  57 KEY_EN_8
        ["U", 13, 2, 0x5A],               //  58 KEY_EN_U
        ["J", 13, 3, 0x5B],               //  59 KEY_EN_J
        ["M", 13, 4, 0x5C],               //  60 KEY_EN_M
        ["Logo", 13, 6, 0x5D],            //  61 Logo
        ["F7", 14, 0, 0x68],              //  62 KEY_EN_F7
        ["9", 14, 1, 0x69],               //  63 KEY_EN_9
        ["I", 14, 2, 0x62],               //  64 KEY_EN_I
        ["K", 14, 3, 0x63],               //  65 KEY_EN_K
        [",", 14, 4, 0x64],               //  66 KEY_EN_COMMA
        ["F8", 15, 0, 0x70],              //  67 KEY_EN_F8
        ["0", 15, 1, 0x71],               //  68 KEY_EN_0
        ["O", 15, 2, 0x6A],               //  69 KEY_EN_O
        ["L", 15, 3, 0x6B],               //  70 KEY_EN_L
        [".", 15, 4, 0x6C],               //  71 KEY_EN_PERIOD
        ["Right Alt", 15, 5, 0x6D],       //  72 KEY_EN_RIGHT_ALT
        ["F9", 16, 0, 0x80],              //  73 KEY_EN_F9
        ["-", 16, 1, 0x79],               //  74 KEY_EN_MINUS
        ["P", 16, 2, 0x72],               //  75 KEY_EN_P
        [";", 16, 3, 0x73],               //  76 KEY_EN_SEMICOLON
        ["/", 16, 4, 0x74],               //  77 KEY_EN_FORWARD_SLASH
        ["Fn", 16, 5, 0x75],              //  78 KEY_EN_RIGHT_FUNCTION
        ["F10", 17, 0, 0x88],             //  79 KEY_EN_F10
        ["=", 17, 1, 0x81],               //  80 KEY_EN_EQUALS
        ["[", 17, 2, 0x7A],               //  81 KEY_EN_LEFT_BRACKET
        ["'", 17, 3, 0x7B],               //  82 KEY_EN_QUOTE
        ["Menu", 17, 5, 0x7D],            //  83 KEY_EN_MENU
        ["F11", 18, 0, 0x90],             //  84 KEY_EN_F11
        ["Backspace", 18, 1, 0x89],       //  85 KEY_EN_BACKSPACE
        ["]", 18, 2, 0x82],               //  86 KEY_EN_RIGHT_BRACKET
        ["Right Shift", 18, 4, 0x8C],     //  87 KEY_EN_RIGHT_SHIFT
        ["F12", 19, 0, 0x98],             //  88 KEY_EN_F12
        ["\\", 19, 2, 0x8A],              //  89 KEY_EN_ANSI_BACK_SLASH
        ["Enter", 19, 3, 0x8B],           //  90 KEY_EN_ANSI_ENTER
        ["Right Ctrl", 19, 5, 0x8D],      //  91 KEY_EN_RIGHT_CONTROL
        ["Print Screen", 21, 0, 0xA0],    //  92 KEY_EN_PRINT_SCREEN
        ["Insert", 21, 1, 0xA1],          //  93 KEY_EN_INSERT
        ["Del", 21, 2, 0xA2],             //  94 KEY_EN_DELETE
        ["Left Arrow", 21, 5, 0xA5],      //  95 KEY_EN_LEFT_ARROW
        ["Scroll Lock", 22, 0, 0xA8],     //  96 KEY_EN_SCROLL_LOCK
        ["Home", 22, 1, 0xA9],            //  97 KEY_EN_HOME
        ["End", 22, 2, 0xAA],             //  98 KEY_EN_END
        ["Up Arrow", 22, 4, 0xAC],        //  99 KEY_EN_UP_ARROW
        ["Down Arrow", 22, 5, 0xAD],      // 100 KEY_EN_DOWN_ARROW
        ["Pause Break", 23, 0, 0xB0],     // 101 KEY_EN_PAUSE_BREAK
        ["Page Up", 23, 1, 0xB1],         // 102 KEY_EN_PAGE_UP
        ["Page Down", 23, 2, 0xB2],       // 103 KEY_EN_PAGE_DOWN
        ["Right Arrow", 23, 5, 0xB5],     // 104 KEY_EN_RIGHT_ARROW
      ]),
    }),
  }),
  AsusTufK1Layouts: Object.freeze({
    // ASUS_TUF_K7_LAYOUT_UK, 5 LEDs, matrix ASUS_TUF_K1_LAYOUT_KEYS
    4: Object.freeze({
      width: 5,
      height: 1,
      leds: Object.freeze([
        ["Keyboard LED 1", 0, 0, 0x00],   //   0 Keyboard LED 1
        ["Keyboard LED 2", 1, 0, 0x01],   //   1 Keyboard LED 2
        ["Keyboard LED 3", 2, 0, 0x02],   //   2 Keyboard LED 3
        ["Keyboard LED 4", 3, 0, 0x03],   //   3 Keyboard LED 4
        ["Keyboard LED 5", 4, 0, 0x04],   //   4 Keyboard LED 5
      ]),
    }),
    // ASUS_TUF_K7_LAYOUT_US, 5 LEDs, matrix ASUS_TUF_K1_LAYOUT_KEYS
    17: Object.freeze({
      width: 5,
      height: 1,
      leds: Object.freeze([
        ["Keyboard LED 1", 0, 0, 0x00],   //   0 Keyboard LED 1
        ["Keyboard LED 2", 1, 0, 0x01],   //   1 Keyboard LED 2
        ["Keyboard LED 3", 2, 0, 0x02],   //   2 Keyboard LED 3
        ["Keyboard LED 4", 3, 0, 0x03],   //   3 Keyboard LED 4
        ["Keyboard LED 5", 4, 0, 0x04],   //   4 Keyboard LED 5
      ]),
    }),
  }),
  AsusTUFK3GamingGen2Layouts: Object.freeze({
    // ASUS_TUF_K7_LAYOUT_US, 97 LEDs, matrix ASUS_TUF_K3_GAMING_GEN_II_LAYOUT_KEYS_ANSI
    17: Object.freeze({
      width: 19,
      height: 6,
      leds: Object.freeze([
        ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
        ["`", 0, 1, 0x01],                //   1 KEY_EN_BACK_TICK
        ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
        ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
        ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
        ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
        ["1", 1, 1, 0x09],                //   6 KEY_EN_1
        ["Left Win", 1, 5, 0x0D],         //   7 KEY_EN_LEFT_WINDOWS
        ["F1", 2, 0, 0x10],               //   8 KEY_EN_F1
        ["2", 2, 1, 0x11],                //   9 KEY_EN_2
        ["Q", 2, 2, 0x0A],                //  10 KEY_EN_Q
        ["A", 2, 3, 0x0B],                //  11 KEY_EN_A
        ["Z", 2, 4, 0x14],                //  12 KEY_EN_Z
        ["Left Alt", 2, 5, 0x15],         //  13 KEY_EN_LEFT_ALT
        ["F2", 3, 0, 0x18],               //  14 KEY_EN_F2
        ["3", 3, 1, 0x19],                //  15 KEY_EN_3
        ["W", 3, 2, 0x12],                //  16 KEY_EN_W
        ["S", 3, 3, 0x13],                //  17 KEY_EN_S
        ["X", 3, 4, 0x1C],                //  18 KEY_EN_X
        ["F3", 4, 0, 0x20],               //  19 KEY_EN_F3
        ["4", 4, 1, 0x21],                //  20 KEY_EN_4
        ["E", 4, 2, 0x1A],                //  21 KEY_EN_E
        ["D", 4, 3, 0x1B],                //  22 KEY_EN_D
        ["C", 4, 4, 0x24],                //  23 KEY_EN_C
        ["F4", 5, 0, 0x28],               //  24 KEY_EN_F4
        ["5", 5, 1, 0x29],                //  25 KEY_EN_5
        ["R", 5, 2, 0x22],                //  26 KEY_EN_R
        ["F", 5, 3, 0x23],                //  27 KEY_EN_F
        ["V", 5, 4, 0x2C],                //  28 KEY_EN_V
        ["6", 6, 1, 0x31],                //  29 KEY_EN_6
        ["T", 6, 2, 0x2A],                //  30 KEY_EN_T
        ["G", 6, 3, 0x2B],                //  31 KEY_EN_G
        ["B", 6, 4, 0x34],                //  32 KEY_EN_B
        ["Space", 6, 5, 0x35],            //  33 KEY_EN_SPACE
        ["F5", 7, 0, 0x30],               //  34 KEY_EN_F5
        ["7", 7, 1, 0x39],                //  35 KEY_EN_7
        ["Y", 7, 2, 0x32],                //  36 KEY_EN_Y
        ["H", 7, 3, 0x33],                //  37 KEY_EN_H
        ["N", 7, 4, 0x3C],                //  38 KEY_EN_N
        ["F6", 8, 0, 0x38],               //  39 KEY_EN_F6
        ["8", 8, 1, 0x41],                //  40 KEY_EN_8
        ["U", 8, 2, 0x3A],                //  41 KEY_EN_U
        ["J", 8, 3, 0x3B],                //  42 KEY_EN_J
        ["M", 8, 4, 0x44],                //  43 KEY_EN_M
        ["F7", 9, 0, 0x40],               //  44 KEY_EN_F7
        ["9", 9, 1, 0x49],                //  45 KEY_EN_9
        ["I", 9, 2, 0x42],                //  46 KEY_EN_I
        ["K", 9, 3, 0x43],                //  47 KEY_EN_K
        [",", 9, 4, 0x4C],                //  48 KEY_EN_COMMA
        ["F8", 10, 0, 0x48],              //  49 KEY_EN_F8
        ["0", 10, 1, 0x51],               //  50 KEY_EN_0
        ["O", 10, 2, 0x4A],               //  51 KEY_EN_O
        ["L", 10, 3, 0x4B],               //  52 KEY_EN_L
        [".", 10, 4, 0x54],               //  53 KEY_EN_PERIOD
        ["Fn", 10, 5, 0x55],              //  54 KEY_EN_RIGHT_FUNCTION
        ["F9", 11, 0, 0x58],              //  55 KEY_EN_F9
        ["-", 11, 1, 0x59],               //  56 KEY_EN_MINUS
        ["P", 11, 2, 0x52],               //  57 KEY_EN_P
        [";", 11, 3, 0x53],               //  58 KEY_EN_SEMICOLON
        ["/", 11, 4, 0x5C],               //  59 KEY_EN_FORWARD_SLASH
        ["Right Ctrl", 11, 5, 0x5D],      //  60 KEY_EN_RIGHT_CONTROL
        ["F10", 12, 0, 0x60],             //  61 KEY_EN_F10
        ["=", 12, 1, 0x61],               //  62 KEY_EN_EQUALS
        ["[", 12, 2, 0x5A],               //  63 KEY_EN_LEFT_BRACKET
        ["'", 12, 3, 0x5B],               //  64 KEY_EN_QUOTE
        ["Right Shift", 12, 4, 0x6C],     //  65 KEY_EN_RIGHT_SHIFT
        ["F11", 13, 0, 0x68],             //  66 KEY_EN_F11
        ["Backspace", 13, 1, 0x71],       //  67 KEY_EN_BACKSPACE
        ["]", 13, 2, 0x62],               //  68 KEY_EN_RIGHT_BRACKET
        ["Enter", 13, 3, 0x73],           //  69 KEY_EN_ANSI_ENTER
        ["Left Arrow", 13, 5, 0x6D],      //  70 KEY_EN_LEFT_ARROW
        ["F12", 14, 0, 0x70],             //  71 KEY_EN_F12
        ["\\", 14, 2, 0x72],              //  72 KEY_EN_ANSI_BACK_SLASH
        ["Up Arrow", 14, 4, 0x74],        //  73 KEY_EN_UP_ARROW
        ["Down Arrow", 14, 5, 0x75],      //  74 KEY_EN_DOWN_ARROW
        ["Del", 15, 0, 0x80],             //  75 KEY_EN_DELETE
        ["NumLock", 15, 1, 0x79],         //  76 KEY_EN_NUMPAD_LOCK
        ["Num 7", 15, 2, 0x7A],           //  77 KEY_EN_NUMPAD_7
        ["Num 4", 15, 3, 0x7B],           //  78 KEY_EN_NUMPAD_4
        ["Num 1", 15, 4, 0x7C],           //  79 KEY_EN_NUMPAD_1
        ["Right Arrow", 15, 5, 0x7D],     //  80 KEY_EN_RIGHT_ARROW
        ["Insert", 16, 0, 0x78],          //  81 KEY_EN_INSERT
        ["Num /", 16, 1, 0x81],           //  82 KEY_EN_NUMPAD_DIVIDE
        ["Num 8", 16, 2, 0x82],           //  83 KEY_EN_NUMPAD_8
        ["Num 5", 16, 3, 0x83],           //  84 KEY_EN_NUMPAD_5
        ["Num 2", 16, 4, 0x84],           //  85 KEY_EN_NUMPAD_2
        ["Num 0", 16, 5, 0x85],           //  86 KEY_EN_NUMPAD_0
        ["Page Up", 17, 0, 0x88],         //  87 KEY_EN_PAGE_UP
        ["Num *", 17, 1, 0x89],           //  88 KEY_EN_NUMPAD_TIMES
        ["Num 9", 17, 2, 0x8A],           //  89 KEY_EN_NUMPAD_9
        ["Num 6", 17, 3, 0x8B],           //  90 KEY_EN_NUMPAD_6
        ["Num 3", 17, 4, 0x8C],           //  91 KEY_EN_NUMPAD_3
        ["Num .", 17, 5, 0x8D],           //  92 KEY_EN_NUMPAD_PERIOD
        ["Page Down", 18, 0, 0x90],       //  93 KEY_EN_PAGE_DOWN
        ["Num -", 18, 1, 0x91],           //  94 KEY_EN_NUMPAD_MINUS
        ["Num +", 18, 2, 0x92],           //  95 KEY_EN_NUMPAD_PLUS
        ["Num Enter", 18, 4, 0x94],       //  96 KEY_EN_NUMPAD_ENTER
      ]),
    }),
  }),
});

// Scope TKL (AuraKeyboardController, SCOPE_TKL_LAYOUT): the 86 keys at their matrix cells, then
// zone "Logo" (2 LEDs) on row 6 and zone "Underglow" (26 LEDs) on row 7, in OpenRGB's LED order.
export const SCOPE_TKL_LAYOUT = Object.freeze({
  width: 26,
  height: 8,
  leds: Object.freeze([
    ["Esc", 0, 0, 0x00],              //   0 KEY_EN_ESCAPE
    ["`", 0, 1, 0x01],                //   1 KEY_EN_BACK_TICK
    ["Tab", 0, 2, 0x02],              //   2 KEY_EN_TAB
    ["Caps Lock", 0, 3, 0x03],        //   3 KEY_EN_CAPS_LOCK
    ["Left Shift", 0, 4, 0x04],       //   4 KEY_EN_LEFT_SHIFT
    ["Left Ctrl", 0, 5, 0x05],        //   5 KEY_EN_LEFT_CONTROL
    ["ISO_<", 1, 4, 0x0C],            //   6 KEY_EN_ISO_BACK_SLASH
    ["Left Win", 2, 5, 0x15],         //   7 KEY_EN_LEFT_WINDOWS
    ["1", 1, 1, 0x11],                //   8 KEY_EN_1
    ["Q", 2, 2, 0x12],                //   9 KEY_EN_Q
    ["A", 2, 3, 0x13],                //  10 KEY_EN_A
    ["Z", 2, 4, 0x14],                //  11 KEY_EN_Z
    ["Left Alt", 3, 5, 0x1D],         //  12 KEY_EN_LEFT_ALT
    ["F1", 2, 0, 0x18],               //  13 KEY_EN_F1
    ["2", 2, 1, 0x19],                //  14 KEY_EN_2
    ["W", 3, 2, 0x1A],                //  15 KEY_EN_W
    ["S", 3, 3, 0x1B],                //  16 KEY_EN_S
    ["X", 3, 4, 0x1C],                //  17 KEY_EN_X
    ["F2", 3, 0, 0x20],               //  18 KEY_EN_F2
    ["3", 3, 1, 0x21],                //  19 KEY_EN_3
    ["E", 4, 2, 0x22],                //  20 KEY_EN_E
    ["D", 4, 3, 0x23],                //  21 KEY_EN_D
    ["C", 4, 4, 0x24],                //  22 KEY_EN_C
    ["F3", 4, 0, 0x28],               //  23 KEY_EN_F3
    ["4", 4, 1, 0x29],                //  24 KEY_EN_4
    ["R", 5, 2, 0x2A],                //  25 KEY_EN_R
    ["F", 5, 3, 0x2B],                //  26 KEY_EN_F
    ["V", 5, 4, 0x2C],                //  27 KEY_EN_V
    ["F4", 5, 0, 0x30],               //  28 KEY_EN_F4
    ["5", 5, 1, 0x31],                //  29 KEY_EN_5
    ["T", 6, 2, 0x32],                //  30 KEY_EN_T
    ["G", 6, 3, 0x33],                //  31 KEY_EN_G
    ["B", 6, 4, 0x34],                //  32 KEY_EN_B
    ["Space", 7, 5, 0x35],            //  33 KEY_EN_SPACE
    ["6", 6, 1, 0x39],                //  34 KEY_EN_6
    ["Y", 7, 2, 0x3A],                //  35 KEY_EN_Y
    ["H", 7, 3, 0x3B],                //  36 KEY_EN_H
    ["N", 7, 4, 0x3C],                //  37 KEY_EN_N
    ["F5", 6, 0, 0x40],               //  38 KEY_EN_F5
    ["7", 7, 1, 0x41],                //  39 KEY_EN_7
    ["U", 8, 2, 0x42],                //  40 KEY_EN_U
    ["J", 8, 3, 0x43],                //  41 KEY_EN_J
    ["M", 8, 4, 0x44],                //  42 KEY_EN_M
    ["F6", 7, 0, 0x48],               //  43 KEY_EN_F6
    ["8", 8, 1, 0x49],                //  44 KEY_EN_8
    ["I", 9, 2, 0x4A],                //  45 KEY_EN_I
    ["K", 9, 3, 0x4B],                //  46 KEY_EN_K
    [",", 9, 4, 0x4C],                //  47 KEY_EN_COMMA
    ["Right Alt", 9, 5, 0x4D],        //  48 KEY_EN_RIGHT_ALT
    ["F7", 8, 0, 0x50],               //  49 KEY_EN_F7
    ["9", 9, 1, 0x51],                //  50 KEY_EN_9
    ["O", 10, 2, 0x52],               //  51 KEY_EN_O
    ["L", 10, 3, 0x53],               //  52 KEY_EN_L
    [".", 10, 4, 0x54],               //  53 KEY_EN_PERIOD
    ["F8", 9, 0, 0x58],               //  54 KEY_EN_F8
    ["0", 10, 1, 0x59],               //  55 KEY_EN_0
    ["P", 11, 2, 0x5A],               //  56 KEY_EN_P
    [";", 11, 3, 0x5B],               //  57 KEY_EN_SEMICOLON
    ["/", 11, 4, 0x5C],               //  58 KEY_EN_FORWARD_SLASH
    ["Fn", 11, 5, 0x5D],              //  59 KEY_EN_RIGHT_FUNCTION
    ["F9", 10, 0, 0x60],              //  60 KEY_EN_F9
    ["-", 11, 1, 0x61],               //  61 KEY_EN_MINUS
    ["[", 12, 2, 0x62],               //  62 KEY_EN_LEFT_BRACKET
    ["'", 12, 3, 0x63],               //  63 KEY_EN_QUOTE
    ["Menu", 12, 5, 0x65],            //  64 KEY_EN_MENU
    ["F10", 11, 0, 0x68],             //  65 KEY_EN_F10
    ["=", 12, 1, 0x69],               //  66 KEY_EN_EQUALS
    ["]", 13, 2, 0x6A],               //  67 KEY_EN_RIGHT_BRACKET
    ["ISO_#", 13, 3, 0x6B],           //  68 KEY_EN_POUND
    ["F11", 12, 0, 0x70],             //  69 KEY_EN_F11
    ["F12", 13, 0, 0x78],             //  70 KEY_EN_F12
    ["Backspace", 13, 1, 0x79],       //  71 KEY_EN_BACKSPACE
    ["\\", 14, 2, 0x7A],              //  72 KEY_EN_ANSI_BACK_SLASH
    ["Enter", 14, 3, 0x7B],           //  73 KEY_EN_ANSI_ENTER
    ["Right Shift", 12, 4, 0x7C],     //  74 KEY_EN_RIGHT_SHIFT
    ["Right Ctrl", 13, 5, 0x7D],      //  75 KEY_EN_RIGHT_CONTROL
    ["Insert", 15, 1, 0x81],          //  76 KEY_EN_INSERT
    ["Del", 15, 2, 0x82],             //  77 KEY_EN_DELETE
    ["Left Arrow", 15, 5, 0x85],      //  78 KEY_EN_LEFT_ARROW
    ["Home", 16, 1, 0x89],            //  79 KEY_EN_HOME
    ["End", 16, 2, 0x8A],             //  80 KEY_EN_END
    ["Up Arrow", 16, 4, 0x8C],        //  81 KEY_EN_UP_ARROW
    ["Down Arrow", 16, 5, 0x8D],      //  82 KEY_EN_DOWN_ARROW
    ["Page Up", 17, 1, 0x91],         //  83 KEY_EN_PAGE_UP
    ["Page Down", 17, 2, 0x92],       //  84 KEY_EN_PAGE_DOWN
    ["Right Arrow", 17, 5, 0x95],     //  85 KEY_EN_RIGHT_ARROW
    ["Logo 1", 0, 6, 0x80],           //  86 Logo 1
    ["Logo 2", 1, 6, 0x90],           //  87 Logo 2
    ["Underglow 1", 0, 7, 0x06],      //  88 Underglow 1
    ["Underglow 2", 1, 7, 0x0E],      //  89 Underglow 2
    ["Underglow 3", 2, 7, 0x16],      //  90 Underglow 3
    ["Underglow 4", 3, 7, 0x1E],      //  91 Underglow 4
    ["Underglow 5", 4, 7, 0x26],      //  92 Underglow 5
    ["Underglow 6", 5, 7, 0x2E],      //  93 Underglow 6
    ["Underglow 7", 6, 7, 0x36],      //  94 Underglow 7
    ["Underglow 8", 7, 7, 0x3E],      //  95 Underglow 8
    ["Underglow 9", 8, 7, 0x46],      //  96 Underglow 9
    ["Underglow 10", 9, 7, 0x4E],     //  97 Underglow 10
    ["Underglow 11", 10, 7, 0x56],    //  98 Underglow 11
    ["Underglow 12", 11, 7, 0x5E],    //  99 Underglow 12
    ["Underglow 13", 12, 7, 0x66],    // 100 Underglow 13
    ["Underglow 14", 13, 7, 0x6E],    // 101 Underglow 14
    ["Underglow 15", 14, 7, 0x76],    // 102 Underglow 15
    ["Underglow 16", 15, 7, 0x7E],    // 103 Underglow 16
    ["Underglow 17", 16, 7, 0x86],    // 104 Underglow 17
    ["Underglow 18", 17, 7, 0x8E],    // 105 Underglow 18
    ["Underglow 19", 18, 7, 0x96],    // 106 Underglow 19
    ["Underglow 20", 19, 7, 0x9E],    // 107 Underglow 20
    ["Underglow 21", 20, 7, 0xA6],    // 108 Underglow 21
    ["Underglow 22", 21, 7, 0xAE],    // 109 Underglow 22
    ["Underglow 23", 22, 7, 0xB6],    // 110 Underglow 23
    ["Underglow 24", 23, 7, 0xBE],    // 111 Underglow 24
    ["Underglow 25", 24, 7, 0xC6],    // 112 Underglow 25
    ["Underglow 26", 25, 7, 0xCE],    // 113 Underglow 26
  ]),
});
