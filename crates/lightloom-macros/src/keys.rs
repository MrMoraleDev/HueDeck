// SPDX-License-Identifier: GPL-3.0-or-later
//! Key and mouse-button names of macro scripts (docs/host-api/macros-system.md,
//! critic pass "Full key-name vocabulary"): what `keys.press("Left Ctrl + Left
//! Win + Right Arrow")` and `mouse.click("Right Click")` take, and what
//! `onKey` / `onMouse` events carry (`data.key`, `data.keyString`,
//! `data.button`).
//!
//! Names map to Windows virtual-key codes; letters are single upper-case
//! characters, digits and punctuation the unshifted character. Matching is
//! case-insensitive, `Backspace` = `Back Space`, `Apps` = `Menu` =
//! `ContextMenu`. `keyString` is `Ctrl + `, `Win + `, `Shift + `, `Alt + ` (in
//! that order, as held) before the key's name.

use lightloom_hal::input::{MouseButton, SynthInput};

/// One named key: name, virtual-key code, extended-key flag.
type KeyRow = (&'static str, u16, bool);

/// The key table (first name of a code is the one events carry).
const KEYS: &[KeyRow] = &[
    ("Esc", 0x1B, false),
    ("Escape", 0x1B, false),
    ("F1", 0x70, false),
    ("F2", 0x71, false),
    ("F3", 0x72, false),
    ("F4", 0x73, false),
    ("F5", 0x74, false),
    ("F6", 0x75, false),
    ("F7", 0x76, false),
    ("F8", 0x77, false),
    ("F9", 0x78, false),
    ("F10", 0x79, false),
    ("F11", 0x7A, false),
    ("F12", 0x7B, false),
    ("F13", 0x7C, false),
    ("F14", 0x7D, false),
    ("F15", 0x7E, false),
    ("F16", 0x7F, false),
    ("F17", 0x80, false),
    ("F18", 0x81, false),
    ("F19", 0x82, false),
    ("F20", 0x83, false),
    ("F21", 0x84, false),
    ("F22", 0x85, false),
    ("F23", 0x86, false),
    ("F24", 0x87, false),
    ("Print Screen", 0x2C, true),
    ("Scroll Lock", 0x91, false),
    ("Pause Break", 0x13, false),
    ("Pause", 0x13, false),
    ("`", 0xC0, false),
    ("1", 0x31, false),
    ("2", 0x32, false),
    ("3", 0x33, false),
    ("4", 0x34, false),
    ("5", 0x35, false),
    ("6", 0x36, false),
    ("7", 0x37, false),
    ("8", 0x38, false),
    ("9", 0x39, false),
    ("0", 0x30, false),
    ("-", 0xBD, false),
    ("=", 0xBB, false),
    ("Back Space", 0x08, false),
    ("Backspace", 0x08, false),
    ("Tab", 0x09, false),
    ("[", 0xDB, false),
    ("]", 0xDD, false),
    ("\\", 0xDC, false),
    ("Caps Lock", 0x14, false),
    (";", 0xBA, false),
    ("'", 0xDE, false),
    ("Enter", 0x0D, false),
    ("Left Shift", 0xA0, false),
    (",", 0xBC, false),
    (".", 0xBE, false),
    ("/", 0xBF, false),
    ("Right Shift", 0xA1, false),
    ("Left Ctrl", 0xA2, false),
    ("Left Win", 0x5B, true),
    ("Left Alt", 0xA4, false),
    ("Space", 0x20, false),
    ("Right Alt", 0xA5, true),
    ("Right Win", 0x5C, true),
    ("Menu", 0x5D, true),
    ("Apps", 0x5D, true),
    ("ContextMenu", 0x5D, true),
    ("Right Ctrl", 0xA3, true),
    ("Ctrl", 0x11, false),
    ("Shift", 0x10, false),
    ("Alt", 0x12, false),
    ("Win", 0x5B, true),
    ("NumLock", 0x90, true),
    ("Num /", 0x6F, true),
    ("Num *", 0x6A, false),
    ("Num -", 0x6D, false),
    ("Num 7", 0x67, false),
    ("Num 8", 0x68, false),
    ("Num 9", 0x69, false),
    ("Num 4", 0x64, false),
    ("Num 5", 0x65, false),
    ("Num 6", 0x66, false),
    ("Num +", 0x6B, false),
    ("Num 1", 0x61, false),
    ("Num 2", 0x62, false),
    ("Num 3", 0x63, false),
    ("Num 0", 0x60, false),
    ("Num .", 0x6E, false),
    ("Insert", 0x2D, true),
    ("Delete", 0x2E, true),
    ("Del", 0x2E, true),
    ("Home", 0x24, true),
    ("End", 0x23, true),
    ("Page Up", 0x21, true),
    ("Page Down", 0x22, true),
    ("Up Arrow", 0x26, true),
    ("Left Arrow", 0x25, true),
    ("Down Arrow", 0x28, true),
    ("Right Arrow", 0x27, true),
    ("AudioMute", 0xAD, true),
    ("AudioVolume Down", 0xAE, true),
    ("AudioVolume Up", 0xAF, true),
    ("MediaNextTrack", 0xB0, true),
    ("MediaPreviousTrack", 0xB1, true),
    ("MediaStop", 0xB2, true),
    ("MediaPlayPause", 0xB3, true),
];

/// `Num Enter`: `Enter` with the extended flag.
pub const NUM_ENTER: &str = "Num Enter";

/// Virtual-key codes of the modifiers.
pub mod vk {
    /// `VK_SHIFT`, `VK_CONTROL`, `VK_MENU` (generic).
    pub const SHIFT: u16 = 0x10;
    /// `VK_CONTROL`.
    pub const CONTROL: u16 = 0x11;
    /// `VK_MENU` (Alt).
    pub const ALT: u16 = 0x12;
    /// `VK_LSHIFT` / `VK_RSHIFT`.
    pub const LSHIFT: u16 = 0xA0;
    /// Right Shift.
    pub const RSHIFT: u16 = 0xA1;
    /// `VK_LCONTROL` / `VK_RCONTROL`.
    pub const LCONTROL: u16 = 0xA2;
    /// Right Ctrl.
    pub const RCONTROL: u16 = 0xA3;
    /// `VK_LMENU` / `VK_RMENU`.
    pub const LALT: u16 = 0xA4;
    /// Right Alt.
    pub const RALT: u16 = 0xA5;
    /// `VK_LWIN` / `VK_RWIN`.
    pub const LWIN: u16 = 0x5B;
    /// Right Win.
    pub const RWIN: u16 = 0x5C;
    /// `V`.
    pub const V: u16 = 0x56;
    /// `VK_F12`.
    pub const F12: u16 = 0x7B;
    /// `VK_RETURN`.
    pub const RETURN: u16 = 0x0D;
}

/// One key of a combo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    /// Virtual-key code.
    pub vk: u16,
    /// Extended key.
    pub extended: bool,
}

