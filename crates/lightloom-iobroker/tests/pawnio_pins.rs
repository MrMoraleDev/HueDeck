// SPDX-License-Identifier: GPL-3.0-or-later
//! Drift test: the PawnIO module pins compiled into the broker (`pins.rs`) are the ones
//! `tools/package/fetch-pawnio-modules.ps1` stages, for the same PawnIO.Modules release.
//! `tools/pawnio-hash-pin` recomputes them from the release files when the release changes.

use lightloom_iobroker::pins::{MODULES, MODULES_RELEASE};
use std::collections::BTreeMap;

const FETCH: &str = include_str!("../../../tools/package/fetch-pawnio-modules.ps1");

/// `"<file>" = "<sha256>"` entries of the script's `$fromZip` table.
fn script_pins() -> BTreeMap<String, String> {
    let start = FETCH
        .find("$fromZip = [ordered]@{")
        .expect("$fromZip in the script");
    let body = &FETCH[start..];
    let body = &body[..body.find('}').expect("end of $fromZip")];
    body.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            let k = k.trim().trim_matches('"');
            let v = v.trim().trim_matches('"');
            (k.ends_with(".bin") || k == "COPYING").then(|| (k.to_string(), v.to_string()))
        })
        .collect()
}

#[test]
fn the_script_stages_the_release_the_broker_pins() {
    assert!(
        FETCH.contains(&format!("$release = \"{MODULES_RELEASE}\"")),
        "fetch-pawnio-modules.ps1 fetches another release than pins.rs ({MODULES_RELEASE})"
    );
}

#[test]
fn every_module_pin_matches_the_script() {
    let script = script_pins();
    assert!(script.contains_key("COPYING"), "{script:?}");
    for pin in MODULES {
        assert_eq!(
            script.get(pin.file).map(String::as_str),
            Some(pin.sha256),
            "{}: pins.rs and fetch-pawnio-modules.ps1 disagree",
            pin.file
        );
    }
    let bins = script.keys().filter(|k| k.ends_with(".bin")).count();
    assert_eq!(
        bins,
        MODULES.len(),
        "the script stages a module the broker does not pin"
    );
}
