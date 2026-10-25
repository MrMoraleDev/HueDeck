// SPDX-License-Identifier: GPL-3.0-or-later
//! The log history behind `GET /v1/logs` (docs/api/openapi.yaml).
//!
//! * [`RingLayer`]: a `tracing` layer keeping this run's last [`RING_LINES`]
//!   lines in memory ([`ring`]), with the plugins' own `device.log` lines
//!   (target `plugin`, at debug level, which the console filter usually hides).
//! * [`LogFiles`]: the files `lightloomw` sends the daemon's output to
//!   (`<data>\logs\lightloomd.log`, the previous run's `lightloomd.old.log`);
//!   it names the current one in [`LOG_FILE_ENV`]. Without it (a console run,
//!   tests) only the memory buffer exists.
//!
//! The API side filters the lines and redacts the token (`lightloom_api::logs`).

use lightloom_api::logs::{parse_lines, source_of, LogBatch, LogEntry};
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

/// Lines the memory buffer keeps.
pub const RING_LINES: usize = 5000;
/// Set by `lightloomw` on the daemon it starts: the file its output goes to.
pub const LOG_FILE_ENV: &str = "LIGHTLOOM_LOG_FILE";
/// The tail of each log file that is read (older text is left out).
pub const FILE_TAIL_BYTES: u64 = 4 << 20;

/// This run's last lines.
#[derive(Debug, Default)]
pub struct LogRing {
    lines: Mutex<VecDeque<LogEntry>>,
}

impl LogRing {
    /// Keep `entry`, dropping the oldest line when full.
    pub fn push(&self, entry: LogEntry) {
        let mut lines = self.lines.lock().unwrap_or_else(|e| e.into_inner());
        if lines.len() == RING_LINES {
            lines.pop_front();
        }
        lines.push_back(entry);
    }

    /// Every line, oldest first.
    pub fn entries(&self) -> Vec<LogEntry> {
        let lines = self.lines.lock().unwrap_or_else(|e| e.into_inner());
        lines.iter().cloned().collect()
    }
}

/// The process-wide buffer [`RingLayer`] fills.
pub fn ring() -> &'static LogRing {
    static RING: OnceLock<LogRing> = OnceLock::new();
    RING.get_or_init(LogRing::default)
}

/// A `tracing` layer that copies every event into [`ring`]. Give it its own
/// filter (see [`ring_filter`]).
#[derive(Debug, Default)]
pub struct RingLayer;

/// What the memory buffer keeps: `info` and above, and the plugins' lines.
pub fn ring_filter() -> tracing_subscriber::filter::Targets {
    tracing_subscriber::filter::Targets::new()
        .with_default(Level::INFO)
        .with_target("plugin", Level::DEBUG)
}

impl<S: Subscriber> Layer<S> for RingLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        ring().push(entry_of(event));
    }
}

/// The message, `device`, and the other fields as `key=value`.
#[derive(Default)]
struct Fields {
    message: String,
    device: Option<String>,
    rest: String,
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "message" => self.message = value.to_string(),
            "device" => self.device = Some(value.to_string()),
            name => {
                let _ = write!(self.rest, " {name}={value:?}");
            }
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        match field.name() {
            "message" => self.message = format!("{value:?}"),
            "device" => self.device = Some(format!("{value:?}").trim_matches('"').to_string()),
            name => {
                let _ = write!(self.rest, " {name}={value:?}");
            }
        }
    }
}

fn entry_of(event: &Event<'_>) -> LogEntry {
    let mut f = Fields::default();
    event.record(&mut f);
    let meta = event.metadata();
    let mut message = f.message;
    message.push_str(&f.rest);
    LogEntry {
        time: Some(utc_rfc3339(SystemTime::now())),
        level: Some(meta.level().as_str().to_string()),
        target: meta.target().to_string(),
        source: f.device.or_else(|| source_of(&message)),
        message,
        file: None,
    }
}

/// `t` as `2026-10-25T10:00:00.123Z`.
pub fn utc_rfc3339(t: SystemTime) -> String {
    let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs();
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem / 60 % 60,
        rem % 60,
        d.subsec_millis()
    )
}

/// The log files of a run under `lightloomw`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogFiles {
    /// This run's file (`lightloomd.log`); `None` when the daemon was not
    /// started by `lightloomw`.
    pub current: Option<PathBuf>,
}

impl LogFiles {
    /// From [`LOG_FILE_ENV`].
    pub fn from_env() -> Self {
        Self {
            current: std::env::var_os(LOG_FILE_ENV)
                .map(PathBuf::from)
                .filter(|p| p.is_file()),
        }
    }

    /// The previous run's file next to `current` (`lightloomd.old.log`).
    fn previous(current: &Path) -> PathBuf {
        current.with_extension("old.log")
    }

    /// What can be served: `file` (when there is a file) and `memory`.
    pub fn available(&self) -> Vec<String> {
        let mut a = Vec::new();
        if self.current.is_some() {
            a.push("file".to_string());
        }
        a.push("memory".to_string());
        a
    }

