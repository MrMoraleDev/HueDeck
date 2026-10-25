// SPDX-License-Identifier: GPL-3.0-or-later
//! Contract test: docs/api/compat-openapi.yaml describes exactly the SignalRGB-compatible API.
//!
//! Every route of `compat_router` and `canvas_event_router` (read from `src/server.rs`) and every
//! entry of [`lightloom_api::COMPAT_ROUTES`] and [`lightloom_api::COMPAT_CANVAS_EVENT_ROUTE`] has
//! its path and method in the spec, the spec has no operation the routers lack, and mutating
//! operations declare the bearer token.

mod common;

use common::{router_operations, spec_operations};
use std::collections::BTreeSet;

const SPEC: &str = include_str!("../../../docs/api/compat-openapi.yaml");

fn table() -> Vec<(String, String, bool)> {
    let mut all: Vec<_> = lightloom_api::COMPAT_ROUTES
        .iter()
        .map(|r| {
            (
                format!("{}{}", lightloom_api::COMPAT_PREFIX, r.path),
                r.method.to_ascii_lowercase(),
                r.mutating,
            )
        })
        .collect();
    let c = lightloom_api::COMPAT_CANVAS_EVENT_ROUTE;
    all.push((c.path.to_string(), c.method.to_ascii_lowercase(), c.mutating));
    all
}

fn routers() -> BTreeSet<(String, String)> {
    let mut ops = router_operations("compat_router");
    ops.extend(router_operations("canvas_event_router"));
    ops
}

#[test]
fn the_router_parser_sees_both_routers() {
    let ops = routers();
    assert!(ops.contains(&("/api/v1/lighting".into(), "get".into())));
    assert!(ops.contains(&("/api/v1/lighting/next".into(), "post".into())));
    assert!(ops.contains(&("/canvas/event".into(), "post".into())));
    assert!(ops.len() >= 25, "{ops:?}");
}

#[test]
fn every_compat_route_is_in_the_spec() {
    let spec = spec_operations(SPEC);
    let missing: Vec<_> = routers()
        .into_iter()
        .chain(table().into_iter().map(|(p, m, _)| (p, m)))
        .filter(|op| !spec.contains_key(op))
        .collect();
    assert!(
        missing.is_empty(),
        "compat routes missing from docs/api/compat-openapi.yaml: {missing:?}"
    );
}

#[test]
fn the_spec_has_no_operation_the_routers_lack() {
    let ops = routers();
    let extra: Vec<_> = spec_operations(SPEC)
        .into_keys()
        .filter(|op| !ops.contains(op))
        .collect();
    assert!(
        extra.is_empty(),
        "docs/api/compat-openapi.yaml describes routes the compat routers do not serve: {extra:?}"
    );
}

#[test]
fn mutating_operations_declare_the_token() {
    let spec = spec_operations(SPEC);
    for (path, method, mutating) in table() {
        let secured = spec
            .get(&(path.clone(), method.clone()))
            .copied()
            .unwrap_or(false);
        assert_eq!(
            secured, mutating,
            "{method} {path}: security in the spec must match `mutating`"
        );
    }
}

#[test]
fn the_spec_documents_the_envelope() {
    for needle in [
        "openapi: 3.1.0",
        "CompatEnvelope",
        "api_version",
        "16038",
        "16034",
    ] {
        assert!(SPEC.contains(needle), "compat-openapi.yaml mentions {needle}");
    }
}
