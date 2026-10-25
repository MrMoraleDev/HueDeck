// SPDX-License-Identifier: GPL-3.0-or-later
//! Desktop notifications through `org.freedesktop.Notifications` on the
//! session D-Bus: the Linux [`Notifier`] (Linux only; zbus, pure Rust).
//!
//! [`Notifier::notify`] only queues: one worker thread connects to the
//! session bus on first use and calls `Notify` (app name `Lightloom`, icon
//! `lightloom` as in the desktop entry, server-default timeout). A missing
//! bus or notification server is logged once and retried on the next
//! notification. The body is escaped, since servers may read it as markup.

use crate::dbus;
use lightloom_hal::notify::{Notification, Notifier};
use lightloom_hal::{HalError, HalResult};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::sync::Mutex;
use zbus::{Connection, Variant};

const DEST: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
/// The desktop entry and icon name (`packaging/appimage/lightloom.desktop`).
const APP_ID: &str = "lightloom";

/// `text` with the markup characters of the notification spec escaped.
pub fn escape_body(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
    out
}

fn send(conn: &Connection, n: &Notification) -> zbus::Result<u32> {
    let mut hints: HashMap<&str, Variant<'_>> = HashMap::new();
    hints.insert("desktop-entry", Variant::from(APP_ID));
    let body = escape_body(&n.body);
    dbus::call(
        conn,
        Some(DEST),
        PATH,
        DEST,
        "Notify",
        &(
            "Lightloom",
            0u32,
            APP_ID,
            n.title.as_str(),
            body.as_str(),
            Vec::<&str>::new(),
            hints,
            -1i32,
        ),
    )
}

/// Freedesktop notifications (see the module docs).
pub struct FreedesktopNotifier {
    queue: Mutex<Sender<Notification>>,
}

impl std::fmt::Debug for FreedesktopNotifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FreedesktopNotifier").finish_non_exhaustive()
    }
}

impl Default for FreedesktopNotifier {
    fn default() -> Self {
        Self::new()
    }
}

impl FreedesktopNotifier {
    /// Notifications on the session bus (connected on first use).
    pub fn new() -> Self {
        Self::start(None)
    }

    /// Notifications to whatever serves them on `conn` (tests).
    pub fn with_connection(conn: Connection) -> Self {
        Self::start(Some(conn))
    }

    fn start(given: Option<Connection>) -> Self {
        let (tx, rx) = channel::<Notification>();
        let spawned = std::thread::Builder::new()
            .name("notifications".into())
            .spawn(move || {
                let mut conn = given;
                let mut warned = false;
                for n in rx {
                    if conn.is_none() {
                        conn = dbus::session()
                            .map_err(|e| {
                                if !std::mem::replace(&mut warned, true) {
                                    tracing::warn!("no session D-Bus for notifications: {e}");
                                }
                            })
                            .ok();
                    }
                    let Some(c) = &conn else { continue };
                    if let Err(e) = send(c, &n) {
                        if !std::mem::replace(&mut warned, true) {
                            tracing::warn!("desktop notification failed (no notification server?): {e}");
                        }
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::warn!("notification thread: {e}");
        }
        Self {
            queue: Mutex::new(tx),
        }
    }
}

impl Notifier for FreedesktopNotifier {
    fn notify(&self, notification: &Notification) -> HalResult<()> {
        self.queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .send(notification.clone())
            .map_err(|_| HalError::Backend("the notification thread is gone".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc::Receiver;
    use std::time::Duration;
    use zbus::connection::Builder;
    use zbus::OwnedVariant;

    /// App name, icon, summary, body, timeout, `desktop-entry` hint.
    type Seen = (String, String, String, String, i32, Option<String>);

    struct FakeServer {
        seen: Mutex<Sender<Seen>>,
    }

    #[zbus::interface(name = "org.freedesktop.Notifications")]
    impl FakeServer {
        #[allow(clippy::too_many_arguments)]
        fn notify(
            &self,
            app_name: String,
            _replaces_id: u32,
            app_icon: String,
            summary: String,
            body: String,
            _actions: Vec<String>,
            hints: HashMap<String, OwnedVariant>,
            expire_timeout: i32,
        ) -> u32 {
            let entry = hints
                .get("desktop-entry")
                .and_then(|v| String::try_from(v.clone()).ok());
            let _ =
                self.seen
                    .lock()
                    .unwrap()
                    .send((app_name, app_icon, summary, body, expire_timeout, entry));
            7
        }
    }

    fn peers() -> (Connection, Receiver<Seen>, Connection) {
        let (a, b) = UnixStream::pair().unwrap();
        let (tx, rx) = channel();
        let guid = zbus::Guid::generate();
        let server = std::thread::spawn(move || {
            dbus::wait(
                Builder::unix_stream(a)
                    .server(guid)
                    .p2p()
                    .serve_at(PATH, FakeServer { seen: Mutex::new(tx) })
                    .build(),
            )
            .unwrap()
        });
        let client = dbus::wait(Builder::unix_stream(b).p2p().build()).unwrap();
        (server.join().unwrap(), rx, client)
    }

    #[test]
    fn notifications_reach_the_server_escaped() {
        let (_server, seen, client) = peers();
        let n = FreedesktopNotifier::with_connection(client);
        n.notify(&Notification {
            title: "Device lost".into(),
            body: "Strip <A> & B".into(),
        })
        .unwrap();
        let (app, icon, summary, body, timeout, entry) = seen.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!((app.as_str(), icon.as_str()), ("Lightloom", "lightloom"));
        assert_eq!(summary, "Device lost");
        assert_eq!(body, "Strip &lt;A&gt; &amp; B");
        assert_eq!(timeout, -1);
        assert_eq!(entry.as_deref(), Some("lightloom"));
    }

    #[test]
    fn escaping_leaves_plain_text_alone() {
        assert_eq!(escape_body("50% brighter"), "50% brighter");
        assert_eq!(escape_body("a<b>&c"), "a&lt;b&gt;&amp;c");
    }
}