    /// Every line: from the files (`from` is `file` or `None`, when there
    /// are files), else from `ring`.
    pub fn batch(&self, from: Option<&str>, ring: &LogRing) -> LogBatch {
        let available = self.available();
        match (&self.current, from) {
            (Some(current), None | Some("file")) => {
                let mut files = Vec::new();
                let mut entries = Vec::new();
                for path in [Self::previous(current), current.clone()] {
                    let Some(text) = read_tail(&path, FILE_TAIL_BYTES) else {
                        continue;
                    };
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    entries.extend(parse_lines(&text, Some(&name)));
                    files.push(path.display().to_string());
                }
                LogBatch {
                    from: "file".into(),
                    available,
                    files,
                    entries,
                }
            }
            _ => LogBatch {
                from: "memory".into(),
                available,
                files: Vec::new(),
                entries: ring.entries(),
            },
        }
    }
}

/// The last `max` bytes of `path` as text, from the first whole line.
fn read_tail(path: &Path, max: u64) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let start = len.saturating_sub(max);
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::with_capacity((len - start) as usize);
    f.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if start > 0 {
        // Cut in the middle of a line: skip to the next one.
        return Some(
            text.split_once('\n')
                .map_or(String::new(), |(_, rest)| rest.to_string()),
        );
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::layer::SubscriberExt;

    #[test]
    fn formats_utc_times() {
        let t = UNIX_EPOCH + std::time::Duration::from_millis(1_793_000_000_123);
        assert_eq!(utc_rfc3339(t), "2026-10-26T07:33:20.123Z");
        assert_eq!(utc_rfc3339(UNIX_EPOCH), "1970-01-01T00:00:00.000Z");
        let leap = UNIX_EPOCH + std::time::Duration::from_secs(951_782_400); // 2000-02-29
        assert!(utc_rfc3339(leap).starts_with("2000-02-29T"));
    }

    #[test]
    fn the_ring_keeps_plugin_lines_and_fields() {
        let subscriber = tracing_subscriber::registry().with(RingLayer.with_filter(ring_filter()));
        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!(target: "plugin", device = %"hid:1532:007c", "ring-test Initialized");
            tracing::info!(port = 53000, "ring-test listening");
            tracing::debug!("ring-test hidden");
        });
        let lines: Vec<_> = ring()
            .entries()
            .into_iter()
            .filter(|e| e.message.starts_with("ring-test"))
            .collect();
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines[0].source.as_deref(), Some("hid:1532:007c"));
        assert_eq!(lines[0].level.as_deref(), Some("DEBUG"));
        assert_eq!(lines[0].target, "plugin");
        assert_eq!(lines[1].message, "ring-test listening port=53000");
        assert_eq!(lines[1].source, None);
    }

    #[test]
    fn the_ring_is_bounded() {
        let ring = LogRing::default();
        for i in 0..RING_LINES + 3 {
            ring.push(LogEntry {
                message: i.to_string(),
                ..LogEntry::default()
            });
        }
        let lines = ring.entries();
        assert_eq!(lines.len(), RING_LINES);
        assert_eq!(lines[0].message, "3");
    }

    #[test]
    fn reads_both_files_and_falls_back_to_memory() {
        let dir = std::env::temp_dir().join(format!("lightloom-logs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let current = dir.join("lightloomd.log");
        std::fs::write(
            dir.join("lightloomd.old.log"),
            "2026-10-24T09:00:00Z  INFO lightloomd: old run\n",
        )
        .unwrap();
        std::fs::write(
            &current,
            "2026-10-25T09:00:00Z  WARN lightloom_daemon::engine: new run\n",
        )
        .unwrap();
        let ring = LogRing::default();
        ring.push(LogEntry {
            message: "in memory".into(),
            ..LogEntry::default()
        });
        let files = LogFiles {
            current: Some(current),
        };
        let b = files.batch(None, &ring);
        assert_eq!(b.from, "file");
        assert_eq!(b.available, ["file", "memory"]);
        assert_eq!(b.files.len(), 2);
        let messages: Vec<_> = b.entries.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(messages, ["old run", "new run"]);
        assert_eq!(b.entries[0].file.as_deref(), Some("lightloomd.old.log"));
        let m = files.batch(Some("memory"), &ring);
        assert_eq!((m.from.as_str(), m.entries.len()), ("memory", 1));
        // No lightloomw: memory only, even when the files are asked for.
        let none = LogFiles::default().batch(Some("file"), &ring);
        assert_eq!((none.from.as_str(), none.available.len()), ("memory", 1));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_tail_starts_at_a_whole_line() {
        let dir = std::env::temp_dir().join(format!("lightloom-tail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.log");
        std::fs::write(&path, "first line\nsecond line\nthird\n").unwrap();
        assert_eq!(read_tail(&path, 14).unwrap(), "third\n");
        assert_eq!(
            read_tail(&path, 1000).unwrap(),
            "first line\nsecond line\nthird\n"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
