// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom plugin: Cooler Master MasterKeys (V1) key tables, as data. Generated from the OpenRGB
// tables by a model of OpenRGB's KeyboardLayoutManager; do not edit by hand.
// Adapted from OpenRGB Controllers/CoolerMasterController/CMKeyboardController/
// CMKeyboardDevices.cpp (mk_pro_s_keymap, mk_pro_l_keymap, mk_pro_s_layout, mk_pro_l_layout) and
// KeyboardLayoutManager/KeyboardLayoutManager.cpp (keyboard_zone_main, keyboard_zone_fn_row,
// keyboard_zone_extras, keyboard_zone_numpad, ansi_qwerty, iso_qwerty, jis and the key
// insert / swap rules) (GPL-2.0-or-later) at commit d4798c120659ec5ba2e664a9e87bedff8d36f880,
// https://gitlab.com/CalcProgrammer1/OpenRGB
// Original authors: "Tam D (too.manyhobbies)  30 Nov 2023" (CMKeyboardDevices);
// "Chris M (Dr_No)  04 Feb 2023" (KeyboardLayoutManager).
//
// One table per OpenRGB key layout and regional variant (the keyboard's firmware string picks
// the variant: ANSI, ISO or JIS). Per LED, in OpenRGB's LED order:
//   [name, column, row, value]: the key (Lightloom key names; the comment keeps OpenRGB's
//   constant), its cell in OpenRGB's matrix map, and the LED value (the slot of the
//   128-colour direct buffer the key's colour goes to).

export const PRO_S_ANSI = Object.freeze({
  width: 17,
  height: 6,
  leds: Object.freeze([
    ["Esc", 0, 0, 96],                      //   0 KEY_EN_ESCAPE
    ["F1", 2, 0, 97],                       //   1 KEY_EN_F1
    ["F2", 3, 0, 98],                       //   2 KEY_EN_F2
    ["F3", 4, 0, 99],                       //   3 KEY_EN_F3
    ["F4", 5, 0, 104],                      //   4 KEY_EN_F4
    ["F5", 6, 0, 105],                      //   5 KEY_EN_F5
    ["F6", 7, 0, 106],                      //   6 KEY_EN_F6
    ["F7", 8, 0, 112],                      //   7 KEY_EN_F7
    ["F8", 9, 0, 113],                      //   8 KEY_EN_F8
    ["F9", 10, 0, 114],                     //   9 KEY_EN_F9
    ["F10", 11, 0, 67],                     //  10 KEY_EN_F10
    ["F11", 12, 0, 68],                     //  11 KEY_EN_F11
    ["F12", 13, 0, 69],                     //  12 KEY_EN_F12
    ["Print Screen", 14, 0, 102],           //  13 KEY_EN_PRINT_SCREEN
    ["Scroll Lock", 15, 0, 103],            //  14 KEY_EN_SCROLL_LOCK
    ["Pause Break", 16, 0, 107],            //  15 KEY_EN_PAUSE_BREAK
    ["`", 0, 1, 0],                         //  16 KEY_EN_BACK_TICK
    ["1", 1, 1, 1],                         //  17 KEY_EN_1
    ["2", 2, 1, 8],                         //  18 KEY_EN_2
    ["3", 3, 1, 9],                         //  19 KEY_EN_3
    ["4", 4, 1, 16],                        //  20 KEY_EN_4
    ["5", 5, 1, 17],                        //  21 KEY_EN_5
    ["6", 6, 1, 24],                        //  22 KEY_EN_6
    ["7", 7, 1, 25],                        //  23 KEY_EN_7
    ["8", 8, 1, 32],                        //  24 KEY_EN_8
    ["9", 9, 1, 33],                        //  25 KEY_EN_9
    ["0", 10, 1, 40],                       //  26 KEY_EN_0
    ["-", 11, 1, 41],                       //  27 KEY_EN_MINUS
    ["=", 12, 1, 48],                       //  28 KEY_EN_EQUALS
    ["Backspace", 13, 1, 49],               //  29 KEY_EN_BACKSPACE
    ["Insert", 14, 1, 56],                  //  30 KEY_EN_INSERT
    ["Home", 15, 1, 57],                    //  31 KEY_EN_HOME
    ["Page Up", 16, 1, 64],                 //  32 KEY_EN_PAGE_UP
    ["Tab", 0, 2, 2],                       //  33 KEY_EN_TAB
    ["Q", 1, 2, 3],                         //  34 KEY_EN_Q
    ["W", 2, 2, 10],                        //  35 KEY_EN_W
    ["E", 3, 2, 11],                        //  36 KEY_EN_E
    ["R", 4, 2, 18],                        //  37 KEY_EN_R
    ["T", 5, 2, 19],                        //  38 KEY_EN_T
    ["Y", 6, 2, 26],                        //  39 KEY_EN_Y
    ["U", 7, 2, 27],                        //  40 KEY_EN_U
    ["I", 8, 2, 34],                        //  41 KEY_EN_I
    ["O", 9, 2, 35],                        //  42 KEY_EN_O
    ["P", 10, 2, 42],                       //  43 KEY_EN_P
    ["[", 11, 2, 43],                       //  44 KEY_EN_LEFT_BRACKET
    ["]", 12, 2, 50],                       //  45 KEY_EN_RIGHT_BRACKET
    ["\\", 13, 2, 51],                      //  46 KEY_EN_ANSI_BACK_SLASH
    ["Del", 14, 2, 58],                     //  47 KEY_EN_DELETE
    ["End", 15, 2, 59],                     //  48 KEY_EN_END
    ["Page Down", 16, 2, 66],               //  49 KEY_EN_PAGE_DOWN
    ["Caps Lock", 0, 3, 4],                 //  50 KEY_EN_CAPS_LOCK
    ["A", 1, 3, 5],                         //  51 KEY_EN_A
    ["S", 2, 3, 12],                        //  52 KEY_EN_S
    ["D", 3, 3, 13],                        //  53 KEY_EN_D
    ["F", 4, 3, 20],                        //  54 KEY_EN_F
    ["G", 5, 3, 21],                        //  55 KEY_EN_G
    ["H", 6, 3, 28],                        //  56 KEY_EN_H
    ["J", 7, 3, 29],                        //  57 KEY_EN_J
    ["K", 8, 3, 36],                        //  58 KEY_EN_K
    ["L", 9, 3, 37],                        //  59 KEY_EN_L
    [";", 10, 3, 44],                       //  60 KEY_EN_SEMICOLON
    ["'", 11, 3, 45],                       //  61 KEY_EN_QUOTE
    ["Enter", 13, 3, 52],                   //  62 KEY_EN_ANSI_ENTER
    ["Left Shift", 0, 4, 6],                //  63 KEY_EN_LEFT_SHIFT
    ["Z", 2, 4, 7],                         //  64 KEY_EN_Z
    ["X", 3, 4, 14],                        //  65 KEY_EN_X
    ["C", 4, 4, 15],                        //  66 KEY_EN_C
    ["V", 5, 4, 22],                        //  67 KEY_EN_V
    ["B", 6, 4, 23],                        //  68 KEY_EN_B
    ["N", 7, 4, 30],                        //  69 KEY_EN_N
    ["M", 8, 4, 31],                        //  70 KEY_EN_M
    [",", 9, 4, 38],                        //  71 KEY_EN_COMMA
    [".", 10, 4, 39],                       //  72 KEY_EN_PERIOD
    ["/", 11, 4, 46],                       //  73 KEY_EN_FORWARD_SLASH
    ["Right Shift", 13, 4, 47],             //  74 KEY_EN_RIGHT_SHIFT
    ["Up Arrow", 15, 4, 61],                //  75 KEY_EN_UP_ARROW
    ["Left Ctrl", 0, 5, 91],                //  76 KEY_EN_LEFT_CONTROL
    ["Left Win", 1, 5, 90],                 //  77 KEY_EN_LEFT_WINDOWS
    ["Left Alt", 2, 5, 92],                 //  78 KEY_EN_LEFT_ALT
    ["Space", 6, 5, 93],                    //  79 KEY_EN_SPACE
    ["Right Alt", 10, 5, 94],               //  80 KEY_EN_RIGHT_ALT
    ["Fn", 11, 5, 60],                      //  81 KEY_EN_RIGHT_FUNCTION
    ["Menu", 12, 5, 95],                    //  82 KEY_EN_MENU
    ["Right Ctrl", 13, 5, 54],              //  83 KEY_EN_RIGHT_CONTROL
    ["Left Arrow", 14, 5, 63],              //  84 KEY_EN_LEFT_ARROW
    ["Down Arrow", 15, 5, 62],              //  85 KEY_EN_DOWN_ARROW
    ["Right Arrow", 16, 5, 70],             //  86 KEY_EN_RIGHT_ARROW
  ]),
});

