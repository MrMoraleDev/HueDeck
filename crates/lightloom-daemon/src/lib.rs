// SPDX-License-Identifier: GPL-3.0-or-later
//! `lightloomd`, the per-user Lightloom daemon, as a library: the binary in
//! `main.rs` parses arguments and calls [`run`]; tests drive the parts.
//!
//! [`run`] takes the single-instance lock, loads the API token, picks the
//! native port ([`port`]), starts the [`engine`] thread (devices, renderer,
//! database) and serves the local API on the loopback interface (writing the
//! port to `api-port` in the data directory for `lightloom`) until Ctrl-C (or console close / log-off /
//! shutdown on Windows), then stops every device session so plugins restore
//! their devices.

#![forbid(unsafe_code)]

pub mod battery;
pub mod config;
pub mod engine;
pub mod instance;
pub mod lifecycle;
pub mod paths;
pub mod port;
pub mod store;

use lightloom_api::ApiContext;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::watch;

/// How a [`run`] goes.
pub struct RunOptions {
    /// Directories.
    pub paths: paths::Paths,
    /// `config.toml` (already read).
    pub config: config::Config,
    /// Never write to devices.
    pub dry_run: bool,
    /// Which devices.
    pub hardware: engine::HardwareMode,
    /// Keep the database in memory.
    pub ephemeral: bool,
    /// Open the dashboard in the default browser once listening.
    pub open_dashboard: bool,
    /// `config.toml` (for messages about `[api] port`).
    pub config_file: PathBuf,
    /// `--port`: the native port for this run only (wins over every setting).
    pub port: Option<u16>,
}

/// How often a compatible listener whose port is taken (SignalRGB running) is tried again.
const COMPAT_RETRY: Duration = Duration::from_secs(30);

/// Run the daemon until asked to stop.
pub fn run(opts: RunOptions) -> anyhow::Result<()> {
    let _lock = instance::acquire(&opts.paths.lock_file())?;
    let token: Arc<str> = Arc::from(instance::load_or_create_token(&opts.paths.token_file())?);
    let ui_dir: Option<PathBuf> = opts
        .config
        .ui
        .dir
        .clone()
        .filter(|d| d.is_dir())
        .or_else(|| paths::resource_dir("ui", "ui/dist"));
    let api = opts.config.api.clone();
    let mut engine_opts = engine::locate(
        opts.paths.clone(),
        opts.config.clone(),
        opts.dry_run,
        opts.hardware,
    );
    engine_opts.ephemeral = opts.ephemeral;
    tracing::info!(
        data = %opts.paths.data.display(),
        config = %opts.paths.config.display(),
        renderer = ?engine_opts.renderer,
        ui = ?ui_dir,
        "starting"
    );

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("lightloom-api")
        .build()?;
    // The native port: --port, else the one saved from the dashboard, else
    // config.toml, else the default (see `port`). Bind before starting the
    // engine: a second daemon must not drive the devices when it cannot serve.
    let saved = if opts.ephemeral {
        None
    } else {
        port::saved_port(&opts.paths.state_db())
    };
    let choice = port::choose(opts.port, saved, api.port, &opts.config_file)?;
    for w in &choice.warnings {
        tracing::warn!("{w}");
    }
    let (native_listener, native_port, notice) = rt.block_on(port::bind_at_startup(
        &choice,
        lightloom_api::NATIVE_PORT,
        &opts.config_file,
    ))?;
    engine_opts.api_port = native_port;
    engine_opts.api_port_notice = notice;

    let engine = engine::start(engine_opts)?;
    let shared = engine.shared();
    let data_dir = opts.paths.data.clone();
    let native = Arc::new(
        lightloom_api::NativeListener::new(lightloom_api::listener::DEFAULT_GRACE)
            .on_serve(move |port| {
                tracing::info!("dashboard and API on {}", lightloom_api::native_url(port));
                if let Err(e) = lightloom_runtime::dirs::write_api_port(&data_dir, port) {
                    tracing::warn!("cannot write the api-port file in {}: {e}", data_dir.display());
                }
            })
            .taken_elsewhere(port_taken_elsewhere),
    );
    let ctx = ApiContext {
        backend: shared.clone(),
        token,
        ui_dir,
        open_compat_mutations: api.compat_open_mutations,
        native: Some(Arc::clone(&native)),
    };
    let (stop_tx, stop_rx) = watch::channel(false);

    let result: anyhow::Result<()> = rt.block_on(async {
        let mut tasks = Vec::new();
        native.serve(native_listener, ctx.clone());
        if api.compat {
            let r = lightloom_api::compat_router(ctx.clone(), lightloom_api::COMPAT_REST_PORT);
            tasks.push(tokio::spawn(serve_when_free(
                lightloom_api::COMPAT_REST_PORT,
                r,
                stop_rx.clone(),
            )));
            let r = lightloom_api::canvas_event_router(
                ctx.clone(),
                lightloom_api::COMPAT_CANVAS_EVENT_PORT,
                api.canvas_events_need_token,
            );
            tasks.push(tokio::spawn(serve_when_free(
                lightloom_api::COMPAT_CANVAS_EVENT_PORT,
                r,
                stop_rx.clone(),
            )));
        }
        // The OpenRGB SDK server follows its setting (off by default).
        tasks.push(tokio::spawn(lightloom_api::openrgb::serve_when_enabled(
            ctx.backend.clone(),
            lightloom_api::OPENRGB_SDK_PORT,
            stop_rx.clone(),
            Arc::new(port_taken_elsewhere),
        )));
        if opts.open_dashboard {
            open_in_browser(&lightloom_api::native_url(native.port()));
        }
        tokio::select! {
            _ = wait_for_stop() => {}
            _ = shared.shutdown_requested() => {}
        }
        tracing::info!("stopping");
        let _ = stop_tx.send(true);
        native.shutdown().await;
        for t in tasks {
            let _ = t.await;
        }
        Ok(())
    });
    // `lightloom` stops looking for this daemon.
    lightloom_runtime::dirs::remove_api_port(&opts.paths.data, native.port());
    // Plugins restore their devices before the process ends.
    engine.shutdown();
    result
}

