// SPDX-License-Identifier: GPL-3.0-or-later
//! Corpus dry-run (`huedeck compat dry-run <dir>`).
//!
//! Every `*.js` file under a directory is loaded into a fresh runtime on its
//! own OS thread with the stub host, its namespace read with the
//! call-if-function rule and, optionally, `Initialize()` followed by
//! `Shutdown(false)` is run against stubbed `device`/`bus` objects. Nothing
//! touches hardware or the network.

use crate::meta::PluginType;
use crate::runtime::{run_isolated, CallArgs, JsFailure, PluginRuntime, RuntimeOptions};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Dry-run configuration.
#[derive(Clone, Debug)]
pub struct DryRunOptions {
    /// Also call `Initialize()` then `Shutdown(false)`.
    pub initialize: bool,
    /// Budget for parse + evaluation + metadata.
    pub eval_budget: Duration,
    /// Budget for each lifecycle call.
    pub call_budget: Duration,
    /// Worker threads (each plugin still gets its own thread and runtime).
    pub jobs: usize,
    /// Runtime settings (memory cap, module stubs vs facades).
    pub runtime: RuntimeOptions,
}

impl Default for DryRunOptions {
    fn default() -> Self {
        Self {
            initialize: false,
            eval_budget: Duration::from_secs(10),
            call_budget: Duration::from_secs(5),
            jobs: std::thread::available_parallelism().map_or(4, |n| n.get()),
            runtime: RuntimeOptions::default(),
        }
    }
}

/// How the file was classified.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    /// USB-class device plugin (exports `ProductId`).
    Device,
    /// `Type() == "SMBUS"`.
    Smbus,
    /// `Type() == "network"`.
    Network,
    /// Module without `ProductId` (shared protocol/helper code, `*.test.js`) — skipped, not a failure.
    Helper,
    /// Could not be evaluated; classified from the source text.
    Unknown,
}

/// Metadata summary of one plugin.
#[derive(Clone, Debug, Default, Serialize)]
pub struct MetaSummary {
    /// `Name()`.
    pub name: String,
    /// `VendorId()`.
    pub vid: u16,
    /// Resolved `ProductId()`.
    pub pids: Vec<u16>,
    /// `Type()` (canonical spelling).
    #[serde(rename = "type")]
    pub plugin_type: String,
    /// `DeviceType()`.
    pub device_type: String,
    /// Lifecycle exports present.
    pub lifecycle: Vec<String>,
    /// `on*Changed` exports.
    pub change_hooks: Vec<String>,
    /// Number of `ControllableParameters`.
    pub parameters: usize,
}

/// Outcome of one lifecycle call.
#[derive(Clone, Debug, Serialize)]
pub struct CallOutcome {
    /// Whether the export existed and was called.
    pub called: bool,
    /// Exception, if any.
    pub error: Option<JsFailure>,
    /// Wall time.
    pub ms: u64,
}

/// Per-file result.
#[derive(Clone, Debug, Serialize)]
pub struct FileReport {
    /// Path relative to the dry-run root.
    pub file: String,
    /// Classification.
    pub kind: FileKind,
    /// Compiled (no `SyntaxError`).
    pub parsed: bool,
    /// Evaluated and namespace read.
    pub evaluated: bool,
    /// `evaluated` and (helper or meta-complete) and no lifecycle exception.
    pub ok: bool,
    /// First fatal error (parse / evaluation).
    pub error: Option<JsFailure>,
    /// Metadata summary.
    pub meta: Option<MetaSummary>,
    /// Name + VendorId + ProductId, or a named SMBus / network plugin.
    pub meta_complete: bool,
    /// Metadata exports whose call threw.
    pub meta_errors: BTreeMap<String, String>,
    /// Metadata shape warnings.
    pub meta_warnings: Vec<String>,
    /// `Initialize()` (with `--initialize`).
    pub initialize: Option<CallOutcome>,
    /// `Shutdown(false)` (with `--initialize`).
    pub shutdown: Option<CallOutcome>,
    /// `API_GAP:<object>.<member>` keys hit.
    pub api_gaps: Vec<String>,
    /// Plugin-defined fields read from the stub `controller` / `discovery`
    /// objects that the stub does not carry (stub artifacts, not API gaps).
    pub stub_fields: Vec<String>,
    /// Documented host members called, with call counts.
    pub host_calls: BTreeMap<String, u64>,
    /// Console/log lines written.
    pub console_lines: u64,
    /// First log lines (debugging aid).
    pub log_tail: Vec<String>,
    /// What the plugin reported through the `battery` feature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery: Option<crate::runtime::StubBattery>,
    /// Engine heap after the run.
    pub memory_bytes: i64,
    /// Wall time.
    pub ms: u64,
}

