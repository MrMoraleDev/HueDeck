// SPDX-License-Identifier: GPL-3.0-or-later
//! Contract test: docs/api/openapi.yaml describes exactly the native API.
//!
//! Every `/v1` route of `native_router` (read from `src/server.rs`) and every entry of
//! [`lightloom_api::NATIVE_ROUTES`] has its path and method in the spec, and the spec has no
//! operation the router lacks. Mutating operations declare the bearer token. The YAML is read
//! line by line (the spec keeps paths at two spaces and methods at four), so no YAML parser
//! is needed.

mod common;

use common::{router_operations, spec_operations, Ops};
use std::collections::BTreeSet;

const SPEC: &str = include_str!("../../../docs/api/openapi.yaml");

fn native_spec() -> Ops {
    spec_operations(SPEC)
}

fn native_router_ops() -> BTreeSet<(String, String)> {
    router_operations("native_router")
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
    let router = native_router_ops();
    assert!(router.contains(&("/v1/health".into(), "get".into())));
    assert!(router.contains(&("/v1/settings".into(), "patch".into())));
    assert!(router.contains(&("/v1/effects/{id}/presets/{name}".into(), "delete".into())));
    assert!(router.contains(&("/v1/layout".into(), "patch".into())));
    assert!(router.len() >= 30, "{router:?}");
}

#[test]
fn every_router_route_is_in_the_spec() {
    let spec = native_spec();
    let missing: Vec<_> = native_router_ops()
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
    let spec = native_spec();
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
    let router = native_router_ops();
    let extra: Vec<_> = native_spec()
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
    let spec = native_spec();
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
