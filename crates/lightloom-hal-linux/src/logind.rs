// SPDX-License-Identifier: GPL-3.0-or-later
//! Sleep and resume, lock and unlock from systemd-logind over the system
//! D-Bus: the Linux [`PowerSource`] (Linux only; zbus, pure Rust).
//!
//! * `org.freedesktop.login1.Manager.PrepareForSleep(true)` →
//!   [`PowerEvent::Suspend`], `(false)` → [`PowerEvent::Resume`]. A `delay`
//!   inhibitor lock (`Manager.Inhibit("sleep", …, "delay")`) is held while
//!   awake, so logind waits for the sink (at most `InhibitDelayMaxSec`, 5 s
//!   by default) before the system sleeps; the lock is closed once the sink
//!   returns and taken again on resume.
//! * The user's session: `org.freedesktop.login1.Session` `Lock` / `Unlock`
//!   (what `loginctl lock-session` sends to the screen locker) and changes of
//!   its `LockedHint` property (what lockers set once the screen is locked)
//!   → [`PowerEvent::Lock`] / [`PowerEvent::Unlock`]. Consumers treat repeats
//!   as no-ops. The session is the caller's (`GetSessionByPID`), else, for a
//!   `systemd --user` service outside any session, the user's display session
//!   (`/org/freedesktop/login1/user/self`, property `Display`).
//!
//! logind has no display on/off signal, so `DisplayOff` / `DisplayOn` never
//! come from here. Without a system bus or logind (containers, some Flatpak
//! setups) [`PowerSource::subscribe`] fails and the caller runs without.

use crate::dbus;
use lightloom_hal::power::{PowerEvent, PowerSink, PowerSource};
use lightloom_hal::{HalError, HalResult};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use zbus::{Connection, OwnedFd, OwnedObjectPath, OwnedVariant};

/// logind's bus name.
pub const LOGIND: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";
const SESSION: &str = "org.freedesktop.login1.Session";
const USER: &str = "org.freedesktop.login1.User";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

/// The event a logind signal means, if any. `session` is the watched
/// session's object path; `msg` the signal.
fn event_of(msg: &zbus::Message, session: Option<&str>) -> Option<PowerEvent> {
    let header = msg.header();
    let interface = header.interface()?.as_str();
    let member = header.member()?.as_str();
    let path = header.path().map(|p| p.as_str());
    match (interface, member) {
        (MANAGER, "PrepareForSleep") => {
            let sleeping: bool = msg.body().deserialize().ok()?;
            Some(if sleeping {
                PowerEvent::Suspend
            } else {
                PowerEvent::Resume
            })
        }
        (SESSION, "Lock") if path.is_some() && path == session => Some(PowerEvent::Lock),
        (SESSION, "Unlock") if path.is_some() && path == session => Some(PowerEvent::Unlock),
        (PROPERTIES, "PropertiesChanged") if path.is_some() && path == session => {
            let (iface, changed, _): (
                String,
                std::collections::HashMap<String, OwnedVariant>,
                Vec<String>,
            ) = msg.body().deserialize().ok()?;
            if iface != SESSION {
                return None;
            }
            let locked = bool::try_from(changed.get("LockedHint")?).ok()?;
            Some(if locked {
                PowerEvent::Lock
            } else {
                PowerEvent::Unlock
            })
        }
        _ => None,
    }
}

/// A `delay` sleep inhibitor lock; closing the fd releases it.
fn inhibit(conn: &Connection) -> zbus::Result<OwnedFd> {
    dbus::call(
        conn,
        Some(LOGIND),
        MANAGER_PATH,
        MANAGER,
        "Inhibit",
        &(
            "sleep",
            "Lightloom",
            "Turning the lighting off before the system sleeps",
            "delay",
        ),
    )
}

/// The object path of the session to watch for lock and unlock.
fn session_path(conn: &Connection) -> Option<String> {
    let by_pid: zbus::Result<OwnedObjectPath> = dbus::call(
        conn,
        Some(LOGIND),
        MANAGER_PATH,
        MANAGER,
        "GetSessionByPID",
        &(std::process::id()),
    );
    if let Ok(p) = by_pid {
        return Some(p.as_str().to_string());
    }
    // A user service is in no session: the user's graphical one.
    let display: OwnedVariant = dbus::call(
        conn,
        Some(LOGIND),
        "/org/freedesktop/login1/user/self",
        PROPERTIES,
        "Get",
        &(USER, "Display"),
    )
    .ok()?;
    let display: (String, OwnedObjectPath) = display.try_into().ok()?;
    (display.1.as_str() != "/").then(|| display.1.as_str().to_string())
}

