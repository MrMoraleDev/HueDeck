// SPDX-License-Identifier: Apache-2.0 OR MIT
//! JavaScript evaluated in the effect's main-frame V8 context *before* any page script runs
//! (CEF `OnContextCreated`).
//!
//! SPIKE A scope: a zeroed `window.engine` (no real audio/screen/sensor inputs yet), the
//! `<meta property>` controls as globals with their defaults (lightscripts such as the stock
//! Rainbow read `speed`/`vertical` from their first animation frame on), and a `load` listener
//! that calls every `on<prop>Changed()` once and then `onEngineReady()`. M3 replaces this with
//! the full load sequence (see docs/host-api/effects-engine.md).

use lightloom_effects::EffectMeta;
use lightloom_protocol::shm::{AUDIO_FREQ_BINS, ZONE_COLS, ZONE_IMAGE_HEIGHT, ZONE_IMAGE_WIDTH, ZONE_ROWS};

/// Key of the injection script in the browser's `extra_info` dictionary (browser → renderer).
pub const INJECT_KEY: &str = "lightloom.inject";
/// `sourceURL` the injected script is evaluated under (shows up in console messages).
pub const INJECT_URL: &str = "lightloom://renderer/inject.js";

/// `window.engine` with every input zeroed.
pub fn engine_bootstrap_js() -> String {
    let cells = ZONE_COLS * ZONE_ROWS;
    let image = ZONE_IMAGE_WIDTH * ZONE_IMAGE_HEIGHT * 4;
    format!(
        "(function () {{\n\
         \x20 var e = window.engine = window.engine || {{}};\n\
         \x20 e.audio = {{ freq: new Int8Array({AUDIO_FREQ_BINS}), level: -100, rawlevel: 0, width: 0, density: 0 }};\n\
         \x20 e.zone = {{ hue: new Int16Array({cells}), saturation: new Int8Array({cells}), \
         lightness: new Int8Array({cells}), imagedata: new Uint8ClampedArray({image}) }};\n\
         \x20 e.vision = {{}};\n\
         \x20 e.sensors = {{}};\n\
         \x20 e.getSensorValue = function (name) {{ return null; }};\n\
         }})();\n"
    )
}

/// Whether `name` is a plain (ASCII) JS identifier.
pub fn is_js_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// Names a `var <name> = ...;` statement must not use: ECMAScript reserved words (one of them
/// would make the whole injection script a SyntaxError, so `window.engine` would be missing too),
/// read-only or navigating window globals (`var location = 5` would navigate the page away), and
/// the globals the injection itself defines.
const NOT_INJECTABLE: &[&str] = &[
    "arguments",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "document",
    "else",
    "engine",
    "enum",
    "eval",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "globalThis",
    "if",
    "implements",
    "import",
    "in",
    "Infinity",
    "instanceof",
    "interface",
    "let",
    "location",
    "NaN",
    "new",
    "null",
    "package",
    "parent",
    "private",
    "protected",
    "public",
    "return",
    "self",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "top",
    "true",
    "try",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "window",
    "with",
    "yield",
];

/// Whether a `<meta property>` name can become a page global (`var <name> = <default>;`).
pub fn is_injectable_global(name: &str) -> bool {
    is_js_identifier(name) && !NOT_INJECTABLE.contains(&name)
}

/// Full injection script for an effect: engine bootstrap, control globals, load hooks.
/// Returns the script and any controls skipped (names that cannot be declared as globals).
pub fn injection_script(meta: &EffectMeta) -> (String, Vec<String>) {
    let mut js = engine_bootstrap_js();
    let mut skipped = Vec::new();
    let mut hooks = Vec::new();
    for control in &meta.properties {
        if !is_injectable_global(&control.property) {
            skipped.push(control.property.clone());
            continue;
        }
        js.push_str(&control.js_initializer(None));
        js.push('\n');
        hooks.push(control.change_hook());
    }
    hooks.push("onEngineReady".to_string());
    let list = hooks
        .iter()
        .map(|h| format!("\"{h}\""))
        .collect::<Vec<_>>()
        .join(", ");
    js.push_str(&format!(
        "window.addEventListener(\"load\", function () {{\n\
         \x20 [{list}].forEach(function (name) {{\n\
         \x20   if (typeof window[name] === \"function\") {{\n\
         \x20     try {{ window[name](); }} catch (err) {{ console.error(\"lightloom: \" + name + \" threw: \" + err); }}\n\
         \x20   }}\n\
         \x20 }});\n\
         }});\n"
    ));
    (js, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers() {
        assert!(is_js_identifier("speed"));
        assert!(is_js_identifier("_a1$"));
        assert!(!is_js_identifier("1abc"));
        assert!(!is_js_identifier("a-b"));
        assert!(!is_js_identifier("a;alert(1)"));
        assert!(!is_js_identifier(""));
        assert!(is_injectable_global("speed"));
        assert!(is_js_identifier("default") && !is_injectable_global("default"));
        assert!(!is_injectable_global("engine"));
        assert!(!is_injectable_global("in"));
        assert!(!is_injectable_global("location"));
        assert!(is_injectable_global("instance"));
    }

    #[test]
    fn reserved_word_control_is_skipped_not_injected() {
        let html = r##"<head><title>T</title>
            <meta property="speed" label="Speed" type="number" min="0" max="100" default="5"/>
            <meta property="default" label="D" type="number" min="0" max="1" default="1"/>
            <meta property="engine" label="E" type="number" min="0" max="1" default="1"/></head>"##;
        let meta = lightloom_effects::parse_effect_meta(html);
        let (js, skipped) = injection_script(&meta);
        assert_eq!(skipped, vec!["default".to_string(), "engine".to_string()]);
        assert!(js.contains("var speed = 5;"));
        assert!(!js.contains("var default"));
        assert!(!js.contains("var engine"));
        assert!(js.contains("\"onspeedChanged\", \"onEngineReady\""));
    }

    #[test]
    fn bootstrap_shapes() {
        let js = engine_bootstrap_js();
        assert!(js.contains("freq: new Int8Array(200)"));
        assert!(js.contains("hue: new Int16Array(560)"));
        assert!(js.contains("imagedata: new Uint8ClampedArray(64000)"));
        assert!(js.contains("level: -100"));
    }

    #[test]
    fn script_for_effect() {
        let html = r##"<head><title>T</title>
            <meta property="speed" label="Speed" type="number" min="0" max="100" default="50"/>
            <meta property="vertical" label="V" type="boolean" default="0"/>
            <meta property="tint" label="Tint" type="color" default="#ff0000"/></head>"##;
        let meta = lightloom_effects::parse_effect_meta(html);
        let (js, skipped) = injection_script(&meta);
        assert!(skipped.is_empty());
        assert!(js.contains("var speed = 50;"));
        assert!(js.contains("var vertical = false;"));
        assert!(js.contains("var tint = \"#ff0000\";"));
        assert!(
            js.contains("\"onspeedChanged\", \"onverticalChanged\", \"ontintChanged\", \"onEngineReady\"")
        );
    }
}
