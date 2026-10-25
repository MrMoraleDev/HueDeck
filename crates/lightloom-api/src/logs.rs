// SPDX-License-Identifier: GPL-3.0-or-later
//! `GET /v1/logs`: the daemon's log history, filtered.
//!
//! The daemon hands over every line it has ([`LogBatch`]: the log files
//! `lightloomw` writes, or this run's in-memory buffer); this module parses
//! file lines ([`parse_lines`]), filters by level, text and device/plugin
//! ([`LogQuery`]), keeps the newest `limit` lines and redacts secrets
//! ([`redact`]) before anything leaves the API.

use serde::{Deserialize, Serialize};

/// Default number of lines answered.
pub const DEFAULT_LIMIT: usize = 1000;
/// Most lines one answer carries.
pub const MAX_LIMIT: usize = 10_000;
/// What replaces a secret in a line.
pub const REDACTED: &str = "[redacted]";

/// One log line.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    /// UTC time (RFC 3339), when the line has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    /// `ERROR`, `WARN`, `INFO`, `DEBUG` or `TRACE`; `None` for lines without
    /// a level (a panic message on the error stream, say).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    /// The module (tracing target) that logged it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target: String,
    /// The device or plugin the line is about, when it names one (a plugin's
    /// own `device.log`, `[key] ...`, `device=key`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The text (with its `key=value` fields).
    pub message: String,
    /// The log file it came from (`from: file`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

/// Everything the daemon has, before filtering.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogBatch {
    /// `file` or `memory`.
    pub from: String,
    /// What the daemon can serve (`memory` always; `file` when it runs under `lightloomw`).
    pub available: Vec<String>,
    /// The files read, oldest first (`file`).
    pub files: Vec<String>,
    /// Every line, oldest first.
    pub entries: Vec<LogEntry>,
}

/// The query string of `GET /v1/logs`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogQuery {
    /// `file` or `memory`; default: the files when there are any.
    pub from: Option<String>,
    /// Lowest level shown (`error`, `warn`, `info`, `debug`, `trace`).
    pub level: Option<String>,
    /// Text the line must contain (case-insensitive).
    pub q: Option<String>,
    /// Device or plugin (case-insensitive part of its name or key).
    pub source: Option<String>,
    /// Newest lines answered (default [`DEFAULT_LIMIT`], at most [`MAX_LIMIT`]).
    pub limit: Option<usize>,
}

/// `GET /v1/logs`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogsView {
    /// Where the lines come from: `file` (`lightloomd.log` and `lightloomd.old.log`
    /// in the data directory's `logs`) or `memory` (this run only).
    pub from: String,
    /// What can be asked for with `from`.
    pub available: Vec<String>,
    /// The files read, oldest first.
    pub files: Vec<String>,
    /// The matching lines, oldest first.
    pub entries: Vec<LogEntry>,
    /// More lines matched than `limit`; the oldest were left out.
    pub truncated: bool,
    /// Every device and plugin the lines name (for a filter), sorted.
    pub sources: Vec<String>,
}

/// Severity rank: 0 is `ERROR`.
fn rank(level: &str) -> Option<u8> {
    match level.trim().to_ascii_uppercase().as_str() {
        "ERROR" => Some(0),
        "WARN" | "WARNING" => Some(1),
        "INFO" => Some(2),
        "DEBUG" => Some(3),
        "TRACE" => Some(4),
        _ => None,
    }
}

