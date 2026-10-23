// SPDX-License-Identifier: GPL-3.0-or-later
//! Which port the native API and the dashboard listen on at startup.
//!
//! Precedence (first that is set wins):
//!
//! 1. `lightloomd --port` (this run only, never saved);
//! 2. the Port setting saved from the dashboard (database key
//!    [`crate::engine::PORT_KEY`]), like the SMBus setting;
//! 3. `config.toml` `[api] port`;
//! 4. [`lightloom_api::NATIVE_PORT`] (53000).
//!
//! When the chosen port is busy at startup the daemon never picks a random
//! one. If the port came from the dashboard or `config.toml` (not from
//! `--port`), nothing that answers like Lightloom listens there, and the default
//! port is free, it serves on the default port instead, logs an error naming
//! the busy port and where it came from, and shows the same notice in the
//! dashboard settings, so the dashboard stays reachable to pick another port.
//! Otherwise startup fails with a message naming the port, the setting and
//! `config.toml`.

use anyhow::bail;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::net::TcpListener;

/// Where the port came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortSource {
    /// `lightloomd --port`.
    CommandLine,
    /// Settings → Port in the dashboard (saved in the database).
    Dashboard,
    /// `[api] port` in this `config.toml`.
    Config(PathBuf),
    /// [`lightloom_api::NATIVE_PORT`].
    Default,
}

impl PortSource {
    /// For log lines and errors.
    pub fn describe(&self) -> String {
        match self {
            PortSource::CommandLine => "lightloomd --port".into(),
            PortSource::Dashboard => {
                "the Port setting saved from the dashboard (Settings → Port; it overrides [api] port in config.toml)"
                    .into()
            }
            PortSource::Config(path) => format!("[api] port in {}", path.display()),
            PortSource::Default => format!("the default port {}", lightloom_api::NATIVE_PORT),
        }
    }
}

/// The port to listen on and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortChoice {
    /// The port.
    pub port: u16,
    /// Where it came from.
    pub source: PortSource,
    /// Settings that were ignored (a saved port out of range).
    pub warnings: Vec<String>,
}

/// Apply the precedence above. A `config.toml` or `--port` value out of range
/// is an error (the user typed it); a saved one is ignored with a warning.
pub fn choose(
    command_line: Option<u16>,
    saved: Option<u16>,
    config_port: Option<u16>,
    config_file: &Path,
) -> anyhow::Result<PortChoice> {
    let mut warnings = Vec::new();
    if let Some(p) = command_line {
        return match lightloom_api::check_native_port(u32::from(p)) {
            Ok(port) => Ok(PortChoice {
                port,
                source: PortSource::CommandLine,
                warnings,
            }),
            Err(e) => bail!("--port {p}: {e}"),
        };
    }
    if let Some(p) = saved {
        match lightloom_api::check_native_port(u32::from(p)) {
            Ok(port) => {
                return Ok(PortChoice {
                    port,
                    source: PortSource::Dashboard,
                    warnings,
                })
            }
            Err(e) => warnings.push(format!("ignoring the port saved from the dashboard: {e}")),
        }
    }
    if let Some(p) = config_port {
        return match lightloom_api::check_native_port(u32::from(p)) {
            Ok(port) => Ok(PortChoice {
                port,
                source: PortSource::Config(config_file.to_path_buf()),
                warnings,
            }),
            Err(e) => bail!("[api] port = {p} in {}: {e}", config_file.display()),
        };
    }
    Ok(PortChoice {
        port: lightloom_api::NATIVE_PORT,
        source: PortSource::Default,
        warnings,
    })
}

/// The port saved from the dashboard in the database at `db`, if any (the
/// database is not created when it does not exist yet).
pub fn saved_port(db: &Path) -> Option<u16> {
    if !db.is_file() {
        return None;
    }
    match crate::store::Store::open(db) {
        Ok(store) => store.get::<u16>(crate::engine::PORT_KEY).ok().flatten(),
        Err(e) => {
            tracing::warn!("cannot read the saved port from {}: {e:#}", db.display());
            None
        }
    }
}

/// Bind the native listener for `choice`, falling back to `default` as the
/// module documentation says. Returns the listener, its port and the notice
/// for the dashboard when it is not the chosen port.
pub async fn bind_at_startup(
    choice: &PortChoice,
    default: u16,
    config_file: &Path,
) -> anyhow::Result<(TcpListener, u16, Option<String>)> {
    let err = match lightloom_api::listener::bind_loopback(choice.port).await {
        Ok(l) => return Ok((l, choice.port, None)),
        Err(e) => e,
    };
    let what = format!(
        "cannot listen on 127.0.0.1:{} ({err}); that port comes from {}",
        choice.port,
        choice.source.describe()
    );
    let how = format!(
        "To fix it, free the port or choose another one: Settings → Port in the dashboard, [api] port in {} \
         (used when no port is saved from the dashboard), or lightloomd --port <PORT> for one run",
        config_file.display()
    );
    let may_fall_back = choice.port != default && choice.source != PortSource::CommandLine;
    if !may_fall_back {
        bail!("{what}. Is another Lightloom running? {how}");
    }
    if answers_like_lightloom(choice.port).await {
        bail!("{what}, and another Lightloom answers there: stop it first. {how}");
    }
    match lightloom_api::listener::bind_loopback(default).await {
        Ok(l) => {
            let notice = format!(
                "Port {} ({}) was in use when Lightloom started, so the dashboard and the API use the default \
                 port {default} for now. Free that port and restart Lightloom, or choose another port here.",
                choice.port,
                match choice.source {
                    PortSource::Dashboard => "saved here".to_string(),
                    _ => choice.source.describe(),
                }
            );
            tracing::error!("{what}; serving on the default port {default} instead. {how}");
            Ok((l, default, Some(notice)))
        }
        Err(e) => bail!("{what}; the default port {default} is not free either ({e}). {how}"),
    }
}