fn classify_source(path: &Path) -> FileKind {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    // Unit tests shipped next to plugins (`*.test.js`, CommonJS/jest style) are not plugins.
    if file_name.ends_with(".test.js") {
        return FileKind::Helper;
    }
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lower = text.to_ascii_lowercase();
    if lower.contains("\"smbus\"") || lower.contains("'smbus'") {
        FileKind::Smbus
    } else if text.contains("DiscoveryService") {
        FileKind::Network
    } else if text.contains("ProductId") {
        FileKind::Device
    } else if text.contains("export") {
        FileKind::Helper
    } else {
        FileKind::Unknown
    }
}

fn ms(t: Instant) -> u64 {
    t.elapsed().as_millis() as u64
}

fn relative_name(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn empty_report(file: String, kind: FileKind) -> FileReport {
    FileReport {
        file,
        kind,
        parsed: false,
        evaluated: false,
        ok: false,
        error: None,
        meta: None,
        meta_complete: false,
        meta_errors: BTreeMap::new(),
        meta_warnings: Vec::new(),
        initialize: None,
        shutdown: None,
        api_gaps: Vec::new(),
        stub_fields: Vec::new(),
        host_calls: BTreeMap::new(),
        console_lines: 0,
        log_tail: Vec::new(),
        battery: None,
        memory_bytes: 0,
        ms: 0,
    }
}

fn run_file_inner(root: &Path, path: &Path, opts: &DryRunOptions) -> FileReport {
    let started = Instant::now();
    let mut report = empty_report(relative_name(root, path), FileKind::Unknown);
    let mut ropts = opts.runtime.clone();
    if ropts.plugin_root.is_none() {
        ropts.plugin_root = Some(root.to_path_buf());
    }
    let rt = match PluginRuntime::new(path, &ropts) {
        Ok(rt) => rt,
        Err(e) => {
            report.error = Some(e);
            report.kind = classify_source(path);
            report.ms = ms(started);
            return report;
        }
    };
    let loaded = rt.load(opts.eval_budget);
    report.parsed = loaded.parsed;
    report.error = loaded.error;
    if let Some(x) = loaded.exports {
        report.evaluated = true;
        let m = &x.meta;
        report.kind = match m.plugin_type {
            PluginType::Smbus => FileKind::Smbus,
            PluginType::Network => FileKind::Network,
            _ if x.has("ProductId") => FileKind::Device,
            _ => FileKind::Helper,
        };
        report.meta_complete = m.is_complete();
        report.meta = Some(MetaSummary {
            name: m.name.clone(),
            vid: m.vendor_id,
            pids: m.product_ids.clone(),
            plugin_type: m.plugin_type.as_compat_str().to_string(),
            device_type: format!("{:?}", m.device_type).to_ascii_lowercase(),
            lifecycle: m.lifecycle.present().into_iter().map(str::to_string).collect(),
            change_hooks: m.lifecycle.change_hooks.clone(),
            parameters: m.controllable_parameters.len(),
        });
        report.meta_errors = x.errors.clone();
        report.meta_warnings = x.warnings.clone();
        report.ok = report.kind == FileKind::Helper || report.meta_complete;

        if opts.initialize && report.kind != FileKind::Helper && m.lifecycle.initialize {
            let pid = m.product_ids.first().copied().unwrap_or(0);
            let t = Instant::now();
            let init = rt
                .prepare_device(m.vendor_id, pid, opts.call_budget)
                .and_then(|_| rt.call_export("Initialize", CallArgs::None, opts.call_budget));
            let init = CallOutcome {
                called: init.as_ref().is_ok_and(|c| *c),
                error: init.err(),
                ms: ms(t),
            };
            let t = Instant::now();
            let shut = rt.call_export("Shutdown", CallArgs::Bool(false), opts.call_budget);
            let shut = CallOutcome {
                called: shut.as_ref().is_ok_and(|c| *c),
                error: shut.err(),
                ms: ms(t),
            };
            report.ok = report.ok && init.error.is_none() && shut.error.is_none();
            report.initialize = Some(init);
            report.shutdown = Some(shut);
        }
    } else {
        report.kind = classify_source(path);
    }
    let rec = rt.record();
    report.api_gaps = rec.gaps.keys().cloned().collect();
    report.stub_fields = rec.fields.keys().cloned().collect();
    report.host_calls = rec.calls;
    report.console_lines = rec.console_lines;
    report.log_tail = rec.log_tail;
    report.battery = rec.battery;
    report.memory_bytes = rt.memory_used();
    report.ms = ms(started);
    report
}

/// Load one file in a fresh runtime on its own thread.
pub fn run_file(root: &Path, path: &Path, opts: &DryRunOptions) -> FileReport {
    match run_isolated(|| run_file_inner(root, path, opts)) {
        Ok(r) => r,
        Err(panic) => {
            let msg = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "panic".into());
            let mut report = empty_report(relative_name(root, path), classify_source(path));
            report.error = Some(JsFailure {
                class: "HostPanic".into(),
                message: msg,
            });
            report
        }
    }
}