/// The key named `name` (case-insensitive); a single letter is that letter.
pub fn key(name: &str) -> Option<Key> {
    let name = name.trim();
    if name.eq_ignore_ascii_case(NUM_ENTER) {
        return Some(Key {
            vk: vk::RETURN,
            extended: true,
        });
    }
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_alphabetic() {
            return Some(Key {
                vk: c.to_ascii_uppercase() as u16,
                extended: false,
            });
        }
    }
    KEYS.iter()
        .find(|(n, _, _)| n.eq_ignore_ascii_case(name))
        .map(|&(_, vk, extended)| Key { vk, extended })
}

/// Whether `vk` must be sent as an extended key.
pub fn extended(vk: u16) -> bool {
    KEYS.iter().any(|&(_, v, e)| v == vk && e)
}

/// The name an event carries for `vk` (`Num Enter` for an extended Enter);
/// `None` for codes outside the table.
pub fn name_of(vk: u16, extended: bool) -> Option<&'static str> {
    if vk == vk::RETURN && extended {
        return Some(NUM_ENTER);
    }
    if (0x41..=0x5A).contains(&vk) {
        const LETTERS: [&str; 26] = [
            "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
            "T", "U", "V", "W", "X", "Y", "Z",
        ];
        return Some(LETTERS[usize::from(vk - 0x41)]);
    }
    KEYS.iter().find(|&&(_, v, _)| v == vk).map(|&(n, _, _)| n)
}

