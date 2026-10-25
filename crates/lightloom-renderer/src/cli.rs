// SPDX-License-Identifier: GPL-3.0-or-later
//! Command line of the renderer.

use clap::Parser;
use std::path::PathBuf;

/// Render a lightscript offscreen in Chromium (CEF) at 320x200, publish every painted frame into
/// a memory-mapped shm ring and print paint-interval statistics as JSON.
#[derive(Clone, Debug, Parser)]
#[command(name = "lightloom-renderer", version, about)]
pub struct Args {
    /// Effect HTML file (loaded as a file:// URL).
    #[arg(long, value_name = "PATH")]
    pub effect: PathBuf,

    /// Frames to record before exiting.
    #[arg(long, default_value_t = 300, value_parser = clap::value_parser!(u64).range(2..))]
    pub frames: u64,

    /// Target frame rate (CEF `windowless_frame_rate`; CEF 154 delivered 60 and 120 fps in tests).
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u32).range(1..=240))]
    pub fps: u32,

    /// Shared-memory ring file (created/truncated; lightloom_protocol::shm layout).
    #[arg(long, value_name = "FILE")]
    pub out: PathBuf,

    /// Write PNG snapshot(s) here. With several --snapshot-frames the frame number is inserted
    /// before the extension (`snap-f10.png`, `snap-f100.png`).
    #[arg(long, value_name = "PATH")]
    pub png: Option<PathBuf>,

    /// 1-based frame numbers to snapshot (hash + metrics in the JSON, PNG with --png).
    #[arg(long, value_delimiter = ',', default_value = "10,100")]
    pub snapshot_frames: Vec<u64>,

    /// Software compositing (Chromium `--disable-gpu-compositing`).
    #[arg(long)]
    pub software_compositing: bool,

    /// Disable the GPU process entirely (Chromium `--disable-gpu`; implies software raster).
    #[arg(long)]
    pub disable_gpu: bool,

    /// Extra Chromium switch for the browser process, `name` or `name=value` (repeatable).
    #[arg(long = "chromium-flag", value_name = "SWITCH")]
    pub chromium_flags: Vec<String>,

    /// Also write the JSON report to this file.
    #[arg(long, value_name = "FILE")]
    pub stats: Option<PathBuf>,

    /// Chromium user-data dir (default: `<out>.cef-cache`). It must be missing, empty, or a dir
    /// this tool created (marker file `.lightloom-renderer-cache`); its old contents are wiped at
    /// start. At exit it is emptied as far as Chromium allows (a few profile locks stay until the
    /// process ends; the next run wipes them). Any other dir is refused, untouched.
    #[arg(long, value_name = "DIR")]
    pub cef_cache: Option<PathBuf>,

    /// CEF log file (default: `<out>.cef.log`).
    #[arg(long, value_name = "FILE")]
    pub cef_log: Option<PathBuf>,

    /// Fail when fewer than --frames frames arrive within this multiple of frames/fps seconds
    /// (measured from browser creation). 1..=1000.
    #[arg(long, default_value_t = 3.0, value_parser = parse_timeout_factor)]
    pub timeout_factor: f64,

    /// Let Chromium resolve host names. By default every DNS lookup fails
    /// (`--host-resolver-rules=MAP * ~NOTFOUND`): effects are local files, and without it
    /// Chromium's background services (component updater, optimization guide, network time)
    /// connect to Google hosts from the network service process.
    #[arg(long)]
    pub allow_network: bool,

    /// Do not inject window.engine / control globals (A/B testing).
    #[arg(long)]
    pub no_inject: bool,

    /// Include every paint interval in the JSON report (`intervals_ms`).
    #[arg(long)]
    pub raw_intervals: bool,

    /// Echo page console messages to stderr as they arrive.
    #[arg(long)]
    pub verbose: bool,

    /// Run until told to stop instead of for `--frames` frames: read control commands
    /// (one JSON object per line, see `control.rs`) from the control channel, and exit when it
    /// closes.
    #[arg(long)]
    pub serve: bool,

    /// Control channel for `--serve`: `stdin`, or `unix:PATH`, a unix domain socket the renderer
    /// connects to (Linux; what lightloomd uses there). Same JSON lines either way.
    #[arg(long, value_name = "ADDR", default_value = "stdin")]
    pub control: crate::ipc::ControlAddr,

    /// Control value overriding the effect's default, `NAME=VALUE` (VALUE parsed as JSON,
    /// else taken as a string; repeatable).
    #[arg(long = "prop", value_name = "NAME=VALUE", value_parser = parse_prop)]
    pub props: Vec<(String, serde_json::Value)>,
}