/// Every `*.js` file under `dir`, sorted (symlinks are not followed).
pub fn collect_js_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let entry = entry?;
            let ft = entry.file_type()?;
            let p = entry.path();
            if ft.is_dir() {
                if entry.file_name() != ".git" {
                    stack.push(p);
                }
            } else if ft.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("js")) {
                out.push(p);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Aggregate numbers of a dry-run.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Summary {
    /// Files visited.
    pub files: usize,
    /// Compiled.
    pub parsed: usize,
    /// Evaluated (namespace read).
    pub evaluated: usize,
    /// Files that are plugins (not helpers/tests), whether or not they evaluated.
    pub plugin_files: usize,
    /// Of those, compiled.
    pub plugin_files_parsed: usize,
    /// Of those, evaluated.
    pub plugin_files_evaluated: usize,
    /// Helper modules (no `ProductId`, not SMBus/network).
    pub helpers: usize,
    /// Evaluated plugins (non-helpers) by kind.
    pub plugins_by_kind: BTreeMap<String, usize>,
    /// Evaluated non-helper plugins.
    pub plugins: usize,
    /// Of those, meta-complete.
    pub meta_complete: usize,
    /// Files that failed to evaluate, by source-text classification.
    pub failed_by_kind: BTreeMap<String, usize>,
    /// Plugins whose `Initialize()` was called.
    pub initialize_called: usize,
    /// … without exception.
    pub initialize_ok: usize,
    /// `Shutdown(false)` without exception (among initialize-called).
    pub shutdown_ok: usize,
    /// Initialize failures by error class.
    pub initialize_error_classes: Vec<(String, usize)>,
    /// API gaps by number of files hitting them (top 30).
    pub top_api_gaps: Vec<(String, usize)>,
    /// Plugin-defined stub fields by number of files reading them (top 30).
    pub top_stub_fields: Vec<(String, usize)>,
    /// Documented host members by number of files calling them (top 30).
    pub top_host_calls: Vec<(String, usize)>,
    /// Error messages by count (top 20), any phase.
    pub top_errors: Vec<(String, usize)>,
    /// Total console lines.
    pub console_lines: u64,
    /// Wall time of the whole run.
    pub ms: u64,
}

fn top(map: HashMap<String, usize>, n: usize) -> Vec<(String, usize)> {
    let mut v: Vec<_> = map.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v.truncate(n);
    v
}

fn short(f: &JsFailure) -> String {
    let mut s = f.to_string();
    if let Some(i) = s.find('\n') {
        s.truncate(i);
    }
    if s.len() > 160 {
        let mut cut = 160;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
        s.push('…');
    }
    s
}

/// Compute the summary of a set of reports.
pub fn summarize(reports: &[FileReport]) -> Summary {
    let mut s = Summary {
        files: reports.len(),
        ..Default::default()
    };
    let mut gaps: HashMap<String, usize> = HashMap::new();
    let mut fields: HashMap<String, usize> = HashMap::new();
    let mut calls: HashMap<String, usize> = HashMap::new();
    let mut errors: HashMap<String, usize> = HashMap::new();
    let mut classes: HashMap<String, usize> = HashMap::new();
    let kind_name = |k: FileKind| {
        serde_json::to_value(k)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    };
    for r in reports {
        s.console_lines += r.console_lines;
        if r.parsed {
            s.parsed += 1;
        }
        if r.kind != FileKind::Helper {
            s.plugin_files += 1;
            s.plugin_files_parsed += usize::from(r.parsed);
            s.plugin_files_evaluated += usize::from(r.evaluated);
        }
        if r.evaluated {
            s.evaluated += 1;
            if r.kind == FileKind::Helper {
                s.helpers += 1;
            } else {
                s.plugins += 1;
                *s.plugins_by_kind.entry(kind_name(r.kind)).or_default() += 1;
                if r.meta_complete {
                    s.meta_complete += 1;
                }
            }
        } else {
            *s.failed_by_kind.entry(kind_name(r.kind)).or_default() += 1;
        }
        if let Some(e) = &r.error {
            *errors.entry(short(e)).or_default() += 1;
        }
        if let Some(i) = &r.initialize {
            if i.called || i.error.is_some() {
                s.initialize_called += 1;
                match &i.error {
                    None => s.initialize_ok += 1,
                    Some(e) => {
                        *classes.entry(e.class.clone()).or_default() += 1;
                        *errors.entry(format!("Initialize: {}", short(e))).or_default() += 1;
                    }
                }
                match &r.shutdown {
                    Some(sd) if sd.error.is_none() => s.shutdown_ok += 1,
                    Some(CallOutcome { error: Some(e), .. }) => {
                        *errors.entry(format!("Shutdown: {}", short(e))).or_default() += 1;
                    }
                    _ => {}
                }
            }
        }
        for g in &r.api_gaps {
            *gaps.entry(g.clone()).or_default() += 1;
        }
        for f in &r.stub_fields {
            *fields.entry(f.clone()).or_default() += 1;
        }
        for c in r.host_calls.keys() {
            *calls.entry(c.clone()).or_default() += 1;
        }
    }
    s.top_api_gaps = top(gaps, 30);
    s.top_stub_fields = top(fields, 30);
    s.top_host_calls = top(calls, 30);
    s.top_errors = top(errors, 20);
    s.initialize_error_classes = top(classes, usize::MAX);
    s
}

/// Dry-run every `*.js` under `dir`; `progress` is called after each file.
pub fn dry_run(
    dir: &Path,
    opts: &DryRunOptions,
    progress: &(dyn Fn(usize, usize, &FileReport) + Sync),
) -> std::io::Result<(Vec<FileReport>, Summary)> {
    let started = Instant::now();
    let root = std::path::absolute(dir)?;
    let files = collect_js_files(&root)?;
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<FileReport>>> = Mutex::new(vec![None; files.len()]);
    let jobs = opts.jobs.clamp(1, 64);
    std::thread::scope(|s| {
        for _ in 0..jobs {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(path) = files.get(i) else { break };
                let r = run_file(&root, path, opts);
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                progress(n, files.len(), &r);
                if let Ok(mut v) = results.lock() {
                    v[i] = Some(r);
                }
            });
        }
    });
    let reports: Vec<FileReport> = results
        .into_inner()
        .unwrap_or_else(|e| e.into_inner())
        .into_iter()
        .flatten()
        .collect();
    let mut summary = summarize(&reports);
    summary.ms = ms(started);
    Ok((reports, summary))
}