/// A combo (`"Ctrl + Shift + F1"`, `"Left Ctrl + Left Win + Right Arrow"`) as
/// keys in press order; a bare number is a virtual-key code. The error is
/// what the script is told.
pub fn combo(text: &str) -> Result<Vec<Key>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Key Parameter must be atleast one character!".into());
    }
    // A lone "+" is the key itself, not a separator.
    if text == "+" {
        return Err("Unknown Key Name: +".into());
    }
    let mut keys = Vec::new();
    for part in text.split(" + ") {
        let k = key(part).ok_or_else(|| format!("Unknown Key Name: {}", part.trim()))?;
        keys.push(k);
    }
    Ok(keys)
}

/// A virtual-key code given as a number.
pub fn code(n: f64) -> Result<Key, String> {
    if n.is_finite() && n.fract() == 0.0 && (1.0..=254.0).contains(&n) {
        let vk = n as u16;
        Ok(Key {
            vk,
            extended: extended(vk),
        })
    } else {
        Err("Key Parameter must be a key name, or a virtual key code!".into())
    }
}

/// Press every key in order and release them in reverse (`keys.press`).
pub fn press(keys: &[Key]) -> Vec<SynthInput> {
    let mut out = hold(keys);
    out.extend(release(keys));
    out
}

/// Press every key in order (`keys.hold`).
pub fn hold(keys: &[Key]) -> Vec<SynthInput> {
    keys.iter()
        .map(|k| SynthInput::Key {
            vk: k.vk,
            down: true,
            extended: k.extended,
        })
        .collect()
}

/// Release every key in reverse order (`keys.release`).
pub fn release(keys: &[Key]) -> Vec<SynthInput> {
    keys.iter()
        .rev()
        .map(|k| SynthInput::Key {
            vk: k.vk,
            down: false,
            extended: k.extended,
        })
        .collect()
}

/// Type `text` as characters (`keys.sendUnicode`).
pub fn unicode(text: &str) -> Vec<SynthInput> {
    text.encode_utf16()
        .flat_map(|unit| {
            [
                SynthInput::Unicode { unit, down: true },
                SynthInput::Unicode { unit, down: false },
            ]
        })
        .collect()
}

/// Ctrl+V (`keys.paste`, after the clipboard is set).
pub fn paste() -> Vec<SynthInput> {
    press(&[
        Key {
            vk: vk::LCONTROL,
            extended: false,
        },
        Key {
            vk: vk::V,
            extended: false,
        },
    ])
}

/// Which modifiers are held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    /// Left or right Ctrl.
    pub control: bool,
    /// Left or right Shift.
    pub shift: bool,
    /// Left or right Alt.
    pub alt: bool,
    /// Left or right Win.
    pub win: bool,
    held: [bool; 8],
}

impl Modifiers {
    /// Follow key `vk` going down or up; `true` when it is a modifier.
    pub fn track(&mut self, vk: u16, released: bool) -> bool {
        let slot = match vk {
            vk::LCONTROL | vk::CONTROL => 0,
            vk::RCONTROL => 1,
            vk::LSHIFT | vk::SHIFT => 2,
            vk::RSHIFT => 3,
            vk::LALT | vk::ALT => 4,
            vk::RALT => 5,
            vk::LWIN => 6,
            vk::RWIN => 7,
            _ => return false,
        };
        self.held[slot] = !released;
        self.control = self.held[0] || self.held[1];
        self.shift = self.held[2] || self.held[3];
        self.alt = self.held[4] || self.held[5];
        self.win = self.held[6] || self.held[7];
        true
    }

    /// `keyString` of `name` pressed with these modifiers (a modifier key
    /// does not prefix itself).
    pub fn key_string(&self, vk: u16, name: &str) -> String {
        let is = |group: &[u16]| group.contains(&vk);
        let mut s = String::new();
        if self.control && !is(&[vk::CONTROL, vk::LCONTROL, vk::RCONTROL]) {
            s.push_str("Ctrl + ");
        }
        if self.win && !is(&[vk::LWIN, vk::RWIN]) {
            s.push_str("Win + ");
        }
        if self.shift && !is(&[vk::SHIFT, vk::LSHIFT, vk::RSHIFT]) {
            s.push_str("Shift + ");
        }
        if self.alt && !is(&[vk::ALT, vk::LALT, vk::RALT]) {
            s.push_str("Alt + ");
        }
        s.push_str(name);
        s
    }
}

