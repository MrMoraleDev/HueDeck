// SPDX-License-Identifier: GPL-3.0-or-later
//! Key and button events a device plugin reports through its `keyboard` and
//! `mouse` feature globals (`device.addFeature("keyboard" | "mouse")`,
//! docs/host-api/macros-system.md, critic pass "Device plugins inject key/mouse
//! events"). The session hands them to an [`InputSink`] (the macro runtime):
//!
//! * `keyboard.sendEvent({key, keyCode, released}, "Key Press")` and
//!   `mouse.sendEvent({buttonCode, released, name}, "Button Press")` become
//!   macro input events (`onKey` / `onMouse`, marked `injected`);
//! * `keyboard.sendHid(vkCode, {released})` and `mouse.sendHid(vkCode,
//!   {released, XButton})` ask for a real key or button press (the macro
//!   runtime injects it only while macros are on).
//!
//! Without a sink (the dry-run, sessions of the CLI) the calls are dropped.

use serde::Deserialize;
use std::sync::Arc;

/// One event from a device plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginInput {
    /// `keyboard.sendEvent(...)`.
    Key {
        /// Key name (the plugin's own, e.g. `"G1"`).
        key: String,
        /// Virtual-key code (often 0).
        key_code: i32,
        /// Released.
        released: bool,
    },
    /// `mouse.sendEvent(...)`.
    Button {
        /// Button name (the plugin's own).
        name: String,
        /// Code (often 0).
        button_code: i32,
        /// Released.
        released: bool,
    },
    /// `keyboard.sendHid(vkCode, {released})`: press or release a real key.
    KeyHid {
        /// Virtual-key code.
        vk: u16,
        /// Released.
        released: bool,
    },
    /// `mouse.sendHid(vkCode, {released, XButton})`: press or release a real button.
    ButtonHid {
        /// Virtual-key code of the button (1 left, 2 right, 4 middle, 5/6 X buttons).
        vk: u16,
        /// Released.
        released: bool,
        /// `XButton` option (1 or 2), when given.
        xbutton: Option<u8>,
    },
}

/// Receives plugin input events: `(device key, event)`. Called on the
/// plugin's thread; must not block.
pub type InputSink = Arc<dyn Fn(&str, PluginInput) + Send + Sync>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Raw {
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    key_code: Option<f64>,
    #[serde(default)]
    button_code: Option<f64>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    released: Option<bool>,
    #[serde(default, rename = "XButton")]
    xbutton: Option<f64>,
}

/// Longest key or button name kept.
const MAX_NAME: usize = 64;

fn clip(s: Option<String>) -> String {
    s.unwrap_or_default().chars().take(MAX_NAME).collect()
}

/// Parse what `js/host.js` hands over: `kind` is `key`, `button`, `keyHid` or
/// `buttonHid`; `json` the plugin's object (with `vk` added for the HID calls).
/// `None` for malformed input (dropped).
pub fn parse(kind: &str, json: &str) -> Option<PluginInput> {
    let raw: Raw = serde_json::from_str(json).ok()?;
    let int = |v: Option<f64>| v.filter(|v| v.is_finite()).map_or(0, |v| v as i32);
    let vk = || {
        let v: serde_json::Value = serde_json::from_str(json).ok()?;
        let n = v.get("vk")?.as_f64()?;
        (n.is_finite() && (1.0..=254.0).contains(&n)).then_some(n as u16)
    };
    Some(match kind {
        "key" => PluginInput::Key {
            key: clip(raw.key),
            key_code: int(raw.key_code),
            released: raw.released.unwrap_or(false),
        },
        "button" => PluginInput::Button {
            name: clip(raw.name),
            button_code: int(raw.button_code),
            released: raw.released.unwrap_or(false),
        },
        // sendHid(): the `released` option is required (SignalRGB: "released option must be specified!").
        "keyHid" => PluginInput::KeyHid {
            vk: vk()?,
            released: raw.released?,
        },
        "buttonHid" => PluginInput::ButtonHid {
            vk: vk()?,
            released: raw.released?,
            xbutton: raw.xbutton.filter(|x| *x == 1.0 || *x == 2.0).map(|x| x as u8),
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_events_parse() {
        assert_eq!(
            parse("key", r#"{"key":"G1","keyCode":0,"released":true}"#),
            Some(PluginInput::Key {
                key: "G1".into(),
                key_code: 0,
                released: true
            })
        );
        assert_eq!(
            parse("button", r#"{"buttonCode":0,"released":false,"name":"DPI Up"}"#),
            Some(PluginInput::Button {
                name: "DPI Up".into(),
                button_code: 0,
                released: false
            })
        );
        assert_eq!(
            parse("keyHid", r#"{"vk":175,"released":false}"#),
            Some(PluginInput::KeyHid {
                vk: 175,
                released: false
            })
        );
        assert_eq!(parse("keyHid", r#"{"vk":175}"#), None, "released is required");
        assert_eq!(parse("keyHid", r#"{"vk":0,"released":true}"#), None);
        assert_eq!(
            parse("buttonHid", r#"{"vk":5,"released":true,"XButton":2}"#),
            Some(PluginInput::ButtonHid {
                vk: 5,
                released: true,
                xbutton: Some(2)
            })
        );
        assert_eq!(parse("other", "{}"), None);
        assert_eq!(parse("key", "not json"), None);
    }
}