impl LogQuery {
    /// Check the parameters; the message says what is wrong.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(l) = self.level.as_deref().filter(|l| !l.is_empty()) {
            if rank(l).is_none() {
                return Err(format!("unknown level {l:?} (error, warn, info, debug, trace)"));
            }
        }
        if let Some(f) = self.from.as_deref().filter(|f| !f.is_empty()) {
            if f != "file" && f != "memory" {
                return Err(format!("unknown log source {f:?} (file, memory)"));
            }
        }
        Ok(())
    }

    /// Whether `e` passes the level, text and source filters.
    pub fn matches(&self, e: &LogEntry) -> bool {
        if let Some(min) = self.level.as_deref().and_then(rank) {
            // Lines without a level (continuations, panics) always pass.
            if e.level.as_deref().and_then(rank).is_some_and(|r| r > min) {
                return false;
            }
        }
        if let Some(q) = self.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
            let q = q.to_lowercase();
            let hit = e.message.to_lowercase().contains(&q)
                || e.target.to_lowercase().contains(&q)
                || e.source.as_deref().is_some_and(|s| s.to_lowercase().contains(&q));
            if !hit {
                return false;
            }
        }
        if let Some(s) = self.source.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            let s = s.to_lowercase();
            let hit = match &e.source {
                Some(src) => src.to_lowercase().contains(&s),
                // Engine lines name devices in their text ("<device>: plugin faulted").
                None => e.message.to_lowercase().contains(&s),
            };
            if !hit {
                return false;
            }
        }
        true
    }

    /// Filter `batch`, keep the newest `limit` lines and redact `secrets`.
    pub fn apply(&self, batch: LogBatch, secrets: &[&str]) -> LogsView {
        let limit = self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let mut sources: Vec<String> = batch.entries.iter().filter_map(|e| e.source.clone()).collect();
        sources.sort_unstable_by_key(|s| s.to_lowercase());
        sources.dedup();
        let mut entries: Vec<LogEntry> = batch.entries.into_iter().filter(|e| self.matches(e)).collect();
        let truncated = entries.len() > limit;
        if truncated {
            entries.drain(..entries.len() - limit);
        }
        for e in &mut entries {
            e.message = redact(&e.message, secrets);
            e.target = redact(&e.target, secrets);
            e.source = e.source.as_deref().map(|s| redact(s, secrets));
        }
        LogsView {
            from: batch.from,
            available: batch.available,
            files: batch.files,
            entries,
            truncated,
            sources: sources.iter().map(|s| redact(s, secrets)).collect(),
        }
    }
}

/// `text` without the `secrets` (the API token) and without anything sent as
/// a bearer credential or a `token=` value.
pub fn redact(text: &str, secrets: &[&str]) -> String {
    let mut out = text.to_string();
    for s in secrets.iter().map(|s| s.trim()).filter(|s| s.len() >= 8) {
        if out.contains(s) {
            out = out.replace(s, REDACTED);
        }
    }
    for marker in [
        "Bearer ",
        "bearer ",
        "token=",
        "Token=",
        "token: ",
        "\"token\":\"",
    ] {
        out = redact_after(&out, marker);
    }
    out
}

/// Replace the word that follows each `marker`.
fn redact_after(text: &str, marker: &str) -> String {
    if !text.contains(marker) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(marker) {
        let after = &rest[i + marker.len()..];
        out.push_str(&rest[..i + marker.len()]);
        let end = after
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ';' | '&' | ')' | '}'))
            .unwrap_or(after.len());
        if end > 0 && &after[..end] != REDACTED {
            out.push_str(REDACTED);
        } else {
            out.push_str(&after[..end]);
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// `text` without ANSI escape sequences (colours of a terminal log).
pub fn strip_ansi(text: &str) -> String {
    if !text.contains('\u{1b}') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        if chars.peek() == Some(&'[') {
            chars.next();
            // CSI: parameters, then one final byte in '@'..='~'.
            for d in chars.by_ref() {
                if ('@'..='~').contains(&d) {
                    break;
                }
            }
        } else {
            chars.next();
        }
    }
    out
}

/// The device or plugin a message names: a leading `[key]` (plugin lines the
/// engine forwards) or a `device=key` field.
pub fn source_of(message: &str) -> Option<String> {
    if let Some(rest) = message.strip_prefix('[') {
        if let Some(end) = rest.find("] ") {
            let key = &rest[..end];
            if !key.is_empty() && !key.contains('[') {
                return Some(key.to_string());
            }
        }
    }
    let i = message.find("device=")?;
    if i > 0 && !message[..i].ends_with(' ') {
        return None;
    }
    let v = &message[i + "device=".len()..];
    let v = v.strip_prefix('"').map_or_else(
        || v.split_whitespace().next().unwrap_or(""),
        |q| q.split('"').next().unwrap_or(""),
    );
    (!v.is_empty()).then(|| v.to_string())
}

