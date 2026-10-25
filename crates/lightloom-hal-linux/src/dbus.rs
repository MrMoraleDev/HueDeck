// SPDX-License-Identifier: GPL-3.0-or-later
//! A blocking face on zbus for the Linux backends and the tray (Linux only).
//!
//! zbus 6 is async; its built-in runtime drives every connection on a
//! helper thread while no thread is inside [`zbus::block_on`]. Each helper
//! here blocks on one small future, so no caller ever runs its own blocking
//! work (a power sink, an HTTP request) inside `block_on`. None of them may
//! be called from a method of a served interface: those run on zbus's
//! runtime, and must only queue work for another thread.

use std::future::Future;
use std::pin::Pin;
use zbus::export::futures_core::Stream;
use zbus::message::Type;
use zbus::{Connection, MatchRule, Message, MessageStream};

/// Run `future` to completion on this thread.
pub fn wait<F: Future>(future: F) -> F::Output {
    zbus::block_on(future)
}

/// A new connection to the session bus.
pub fn session() -> zbus::Result<Connection> {
    wait(Connection::session())
}

/// A new connection to the system bus.
pub fn system() -> zbus::Result<Connection> {
    wait(Connection::system())
}

/// Call `iface.method(body)` on `dest` at `path`; the reply's body.
pub fn call<B, R>(
    conn: &Connection,
    dest: Option<&str>,
    path: &str,
    iface: &str,
    method: &str,
    body: &B,
) -> zbus::Result<R>
where
    B: serde::Serialize + zbus::DynamicType,
    R: for<'d> zbus::wire::DynamicDeserialize<'d>,
{
    let reply = wait(conn.call_method(dest, path, Some(iface), method, body))?;
    let body = reply.body();
    body.deserialize()
}

/// Emit signal `iface.name(body)` from `path` to everyone listening.
pub fn emit<B>(conn: &Connection, path: &str, iface: &str, name: &str, body: &B) -> zbus::Result<()>
where
    B: serde::Serialize + zbus::DynamicType,
{
    wait(conn.emit_signal(None::<&str>, path, iface, name, body))
}

/// The signals `sender` sends on `conn` (on a bus, the rule is registered
/// with it; on a peer connection it filters locally).
pub fn signals(conn: &Connection, sender: &str) -> zbus::Result<MessageStream> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(sender)
        .build()?;
    wait(MessageStream::for_match_rule(rule, conn, None))
}

/// The signals that match `rule`.
pub fn matching(conn: &Connection, rule: MatchRule<'_>) -> zbus::Result<MessageStream> {
    wait(MessageStream::for_match_rule(rule, conn, None))
}

/// The next message of `stream`; `None` once the connection is gone.
pub fn next(stream: &mut MessageStream) -> Option<zbus::Result<Message>> {
    wait(std::future::poll_fn(|cx| Pin::new(&mut *stream).poll_next(cx)))
}

/// Close `conn` (every stream on it ends).
pub fn close(conn: Connection) {
    let _ = wait(conn.close());
}