/// UI names of the mouse buttons and the wheel (`mousecode` options,
/// `onMouse` `data.button`).
pub const BUTTON_NAMES: &[&str] = &[
    "Left Click",
    "Right Click",
    "Middle Click",
    "Mouse Button 4",
    "Mouse Button 5",
    "Scroll Up",
    "Scroll Down",
    "Scroll Left",
    "Scroll Right",
];

/// What `mouse.click(name)` does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    /// Press and release a button.
    Button(MouseButton),
    /// One wheel notch.
    Wheel {
        /// The horizontal wheel.
        horizontal: bool,
        /// +1 up / right, -1 down / left.
        notches: i32,
    },
}

/// The click named `name` (case-insensitive, with the aliases of SignalRGB's
/// native rules: `left`, `mouse4`, `xbutton1`, ...), or a virtual button code
/// (1 left, 2 right, 4 middle, 5/6 X buttons).
pub fn click(name: &str) -> Result<Click, String> {
    let n = name.trim().to_ascii_lowercase();
    let b = Click::Button;
    Ok(match n.as_str() {
        "left click" | "left" | "1" => b(MouseButton::Left),
        "right click" | "right" | "2" => b(MouseButton::Right),
        "middle click" | "middle" | "4" => b(MouseButton::Middle),
        "mouse button 4" | "mouse4" | "xbutton1" | "5" => b(MouseButton::X1),
        "mouse button 5" | "mouse5" | "xbutton2" | "6" => b(MouseButton::X2),
        "scroll up" => Click::Wheel {
            horizontal: false,
            notches: 1,
        },
        "scroll down" => Click::Wheel {
            horizontal: false,
            notches: -1,
        },
        "scroll right" => Click::Wheel {
            horizontal: true,
            notches: 1,
        },
        "scroll left" => Click::Wheel {
            horizontal: true,
            notches: -1,
        },
        "" => return Err("Button Parameter must be a button name or virtual button code!".into()),
        _ => return Err(format!("Unknown Button Name: {}", name.trim())),
    })
}

/// The synthetic input of a click.
pub fn click_input(c: Click) -> Vec<SynthInput> {
    match c {
        Click::Button(button) => vec![
            SynthInput::Button { button, down: true },
            SynthInput::Button { button, down: false },
        ],
        Click::Wheel { horizontal, notches } => vec![SynthInput::Wheel { horizontal, notches }],
    }
}

/// `data.button` and `data.buttonCode` (the `WM_*` message) of a button event.
pub fn button_event(button: MouseButton, released: bool) -> (&'static str, i32) {
    match (button, released) {
        (MouseButton::Left, false) => ("Left Click", 513),
        (MouseButton::Left, true) => ("Left Click", 514),
        (MouseButton::Right, false) => ("Right Click", 516),
        (MouseButton::Right, true) => ("Right Click", 517),
        (MouseButton::Middle, false) => ("Middle Click", 519),
        (MouseButton::Middle, true) => ("Middle Click", 520),
        (MouseButton::X1, false) => ("Mouse Button 4", 523),
        (MouseButton::X1, true) => ("Mouse Button 4", 524),
        (MouseButton::X2, false) => ("Mouse Button 5", 523),
        (MouseButton::X2, true) => ("Mouse Button 5", 524),
    }
}

/// `data.button` and `data.buttonCode` of a wheel notch.
pub fn wheel_event(horizontal: bool, notches: i32) -> (&'static str, i32) {
    match (horizontal, notches >= 0) {
        (false, true) => ("Scroll Up", 522),
        (false, false) => ("Scroll Down", 522),
        (true, true) => ("Scroll Right", 526),
        (true, false) => ("Scroll Left", 526),
    }
}