/// The sink, until [`PowerSource::stop`] takes it away.
type SharedSink = Arc<Mutex<Option<PowerSink>>>;

fn deliver(sink: &SharedSink, event: PowerEvent) -> bool {
    let current = sink.lock().unwrap_or_else(|e| e.into_inner()).clone();
    match current {
        Some(s) => {
            s(event);
            true
        }
        None => false,
    }
}

/// logind power and session events (see the module docs).
pub struct LogindPower {
    /// A connection given by the caller (tests: a peer); `None`: the system bus.
    given: Option<Connection>,
    /// Where events go while subscribed.
    sink: SharedSink,
    stop: Arc<AtomicBool>,
    conn: Option<Connection>,
}

impl std::fmt::Debug for LogindPower {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogindPower")
            .field("subscribed", &self.conn.is_some())
            .finish_non_exhaustive()
    }
}

impl Default for LogindPower {
    fn default() -> Self {
        Self::new()
    }
}

impl LogindPower {
    /// Events from logind on the system bus (connected on subscribe).
    pub fn new() -> Self {
        Self {
            given: None,
            sink: Arc::new(Mutex::new(None)),
            stop: Arc::new(AtomicBool::new(false)),
            conn: None,
        }
    }

    /// Events from whatever answers as logind on `conn` (tests).
    pub fn with_connection(conn: Connection) -> Self {
        let mut power = Self::new();
        power.given = Some(conn);
        power
    }
}

impl PowerSource for LogindPower {
    fn subscribe(&mut self, sink: PowerSink) -> HalResult<()> {
        self.stop();
        let conn = match &self.given {
            Some(c) => c.clone(),
            None => dbus::system().map_err(|e| HalError::Backend(format!("system D-Bus: {e}")))?,
        };
        // Listen before taking the lock so no PrepareForSleep is missed.
        let mut messages =
            dbus::signals(&conn, LOGIND).map_err(|e| HalError::Backend(format!("logind signals: {e}")))?;
        let session = session_path(&conn);
        if session.is_none() {
            tracing::info!("no logind session found; lock and unlock are not watched");
        }
        let mut lock = match inhibit(&conn) {
            Ok(fd) => Some(fd),
            Err(e) => {
                // Not fatal: sleep is then not delayed for the devices.
                tracing::warn!(
                    "logind sleep inhibitor refused ({e}); devices may not be put away before sleep"
                );
                None
            }
        };
        let stop = Arc::new(AtomicBool::new(false));
        *self.sink.lock().unwrap_or_else(|e| e.into_inner()) = Some(sink);
        self.stop = Arc::clone(&stop);
        let shared = Arc::clone(&self.sink);
        let thread_conn = conn.clone();
        std::thread::Builder::new()
            .name("logind-power".into())
            .spawn(move || {
                while let Some(msg) = dbus::next(&mut messages) {
                    if stop.load(Ordering::Acquire) {
                        return;
                    }
                    let Ok(msg) = msg else { continue };
                    let Some(event) = event_of(&msg, session.as_deref()) else {
                        continue;
                    };
                    tracing::debug!(?event, "logind");
                    if !deliver(&shared, event) {
                        return;
                    }
                    match event {
                        // The devices are put away: let the system sleep.
                        PowerEvent::Suspend => drop(lock.take()),
                        PowerEvent::Resume if lock.is_none() => lock = inhibit(&thread_conn).ok(),
                        _ => {}
                    }
                }
            })
            .map_err(HalError::Io)?;
        self.conn = Some(conn);
        Ok(())
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // No event reaches the old sink after this, even before the thread sees the flag.
        *self.sink.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if let Some(conn) = self.conn.take() {
            if self.given.is_none() {
                // Ends the watcher thread's message stream (and drops the inhibitor).
                dbus::close(conn);
            }
        }
    }
}

impl Drop for LogindPower {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::time::Duration;
    use zbus::connection::Builder;
    use zbus::ObjectPath;

    const SESSION_PATH: &str = "/org/freedesktop/login1/session/_32";

    /// A fake logind Manager: counts inhibitor locks.
    struct FakeManager {
        inhibits: Mutex<Sender<String>>,
    }