export const PRO_S_ISO = Object.freeze({
  width: 17,
  height: 6,
  leds: Object.freeze([
    ["Esc", 0, 0, 96],                      //   0 KEY_EN_ESCAPE
    ["F1", 2, 0, 97],                       //   1 KEY_EN_F1
    ["F2", 3, 0, 98],                       //   2 KEY_EN_F2
    ["F3", 4, 0, 99],                       //   3 KEY_EN_F3
    ["F4", 5, 0, 104],                      //   4 KEY_EN_F4
    ["F5", 6, 0, 105],                      //   5 KEY_EN_F5
    ["F6", 7, 0, 106],                      //   6 KEY_EN_F6
    ["F7", 8, 0, 112],                      //   7 KEY_EN_F7
    ["F8", 9, 0, 113],                      //   8 KEY_EN_F8
    ["F9", 10, 0, 114],                     //   9 KEY_EN_F9
    ["F10", 11, 0, 67],                     //  10 KEY_EN_F10
    ["F11", 12, 0, 68],                     //  11 KEY_EN_F11
    ["F12", 13, 0, 69],                     //  12 KEY_EN_F12
    ["Print Screen", 14, 0, 102],           //  13 KEY_EN_PRINT_SCREEN
    ["Scroll Lock", 15, 0, 103],            //  14 KEY_EN_SCROLL_LOCK
    ["Pause Break", 16, 0, 107],            //  15 KEY_EN_PAUSE_BREAK
    ["`", 0, 1, 0],                         //  16 KEY_EN_BACK_TICK
    ["1", 1, 1, 1],                         //  17 KEY_EN_1
    ["2", 2, 1, 8],                         //  18 KEY_EN_2
    ["3", 3, 1, 9],                         //  19 KEY_EN_3
    ["4", 4, 1, 16],                        //  20 KEY_EN_4
    ["5", 5, 1, 17],                        //  21 KEY_EN_5
    ["6", 6, 1, 24],                        //  22 KEY_EN_6
    ["7", 7, 1, 25],                        //  23 KEY_EN_7
    ["8", 8, 1, 32],                        //  24 KEY_EN_8
    ["9", 9, 1, 33],                        //  25 KEY_EN_9
    ["0", 10, 1, 40],                       //  26 KEY_EN_0
    ["-", 11, 1, 41],                       //  27 KEY_EN_MINUS
    ["=", 12, 1, 48],                       //  28 KEY_EN_EQUALS
    ["Backspace", 13, 1, 49],               //  29 KEY_EN_BACKSPACE
    ["Insert", 14, 1, 56],                  //  30 KEY_EN_INSERT
    ["Home", 15, 1, 57],                    //  31 KEY_EN_HOME
    ["Page Up", 16, 1, 64],                 //  32 KEY_EN_PAGE_UP
    ["Tab", 0, 2, 2],                       //  33 KEY_EN_TAB
    ["Q", 1, 2, 3],                         //  34 KEY_EN_Q
    ["W", 2, 2, 10],                        //  35 KEY_EN_W
    ["E", 3, 2, 11],                        //  36 KEY_EN_E
    ["R", 4, 2, 18],                        //  37 KEY_EN_R
    ["T", 5, 2, 19],                        //  38 KEY_EN_T
    ["Y", 6, 2, 26],                        //  39 KEY_EN_Y
    ["U", 7, 2, 27],                        //  40 KEY_EN_U
    ["I", 8, 2, 34],                        //  41 KEY_EN_I
    ["O", 9, 2, 35],                        //  42 KEY_EN_O
    ["P", 10, 2, 42],                       //  43 KEY_EN_P
    ["[", 11, 2, 43],                       //  44 KEY_EN_LEFT_BRACKET
    ["]", 12, 2, 50],                       //  45 KEY_EN_RIGHT_BRACKET
    ["Del", 14, 2, 58],                     //  46 KEY_EN_DELETE
    ["End", 15, 2, 59],                     //  47 KEY_EN_END
    ["Page Down", 16, 2, 66],               //  48 KEY_EN_PAGE_DOWN
    ["Caps Lock", 0, 3, 4],                 //  49 KEY_EN_CAPS_LOCK
    ["A", 1, 3, 5],                         //  50 KEY_EN_A
    ["S", 2, 3, 12],                        //  51 KEY_EN_S
    ["D", 3, 3, 13],                        //  52 KEY_EN_D
    ["F", 4, 3, 20],                        //  53 KEY_EN_F
    ["G", 5, 3, 21],                        //  54 KEY_EN_G
    ["H", 6, 3, 28],                        //  55 KEY_EN_H
    ["J", 7, 3, 29],                        //  56 KEY_EN_J
    ["K", 8, 3, 36],                        //  57 KEY_EN_K
    ["L", 9, 3, 37],                        //  58 KEY_EN_L
    [";", 10, 3, 44],                       //  59 KEY_EN_SEMICOLON
    ["'", 11, 3, 45],                       //  60 KEY_EN_QUOTE
    ["ISO_#", 12, 3, 89],                   //  61 KEY_EN_POUND
    ["Enter", 13, 3, 52],                   //  62 KEY_EN_ANSI_ENTER
    ["Left Shift", 0, 4, 6],                //  63 KEY_EN_LEFT_SHIFT
    ["ISO_<", 1, 4, 100],                   //  64 KEY_EN_ISO_BACK_SLASH
    ["Z", 2, 4, 7],                         //  65 KEY_EN_Z
    ["X", 3, 4, 14],                        //  66 KEY_EN_X
    ["C", 4, 4, 15],                        //  67 KEY_EN_C
    ["V", 5, 4, 22],                        //  68 KEY_EN_V
    ["B", 6, 4, 23],                        //  69 KEY_EN_B
    ["N", 7, 4, 30],                        //  70 KEY_EN_N
    ["M", 8, 4, 31],                        //  71 KEY_EN_M
    [",", 9, 4, 38],                        //  72 KEY_EN_COMMA
    [".", 10, 4, 39],                       //  73 KEY_EN_PERIOD
    ["/", 11, 4, 46],                       //  74 KEY_EN_FORWARD_SLASH
    ["Right Shift", 13, 4, 47],             //  75 KEY_EN_RIGHT_SHIFT
    ["Up Arrow", 15, 4, 61],                //  76 KEY_EN_UP_ARROW
    ["Left Ctrl", 0, 5, 91],                //  77 KEY_EN_LEFT_CONTROL
    ["Left Win", 1, 5, 90],                 //  78 KEY_EN_LEFT_WINDOWS
    ["Left Alt", 2, 5, 92],                 //  79 KEY_EN_LEFT_ALT
    ["Space", 6, 5, 93],                    //  80 KEY_EN_SPACE
    ["Right Alt", 10, 5, 94],               //  81 KEY_EN_RIGHT_ALT
    ["Fn", 11, 5, 60],                      //  82 KEY_EN_RIGHT_FUNCTION
    ["Menu", 12, 5, 95],                    //  83 KEY_EN_MENU
    ["Right Ctrl", 13, 5, 54],              //  84 KEY_EN_RIGHT_CONTROL
    ["Left Arrow", 14, 5, 63],              //  85 KEY_EN_LEFT_ARROW
    ["Down Arrow", 15, 5, 62],              //  86 KEY_EN_DOWN_ARROW
    ["Right Arrow", 16, 5, 70],             //  87 KEY_EN_RIGHT_ARROW
  ]),
});

