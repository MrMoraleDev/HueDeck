// SPDX-License-Identifier: GPL-3.0-or-later
//! `huedeck compat …` — SignalRGB-compatibility tooling.
//!
//! `huedeck compat dry-run <dir> [--initialize] [--json out.json] [--jobs N]`
//! loads every `*.js` under `<dir>` into its own QuickJS-NG runtime with the
//! recording stub host (no hardware, no network) and prints a summary.
//!
//! `huedeck compat replay <trace.json>` plays a HID trace recorded with
//! `huedeck light <colour> --record trace.json` back to the plugins, without
//! hardware, and reports where they now send something else.

use crate::run::CatalogArgs;
use anyhow::Context;
use clap::Subcommand;
use huedeck_engine::{FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use huedeck_hal::trace::{compare, HidTrace, ReplayHid, Sent, SentKind, TraceRestoreCheck};
use huedeck_hal::HidBackend;
use huedeck_plugin_host::device::Brightness;
use huedeck_plugin_host::dryrun::{dry_run, DryRunOptions, FileReport, Summary};
use huedeck_plugin_host::manager::{stop_sessions, STOP_TIMEOUT};
use huedeck_plugin_host::session::{self, RestoreCheckMode, RestoreProbe, SessionDevice, SessionOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// `huedeck compat` subcommands.
#[derive(Debug, Subcommand)]
pub enum CompatCommand {
    /// Load every plugin in a directory with stubbed host objects and report API gaps.
    DryRun {
        /// Plugin directory (e.g. the user's SignalRGB plugin cache).
        dir: PathBuf,
        /// Also run Initialize() then Shutdown(false) under the harness.
        #[arg(long)]
        initialize: bool,
        /// Write per-file results and the summary as JSON.
        #[arg(long, value_name = "OUT.json")]
        json: Option<PathBuf>,
        /// Worker threads (default: available parallelism).
        #[arg(long)]
        jobs: Option<usize>,
        /// Use the throwing prelude facades for Rust-backed `@SignalRGB/*`
        /// modules instead of recording stubs.
        #[arg(long)]
        facades: bool,
        /// Seconds allowed for parse + evaluation of one file.
        #[arg(long, default_value_t = 10)]
        eval_timeout: u64,
        /// Seconds allowed for each lifecycle call.
        #[arg(long, default_value_t = 5)]
        call_timeout: u64,
        /// Print one line per file.
        #[arg(long, short)]
        verbose: bool,
    },
    /// Play a HID trace (recorded with `huedeck light <colour> --record FILE`)
    /// back to the plugins without hardware, and report where they now send
    /// something else.
    Replay {
        /// Trace file.
        trace: PathBuf,
        #[command(flatten)]
        catalog: CatalogArgs,
        /// Give up waiting for a frame after this many seconds.
        #[arg(long, default_value_t = 30)]
        frame_timeout: u64,
        /// Write what the plugins did during the replay as a trace (JSON).
        #[arg(long, value_name = "FILE")]
        out: Option<PathBuf>,
    },
}

/// Run a compat subcommand; `false` when a replay diverged.
pub fn run(command: CompatCommand) -> anyhow::Result<bool> {
    match command {
        CompatCommand::Replay {
            trace,
            catalog,
            frame_timeout,
            out,
        } => replay(
            &trace,
            &catalog,
            Duration::from_secs(frame_timeout),
            out.as_deref(),
        ),
        CompatCommand::DryRun {
            dir,
            initialize,
            json,
            jobs,
            facades,
            eval_timeout,
            call_timeout,
            verbose,
        } => {
            let mut opts = DryRunOptions {
                initialize,
                eval_budget: Duration::from_secs(eval_timeout),
                call_budget: Duration::from_secs(call_timeout),
                ..DryRunOptions::default()
            };
            if let Some(j) = jobs {
                opts.jobs = j;
            }
            opts.runtime.module_stubs = !facades;
            let progress = |n: usize, total: usize, r: &FileReport| {
                if verbose {
                    eprintln!("[{n}/{total}] {}", file_line(r));
                } else if n.is_multiple_of(50) || n == total {
                    eprint!("\r{n}/{total} files");
                    if n == total {
                        eprintln!();
                    }
                }
            };
            let (reports, summary) = dry_run(&dir, &opts, &progress)?;
            print_summary(&dir, &summary, initialize);
            if let Some(path) = json {
                let doc = serde_json::json!({
                    "dir": dir,
                    "initialize": initialize,
                    "module_stubs": opts.runtime.module_stubs,
                    "summary": summary,
                    "files": reports,
                });
                let mut f = std::fs::File::create(&path)?;
                serde_json::to_writer_pretty(&mut f, &doc)?;
                f.write_all(b"\n")?;
                println!("wrote {}", path.display());
            }
            Ok(true)
        }
    }
}

fn file_line(r: &FileReport) -> String {
    let status = if r.ok { "ok  " } else { "FAIL" };
    let name = r.meta.as_ref().map(|m| m.name.as_str()).unwrap_or("");
    let err = r
        .error
        .as_ref()
        .or_else(|| r.initialize.as_ref().and_then(|i| i.error.as_ref()))
        .map(|e| format!(" — {e}"))
        .unwrap_or_default();
    format!("{status} {:?} {} [{name}]{err}", r.kind, r.file)
}

fn pct(n: usize, d: usize) -> String {
    if d == 0 {
        "-".into()
    } else {
        format!("{:.1}%", 100.0 * n as f64 / d as f64)
    }
}

fn print_summary(dir: &std::path::Path, s: &Summary, initialize: bool) {
    println!("compat dry-run: {}", dir.display());
    println!("  files            {}", s.files);
    println!("  parsed           {} ({})", s.parsed, pct(s.parsed, s.files));
    println!(
        "  evaluated        {} ({})",
        s.evaluated,
        pct(s.evaluated, s.files)
    );
    println!(
        "  plugin files     {} (parsed {} = {}, evaluated {} = {})",
        s.plugin_files,
        s.plugin_files_parsed,
        pct(s.plugin_files_parsed, s.plugin_files),
        s.plugin_files_evaluated,
        pct(s.plugin_files_evaluated, s.plugin_files)
    );
    println!("  helpers          {}", s.helpers);
    let kinds: Vec<String> = s
        .plugins_by_kind
        .iter()
        .map(|(k, v)| format!("{k} {v}"))
        .collect();
    println!("  plugins          {} ({})", s.plugins, kinds.join(", "));
    println!(
        "  meta-complete    {} ({} of plugins)",
        s.meta_complete,
        pct(s.meta_complete, s.plugins)
    );
    if !s.failed_by_kind.is_empty() {
        let kinds: Vec<String> = s.failed_by_kind.iter().map(|(k, v)| format!("{k} {v}")).collect();
        println!("  not evaluated    {}", kinds.join(", "));
    }
    if initialize {
        println!(
            "  initialize-ok    {} / {} called ({})",
            s.initialize_ok,
            s.initialize_called,
            pct(s.initialize_ok, s.initialize_called)
        );
        println!("  shutdown-ok      {}", s.shutdown_ok);
        for (class, n) in &s.initialize_error_classes {
            println!("    {n:>5}  {class}");
        }
    }
    println!("  console lines    {}", s.console_lines);
    println!("  wall time        {} ms", s.ms);
    println!("top API gaps (files):");
    for (g, n) in &s.top_api_gaps {
        println!("  {n:>5}  API_GAP:{g}");
    }
    if !s.top_stub_fields.is_empty() {
        println!("stub fields read (plugin-defined controller/discovery data the stub lacks; not API gaps):");
        for (f, n) in &s.top_stub_fields {
            println!("  {n:>5}  {f}");
        }
    }
    println!("top errors:");
    for (e, n) in &s.top_errors {
        println!("  {n:>5}  {e}");
    }
}

/// `huedeck compat replay`: `true` when every collection received the same packets.
fn replay(
    path: &std::path::Path,
    catalog: &CatalogArgs,
    frame_timeout: Duration,
    out: Option<&std::path::Path>,
) -> anyhow::Result<bool> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let trace = HidTrace::from_json(&text)?;
    let canvas = trace.canvas.as_deref().context(
        "this trace was recorded with a moving pattern; record with `huedeck light <colour> --record FILE`",
    )?;
    let colour = Rgb::from_hex(canvas).with_context(|| format!("bad canvas colour {canvas}"))?;
    let (catalog, _) = catalog.scan()?;
    let replay = Arc::new(ReplayHid::new(&trace));
    let backend: Arc<dyn HidBackend> = replay.clone();
    let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    let frame = huedeck_engine::Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, colour);

    let mut handles = Vec::new();
    let mut wanted = Vec::new();
    for d in &trace.devices {
        if d.sessions > 1 {
            anyhow::bail!(
                "{} ran {} sessions during the recording (a hotplug restart); such a trace cannot be replayed",
                d.key,
                d.sessions
            );
        }
        let endpoints: Vec<_> = d
            .paths
            .iter()
            .filter_map(|p| trace.endpoints.iter().find(|e| &e.path == p).cloned())
            .collect();
        let plugin = catalog
            .lookup_device(d.vid, d.pid, &endpoints)
            .with_context(|| format!("no plugin claims {} (use --plugins DIR or --signalrgb)", d.key))?
            .clone();
        eprintln!(
            "huedeck replay: {} -> {} ({} frames)",
            d.key, plugin.meta.name, d.frames
        );
        let device = SessionDevice {
            smbus: None,
            key: d.key.clone(),
            vid: d.vid,
            pid: d.pid,
            endpoints,
        };
        let mut channel_components = std::collections::BTreeMap::new();
        for (name, array) in &d.channels {
            let (instances, notes) = huedeck_engine::parse_channel(&array.to_string())
                .map_err(anyhow::Error::msg)
                .with_context(|| format!("{}: channel {name} in the trace", d.key))?;
            for note in notes {
                eprintln!("huedeck replay: {}: {name}: {note}", d.key);
            }
            channel_components.insert(name.clone(), instances);
        }
        let opts = SessionOptions {
            parameters: trace.parameters.clone(),
            system_info: trace.system_info.clone().map(Arc::new),
            channel_components,
            brightness: Brightness {
                global: trace.options.brightness,
                layout: 100,
            },
            skip_shutdown: trace.options.skip_shutdown,
            restore_check: match &trace.options.restore_check {
                TraceRestoreCheck::Off => RestoreCheckMode::Off,
                TraceRestoreCheck::Declared => RestoreCheckMode::DeclaredFeatureReports,
                TraceRestoreCheck::Reports(list) => RestoreCheckMode::Reports(
                    list.iter()
                        .map(|&(report_id, len)| RestoreProbe { report_id, len })
                        .collect(),
                ),
            },
            ..SessionOptions::default()
        };
        handles.push(session::spawn(
            device,
            plugin,
            Arc::clone(&backend),
            Arc::clone(&frames),
            opts,
        )?);
        wanted.push(d.frames);
    }

    // Lockstep: publish a frame, wait until every session rendered it, as many
    // times as the recording rendered, so both runs send the same packets.
    // A session that has rendered its recorded frames stops at once, so its
    // Shutdown() comes where it came in the recording.
    let stop_done = |handles: &[session::SessionHandle]| {
        for (h, &w) in handles.iter().zip(&wanted) {
            if h.live().frames >= w {
                h.stop();
            }
        }
    };
    let total = wanted.iter().copied().max().unwrap_or(0);
    let mut stalled = None;
    stop_done(&handles);
    for n in 1..=total {
        frames.publish(frame.clone());
        let deadline = Instant::now() + frame_timeout;
        while handles
            .iter()
            .zip(&wanted)
            .any(|(h, &w)| n <= w && !h.is_finished() && h.live().frames < n)
        {
            if Instant::now() > deadline {
                stalled = Some(n);
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        if stalled.is_some() {
            break;
        }
        stop_done(&handles);
    }
    let (reports, failed) = stop_sessions(handles, STOP_TIMEOUT);
    for f in &failed {
        eprintln!("huedeck replay: session {f}");
    }
    if let Some(n) = stalled {
        eprintln!(
            "huedeck replay: frame {n} was not rendered within {} s",
            frame_timeout.as_secs()
        );
    }
    let replayed = replay.trace();
    if let Some(out) = out {
        std::fs::write(out, serde_json::to_string_pretty(&replayed)?)
            .with_context(|| format!("writing {}", out.display()))?;
    }
    for r in &reports {
        for f in &r.failures {
            eprintln!("huedeck replay: {}: {} failed: {}", r.device, f.phase, f.error);
        }
    }
    let divergences = compare(&trace, &replayed);
    let hex = |p: &Option<Sent>| match p {
        Some(p) => format!(
            "{} {}",
            match p.kind {
                SentKind::Output => "output",
                SentKind::Feature => "feature",
            },
            p.bytes.iter().map(|x| format!("{x:02X}")).collect::<String>()
        ),
        None => "(nothing)".into(),
    };
    for d in &trace.devices {
        for p in &d.paths {
            let (a, b) = (trace.sent_to(p).len(), replayed.sent_to(p).len());
            if a + b > 0 {
                println!("{}  {p}: recorded {a} packets, replayed {b}", d.key);
            }
        }
    }
    if divergences.is_empty() {
        println!("replay matches the recording");
    }
    for d in &divergences {
        println!("DIVERGES at packet #{} on {}", d.index, d.path);
        println!("  recorded: {}", hex(&d.recorded));
        println!("  replayed: {}", hex(&d.replayed));
    }
    Ok(divergences.is_empty() && failed.is_empty() && stalled.is_none())
}

#[cfg(test)]
mod tests {
    use super::*;
    use huedeck_engine::Frame;
    use huedeck_hal::trace::RecordingHid;
    use huedeck_plugin_host::catalog::Catalog;

    #[test]
    fn a_solid_colour_recording_replays_exactly() {
        let plugins = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
        let plugin = Catalog::scan(std::slice::from_ref(&plugins), Duration::from_secs(10))
            .lookup(0x1462, 0x7C91)
            .expect("first-party MSI plugin")
            .clone();
        let (mock, device) = crate::run::permissive_mock(0x1462, 0x7C91).unwrap();
        let rec = Arc::new(RecordingHid::new(mock, "unit test"));
        rec.set_conditions(Some("#ff0000".into()), serde_json::Map::new());
        rec.note_device(&device.key, device.vid, device.pid, &device.endpoints);
        let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
        let backend: Arc<dyn HidBackend> = rec.clone();
        let handle = session::spawn(
            device.clone(),
            plugin,
            backend,
            Arc::clone(&frames),
            SessionOptions::default(),
        )
        .unwrap();
        for n in 1..=4 {
            frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, Rgb::new(255, 0, 0)));
            let deadline = Instant::now() + Duration::from_secs(20);
            while handle.live().frames < n {
                assert!(Instant::now() < deadline, "frame {n}");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        let (reports, failed) = stop_sessions(vec![handle], STOP_TIMEOUT);
        assert!(failed.is_empty() && reports[0].is_clean(), "{reports:#?}");
        rec.add_session(&device.key, reports[0].frames);
        let trace = rec.trace();
        assert!(trace.events.iter().any(|e| e.op.sent().is_some()));
        let path = std::env::temp_dir().join(format!("huedeck-replay-{}.json", std::process::id()));
        std::fs::write(&path, serde_json::to_string(&trace).unwrap()).unwrap();
        let catalog = CatalogArgs {
            plugin_dirs: vec![plugins],
            signalrgb: false,
            no_plugin_cache: true,
        };
        let clean = replay(&path, &catalog, Duration::from_secs(20), None);
        // A trace of a moving pattern is refused.
        let mut moving = trace.clone();
        moving.canvas = None;
        std::fs::write(&path, serde_json::to_string(&moving).unwrap()).unwrap();
        let refused = replay(&path, &catalog, Duration::from_secs(20), None);
        std::fs::remove_file(&path).ok();
        assert!(clean.unwrap(), "the replay diverged");
        assert!(refused.unwrap_err().to_string().contains("moving pattern"));
    }

    #[test]
    fn pct_formats() {
        assert_eq!(pct(1, 4), "25.0%");
        assert_eq!(pct(0, 0), "-");
    }
}