fn parse_prop(s: &str) -> Result<(String, serde_json::Value), String> {
    let (name, value) = s
        .split_once('=')
        .ok_or_else(|| format!("expected NAME=VALUE, got {s:?}"))?;
    let name = name.trim();
    if name.is_empty() {
        return Err(format!("empty property name in {s:?}"));
    }
    let value = serde_json::from_str(value).unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
    Ok((name.to_string(), value))
}

/// Chromium switch that makes every host name lookup fail (see [`Args::allow_network`]).
pub const BLOCK_DNS_SWITCH: &str = "host-resolver-rules=MAP * ~NOTFOUND";

/// Linux default display backend (see [`Args::chromium_switches_for`]).
pub const OZONE_HEADLESS_SWITCH: &str = "ozone-platform=headless";

/// Environment variable replacing the Linux default Ozone platform (`x11`, `wayland`), for a
/// renderer started by lightloomd, whose command line the user does not write.
pub const OZONE_ENV: &str = "LIGHTLOOM_OZONE_PLATFORM";

fn parse_timeout_factor(s: &str) -> Result<f64, String> {
    let v: f64 = s.trim().parse().map_err(|e| format!("{e}"))?;
    if v.is_finite() && (1.0..=1000.0).contains(&v) {
        Ok(v)
    } else {
        Err(format!("{v} is not in 1..=1000"))
    }
}

impl Args {
    /// `--prop` values by name (a later one wins).
    pub fn prop_values(&self) -> serde_json::Map<String, serde_json::Value> {
        self.props.iter().cloned().collect()
    }

    /// Deadline for all frames, from browser creation (at least 1 s; saturates instead of
    /// panicking for absurd `--frames`).
    pub fn deadline(&self) -> std::time::Duration {
        let expected = self.frames as f64 / f64::from(self.fps.max(1));
        let secs = (expected * self.timeout_factor.max(1.0)).max(1.0);
        std::time::Duration::try_from_secs_f64(secs).unwrap_or(std::time::Duration::MAX)
    }

    /// Chromium switches for the browser process (name or name=value).
    pub fn chromium_switches(&self) -> Vec<String> {
        let ozone = std::env::var(OZONE_ENV).ok();
        self.chromium_switches_for(cfg!(target_os = "linux"), ozone.as_deref())
    }