export const PRO_S_JIS = Object.freeze({
  width: 17,
  height: 6,
  leds: Object.freeze([
    ["Esc", 0, 0, 96],                      //   0 KEY_EN_ESCAPE
    ["F1", 2, 0, 97],                       //   1 KEY_EN_F1
    ["F2", 3, 0, 98],                       //   2 KEY_EN_F2
    ["F3", 4, 0, 99],                       //   3 KEY_EN_F3
    ["F4", 5, 0, 104],                      //   4 KEY_EN_F4
    ["F5", 6, 0, 105],                      //   5 KEY_EN_F5
    ["F6", 7, 0, 106],                      //   6 KEY_EN_F6
    ["F7", 8, 0, 112],                      //   7 KEY_EN_F7
    ["F8", 9, 0, 113],                      //   8 KEY_EN_F8
    ["F9", 10, 0, 114],                     //   9 KEY_EN_F9
    ["F10", 11, 0, 67],                     //  10 KEY_EN_F10
    ["F11", 12, 0, 68],                     //  11 KEY_EN_F11
    ["F12", 13, 0, 69],                     //  12 KEY_EN_F12
    ["Print Screen", 14, 0, 102],           //  13 KEY_EN_PRINT_SCREEN
    ["Scroll Lock", 15, 0, 103],            //  14 KEY_EN_SCROLL_LOCK
    ["Pause Break", 16, 0, 107],            //  15 KEY_EN_PAUSE_BREAK
    ["`", 0, 1, 0],                         //  16 KEY_EN_BACK_TICK
    ["1", 1, 1, 1],                         //  17 KEY_EN_1
    ["2", 2, 1, 8],                         //  18 KEY_EN_2
    ["3", 3, 1, 9],                         //  19 KEY_EN_3
    ["4", 4, 1, 16],                        //  20 KEY_EN_4
    ["5", 5, 1, 17],                        //  21 KEY_EN_5
    ["6", 6, 1, 24],                        //  22 KEY_EN_6
    ["7", 7, 1, 25],                        //  23 KEY_EN_7
    ["8", 8, 1, 32],                        //  24 KEY_EN_8
    ["9", 9, 1, 33],                        //  25 KEY_EN_9
    ["0", 10, 1, 40],                       //  26 KEY_EN_0
    ["-", 11, 1, 41],                       //  27 KEY_EN_MINUS
    ["=", 12, 1, 48],                       //  28 KEY_EN_EQUALS
    ["Backspace", 13, 1, 49],               //  29 KEY_EN_BACKSPACE
    ["Insert", 14, 1, 56],                  //  30 KEY_EN_INSERT
    ["Home", 15, 1, 57],                    //  31 KEY_EN_HOME
    ["Page Up", 16, 1, 64],                 //  32 KEY_EN_PAGE_UP
    ["Tab", 0, 2, 2],                       //  33 KEY_EN_TAB
    ["Q", 1, 2, 3],                         //  34 KEY_EN_Q
    ["W", 2, 2, 10],                        //  35 KEY_EN_W
    ["E", 3, 2, 11],                        //  36 KEY_EN_E
    ["R", 4, 2, 18],                        //  37 KEY_EN_R
    ["T", 5, 2, 19],                        //  38 KEY_EN_T
    ["Y", 6, 2, 26],                        //  39 KEY_EN_Y
    ["U", 7, 2, 27],                        //  40 KEY_EN_U
    ["I", 8, 2, 34],                        //  41 KEY_EN_I
    ["O", 9, 2, 35],                        //  42 KEY_EN_O
    ["P", 10, 2, 42],                       //  43 KEY_EN_P
    ["[", 11, 2, 43],                       //  44 KEY_EN_LEFT_BRACKET
    ["]", 12, 2, 50],                       //  45 KEY_EN_RIGHT_BRACKET
    ["\\", 13, 2, 51],                      //  46 KEY_EN_ANSI_BACK_SLASH
    ["Del", 14, 2, 58],                     //  47 KEY_EN_DELETE
    ["End", 15, 2, 59],                     //  48 KEY_EN_END
    ["Page Down", 16, 2, 66],               //  49 KEY_EN_PAGE_DOWN
    ["Caps Lock", 0, 3, 4],                 //  50 KEY_EN_CAPS_LOCK
    ["A", 1, 3, 5],                         //  51 KEY_EN_A
    ["S", 2, 3, 12],                        //  52 KEY_EN_S
    ["D", 3, 3, 13],                        //  53 KEY_EN_D
    ["F", 4, 3, 20],                        //  54 KEY_EN_F
    ["G", 5, 3, 21],                        //  55 KEY_EN_G
    ["H", 6, 3, 28],                        //  56 KEY_EN_H
    ["J", 7, 3, 29],                        //  57 KEY_EN_J
    ["K", 8, 3, 36],                        //  58 KEY_EN_K
    ["L", 9, 3, 37],                        //  59 KEY_EN_L
    [";", 10, 3, 44],                       //  60 KEY_EN_SEMICOLON
    ["'", 11, 3, 45],                       //  61 KEY_EN_QUOTE
    ["ISO_#", 12, 3, 89],                   //  62 KEY_EN_POUND
    ["Enter", 13, 3, 52],                   //  63 KEY_EN_ANSI_ENTER
    ["Left Shift", 0, 4, 6],                //  64 KEY_EN_LEFT_SHIFT
    ["ISO_<", 1, 4, 100],                   //  65 KEY_EN_ISO_BACK_SLASH
    ["Z", 2, 4, 7],                         //  66 KEY_EN_Z
    ["X", 3, 4, 14],                        //  67 KEY_EN_X
    ["C", 4, 4, 15],                        //  68 KEY_EN_C
    ["V", 5, 4, 22],                        //  69 KEY_EN_V
    ["B", 6, 4, 23],                        //  70 KEY_EN_B
    ["N", 7, 4, 30],                        //  71 KEY_EN_N
    ["M", 8, 4, 31],                        //  72 KEY_EN_M
    [",", 9, 4, 38],                        //  73 KEY_EN_COMMA
    [".", 10, 4, 39],                       //  74 KEY_EN_PERIOD
    ["/", 11, 4, 46],                       //  75 KEY_EN_FORWARD_SLASH
    ["Right Shift", 13, 4, 47],             //  76 KEY_EN_RIGHT_SHIFT
    ["Up Arrow", 15, 4, 61],                //  77 KEY_EN_UP_ARROW
    ["Left Ctrl", 0, 5, 91],                //  78 KEY_EN_LEFT_CONTROL
    ["Left Win", 1, 5, 90],                 //  79 KEY_EN_LEFT_WINDOWS
    ["Left Alt", 2, 5, 92],                 //  80 KEY_EN_LEFT_ALT
    ["Space", 6, 5, 93],                    //  81 KEY_EN_SPACE
    ["Right Alt", 10, 5, 94],               //  82 KEY_EN_RIGHT_ALT
    ["Fn", 11, 5, 60],                      //  83 KEY_EN_RIGHT_FUNCTION
    ["Menu", 12, 5, 95],                    //  84 KEY_EN_MENU
    ["Right Ctrl", 13, 5, 54],              //  85 KEY_EN_RIGHT_CONTROL
    ["Left Arrow", 14, 5, 63],              //  86 KEY_EN_LEFT_ARROW
    ["Down Arrow", 15, 5, 62],              //  87 KEY_EN_DOWN_ARROW
    ["Right Arrow", 16, 5, 70],             //  88 KEY_EN_RIGHT_ARROW
  ]),
});

