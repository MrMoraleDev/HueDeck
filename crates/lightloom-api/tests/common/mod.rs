// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared by the OpenAPI contract tests: read the operations of a spec (paths at two spaces,
//! methods at four, `security` at six) and the `.route(...)` calls of a router in `server.rs`,
//! line by line, without a YAML parser.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

pub const SERVER: &str = include_str!("../../src/server.rs");
pub const METHODS: [&str; 5] = ["get", "put", "post", "patch", "delete"];

/// One operation of the spec: path, method, and whether it lists `security`.
pub type Ops = BTreeMap<(String, String), bool>;

/// The operations of `spec`.
pub fn spec_operations(spec: &str) -> Ops {
    let mut ops = Ops::new();
    let mut in_paths = false;
    let mut path: Option<String> = None;
    let mut method: Option<String> = None;
    for line in spec.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent == 0 {
            in_paths = line.starts_with("paths:");
            path = None;
            method = None;
            continue;
        }
        if !in_paths {
            continue;
        }
        let text = line.trim();
        if indent == 2 {
            path = text.strip_suffix(':').map(str::to_string);
            method = None;
        } else if indent == 4 {
            let name = text.trim_end_matches(':');
            method = METHODS.contains(&name).then(|| name.to_string());
            if let (Some(p), Some(m)) = (&path, &method) {
                ops.insert((p.clone(), m.clone()), false);
            }
        } else if indent == 6 && text.starts_with("security:") {
            if let (Some(p), Some(m)) = (&path, &method) {
                ops.insert((p.clone(), m.clone()), true);
            }
        }
    }
    ops
}

/// `(path, method)` of every `.route(...)` call inside `router` (a `pub fn` of server.rs).
pub fn router_operations(router: &str) -> BTreeSet<(String, String)> {
    let start = SERVER
        .find(&format!("pub fn {router}("))
        .unwrap_or_else(|| panic!("{router} in server.rs"));
    let body = &SERVER[start..];
    let end = [".fallback(", ".with_state("]
        .iter()
        .filter_map(|m| body.find(m))
        .min()
        .unwrap_or_else(|| panic!("{router} ends with a fallback or its state"));
    let body = &body[..end];
    let mut ops = BTreeSet::new();
    let mut rest = body;
    while let Some(i) = rest.find(".route(") {
        let call = &rest[i + ".route(".len()..];
        // The call's arguments, up to the matching parenthesis.
        let mut depth = 1;
        let mut len = 0;
        for (j, c) in call.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        len = j;
                        break;
                    }
                }
                _ => {}
            }
        }
        let args = &call[..len];
        let q1 = args.find('"').expect("route path literal");
        let q2 = args[q1 + 1..].find('"').expect("route path literal end") + q1 + 1;
        let path = args[q1 + 1..q2].to_string();
        let handlers = &args[q2 + 1..];
        for m in METHODS {
            let needle = format!("{m}(");
            let found = handlers.match_indices(&needle).any(|(k, _)| {
                let before = handlers[..k].chars().last();
                matches!(before, Some('.') | Some(':') | Some(' ') | Some('\n') | Some(','))
            });
            if found {
                ops.insert((path.clone(), m.to_string()));
            }
        }
        rest = &call[len..];
    }
    ops
}