    #[zbus::interface(name = "org.freedesktop.login1.Manager")]
    impl FakeManager {
        fn inhibit(&self, what: String, _who: String, _why: String, mode: String) -> OwnedFd {
            let _ = self.inhibits.lock().unwrap().send(format!("{what}:{mode}"));
            let fd: std::os::fd::OwnedFd = std::fs::File::open("/dev/null").unwrap().into();
            fd.into()
        }

        #[zbus(name = "GetSessionByPID")]
        fn get_session_by_pid(&self, _pid: u32) -> OwnedObjectPath {
            ObjectPath::try_from(SESSION_PATH).unwrap().into()
        }
    }

    /// A peer connection pair: (fake logind server, client).
    fn peers() -> (Connection, Receiver<String>, Connection) {
        let (a, b) = UnixStream::pair().unwrap();
        let (tx, rx) = channel();
        let guid = zbus::Guid::generate();
        let server = std::thread::spawn(move || {
            dbus::wait(
                Builder::unix_stream(a)
                    .server(guid)
                    .p2p()
                    .serve_at(
                        MANAGER_PATH,
                        FakeManager {
                            inhibits: Mutex::new(tx),
                        },
                    )
                    .build(),
            )
            .unwrap()
        });
        let client = dbus::wait(Builder::unix_stream(b).p2p().build()).unwrap();
        (server.join().unwrap(), rx, client)
    }

    fn recorder() -> (PowerSink, Receiver<PowerEvent>) {
        let (tx, rx) = channel();
        let tx = Mutex::new(tx);
        (
            Arc::new(move |e| {
                let _ = tx.lock().unwrap().send(e);
            }),
            rx,
        )
    }

    fn next(rx: &Receiver<PowerEvent>) -> PowerEvent {
        rx.recv_timeout(Duration::from_secs(5)).expect("an event")
    }

    #[test]
    fn sleep_lock_and_unlock_reach_the_sink_and_the_inhibitor_is_renewed() {
        let (server, inhibits, client) = peers();
        let mut power = LogindPower::with_connection(client);
        let (sink, events) = recorder();
        power.subscribe(sink).unwrap();
        assert_eq!(
            inhibits.recv_timeout(Duration::from_secs(5)).unwrap(),
            "sleep:delay"
        );

        dbus::emit(&server, MANAGER_PATH, MANAGER, "PrepareForSleep", &(true,)).unwrap();
        assert_eq!(next(&events), PowerEvent::Suspend);
        dbus::emit(&server, MANAGER_PATH, MANAGER, "PrepareForSleep", &(false,)).unwrap();
        assert_eq!(next(&events), PowerEvent::Resume);
        // A new delay lock after resume.
        assert_eq!(
            inhibits.recv_timeout(Duration::from_secs(5)).unwrap(),
            "sleep:delay"
        );

        dbus::emit(&server, SESSION_PATH, SESSION, "Lock", &()).unwrap();
        assert_eq!(next(&events), PowerEvent::Lock);
        // Another session's unlock is not ours.
        dbus::emit(
            &server,
            "/org/freedesktop/login1/session/c9",
            SESSION,
            "Unlock",
            &(),
        )
        .unwrap();
        let mut changed = std::collections::HashMap::new();
        changed.insert("LockedHint", zbus::Variant::from(false));
        dbus::emit(
            &server,
            SESSION_PATH,
            PROPERTIES,
            "PropertiesChanged",
            &(SESSION, changed, Vec::<String>::new()),
        )
        .unwrap();
        assert_eq!(
            next(&events),
            PowerEvent::Unlock,
            "from LockedHint, not session c9"
        );

        power.stop();
        dbus::emit(&server, SESSION_PATH, SESSION, "Lock", &()).unwrap();
        assert!(
            events.recv_timeout(Duration::from_millis(300)).is_err(),
            "stopped"
        );
    }

    #[test]
    fn unrelated_signals_mean_nothing() {
        let (server, _inhibits, client) = peers();
        let mut power = LogindPower::with_connection(client);
        let (sink, events) = recorder();
        power.subscribe(sink).unwrap();
        dbus::emit(
            &server,
            MANAGER_PATH,
            MANAGER,
            "SessionNew",
            &("3", ObjectPath::try_from("/x").unwrap()),
        )
        .unwrap();
        dbus::emit(&server, MANAGER_PATH, MANAGER, "PrepareForShutdown", &(true,)).unwrap();
        assert!(events.recv_timeout(Duration::from_millis(300)).is_err());
    }
}