export const PRO_L_ANSI = Object.freeze({
  width: 21,
  height: 6,
  leds: Object.freeze([
    ["Esc", 0, 0, 11],                      //   0 KEY_EN_ESCAPE
    ["F1", 2, 0, 22],                       //   1 KEY_EN_F1
    ["F2", 3, 0, 30],                       //   2 KEY_EN_F2
    ["F3", 4, 0, 25],                       //   3 KEY_EN_F3
    ["F4", 5, 0, 27],                       //   4 KEY_EN_F4
    ["F5", 6, 0, 7],                        //   5 KEY_EN_F5
    ["F6", 7, 0, 51],                       //   6 KEY_EN_F6
    ["F7", 8, 0, 57],                       //   7 KEY_EN_F7
    ["F8", 9, 0, 62],                       //   8 KEY_EN_F8
    ["F9", 10, 0, 86],                      //   9 KEY_EN_F9
    ["F10", 11, 0, 87],                     //  10 KEY_EN_F10
    ["F11", 12, 0, 83],                     //  11 KEY_EN_F11
    ["F12", 13, 0, 85],                     //  12 KEY_EN_F12
    ["Print Screen", 14, 0, 79],            //  13 KEY_EN_PRINT_SCREEN
    ["Scroll Lock", 15, 0, 72],             //  14 KEY_EN_SCROLL_LOCK
    ["Pause Break", 16, 0, 0],              //  15 KEY_EN_PAUSE_BREAK
    ["P1", 17, 0, 101],                     //  16 Key: P1
    ["P2", 18, 0, 109],                     //  17 Key: P2
    ["P3", 19, 0, 117],                     //  18 Key: P3
    ["P4", 20, 0, 119],                     //  19 Key: P4
    ["`", 0, 1, 14],                        //  20 KEY_EN_BACK_TICK
    ["1", 1, 1, 15],                        //  21 KEY_EN_1
    ["2", 2, 1, 23],                        //  22 KEY_EN_2
    ["3", 3, 1, 31],                        //  23 KEY_EN_3
    ["4", 4, 1, 39],                        //  24 KEY_EN_4
    ["5", 5, 1, 38],                        //  25 KEY_EN_5
    ["6", 6, 1, 46],                        //  26 KEY_EN_6
    ["7", 7, 1, 47],                        //  27 KEY_EN_7
    ["8", 8, 1, 55],                        //  28 KEY_EN_8
    ["9", 9, 1, 63],                        //  29 KEY_EN_9
    ["0", 10, 1, 71],                       //  30 KEY_EN_0
    ["-", 11, 1, 70],                       //  31 KEY_EN_MINUS
    ["=", 12, 1, 54],                       //  32 KEY_EN_EQUALS
    ["Backspace", 13, 1, 81],               //  33 KEY_EN_BACKSPACE
    ["Insert", 14, 1, 3],                   //  34 KEY_EN_INSERT
    ["Home", 15, 1, 1],                     //  35 KEY_EN_HOME
    ["Page Up", 16, 1, 2],                  //  36 KEY_EN_PAGE_UP
    ["NumLock", 17, 1, 100],                //  37 KEY_EN_NUMPAD_LOCK
    ["Num /", 18, 1, 108],                  //  38 KEY_EN_NUMPAD_DIVIDE
    ["Num *", 19, 1, 116],                  //  39 KEY_EN_NUMPAD_TIMES
    ["Num -", 20, 1, 118],                  //  40 KEY_EN_NUMPAD_MINUS
    ["Tab", 0, 2, 9],                       //  41 KEY_EN_TAB
    ["Q", 1, 2, 8],                         //  42 KEY_EN_Q
    ["W", 2, 2, 16],                        //  43 KEY_EN_W
    ["E", 3, 2, 24],                        //  44 KEY_EN_E
    ["R", 4, 2, 32],                        //  45 KEY_EN_R
    ["T", 5, 2, 33],                        //  46 KEY_EN_T
    ["Y", 6, 2, 41],                        //  47 KEY_EN_Y
    ["U", 7, 2, 40],                        //  48 KEY_EN_U
    ["I", 8, 2, 48],                        //  49 KEY_EN_I
    ["O", 9, 2, 56],                        //  50 KEY_EN_O
    ["P", 10, 2, 64],                       //  51 KEY_EN_P
    ["[", 11, 2, 65],                       //  52 KEY_EN_LEFT_BRACKET
    ["]", 12, 2, 49],                       //  53 KEY_EN_RIGHT_BRACKET
    ["\\", 13, 2, 82],                      //  54 KEY_EN_ANSI_BACK_SLASH
    ["Del", 14, 2, 94],                     //  55 KEY_EN_DELETE
    ["End", 15, 2, 92],                     //  56 KEY_EN_END
    ["Page Down", 16, 2, 88],               //  57 KEY_EN_PAGE_DOWN
    ["Num 7", 17, 2, 96],                   //  58 KEY_EN_NUMPAD_7
    ["Num 8", 18, 2, 104],                  //  59 KEY_EN_NUMPAD_8
    ["Num 9", 19, 2, 112],                  //  60 KEY_EN_NUMPAD_9
    ["Num +", 20, 2, 110],                  //  61 KEY_EN_NUMPAD_PLUS
    ["Caps Lock", 0, 3, 17],                //  62 KEY_EN_CAPS_LOCK
    ["A", 1, 3, 10],                        //  63 KEY_EN_A
    ["S", 2, 3, 18],                        //  64 KEY_EN_S
    ["D", 3, 3, 26],                        //  65 KEY_EN_D
    ["F", 4, 3, 34],                        //  66 KEY_EN_F
    ["G", 5, 3, 35],                        //  67 KEY_EN_G
    ["H", 6, 3, 43],                        //  68 KEY_EN_H
    ["J", 7, 3, 42],                        //  69 KEY_EN_J
    ["K", 8, 3, 50],                        //  70 KEY_EN_K
    ["L", 9, 3, 58],                        //  71 KEY_EN_L
    [";", 10, 3, 66],                       //  72 KEY_EN_SEMICOLON
    ["'", 11, 3, 67],                       //  73 KEY_EN_QUOTE
    ["Enter", 13, 3, 84],                   //  74 KEY_EN_ANSI_ENTER
    ["Num 4", 17, 3, 97],                   //  75 KEY_EN_NUMPAD_4
    ["Num 5", 18, 3, 105],                  //  76 KEY_EN_NUMPAD_5
    ["Num 6", 19, 3, 113],                  //  77 KEY_EN_NUMPAD_6
    ["Left Shift", 0, 4, 73],               //  78 KEY_EN_LEFT_SHIFT
    ["Z", 2, 4, 12],                        //  79 KEY_EN_Z
    ["X", 3, 4, 20],                        //  80 KEY_EN_X
    ["C", 4, 4, 28],                        //  81 KEY_EN_C
    ["V", 5, 4, 36],                        //  82 KEY_EN_V
    ["B", 6, 4, 37],                        //  83 KEY_EN_B
    ["N", 7, 4, 45],                        //  84 KEY_EN_N
    ["M", 8, 4, 44],                        //  85 KEY_EN_M
    [",", 9, 4, 52],                        //  86 KEY_EN_COMMA
    [".", 10, 4, 60],                       //  87 KEY_EN_PERIOD
    ["/", 11, 4, 69],                       //  88 KEY_EN_FORWARD_SLASH
    ["Right Shift", 13, 4, 74],             //  89 KEY_EN_RIGHT_SHIFT
    ["Up Arrow", 15, 4, 80],                //  90 KEY_EN_UP_ARROW
    ["Num 1", 17, 4, 98],                   //  91 KEY_EN_NUMPAD_1
    ["Num 2", 18, 4, 106],                  //  92 KEY_EN_NUMPAD_2
    ["Num 3", 19, 4, 114],                  //  93 KEY_EN_NUMPAD_3
    ["Num Enter", 20, 4, 111],              //  94 KEY_EN_NUMPAD_ENTER
    ["Left Ctrl", 0, 5, 6],                 //  95 KEY_EN_LEFT_CONTROL
    ["Left Win", 1, 5, 90],                 //  96 KEY_EN_LEFT_WINDOWS
    ["Left Alt", 2, 5, 75],                 //  97 KEY_EN_LEFT_ALT
    ["Space", 6, 5, 91],                    //  98 KEY_EN_SPACE
    ["Right Alt", 10, 5, 77],               //  99 KEY_EN_RIGHT_ALT
    ["Fn", 11, 5, 78],                      // 100 KEY_EN_RIGHT_FUNCTION
    ["Menu", 12, 5, 61],                    // 101 KEY_EN_MENU
    ["Right Ctrl", 13, 5, 4],               // 102 KEY_EN_RIGHT_CONTROL
    ["Left Arrow", 14, 5, 95],              // 103 KEY_EN_LEFT_ARROW
    ["Down Arrow", 15, 5, 93],              // 104 KEY_EN_DOWN_ARROW
    ["Right Arrow", 16, 5, 5],              // 105 KEY_EN_RIGHT_ARROW
    ["Num 0", 18, 5, 107],                  // 106 KEY_EN_NUMPAD_0
    ["Num .", 19, 5, 115],                  // 107 KEY_EN_NUMPAD_PERIOD
  ]),
});