/// Parse one line of `tracing_subscriber::fmt`'s default format
/// (`2026-10-25T10:00:00.123456Z  INFO lightloom_daemon::engine: text`).
/// `None` when it is not one (a continuation line).
pub fn parse_line(line: &str) -> Option<LogEntry> {
    let line = strip_ansi(line);
    let line = line.trim_end();
    let (time, rest) = line.split_once(' ')?;
    let looks_like_time = time.len() >= 19
        && time.as_bytes()[0].is_ascii_digit()
        && time.as_bytes()[4] == b'-'
        && time.contains('T');
    if !looks_like_time {
        return None;
    }
    let rest = rest.trim_start();
    let (level, rest) = rest.split_once(' ').unwrap_or((rest, ""));
    rank(level)?;
    let rest = rest.trim_start();
    // `target: message`; spans would come first (`span{..}: target: message`).
    let (target, message) = match rest.split_once(": ") {
        Some((t, m)) if !t.is_empty() && !t.contains(' ') => (t.to_string(), m.to_string()),
        _ => (String::new(), rest.to_string()),
    };
    Some(LogEntry {
        time: Some(time.to_string()),
        level: Some(level.to_ascii_uppercase()),
        target,
        source: source_of(&message),
        message,
        file: None,
    })
}

/// Parse a log file's text; lines that are not log lines (a panic, a
/// backtrace) join the line before, or stand alone without a level.
pub fn parse_lines(text: &str, file: Option<&str>) -> Vec<LogEntry> {
    let mut out: Vec<LogEntry> = Vec::new();
    for raw in text.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        match parse_line(raw) {
            Some(mut e) => {
                e.file = file.map(str::to_string);
                out.push(e);
            }
            None => {
                let text = strip_ansi(raw.trim_end());
                match out.last_mut() {
                    // Continuations of a line (indented fields, backtraces).
                    Some(last) if raw.starts_with([' ', '\t']) => {
                        last.message.push('\n');
                        last.message.push_str(&text);
                    }
                    _ => out.push(LogEntry {
                        message: text,
                        file: file.map(str::to_string),
                        ..LogEntry::default()
                    }),
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(level: &str, source: Option<&str>, message: &str) -> LogEntry {
        LogEntry {
            time: Some("2026-10-25T10:00:00Z".into()),
            level: Some(level.into()),
            target: "lightloom_daemon::engine".into(),
            source: source.map(str::to_string),
            message: message.into(),
            file: None,
        }
    }

    #[test]
    fn parses_the_fmt_format_with_and_without_colours() {
        let e = parse_line(
            "2026-10-25T10:00:00.123456Z  INFO lightloom_daemon::engine: SMBus: 2 device(s) found",
        )
        .unwrap();
        assert_eq!(e.level.as_deref(), Some("INFO"));
        assert_eq!(e.target, "lightloom_daemon::engine");
        assert_eq!(e.message, "SMBus: 2 device(s) found");
        assert_eq!(e.time.as_deref(), Some("2026-10-25T10:00:00.123456Z"));

        let coloured = "\u{1b}[2m2026-10-25T10:00:01.5Z\u{1b}[0m \u{1b}[33m WARN\u{1b}[0m \
                        \u{1b}[2mplugin\u{1b}[0m\u{1b}[2m:\u{1b}[0m hello \u{1b}[3mdevice\u{1b}[0m\u{1b}[2m=\u{1b}[0mhid:1462:7c91";
        let e = parse_line(coloured).unwrap();
        assert_eq!(e.level.as_deref(), Some("WARN"));
        assert_eq!(e.target, "plugin");
        assert_eq!(e.source.as_deref(), Some("hid:1462:7c91"));
        assert!(parse_line("thread 'main' panicked at src/main.rs:1").is_none());
    }

    #[test]
    fn continuation_lines_join_the_line_before() {
        let text = "2026-10-25T10:00:00Z ERROR lightloomd: failed\n   0: backtrace frame\n\
                    thread 'x' panicked\n";
        let lines = parse_lines(text, Some("lightloomd.log"));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].message, "failed\n   0: backtrace frame");
        assert_eq!(lines[1].level, None);
        assert_eq!(lines[1].file.as_deref(), Some("lightloomd.log"));
    }

    #[test]
    fn sources_come_from_brackets_or_the_device_field() {
        assert_eq!(
            source_of("[hid:1532:007c] Initialized").as_deref(),
            Some("hid:1532:007c")
        );
        assert_eq!(
            source_of("line device=smbus:0x50 x=1").as_deref(),
            Some("smbus:0x50")
        );
        assert_eq!(source_of("line device=\"My Pad\"").as_deref(), Some("My Pad"));
        assert_eq!(source_of("no source here"), None);
        assert_eq!(source_of("subdevice=x"), None);
    }

    #[test]
    fn filters_by_level_text_and_source() {
        let batch = LogBatch {
            from: "memory".into(),
            available: vec!["memory".into()],
            files: vec![],
            entries: vec![
                entry("ERROR", None, "Razer DeathAdder: plugin faulted"),
                entry("INFO", Some("hid:1532:007c"), "Initialize"),
                entry("DEBUG", Some("hid:1462:7c91"), "sent 64 bytes"),
                entry("WARN", None, "renderer slow"),
            ],
        };
        let q = |level: Option<&str>, text: Option<&str>, source: Option<&str>| LogQuery {
            level: level.map(Into::into),
            q: text.map(Into::into),
            source: source.map(Into::into),
            ..LogQuery::default()
        };
        let v = q(Some("warn"), None, None).apply(batch.clone(), &[]);
        assert_eq!(v.entries.len(), 2);
        assert_eq!(v.sources, vec!["hid:1462:7c91", "hid:1532:007c"]);
        assert_eq!(
            q(None, Some("BYTES"), None)
                .apply(batch.clone(), &[])
                .entries
                .len(),
            1
        );
        assert_eq!(
            q(None, None, Some("1532"))
                .apply(batch.clone(), &[])
                .entries
                .len(),
            1
        );
        // A device named in the text of an engine line.
        assert_eq!(
            q(None, None, Some("deathadder"))
                .apply(batch.clone(), &[])
                .entries
                .len(),
            1
        );
        let v = LogQuery {
            limit: Some(1),
            ..LogQuery::default()
        }
        .apply(batch, &[]);
        assert!(v.truncated);
        assert_eq!(v.entries[0].message, "renderer slow", "the newest line stays");
    }

    #[test]
    fn rejects_unknown_levels_and_sources() {
        let bad = LogQuery {
            level: Some("loud".into()),
            ..LogQuery::default()
        };
        assert!(bad.validate().is_err());
        let bad = LogQuery {
            from: Some("disk".into()),
            ..LogQuery::default()
        };
        assert!(bad.validate().is_err());
        assert!(LogQuery::default().validate().is_ok());
    }

    #[test]
    fn the_token_never_leaves() {
        let token = "4f1c2a9e0b7d4e21a3c5";
        let text = format!("sent Authorization: Bearer {token} to 127.0.0.1 (token={token}) {token}");
        let out = redact(&text, &[token]);
        assert!(!out.contains(token), "{out}");
        assert_eq!(
            out,
            "sent Authorization: Bearer [redacted] to 127.0.0.1 (token=[redacted]) [redacted]"
        );
        // Any bearer credential, even one that is not ours.
        assert_eq!(redact("Bearer abc.def", &[]), "Bearer [redacted]");
        assert_eq!(redact("nothing secret", &[token]), "nothing secret");
        let v = LogQuery::default().apply(
            LogBatch {
                entries: vec![entry("INFO", Some(token), &format!("t {token}"))],
                ..LogBatch::default()
            },
            &[token],
        );
        assert_eq!(v.entries[0].message, "t [redacted]");
        assert_eq!(v.entries[0].source.as_deref(), Some(REDACTED));
        assert_eq!(v.sources, vec![REDACTED]);
    }
}
