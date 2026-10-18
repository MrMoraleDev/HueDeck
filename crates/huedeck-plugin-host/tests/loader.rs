// SPDX-License-Identifier: Apache-2.0 OR MIT
//! rquickjs loader + stub host against the synthetic fixtures in `testdata/`.

use huedeck_plugin_host::dryrun::{dry_run, run_file, DryRunOptions, FileKind, FileReport};
use huedeck_plugin_host::runtime::{run_isolated, CallArgs, PluginRuntime, RuntimeOptions};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn testdata() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata")
}

fn opts(initialize: bool) -> DryRunOptions {
    DryRunOptions {
        initialize,
        eval_budget: Duration::from_secs(10),
        call_budget: Duration::from_millis(500),
        jobs: 2,
        ..DryRunOptions::default()
    }
}

fn by_file<'a>(reports: &'a [FileReport], file: &str) -> &'a FileReport {
    reports
        .iter()
        .find(|r| r.file == file)
        .unwrap_or_else(|| panic!("{file} not in report"))
}

#[test]
fn dry_run_over_fixtures() {
    let (reports, summary) = dry_run(&testdata().join("plugins"), &opts(true), &|_, _, _| {}).unwrap();
    assert_eq!(summary.files, 6);
    assert_eq!(summary.parsed, 6);
    assert_eq!(summary.evaluated, 6);
    assert_eq!(summary.helpers, 1);
    assert_eq!(summary.plugins, 5);
    assert_eq!(summary.meta_complete, 5);
    assert_eq!(summary.initialize_called, 5);
    assert_eq!(summary.initialize_ok, 4);
    assert_eq!(summary.initialize_error_classes, vec![("Timeout".to_string(), 1)]);

    let mouse = by_file(&reports, "hid_mouse.js");
    assert!(mouse.ok, "{mouse:?}");
    assert_eq!(mouse.kind, FileKind::Device);
    let m = mouse.meta.as_ref().unwrap();
    assert_eq!(m.name, "Synthetic Mouse");
    assert_eq!((m.vid, m.pids.clone()), (0x1234, vec![1, 2]));
    assert_eq!(m.device_type, "mouse");
    assert_eq!(m.parameters, 4);
    assert_eq!(m.lifecycle, ["Validate", "Initialize", "Render", "Shutdown"]);
    // Module namespace keys are sorted by code unit (ECMA-262 §10.4.6).
    assert_eq!(m.change_hooks, ["onBrightnessChanged", "ondpiChanged"]);
    assert!(mouse.initialize.as_ref().unwrap().error.is_none());
    assert!(mouse.shutdown.as_ref().unwrap().error.is_none());
    assert_eq!(mouse.console_lines, 2, "{:?}", mouse.log_tail);
    assert!(
        mouse.log_tail[1].contains("#000000"),
        "shutdownColor global bound"
    );
    assert!(mouse.api_gaps.is_empty(), "{:?}", mouse.api_gaps);
    // Documented members are recording stubs: every call is counted.
    let calls = |name: &str| mouse.host_calls.get(name).copied().unwrap_or(0);
    assert_eq!(calls("device.write"), 1, "{:?}", mouse.host_calls);
    assert_eq!(calls("device.read"), 1);
    assert_eq!(calls("device.productId"), 2);
    assert_eq!(calls("battery.setBatteryLevel"), 1);
    assert_eq!(calls("device.log"), 1);

    let kb = by_file(&reports, "vendor/class_keyboard.js");
    assert!(kb.ok, "{kb:?}");
    assert_eq!(kb.meta.as_ref().unwrap().pids, vec![0x0DA0, 0x0DA1]);

    let helper = by_file(&reports, "vendor/lib/protocol.js");
    assert_eq!(helper.kind, FileKind::Helper);
    assert!(helper.ok && helper.initialize.is_none());

    let ram = by_file(&reports, "smbus_ram.js");
    assert_eq!(ram.kind, FileKind::Smbus);
    assert!(ram.ok && ram.meta_complete, "{ram:?}");
    assert_eq!(
        ram.meta.as_ref().unwrap().lifecycle,
        ["Initialize", "Render", "Shutdown", "Scan"]
    );

    let lan = by_file(&reports, "network_lights.js");
    assert_eq!(lan.kind, FileKind::Network);
    assert!(lan.ok, "{lan:?}");
    assert!(lan
        .meta
        .as_ref()
        .unwrap()
        .lifecycle
        .contains(&"DiscoveryService".to_string()));
    // `controller` is plugin-defined data: a field the stub lacks is not an API gap.
    assert!(lan.api_gaps.is_empty(), "{:?}", lan.api_gaps);
    assert_eq!(lan.stub_fields, ["controller.streamingPort"]);
    assert_eq!(
        summary.top_stub_fields,
        vec![("controller.streamingPort".to_string(), 1)]
    );

    let gaps = by_file(&reports, "gaps_and_loops.js");
    assert!(!gaps.ok);
    assert_eq!(
        gaps.initialize.as_ref().unwrap().error.as_ref().unwrap().class,
        "Timeout"
    );
    assert_eq!(gaps.api_gaps, ["bus.NotARealMember", "device.frobnicate"]);
    // The runtime stays usable after an interrupt.
    assert!(gaps.shutdown.as_ref().unwrap().error.is_none());
    assert_eq!(summary.top_api_gaps.len(), 2);
}