export const PRO_L_ISO = Object.freeze({
  width: 21,
  height: 6,
  leds: Object.freeze([
    ["Esc", 0, 0, 11],                      //   0 KEY_EN_ESCAPE
    ["F1", 2, 0, 22],                       //   1 KEY_EN_F1
    ["F2", 3, 0, 30],                       //   2 KEY_EN_F2
    ["F3", 4, 0, 25],                       //   3 KEY_EN_F3
    ["F4", 5, 0, 27],                       //   4 KEY_EN_F4
    ["F5", 6, 0, 7],                        //   5 KEY_EN_F5
    ["F6", 7, 0, 51],                       //   6 KEY_EN_F6
    ["F7", 8, 0, 57],                       //   7 KEY_EN_F7
    ["F8", 9, 0, 62],                       //   8 KEY_EN_F8
    ["F9", 10, 0, 86],                      //   9 KEY_EN_F9
    ["F10", 11, 0, 87],                     //  10 KEY_EN_F10
    ["F11", 12, 0, 83],                     //  11 KEY_EN_F11
    ["F12", 13, 0, 85],                     //  12 KEY_EN_F12
    ["Print Screen", 14, 0, 79],            //  13 KEY_EN_PRINT_SCREEN
    ["Scroll Lock", 15, 0, 72],             //  14 KEY_EN_SCROLL_LOCK
    ["Pause Break", 16, 0, 0],              //  15 KEY_EN_PAUSE_BREAK
    ["P1", 17, 0, 101],                     //  16 Key: P1
    ["P2", 18, 0, 109],                     //  17 Key: P2
    ["P3", 19, 0, 117],                     //  18 Key: P3
    ["P4", 20, 0, 119],                     //  19 Key: P4
    ["`", 0, 1, 14],                        //  20 KEY_EN_BACK_TICK
    ["1", 1, 1, 15],                        //  21 KEY_EN_1
    ["2", 2, 1, 23],                        //  22 KEY_EN_2
    ["3", 3, 1, 31],                        //  23 KEY_EN_3
    ["4", 4, 1, 39],                        //  24 KEY_EN_4
    ["5", 5, 1, 38],                        //  25 KEY_EN_5
    ["6", 6, 1, 46],                        //  26 KEY_EN_6
    ["7", 7, 1, 47],                        //  27 KEY_EN_7
    ["8", 8, 1, 55],                        //  28 KEY_EN_8
    ["9", 9, 1, 63],                        //  29 KEY_EN_9
    ["0", 10, 1, 71],                       //  30 KEY_EN_0
    ["-", 11, 1, 70],                       //  31 KEY_EN_MINUS
    ["=", 12, 1, 54],                       //  32 KEY_EN_EQUALS
    ["Backspace", 13, 1, 81],               //  33 KEY_EN_BACKSPACE
    ["Insert", 14, 1, 3],                   //  34 KEY_EN_INSERT
    ["Home", 15, 1, 1],                     //  35 KEY_EN_HOME
    ["Page Up", 16, 1, 2],                  //  36 KEY_EN_PAGE_UP
    ["NumLock", 17, 1, 100],                //  37 KEY_EN_NUMPAD_LOCK
    ["Num /", 18, 1, 108],                  //  38 KEY_EN_NUMPAD_DIVIDE
    ["Num *", 19, 1, 116],                  //  39 KEY_EN_NUMPAD_TIMES
    ["Num -", 20, 1, 118],                  //  40 KEY_EN_NUMPAD_MINUS
    ["Tab", 0, 2, 9],                       //  41 KEY_EN_TAB
    ["Q", 1, 2, 8],                         //  42 KEY_EN_Q
    ["W", 2, 2, 16],                        //  43 KEY_EN_W
    ["E", 3, 2, 24],                        //  44 KEY_EN_E
    ["R", 4, 2, 32],                        //  45 KEY_EN_R
    ["T", 5, 2, 33],                        //  46 KEY_EN_T
    ["Y", 6, 2, 41],                        //  47 KEY_EN_Y
    ["U", 7, 2, 40],                        //  48 KEY_EN_U
    ["I", 8, 2, 48],                        //  49 KEY_EN_I
    ["O", 9, 2, 56],                        //  50 KEY_EN_O
    ["P", 10, 2, 64],                       //  51 KEY_EN_P
    ["[", 11, 2, 65],                       //  52 KEY_EN_LEFT_BRACKET
    ["]", 12, 2, 49],                       //  53 KEY_EN_RIGHT_BRACKET
    ["Del", 14, 2, 94],                     //  54 KEY_EN_DELETE
    ["End", 15, 2, 92],                     //  55 KEY_EN_END
    ["Page Down", 16, 2, 88],               //  56 KEY_EN_PAGE_DOWN
    ["Num 7", 17, 2, 96],                   //  57 KEY_EN_NUMPAD_7
    ["Num 8", 18, 2, 104],                  //  58 KEY_EN_NUMPAD_8
    ["Num 9", 19, 2, 112],                  //  59 KEY_EN_NUMPAD_9
    ["Num +", 20, 2, 110],                  //  60 KEY_EN_NUMPAD_PLUS
    ["Caps Lock", 0, 3, 17],                //  61 KEY_EN_CAPS_LOCK
    ["A", 1, 3, 10],                        //  62 KEY_EN_A
    ["S", 2, 3, 18],                        //  63 KEY_EN_S
    ["D", 3, 3, 26],                        //  64 KEY_EN_D
    ["F", 4, 3, 34],                        //  65 KEY_EN_F
    ["G", 5, 3, 35],                        //  66 KEY_EN_G
    ["H", 6, 3, 43],                        //  67 KEY_EN_H
    ["J", 7, 3, 42],                        //  68 KEY_EN_J
    ["K", 8, 3, 50],                        //  69 KEY_EN_K
    ["L", 9, 3, 58],                        //  70 KEY_EN_L
    [";", 10, 3, 66],                       //  71 KEY_EN_SEMICOLON
    ["'", 11, 3, 67],                       //  72 KEY_EN_QUOTE
    ["ISO_#", 12, 3, 68],                   //  73 KEY_EN_POUND
    ["Enter", 13, 3, 84],                   //  74 KEY_EN_ANSI_ENTER
    ["Num 4", 17, 3, 97],                   //  75 KEY_EN_NUMPAD_4
    ["Num 5", 18, 3, 105],                  //  76 KEY_EN_NUMPAD_5
    ["Num 6", 19, 3, 113],                  //  77 KEY_EN_NUMPAD_6
    ["Left Shift", 0, 4, 73],               //  78 KEY_EN_LEFT_SHIFT
    ["ISO_<", 1, 4, 19],                    //  79 KEY_EN_ISO_BACK_SLASH
    ["Z", 2, 4, 12],                        //  80 KEY_EN_Z
    ["X", 3, 4, 20],                        //  81 KEY_EN_X
    ["C", 4, 4, 28],                        //  82 KEY_EN_C
    ["V", 5, 4, 36],                        //  83 KEY_EN_V
    ["B", 6, 4, 37],                        //  84 KEY_EN_B
    ["N", 7, 4, 45],                        //  85 KEY_EN_N
    ["M", 8, 4, 44],                        //  86 KEY_EN_M
    [",", 9, 4, 52],                        //  87 KEY_EN_COMMA
    [".", 10, 4, 60],                       //  88 KEY_EN_PERIOD
    ["/", 11, 4, 69],                       //  89 KEY_EN_FORWARD_SLASH
    ["Right Shift", 13, 4, 74],             //  90 KEY_EN_RIGHT_SHIFT
    ["Up Arrow", 15, 4, 80],                //  91 KEY_EN_UP_ARROW
    ["Num 1", 17, 4, 98],                   //  92 KEY_EN_NUMPAD_1
    ["Num 2", 18, 4, 106],                  //  93 KEY_EN_NUMPAD_2
    ["Num 3", 19, 4, 114],                  //  94 KEY_EN_NUMPAD_3
    ["Num Enter", 20, 4, 111],              //  95 KEY_EN_NUMPAD_ENTER
    ["Left Ctrl", 0, 5, 6],                 //  96 KEY_EN_LEFT_CONTROL
    ["Left Win", 1, 5, 90],                 //  97 KEY_EN_LEFT_WINDOWS
    ["Left Alt", 2, 5, 75],                 //  98 KEY_EN_LEFT_ALT
    ["Space", 6, 5, 91],                    //  99 KEY_EN_SPACE
    ["Right Alt", 10, 5, 77],               // 100 KEY_EN_RIGHT_ALT
    ["Fn", 11, 5, 78],                      // 101 KEY_EN_RIGHT_FUNCTION
    ["Menu", 12, 5, 61],                    // 102 KEY_EN_MENU
    ["Right Ctrl", 13, 5, 4],               // 103 KEY_EN_RIGHT_CONTROL
    ["Left Arrow", 14, 5, 95],              // 104 KEY_EN_LEFT_ARROW
    ["Down Arrow", 15, 5, 93],              // 105 KEY_EN_DOWN_ARROW
    ["Right Arrow", 16, 5, 5],              // 106 KEY_EN_RIGHT_ARROW
    ["Num 0", 18, 5, 107],                  // 107 KEY_EN_NUMPAD_0
    ["Num .", 19, 5, 115],                  // 108 KEY_EN_NUMPAD_PERIOD
  ]),
});

