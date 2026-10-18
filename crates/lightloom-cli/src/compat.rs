// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `lightloom compat …` — SignalRGB-compatibility tooling.
//!
//! `lightloom compat dry-run <dir> [--initialize] [--json out.json] [--jobs N]`
//! loads every `*.js` under `<dir>` into its own QuickJS-NG runtime with the
//! recording stub host (no hardware, no network) and prints a summary.

use clap::Subcommand;
use lightloom_plugin_host::dryrun::{dry_run, DryRunOptions, FileReport, Summary};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

/// `lightloom compat` subcommands.
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
}

/// Run a compat subcommand.
pub fn run(command: CompatCommand) -> anyhow::Result<()> {
    match command {
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
                } else if n % 50 == 0 || n == total {
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
            Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pct_formats() {
        assert_eq!(pct(1, 4), "25.0%");
        assert_eq!(pct(0, 0), "-");
    }
}