#[test]
fn edge_cases_are_classified() {
    let mut o = opts(true);
    o.call_budget = Duration::from_millis(300);
    let (reports, summary) = dry_run(&testdata().join("edge"), &o, &|_, _, _| {}).unwrap();
    assert_eq!(summary.files, 9);
    // Every file compiles: link, dependency and run-time errors are not parse failures.
    assert_eq!(summary.parsed, 9);

    // The deadline still counts when it cuts off a promise job (drained by the job loop).
    let spin = by_file(&reports, "job_spin.js");
    let err = spin.initialize.as_ref().unwrap().error.as_ref();
    assert_eq!(err.map(|e| e.class.as_str()), Some("Timeout"), "{spin:?}");
    assert!(
        spin.shutdown.as_ref().unwrap().error.is_none(),
        "runtime usable after the interrupt"
    );

    let never = by_file(&reports, "async_never.js");
    let err = never.initialize.as_ref().unwrap().error.as_ref();
    assert_eq!(err.map(|e| e.class.as_str()), Some("PromisePending"));

    // prelude/modules/README.md rule 4: unknown host module -> HostApiMissing at evaluation.
    let unknown = by_file(&reports, "unknown_module.js");
    assert!(unknown.parsed && !unknown.evaluated);
    let err = unknown.error.as_ref().unwrap();
    assert_eq!(err.class, "HostApiMissing");
    assert!(err.message.contains("@SignalRGB/nope"), "{err}");
    assert_eq!(unknown.api_gaps, ["import:@SignalRGB/nope"]);

    for file in ["link_error.js", "runtime_syntax_error.js"] {
        let r = by_file(&reports, file);
        assert!(r.parsed && !r.evaluated, "{r:?}");
        assert_eq!(r.error.as_ref().unwrap().class, "SyntaxError");
    }

    // The host helper object is read-only: no metadata spoofing.
    let tamper = by_file(&reports, "host_tamper.js");
    assert!(!tamper.evaluated && tamper.meta.is_none());
    assert_eq!(tamper.error.as_ref().unwrap().class, "TypeError");

    let internal = by_file(&reports, "internal_import.js");
    assert!(internal.parsed && !internal.evaluated);
    assert_eq!(internal.api_gaps, ["import:huedeck:prelude/BinaryUtils.js"]);

    let thrown = by_file(&reports, "throw_object.js").error.as_ref().unwrap();
    assert_eq!(
        (thrown.class.as_str(), thrown.message.as_str()),
        ("Throw", "not an Error instance")
    );
}

#[test]
fn syntax_errors_are_parse_failures() {
    let root = testdata().join("broken");
    let r = run_file(&root, &root.join("syntax_error.js"), &opts(false));
    assert!(!r.parsed && !r.evaluated && !r.ok);
    assert_eq!(r.error.unwrap().class, "SyntaxError");
}

#[test]
fn smbus_scan_receives_free_bus() {
    let root = testdata().join("plugins");
    let path = root.join("smbus_ram.js");
    let found = run_isolated(|| {
        let rt = PluginRuntime::new(&path, &RuntimeOptions::default()).unwrap();
        assert!(rt.load(Duration::from_secs(5)).error.is_none());
        rt.call_export("Scan", CallArgs::FreeBus, Duration::from_secs(1))
    })
    .unwrap();
    assert_eq!(found, Ok(true));
}

#[test]
fn facades_throw_host_api_missing() {
    let root = testdata().join("plugins");
    let mut o = opts(true);
    o.runtime.module_stubs = false;
    let r = run_file(&root, &root.join("network_lights.js"), &o);
    assert!(r.evaluated, "importing a facade never throws");
    let err = r.initialize.unwrap().error.unwrap();
    assert_eq!(err.class, "HostApiMissing");
    assert!(err.message.contains("@SignalRGB/udp.send"), "{err}");
    assert!(
        r.api_gaps.contains(&"@SignalRGB/udp.send".to_string()),
        "{:?}",
        r.api_gaps
    );
}

#[test]
fn memory_cap_stops_runaway_allocation() {
    let root = testdata().join("oom");
    let mut o = opts(false);
    o.runtime.memory_limit = 8 * 1024 * 1024;
    let r = run_file(&root, &root.join("hoarder.js"), &o);
    assert!(r.parsed && !r.evaluated);
    assert_eq!(r.error.unwrap().class, "OutOfMemory");
}
