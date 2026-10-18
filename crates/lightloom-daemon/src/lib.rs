// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `lightloomd`, the per-user Lightloom daemon, as a library: the binary in
//! `main.rs` parses arguments and calls [`run`]; tests drive the parts.
//!
//! [`run`] takes the single-instance lock, loads the API token, starts the
//! [`engine`] thread (devices, renderer, database) and serves the local API
//! on the loopback interface until Ctrl-C (or console close / log-off /
//! shutdown on Windows), then stops every device session so plugins restore
//! their devices.

#![forbid(unsafe_code)]

pub mod config;
pub mod engine;
pub mod instance;
pub mod lifecycle;
pub mod paths;
pub mod store;

use anyhow::Context;
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
    // Bind before starting the engine: a second daemon on another data
    // directory must not drive the devices if the port is taken.
    let native = rt
        .block_on(TcpListener::bind(SocketAddr::from((
            Ipv4Addr::LOCALHOST,
            api.port,
        ))))
        .with_context(|| {
            format!(
                "listening on 127.0.0.1:{} (is another Lightloom running?)",
                api.port
            )
        })?;

    let engine = engine::start(engine_opts)?;
    let ctx = ApiContext {
        backend: engine.shared(),
        token,
        ui_dir,
        open_compat_mutations: api.compat_open_mutations,
    };
    let url = format!("http://127.0.0.1:{}/", api.port);
    let (stop_tx, stop_rx) = watch::channel(false);

    let result: anyhow::Result<()> = rt.block_on(async {
        let mut tasks = Vec::new();
        let router = lightloom_api::native_router(ctx.clone(), api.port);
        tasks.push(tokio::spawn(serve(native, router, stop_rx.clone())));
        tracing::info!("dashboard and API on {url}");
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
        if opts.open_dashboard {
            open_in_browser(&url);
        }
        wait_for_stop().await;
        tracing::info!("stopping");
        let _ = stop_tx.send(true);
        for t in tasks {
            let _ = t.await;
        }
        Ok(())
    });
    // Plugins restore their devices before the process ends.
    engine.shutdown();
    result
}

async fn serve(listener: TcpListener, router: axum::Router, mut stop: watch::Receiver<bool>) {
    let shutdown = async move {
        let _ = stop.wait_for(|s| *s).await;
    };
    if let Err(e) = axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await
    {
        tracing::warn!("listener ended: {e}");
    }
}

/// Serve on `port` once it is free (another application, typically SignalRGB,
/// may own it); try again every [`COMPAT_RETRY`].
async fn serve_when_free(port: u16, router: axum::Router, mut stop: watch::Receiver<bool>) {
    let mut warned = false;
    loop {
        match TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await {
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