async fn serve(listener: TcpListener, router: axum::Router, stop: watch::Receiver<bool>) {
    lightloom_api::listener::serve_until(listener, router, stop).await;
}

/// Whether another application listens on `port` on any address. SignalRGB
/// listens on the IPv6 wildcard with address reuse, so Windows would still let
/// us bind `127.0.0.1` and take IPv4 loopback clients away from it; the
/// listener table tells.
fn port_taken_elsewhere(port: u16) -> bool {
    !lightloom_runtime::system::other_listeners(port).is_empty()
}

/// Serve on `port` once no other application (typically SignalRGB) listens on
/// it; try again every [`COMPAT_RETRY`].
async fn serve_when_free(port: u16, router: axum::Router, mut stop: watch::Receiver<bool>) {
    let mut warned = false;
    loop {
        let bound = if port_taken_elsewhere(port) {
            Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "another application listens on it",
            ))
        } else {
            TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await
        };
        match bound {
            Ok(listener) => {
                tracing::info!("SignalRGB-compatible listener on 127.0.0.1:{port}");
                serve(listener, router, stop).await;
                return;
            }
            Err(e) => {
                if !warned {
                    tracing::info!("port {port} is taken ({e}); the compatible listener waits for it");
                    warned = true;
                }
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(COMPAT_RETRY) => {}
            _ = stop.wait_for(|s| *s) => return,
        }
    }
}

/// Ctrl-C; on Windows also Ctrl-Break, console close, log-off and shutdown.
async fn wait_for_stop() {
    #[cfg(windows)]
    {
        use tokio::signal::windows;
        let (Ok(mut brk), Ok(mut close), Ok(mut logoff), Ok(mut shutdown)) = (
            windows::ctrl_break(),
            windows::ctrl_close(),
            windows::ctrl_logoff(),
            windows::ctrl_shutdown(),
        ) else {
            let _ = tokio::signal::ctrl_c().await;
            return;
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = brk.recv() => {}
            _ = close.recv() => {}
            _ = logoff.recv() => {}
            _ = shutdown.recv() => {}
        }
    }
    #[cfg(not(windows))]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = async { match term.as_mut() { Some(t) => { t.recv().await; } None => std::future::pending::<()>().await } } => {}
        }
    }
}

/// Open `url` in the default browser (best effort).
pub fn open_in_browser(url: &str) {
    #[cfg(windows)]
    let spawned = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let spawned = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let spawned = std::process::Command::new("xdg-open").arg(url).spawn();
    if let Err(e) = spawned {
        tracing::warn!("cannot open {url}: {e}");
    }
}
