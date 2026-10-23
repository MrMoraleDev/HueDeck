// SPDX-License-Identifier: GPL-3.0-or-later
//! Contract test: docs/api/openapi.yaml describes exactly the native API.
//!
//! Every `/v1` route of `native_router` (read from `src/server.rs`) and every entry of
//! [`lightloom_api::NATIVE_ROUTES`] has its path and method in the spec, and the spec has no
//! operation the router lacks. Mutating operations declare the bearer token. The YAML is read
//! line by line (the spec keeps paths at two spaces and methods at four), so no YAML parser
//! is needed.

use std::collections::{BTreeMap, BTreeSet};

const SPEC: &str = include_str!("../../../docs/api/openapi.yaml");
const SERVER: &str = include_str!("../src/server.rs");
const METHODS: [&str; 5] = ["get", "put", "post", "patch", "delete"];

/// One operation of the spec: path, method, and whether it lists `security`.
type Ops = BTreeMap<(String, String), bool>;

fn spec_operations() -> Ops {
    let mut ops = Ops::new();
    let mut in_paths = false;
    let mut path: Option<String> = None;
    let mut method: Option<String> = None;
    for line in SPEC.lines() {
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

/// `(path, method)` of every `.route(...)` call inside `native_router`.
fn router_operations() -> BTreeSet<(String, String)> {
    let start = SERVER
        .find("pub fn native_router")
        .expect("native_router in server.rs");
    let body = &SERVER[start..];
    let end = body
        .find(".fallback(")
        .expect("native_router ends with a fallback");
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

fn table_operations() -> BTreeSet<(String, String)> {
    lightloom_api::NATIVE_ROUTES
        .iter()
        .map(|r| {
            (
                format!("{}{}", lightloom_api::NATIVE_PREFIX, r.path),
                r.method.to_ascii_lowercase(),
            )
        })
        .collect()
}

#[test]
fn router_parser_sees_the_router() {
    let router = router_operations();
    assert!(router.contains(&("/v1/health".into(), "get".into())));
    assert!(router.contains(&("/v1/settings".into(), "patch".into())));
    assert!(router.contains(&("/v1/effects/{id}/presets/{name}".into(), "delete".into())));
    assert!(router.contains(&("/v1/layout".into(), "patch".into())));
    assert!(router.len() >= 30, "{router:?}");
}

#[test]
fn every_router_route_is_in_the_spec() {
    let spec = spec_operations();
    let missing: Vec<_> = router_operations()
        .into_iter()
        .filter(|(path, _)| path.starts_with("/v1"))
        .filter(|op| !spec.contains_key(op))
        .collect();
    assert!(
        missing.is_empty(),
        "routes of native_router missing from docs/api/openapi.yaml: {missing:?}"
    );
}

#[test]
fn every_route_table_entry_is_in_the_spec() {
    let spec = spec_operations();
    let missing: Vec<_> = table_operations()
        .into_iter()
        .filter(|op| !spec.contains_key(op))
        .collect();
    assert!(
        missing.is_empty(),
        "NATIVE_ROUTES missing from docs/api/openapi.yaml: {missing:?}"
    );
}

#[test]
fn the_spec_has_no_operation_the_router_lacks() {
    let router = router_operations();
    let extra: Vec<_> = spec_operations()
        .into_keys()
        .filter(|op| !router.contains(op))
        .collect();
    assert!(
        extra.is_empty(),
        "docs/api/openapi.yaml describes routes native_router does not serve: {extra:?}"
    );
}

#[test]
fn mutating_operations_declare_the_token() {
    let spec = spec_operations();
    for r in lightloom_api::NATIVE_ROUTES {
        let key = (format!("/v1{}", r.path), r.method.to_ascii_lowercase());
        let secured = spec.get(&key).copied().unwrap_or(false);
        assert_eq!(
            secured, r.mutating,
            "{} {}: security in the spec must match `mutating`",
            r.method, r.path
        );
    }
}

#[test]
fn the_spec_documents_the_port_errors() {
    for needle in ["port_in_use", "invalid_port", "\"409\":", "openapi: 3.1.0"] {
        assert!(SPEC.contains(needle), "openapi.yaml mentions {needle}");
    }
}
