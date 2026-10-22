// SPDX-License-Identifier: GPL-3.0-or-later
//! The CC0 starter effects in `effects/` parse cleanly: title, description,
//! publisher, the CC0 licence header, and controls whose defaults are valid for
//! their type (in range, one of the listed values, a `#rrggbb` colour).

use lightloom_effects::{parse_effect_meta, ControlType};
use std::path::PathBuf;

fn effects_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../effects")
}

#[test]
fn every_starter_effect_has_metadata_and_valid_defaults() {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(effects_dir())
        .expect("effects/")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "html"))
        .collect();
    paths.sort();
    assert!(paths.len() >= 13, "{paths:?}");
    for path in &paths {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let html = std::fs::read_to_string(path).unwrap();
        assert!(
            html.contains("SPDX-License-Identifier: CC0-1.0"),
            "{name}: licence header"
        );
        let meta = parse_effect_meta(&html);
        assert!(meta.warnings.is_empty(), "{name}: {:?}", meta.warnings);
        assert!(!meta.title.is_empty(), "{name}: title");
        assert!(!meta.description.is_empty(), "{name}: description");
        assert_eq!(meta.publisher, "Lightloom", "{name}: publisher");
        assert!(!meta.properties.is_empty(), "{name}: controls");
        for c in &meta.properties {
            let what = format!("{name}: {}", c.property);
            let raw = c
                .default
                .as_deref()
                .unwrap_or_else(|| panic!("{what}: no default"));
            match c.control_type {
                ControlType::Number | ControlType::Hue => {
                    let v: f64 = raw.parse().unwrap_or_else(|_| panic!("{what}: default {raw}"));
                    let (min, max) = (c.min.expect("min"), c.max.expect("max"));
                    assert!(min <= v && v <= max, "{what}: {v} outside {min}..={max}");
                }
                ControlType::Boolean => {
                    assert!(
                        ["true", "false", "0", "1"].contains(&raw),
                        "{what}: default {raw}"
                    )
                }
                ControlType::Color => assert!(
                    raw.len() == 7 && raw.starts_with('#') && raw[1..].chars().all(|c| c.is_ascii_hexdigit()),
                    "{what}: default {raw}"
                ),
                ControlType::Combobox => assert!(c.values.iter().any(|v| v == raw), "{what}: default {raw}"),
                ControlType::Textfield | ControlType::Sensor => {}
            }
        }
    }
}