export const PRO_L_JIS = Object.freeze({
  width: 21,
  height: 6,
  leds: Object.freeze([
    ["Esc", 0, 0, 11],                      //   0 KEY_EN_ESCAPE
    ["F1", 2, 0, 22],                       //   1 KEY_EN_F1
    ["F2", 3, 0, 30],                       //   2 KEY_EN_F2
    ["F3", 4, 0, 25],                       //   3 KEY_EN_F3
    ["F4", 5, 0, 27],                       //   4 KEY_EN_F4
    ["F5", 6, 0, 7],                        //   5 KEY_EN_F5
    ["F6", 7, 0, 51],                       //   6 KEY_EN_F6
    ["F7", 8, 0, 57],                       //   7 KEY_EN_F7
    ["F8", 9, 0, 62],                       //   8 KEY_EN_F8
    ["F9", 10, 0, 86],                      //   9 KEY_EN_F9
    ["F10", 11, 0, 87],                     //  10 KEY_EN_F10
    ["F11", 12, 0, 83],                     //  11 KEY_EN_F11
    ["F12", 13, 0, 85],                     //  12 KEY_EN_F12
    ["Print Screen", 14, 0, 79],            //  13 KEY_EN_PRINT_SCREEN
    ["Scroll Lock", 15, 0, 72],             //  14 KEY_EN_SCROLL_LOCK
    ["Pause Break", 16, 0, 0],              //  15 KEY_EN_PAUSE_BREAK
    ["P1", 17, 0, 101],                     //  16 Key: P1
    ["P2", 18, 0, 109],                     //  17 Key: P2
    ["P3", 19, 0, 117],                     //  18 Key: P3
    ["P4", 20, 0, 119],                     //  19 Key: P4
    ["`", 0, 1, 14],                        //  20 KEY_EN_BACK_TICK
    ["1", 1, 1, 15],                        //  21 KEY_EN_1
    ["2", 2, 1, 23],                        //  22 KEY_EN_2
    ["3", 3, 1, 31],                        //  23 KEY_EN_3
    ["4", 4, 1, 39],                        //  24 KEY_EN_4
    ["5", 5, 1, 38],                        //  25 KEY_EN_5
    ["6", 6, 1, 46],                        //  26 KEY_EN_6
    ["7", 7, 1, 47],                        //  27 KEY_EN_7
    ["8", 8, 1, 55],                        //  28 KEY_EN_8
    ["9", 9, 1, 63],                        //  29 KEY_EN_9
    ["0", 10, 1, 71],                       //  30 KEY_EN_0
    ["-", 11, 1, 70],                       //  31 KEY_EN_MINUS
    ["=", 12, 1, 54],                       //  32 KEY_EN_EQUALS
    ["Backspace", 13, 1, 81],               //  33 KEY_EN_BACKSPACE
    ["Insert", 14, 1, 3],                   //  34 KEY_EN_INSERT
    ["Home", 15, 1, 1],                     //  35 KEY_EN_HOME
    ["Page Up", 16, 1, 2],                  //  36 KEY_EN_PAGE_UP
    ["NumLock", 17, 1, 100],                //  37 KEY_EN_NUMPAD_LOCK
    ["Num /", 18, 1, 108],                  //  38 KEY_EN_NUMPAD_DIVIDE
    ["Num *", 19, 1, 116],                  //  39 KEY_EN_NUMPAD_TIMES
    ["Num -", 20, 1, 118],                  //  40 KEY_EN_NUMPAD_MINUS
    ["Tab", 0, 2, 9],                       //  41 KEY_EN_TAB
    ["Q", 1, 2, 8],                         //  42 KEY_EN_Q
    ["W", 2, 2, 16],                        //  43 KEY_EN_W
    ["E", 3, 2, 24],                        //  44 KEY_EN_E
    ["R", 4, 2, 32],                        //  45 KEY_EN_R
    ["T", 5, 2, 33],                        //  46 KEY_EN_T
    ["Y", 6, 2, 41],                        //  47 KEY_EN_Y
    ["U", 7, 2, 40],                        //  48 KEY_EN_U
    ["I", 8, 2, 48],                        //  49 KEY_EN_I
    ["O", 9, 2, 56],                        //  50 KEY_EN_O
    ["P", 10, 2, 64],                       //  51 KEY_EN_P
    ["[", 11, 2, 65],                       //  52 KEY_EN_LEFT_BRACKET
    ["]", 12, 2, 49],                       //  53 KEY_EN_RIGHT_BRACKET
    ["\\", 13, 2, 82],                      //  54 KEY_EN_ANSI_BACK_SLASH
    ["Del", 14, 2, 94],                     //  55 KEY_EN_DELETE
    ["End", 15, 2, 92],                     //  56 KEY_EN_END
    ["Page Down", 16, 2, 88],               //  57 KEY_EN_PAGE_DOWN
    ["Num 7", 17, 2, 96],                   //  58 KEY_EN_NUMPAD_7
    ["Num 8", 18, 2, 104],                  //  59 KEY_EN_NUMPAD_8
    ["Num 9", 19, 2, 112],                  //  60 KEY_EN_NUMPAD_9
    ["Num +", 20, 2, 110],                  //  61 KEY_EN_NUMPAD_PLUS
    ["Caps Lock", 0, 3, 17],                //  62 KEY_EN_CAPS_LOCK
    ["A", 1, 3, 10],                        //  63 KEY_EN_A
    ["S", 2, 3, 18],                        //  64 KEY_EN_S
    ["D", 3, 3, 26],                        //  65 KEY_EN_D
    ["F", 4, 3, 34],                        //  66 KEY_EN_F
    ["G", 5, 3, 35],                        //  67 KEY_EN_G
    ["H", 6, 3, 43],                        //  68 KEY_EN_H
    ["J", 7, 3, 42],                        //  69 KEY_EN_J
    ["K", 8, 3, 50],                        //  70 KEY_EN_K
    ["L", 9, 3, 58],                        //  71 KEY_EN_L
    [";", 10, 3, 66],                       //  72 KEY_EN_SEMICOLON
    ["'", 11, 3, 67],                       //  73 KEY_EN_QUOTE
    ["ISO_#", 12, 3, 68],                   //  74 KEY_EN_POUND
    ["Enter", 13, 3, 84],                   //  75 KEY_EN_ANSI_ENTER
    ["Num 4", 17, 3, 97],                   //  76 KEY_EN_NUMPAD_4
    ["Num 5", 18, 3, 105],                  //  77 KEY_EN_NUMPAD_5
    ["Num 6", 19, 3, 113],                  //  78 KEY_EN_NUMPAD_6
    ["Left Shift", 0, 4, 73],               //  79 KEY_EN_LEFT_SHIFT
    ["ISO_<", 1, 4, 19],                    //  80 KEY_EN_ISO_BACK_SLASH
    ["Z", 2, 4, 12],                        //  81 KEY_EN_Z
    ["X", 3, 4, 20],                        //  82 KEY_EN_X
    ["C", 4, 4, 28],                        //  83 KEY_EN_C
    ["V", 5, 4, 36],                        //  84 KEY_EN_V
    ["B", 6, 4, 37],                        //  85 KEY_EN_B
    ["N", 7, 4, 45],                        //  86 KEY_EN_N
    ["M", 8, 4, 44],                        //  87 KEY_EN_M
    [",", 9, 4, 52],                        //  88 KEY_EN_COMMA
    [".", 10, 4, 60],                       //  89 KEY_EN_PERIOD
    ["/", 11, 4, 69],                       //  90 KEY_EN_FORWARD_SLASH
    ["Right Shift", 13, 4, 74],             //  91 KEY_EN_RIGHT_SHIFT
    ["Up Arrow", 15, 4, 80],                //  92 KEY_EN_UP_ARROW
    ["Num 1", 17, 4, 98],                   //  93 KEY_EN_NUMPAD_1
    ["Num 2", 18, 4, 106],                  //  94 KEY_EN_NUMPAD_2
    ["Num 3", 19, 4, 114],                  //  95 KEY_EN_NUMPAD_3
    ["Num Enter", 20, 4, 111],              //  96 KEY_EN_NUMPAD_ENTER
    ["Left Ctrl", 0, 5, 6],                 //  97 KEY_EN_LEFT_CONTROL
    ["Left Win", 1, 5, 90],                 //  98 KEY_EN_LEFT_WINDOWS
    ["Left Alt", 2, 5, 75],                 //  99 KEY_EN_LEFT_ALT
    ["Space", 6, 5, 91],                    // 100 KEY_EN_SPACE
    ["Right Alt", 10, 5, 77],               // 101 KEY_EN_RIGHT_ALT
    ["Fn", 11, 5, 78],                      // 102 KEY_EN_RIGHT_FUNCTION
    ["Menu", 12, 5, 61],                    // 103 KEY_EN_MENU
    ["Right Ctrl", 13, 5, 4],               // 104 KEY_EN_RIGHT_CONTROL
    ["Left Arrow", 14, 5, 95],              // 105 KEY_EN_LEFT_ARROW
    ["Down Arrow", 15, 5, 93],              // 106 KEY_EN_DOWN_ARROW
    ["Right Arrow", 16, 5, 5],              // 107 KEY_EN_RIGHT_ARROW
    ["Num 0", 18, 5, 107],                  // 108 KEY_EN_NUMPAD_0
    ["Num .", 19, 5, 115],                  // 109 KEY_EN_NUMPAD_PERIOD
  ]),
});

export const LAYOUTS = Object.freeze({
  PRO_S: Object.freeze({ ANSI: PRO_S_ANSI, ISO: PRO_S_ISO, JIS: PRO_S_JIS }),
  PRO_L: Object.freeze({ ANSI: PRO_L_ANSI, ISO: PRO_L_ISO, JIS: PRO_L_JIS }),
});