    /// [`Args::chromium_switches`] for a Linux (`linux = true`) or other target.
    ///
    /// On Linux the renderer needs no display: off-screen rendering paints into `OnPaint`
    /// buffers, so Ozone's `headless` platform (software output, no X11 or Wayland connection)
    /// is the default. CEF does not officially support Ozone headless (maintainer, 2020) but it
    /// has been run with OSR since M133 (CEF issue #3902). Where it misbehaves, run under Xvfb
    /// (or on a desktop session) with `--chromium-flag=ozone-platform=x11`, or set
    /// `ozone_env` ([`OZONE_ENV`]) to `x11`/`wayland`. A `--chromium-flag=ozone-platform=...`
    /// wins over the environment, which wins over the default; a value that is not a plain
    /// word is ignored.
    pub fn chromium_switches_for(&self, linux: bool, ozone_env: Option<&str>) -> Vec<String> {
        let mut s: Vec<String> = [
            // keep requestAnimationFrame / timers running at full rate in a never-shown view
            "disable-background-timer-throttling",
            "disable-renderer-backgrounding",
            "disable-backgrounding-occluded-windows",
            // headless-friendly: no audio, no extensions, no first-run / component updates
            "mute-audio",
            "disable-extensions",
            "disable-component-update",
            "no-first-run",
            "no-default-browser-check",
            "disable-default-apps",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        if self.software_compositing {
            s.push("disable-gpu-compositing".into());
        }
        if self.disable_gpu {
            s.push("disable-gpu".into());
            s.push("disable-gpu-compositing".into());
        }
        if !self.allow_network {
            s.push(BLOCK_DNS_SWITCH.into());
        }
        let ozone_given = self
            .chromium_flags
            .iter()
            .any(|f| f == "ozone-platform" || f.starts_with("ozone-platform="));
        if linux && !ozone_given {
            let from_env = ozone_env
                .map(str::trim)
                .filter(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
            s.push(match from_env {
                Some(platform) => format!("ozone-platform={platform}"),
                None => OZONE_HEADLESS_SWITCH.into(),
            });
        }
        s.extend(self.chromium_flags.iter().cloned());
        s.sort();
        s.dedup();
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let a = Args::try_parse_from(["lightloom-renderer", "--effect", "e.html", "--out", "f.shm"]).unwrap();
        assert_eq!(a.frames, 300);
        assert_eq!(a.fps, 30);
        assert_eq!(a.snapshot_frames, vec![10, 100]);
        assert_eq!(a.deadline(), std::time::Duration::from_secs(30));
        let sw = a.chromium_switches();
        assert!(!sw.contains(&"disable-gpu-compositing".to_string()));
        assert!(sw.contains(&BLOCK_DNS_SWITCH.to_string()));
        let open = Args::try_parse_from(["x", "--effect=e", "--out=o", "--allow-network"]).unwrap();
        assert!(!open.chromium_switches().contains(&BLOCK_DNS_SWITCH.to_string()));
    }

    #[test]
    fn serve_and_props_parse() {
        let a = Args::try_parse_from([
            "x",
            "--effect=e",
            "--out=o",
            "--serve",
            "--prop",
            "speed=5",
            "--prop",
            "tint=#ff0000",
            "--prop",
            "speed=6",
        ])
        .unwrap();
        assert!(a.serve);
        let v = a.prop_values();
        assert_eq!(v["speed"], serde_json::json!(6));
        assert_eq!(v["tint"], serde_json::json!("#ff0000"));
        assert!(Args::try_parse_from(["x", "--effect=e", "--out=o", "--prop", "novalue"]).is_err());
        assert!(Args::try_parse_from(["x", "--effect=e", "--out=o", "--prop", "=1"]).is_err());
        assert_eq!(a.control, crate::ipc::ControlAddr::Stdin);
        let s = Args::try_parse_from([
            "x",
            "--effect=e",
            "--out=o",
            "--serve",
            "--control=unix:/tmp/hd/c.sock",
        ])
        .unwrap();
        assert_eq!(s.control, crate::ipc::ControlAddr::Unix("/tmp/hd/c.sock".into()));
        assert!(Args::try_parse_from(["x", "--effect=e", "--out=o", "--control=pipe"]).is_err());
    }

    #[test]
    fn linux_defaults_to_ozone_headless_unless_overridden() {
        let a = Args::try_parse_from(["x", "--effect=e", "--out=o"]).unwrap();
        assert!(a
            .chromium_switches_for(true, None)
            .contains(&OZONE_HEADLESS_SWITCH.to_string()));
        assert!(!a
            .chromium_switches_for(false, Some("x11"))
            .iter()
            .any(|s| s.starts_with("ozone")));
        let ozone = |sw: Vec<String>| {
            sw.into_iter()
                .filter(|s| s.starts_with("ozone-platform"))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ozone(a.chromium_switches_for(true, Some("x11"))),
            vec!["ozone-platform=x11"]
        );
        assert_eq!(
            ozone(a.chromium_switches_for(true, Some("x11 --evil"))),
            vec![OZONE_HEADLESS_SWITCH]
        );
        assert_eq!(
            ozone(a.chromium_switches_for(true, Some(""))),
            vec![OZONE_HEADLESS_SWITCH]
        );
        let x11 = Args::try_parse_from(["x", "--effect=e", "--out=o", "--chromium-flag=ozone-platform=x11"])
            .unwrap();
        let sw = x11.chromium_switches_for(true, Some("wayland"));
        assert_eq!(
            sw.iter()
                .filter(|s| s.starts_with("ozone-platform"))
                .collect::<Vec<_>>(),
            vec!["ozone-platform=x11"]
        );
    }

    #[test]
    fn timeout_factor_is_validated_and_deadline_saturates() {
        for bad in ["0.5", "inf", "NaN", "-3", "1e9", "x"] {
            let r = Args::try_parse_from(["x", "--effect=e", "--out=o", &format!("--timeout-factor={bad}")]);
            assert!(r.is_err(), "{bad} accepted");
        }
        let a = Args::try_parse_from([
            "x",
            "--effect=e",
            "--out=o",
            "--fps=1",
            "--timeout-factor=1000",
            &format!("--frames={}", u64::MAX),
        ])
        .unwrap();
        assert_eq!(a.deadline(), std::time::Duration::MAX);
    }

    #[test]
    fn flags() {
        let a = Args::try_parse_from([
            "lightloom-renderer",
            "--effect=e.html",
            "--out=f.shm",
            "--frames=60",
            "--fps=60",
            "--software-compositing",
            "--chromium-flag=enable-logging=stderr",
            "--snapshot-frames=5",
        ])
        .unwrap();
        let sw = a.chromium_switches();
        assert!(sw.contains(&"disable-gpu-compositing".to_string()));
        assert!(sw.contains(&"enable-logging=stderr".to_string()));
        assert_eq!(a.snapshot_frames, vec![5]);
        assert_eq!(a.deadline(), std::time::Duration::from_secs(3));
        assert!(Args::try_parse_from(["x", "--effect=e", "--out=o", "--frames=1"]).is_err());
    }
}
