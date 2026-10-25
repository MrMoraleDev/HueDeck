// SPDX-License-Identifier: GPL-3.0-or-later
//! The native listener (API + dashboard) and how it moves to another port.
//!
//! [`NativeListener`] serves [`crate::native_router`] on one loopback port at
//! a time. `PATCH /v1/settings` with a `port` binds the new port first (a busy
//! port changes nothing), lets the daemon persist the choice, then
//! [`NativeListener::serve`]s on the new port and keeps the old listener for
//! a grace period, so the response reaches the dashboard and its navigation
//! to the new address works, before shutting the old one down gracefully.

use crate::server::ApiContext;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// How long the previous listener stays up after a port change.
pub const DEFAULT_GRACE: Duration = Duration::from_secs(3);

type PortHook = Box<dyn Fn(u16) + Send + Sync>;
type TakenHook = Box<dyn Fn(u16) -> bool + Send + Sync>;

/// The native API listener; see the module documentation.
pub struct NativeListener {
    inner: Mutex<Inner>,
    grace: Duration,
    on_serve: PortHook,
    taken_elsewhere: TakenHook,
}

#[derive(Default)]
struct Inner {
    /// The port served now (0 before the first [`NativeListener::serve`]).
    port: u16,
    /// Stops the current listener.
    current: Option<Arc<watch::Sender<bool>>>,
    /// Stops every listener still running (current and retiring ones).
    all: Vec<Arc<watch::Sender<bool>>>,
    tasks: Vec<JoinHandle<()>>,
}

impl NativeListener {
    /// A listener that keeps the previous port for `grace` after a change.
    pub fn new(grace: Duration) -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            grace,
            on_serve: Box::new(|_| {}),
            taken_elsewhere: Box::new(|_| false),
        }
    }

    /// Called with the port each time serving starts on one (the daemon
    /// writes its `api-port` file).
    pub fn on_serve(mut self, f: impl Fn(u16) + Send + Sync + 'static) -> Self {
        self.on_serve = Box::new(f);
        self
    }

    /// Whether another program listens on a port even though binding
    /// `127.0.0.1` would succeed (Windows address reuse on the IPv6 wildcard).
    pub fn taken_elsewhere(mut self, f: impl Fn(u16) -> bool + Send + Sync + 'static) -> Self {
        self.taken_elsewhere = Box::new(f);
        self
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The port served now.
    pub fn port(&self) -> u16 {
        self.lock().port
    }

    /// Bind `127.0.0.1:port`, refusing a port another program listens on.
    pub async fn bind(&self, port: u16) -> std::io::Result<TcpListener> {
        if (self.taken_elsewhere)(port) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "another program listens on it",
            ));
        }
        bind_loopback(port).await
    }

    /// Serve the native API on `listener`. A listener served before keeps
    /// running for the grace period, then shuts down gracefully.
    pub fn serve(&self, listener: TcpListener, ctx: ApiContext) {
        let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
        let (tx, rx) = watch::channel(false);
        let tx = Arc::new(tx);
        let router = crate::native_router(ctx, port);
        let task = tokio::spawn(serve_until(listener, router, rx));
        let previous = {
            let mut inner = self.lock();
            inner.port = port;
            inner.all.retain(|s| !*s.borrow());
            inner.all.push(Arc::clone(&tx));
            inner.tasks.retain(|t| !t.is_finished());
            inner.tasks.push(task);
            inner.current.replace(tx)
        };
        (self.on_serve)(port);
        if let Some(old) = previous {
            let grace = self.grace;
            tokio::spawn(async move {
                tokio::time::sleep(grace).await;
                let _ = old.send(true);
            });
        }
    }

    /// Stop every listener and wait for them (in-flight requests finish).
    pub async fn shutdown(&self) {
        let tasks = {
            let mut inner = self.lock();
            for s in &inner.all {
                let _ = s.send(true);
            }
            inner.all.clear();
            inner.current = None;
            std::mem::take(&mut inner.tasks)
        };
        for t in tasks {
            let _ = t.await;
        }
    }
}

/// Bind `127.0.0.1:port`.
pub async fn bind_loopback(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await
}

/// Serve `router` on `listener` until `stop` turns true (graceful: requests
/// in flight finish).
pub async fn serve_until(listener: TcpListener, router: axum::Router, mut stop: watch::Receiver<bool>) {
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let shutdown = async move {
        let _ = stop.wait_for(|s| *s).await;
    };
    if let Err(e) = axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await
    {
        tracing::warn!("listener on port {port} ended: {e}");
    }
    tracing::info!("stopped listening on 127.0.0.1:{port}");
}