/// The mouse button behind a virtual button code of `mouse.sendHid` (`xbutton`
/// picks the X button for code 5/6 when given).
pub fn hid_button(vk: u16, xbutton: Option<u8>) -> Option<MouseButton> {
    Some(match (vk, xbutton) {
        (1, _) => MouseButton::Left,
        (2, _) => MouseButton::Right,
        (4, _) => MouseButton::Middle,
        (5 | 6, Some(2)) => MouseButton::X2,
        (5 | 6, Some(1)) => MouseButton::X1,
        (5, None) => MouseButton::X1,
        (6, None) => MouseButton::X2,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_combos() {
        assert_eq!(
            key("a"),
            Some(Key {
                vk: 0x41,
                extended: false
            })
        );
        assert_eq!(key("back space"), key("Backspace"));
        assert_eq!(
            key("Num Enter"),
            Some(Key {
                vk: 0x0D,
                extended: true
            })
        );
        assert!(key("Right Arrow").unwrap().extended);
        assert_eq!(key("nope"), None);
        let c = combo("Left Ctrl + Left Win + Right Arrow").unwrap();
        assert_eq!(c.iter().map(|k| k.vk).collect::<Vec<_>>(), vec![0xA2, 0x5B, 0x27]);
        assert_eq!(combo("F1").unwrap().len(), 1);
        assert!(combo("Ctrl + Blorp").unwrap_err().contains("Blorp"));
        assert!(combo("").is_err());
        assert!(combo("+").is_err());
        assert_eq!(combo("Shift + =").unwrap()[1].vk, 0xBB);
        assert_eq!(code(65.0).unwrap().vk, 65);
        assert!(code(0.0).is_err() && code(1.5).is_err() && code(300.0).is_err());
        assert_eq!(name_of(0x41, false), Some("A"));
        assert_eq!(name_of(0x08, false), Some("Back Space"));
        assert_eq!(name_of(0x0D, true), Some("Num Enter"));
        assert_eq!(name_of(0x1B, false), Some("Esc"));
        assert_eq!(name_of(0xFF, false), None);
    }

    #[test]
    fn press_holds_in_order_and_releases_in_reverse() {
        let keys = combo("Ctrl + C").unwrap();
        let p = press(&keys);
        let order: Vec<(u16, bool)> = p
            .iter()
            .map(|i| match i {
                SynthInput::Key { vk, down, .. } => (*vk, *down),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(
            order,
            vec![(0x11, true), (0x43, true), (0x43, false), (0x11, false)]
        );
        assert_eq!(unicode("é").len(), 2);
        assert_eq!(unicode("😀").len(), 4, "a surrogate pair is two units");
    }

    #[test]
    fn key_strings_follow_the_held_modifiers() {
        let mut m = Modifiers::default();
        assert!(m.track(0xA2, false));
        assert!(m.track(0xA0, false));
        assert!(!m.track(0x70, false));
        assert_eq!(m.key_string(0x70, "F1"), "Ctrl + Shift + F1");
        assert_eq!(m.key_string(0xA0, "Left Shift"), "Ctrl + Left Shift");
        m.track(0xA2, true);
        assert_eq!(m.key_string(0x70, "F1"), "Shift + F1");
        m.track(0x5C, false);
        m.track(0xA5, false);
        assert_eq!(m.key_string(0x41, "A"), "Win + Shift + Alt + A");
    }

    #[test]
    fn clicks() {
        assert_eq!(click("Right Click"), Ok(Click::Button(MouseButton::Right)));
        assert_eq!(click("xbutton2"), Ok(Click::Button(MouseButton::X2)));
        assert_eq!(click("4"), Ok(Click::Button(MouseButton::Middle)));
        assert!(matches!(
            click("Scroll Left"),
            Ok(Click::Wheel {
                horizontal: true,
                notches: -1
            })
        ));
        assert!(click("Nose").is_err());
        assert_eq!(click_input(click("Left Click").unwrap()).len(), 2);
        assert_eq!(button_event(MouseButton::Left, false), ("Left Click", 513));
        assert_eq!(hid_button(5, Some(2)), Some(MouseButton::X2));
        assert_eq!(hid_button(3, None), None);
    }
}
