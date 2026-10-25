// SPDX-License-Identifier: GPL-3.0-or-later
//! The built-in component library (`components/`) loads without a rejected file or a
//! warning, and the daemon finds it in a source-tree build.

use lightloom_engine::ComponentLibrary;
use std::path::PathBuf;

#[test]
fn builtin_components_load_cleanly() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../components");
    let (lib, issues) = ComponentLibrary::load(std::slice::from_ref(&dir));
    assert!(issues.is_empty(), "{issues:#?}");
    assert!(lib.len() >= 40, "{} components", lib.len());
    assert_eq!(lib.len(), lib.files(), "every file has its own ProductName");
    assert!(lib
        .entries()
        .any(|e| e.component.product_name == "Lightloom Corsair QL120 RGB" && e.component.led_count == 34));

    let found = lightloom_daemon::paths::resource_dir("components", "components").expect("components dir");
    assert_eq!(
        std::fs::canonicalize(found).unwrap(),
        std::fs::canonicalize(dir).unwrap()
    );
}