/// Whether a Lightloom daemon answers `GET /v1/health` on `port`.
async fn answers_like_lightloom(port: u16) -> bool {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let probe = async {
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.ok()?;
        let req = format!("GET /v1/health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
        s.write_all(req.as_bytes()).await.ok()?;
        let mut buf = Vec::new();
        s.read_to_end(&mut buf).await.ok()?;
        Some(String::from_utf8_lossy(&buf).into_owned())
    };
    match tokio::time::timeout(Duration::from_millis(1500), probe).await {
        Ok(Some(text)) => text.contains("\"status\":\"ok\"") && text.contains("\"version\""),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> PathBuf {
        PathBuf::from("C:/x/config.toml")
    }

    #[test]
    fn the_dashboard_wins_over_config_toml_which_wins_over_the_default() {
        let c = choose(None, None, None, &cfg()).unwrap();
        assert_eq!((c.port, c.source), (53000, PortSource::Default));
        let c = choose(None, None, Some(17000), &cfg()).unwrap();
        assert_eq!((c.port, c.source), (17000, PortSource::Config(cfg())));
        let c = choose(None, Some(18000), Some(17000), &cfg()).unwrap();
        assert_eq!((c.port, c.source), (18000, PortSource::Dashboard));
        let c = choose(Some(19000), Some(18000), Some(17000), &cfg()).unwrap();
        assert_eq!((c.port, c.source), (19000, PortSource::CommandLine));
        // A bad saved value is skipped with a warning; bad typed values are errors.
        let c = choose(None, Some(80), Some(17000), &cfg()).unwrap();
        assert_eq!((c.port, c.warnings.len()), (17000, 1));
        let e = choose(None, None, Some(80), &cfg()).unwrap_err().to_string();
        assert!(e.contains("[api] port = 80") && e.contains("config.toml"), "{e}");
        assert!(choose(Some(16038), None, None, &cfg()).is_err());
    }

    #[test]
    fn the_saved_port_is_read_from_the_database() {
        let dir = std::env::temp_dir().join(format!("lightloom-port-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = dir.join("state.sqlite");
        assert_eq!(saved_port(&db), None);
        assert!(!db.exists(), "not created just to look");
        let store = crate::store::Store::open(&db).unwrap();
        assert_eq!(saved_port(&db), None);
        store.set(crate::engine::PORT_KEY, &17123u16).unwrap();
        assert_eq!(saved_port(&db), Some(17123));
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    #[tokio::test]
    async fn a_busy_port_at_startup_falls_back_to_a_free_default_or_fails_clearly() {
        let busy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let busy_port = busy.local_addr().unwrap().port();
        let chosen = |source| PortChoice {
            port: busy_port,
            source,
            warnings: Vec::new(),
        };

        // Free chosen port: used as is.
        let free = free_port();
        let ok = PortChoice {
            port: free,
            source: PortSource::Dashboard,
            warnings: Vec::new(),
        };
        let (l, port, notice) = bind_at_startup(&ok, free_port(), &cfg()).await.unwrap();
        assert_eq!((port, notice), (free, None));
        drop(l);

        // Busy saved port, free default: the default, with a notice.
        let default = free_port();
        let (_l, port, notice) = bind_at_startup(&chosen(PortSource::Dashboard), default, &cfg())
            .await
            .unwrap();
        assert_eq!(port, default);
        assert!(notice.unwrap().contains(&busy_port.to_string()));

        // Busy default too: an error naming the port, the setting and config.toml.
        let e = bind_at_startup(&chosen(PortSource::Dashboard), busy_port, &cfg())
            .await
            .unwrap_err()
            .to_string();
        assert!(
            e.contains(&format!("127.0.0.1:{busy_port}"))
                && e.contains("Settings → Port")
                && e.contains("config.toml"),
            "{e}"
        );
        // `--port` never falls back.
        let e = bind_at_startup(&chosen(PortSource::CommandLine), free_port(), &cfg())
            .await
            .unwrap_err()
            .to_string();
        assert!(e.contains("lightloomd --port"), "{e}");
        drop(busy);
    }
}
