// SPDX-License-Identifier: GPL-3.0-or-later
//! Network I/O of network plugins (docs/host-api/network-lightloom.md).
//!
//! A [`NetContext`] belongs to one plugin context (a network service or a
//! network device session) and owns every socket that context opened: they
//! close when the context ends. The JS host (`js/host.js`) drives it through
//! the `net*` / `http*` natives [`install`] adds: sockets are non-blocking,
//! and what arrives is queued until the host asks for it ([`NetContext::poll`],
//! the JS `pumpNet()`), which happens between plugin calls (every 50 ms in a
//! service, before each `Render()` in a session). Asynchronous HTTP requests
//! run on short-lived threads and complete through the same queue. So does
//! the handshake of the context's one DTLS connection (`dtls` feature).
//!
//! Every destination goes through [`policy::lan_allowed`]. In a dry run a
//! device context records its sends instead of sending them.

pub mod dtls;
pub mod http;
pub mod mdns;
pub mod policy;
pub mod tls;

use crate::device::{panic_text, LogSink, WriteMode};
use crate::runtime::Deadline;
use dtls::{DtlsSession, Psk};
use http::{HttpRequest, HttpResponse};
use policy::{lan_allowed, resolve_lan};
use rquickjs::{Ctx, Function, Object};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Largest datagram (and TCP write) a plugin may send.
pub const MAX_DATAGRAM: usize = 8192;
/// Most sockets one context may hold open.
pub const MAX_SOCKETS: usize = 64;
/// Longest TCP connect.
pub const TCP_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// Most asynchronous HTTP requests in flight per context.
pub const MAX_ASYNC_HTTP: usize = 16;
/// Most events kept between two polls (older ones are dropped and counted).
const MAX_QUEUED: usize = 1024;
/// Write timeout of TCP streams.
const TCP_WRITE_TIMEOUT: Duration = Duration::from_secs(1);

/// Who owns the context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetRole {
    /// A network service (discovery): never in dry-run.
    Service,
    /// A network device session.
    Device,
}

/// Counters of one context.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct NetStats {
    /// Datagrams / TCP writes sent.
    pub sent: u64,
    /// Bytes sent.
    pub sent_bytes: u64,
    /// Datagrams / TCP reads received.
    pub received: u64,
    /// Sends and connects refused by the local-network rule or limits.
    pub refused: u64,
    /// Failed socket operations.
    pub errors: u64,
    /// HTTP requests made.
    pub http_requests: u64,
    /// HTTP requests that failed (status 0).
    pub http_errors: u64,
    /// Sends and requests a dry run held back.
    pub dry_run: u64,
    /// Events dropped because the plugin did not take them in time.
    pub dropped: u64,
    /// DTLS connections established.
    pub dtls_connections: u64,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Something that happened on a socket, for the JS side.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NetEvent {
    /// Socket id (or async request id for `http`).
    pub socket: u32,
    /// `message`, `close`, `error`, `connection`, `listening`, `http`, or
    /// `dtlsOpen` / `dtlsClose` / `dtlsError` (socket 0).
    pub kind: &'static str,
    /// Payload bytes (`message`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<u8>,
    /// Payload as text (UTF-8, lossy).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// Sender address.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub address: String,
    /// Sender port.
    pub port: u16,
    /// Arrived on a discovery socket (goes to `discovery.Discovered`).
    #[serde(skip_serializing_if = "is_false")]
    pub discovery: bool,
    /// `connection` on a listening socket: the accepted socket's id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<u32>,
    /// `error`: what went wrong.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// `http`: the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http: Option<HttpResponse>,
}

impl NetEvent {
    fn new(socket: u32, kind: &'static str) -> Self {
        Self {
            socket,
            kind,
            data: Vec::new(),
            text: String::new(),
            address: String::new(),
            port: 0,
            discovery: false,
            client: None,
            message: None,
            http: None,
        }
    }
}

enum Kind {
    Udp(Option<UdpSocket>),
    TcpIdle,
    TcpStream(TcpStream),
    TcpListen { listener: TcpListener, peer: IpAddr },
    Closed,
}

struct Socket {
    kind: Kind,
    peer: Option<SocketAddr>,
    discovery: bool,
}

/// `QAbstractSocket::SocketState` values plugins compare `socket.state` with.
pub mod state {
    /// Not bound or connected.
    pub const UNCONNECTED: i32 = 0;
    /// Connected (TCP) or with a default peer (UDP).
    pub const CONNECTED: i32 = 3;
    /// Bound (UDP).
    pub const BOUND: i32 = 4;
    /// Listening (TCP server).
    pub const LISTENING: i32 = 5;
}

/// The context's DTLS connection (`device.addFeature("dtls")`: one per context).
enum DtlsSlot {
    None,
    Connecting(Receiver<Result<DtlsSession, String>>),
    Open(Box<DtlsSession>),
}

/// `dtls` states for the JS side.
pub mod dtls_state {
    /// No connection.
    pub const NONE: i32 = 0;
    /// Handshake running.
    pub const CONNECTING: i32 = 1;
    /// Established.
    pub const OPEN: i32 = 2;
}

/// The sockets and pending requests of one plugin context.
pub struct NetContext {
    role: NetRole,
    dry_run: bool,
    sockets: BTreeMap<u32, Socket>,
    next_id: u32,
    http: Vec<(u32, Receiver<HttpResponse>)>,
    events: VecDeque<NetEvent>,
    stats: NetStats,
    log: Option<LogSink>,
    logged: BTreeSet<String>,
    broadcast: Option<(u32, SocketAddr)>,
    dtls: DtlsSlot,
    pins: Option<tls::PinStore>,
}

impl std::fmt::Debug for NetContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetContext")
            .field("role", &self.role)
            .field("dry_run", &self.dry_run)
            .field("sockets", &self.sockets.len())
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

/// A context shared between the plugin thread and the natives.
pub type SharedNet = Arc<Mutex<NetContext>>;

/// Lock a [`SharedNet`], recovering from a poisoned lock.
pub fn lock_net(net: &SharedNet) -> MutexGuard<'_, NetContext> {
    net.lock().unwrap_or_else(|e| e.into_inner())
}

fn new_udp(port: u16) -> std::io::Result<UdpSocket> {
    let s = UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port))?;
    s.set_nonblocking(true)?;
    s.set_broadcast(true)?;
    Ok(s)
}

/// The local address of the interface that routes to `peer` (no packet is sent).
pub fn local_address_for(peer: IpAddr) -> std::io::Result<IpAddr> {
    if peer.is_loopback() {
        return Ok(peer);
    }
    let probe = UdpSocket::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0))?;
    probe.connect(SocketAddr::new(peer, 9))?;
    Ok(probe.local_addr()?.ip())
}

impl NetContext {
    /// An empty context; `mode` matters for device contexts only.
    pub fn new(role: NetRole, mode: WriteMode) -> Self {
        Self {
            role,
            dry_run: role == NetRole::Device && mode == WriteMode::DryRun,
            sockets: BTreeMap::new(),
            next_id: 1,
            http: Vec::new(),
            events: VecDeque::new(),
            stats: NetStats::default(),
            log: None,
            logged: BTreeSet::new(),
            broadcast: None,
            dtls: DtlsSlot::None,
            pins: None,
        }
    }

    /// Where `https` trust-on-first-use pins are kept (the plugin's settings);
    /// without a store `https` needs `tlsPin` or `tlsName`.
    pub fn set_pin_store(&mut self, pins: Option<tls::PinStore>) {
        self.pins = pins;
    }

    /// The pin store.
    pub fn pin_store(&self) -> Option<tls::PinStore> {
        self.pins.clone()
    }

    /// Wrap into a [`SharedNet`].
    pub fn shared(self) -> SharedNet {
        Arc::new(Mutex::new(self))
    }

    /// Send host log lines (refusals, bind failures) to `sink`.
    pub fn set_log_sink(&mut self, sink: Option<LogSink>) {
        self.log = sink;
    }

    /// Counters so far.
    pub fn stats(&self) -> &NetStats {
        &self.stats
    }

    /// Sends are held back (dry run, device context).
    pub fn dry_run(&self) -> bool {
        self.dry_run
    }

    /// Open sockets.
    pub fn open_sockets(&self) -> usize {
        self.sockets
            .values()
            .filter(|s| !matches!(s.kind, Kind::Closed))
            .count()
    }

    fn log_once(&mut self, line: String) {
        if self.logged.len() < 256 && self.logged.insert(line.clone()) {
            tracing::debug!(target: "plugin-net", "{line}");
            if let Some(sink) = &self.log {
                sink(&format!("net: {line}"));
            }
        }
    }

    fn push(&mut self, ev: NetEvent) {
        if self.events.len() >= MAX_QUEUED {
            self.events.pop_front();
            self.stats.dropped += 1;
        }
        self.events.push_back(ev);
    }

    fn error_event(&mut self, id: u32, message: String) {
        self.stats.errors += 1;
        let mut ev = NetEvent::new(id, "error");
        ev.message = Some(message);
        self.push(ev);
    }

    /// A new socket: its id, or -1 past [`MAX_SOCKETS`].
    pub fn create(&mut self, tcp: bool) -> i32 {
        if self.open_sockets() >= MAX_SOCKETS {
            self.stats.refused += 1;
            self.log_once(format!("more than {MAX_SOCKETS} sockets: createSocket() refused"));
            return -1;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.sockets.insert(
            id,
            Socket {
                kind: if tcp { Kind::TcpIdle } else { Kind::Udp(None) },
                peer: None,
                discovery: false,
            },
        );
        id as i32
    }

    /// `socket.bind(port)` (UDP; `0` = ephemeral). One-shot.
    pub fn bind(&mut self, id: u32, port: u16) -> bool {
        let r = match self.sockets.get_mut(&id).map(|s| &mut s.kind) {
            Some(Kind::Udp(slot @ None)) => match new_udp(port) {
                Ok(s) => {
                    *slot = Some(s);
                    Ok(())
                }
                Err(e) => Err(format!("binding UDP port {port}: {e}")),
            },
            Some(Kind::Udp(Some(_))) => Err("UDP Socket Already bound!".to_string()),
            Some(_) => Err("bind() needs an unbound UDP socket".to_string()),
            None => return false,
        };
        match r {
            Ok(()) => {
                self.push(NetEvent::new(id, "listening"));
                true
            }
            Err(e) => {
                self.log_once(e.clone());
                self.error_event(id, e);
                false
            }
        }
    }

    fn udp(&mut self, id: u32) -> Result<&UdpSocket, String> {
        let s = self.sockets.get_mut(&id).ok_or("no such socket")?;
        match &mut s.kind {
            Kind::Udp(slot) => {
                if slot.is_none() {
                    *slot = Some(new_udp(0).map_err(|e| format!("opening a UDP socket: {e}"))?);
                }
                Ok(slot.as_ref().expect("bound above"))
            }
            _ => Err("not a UDP socket".into()),
        }
    }

    fn refuse(&mut self, why: String) -> i32 {
        self.stats.refused += 1;
        self.log_once(why);
        -1
    }

    /// `socket.connect(ip, port)` of a UDP socket: the default peer of `send`.
    pub fn connect_udp(&mut self, id: u32, addr: SocketAddr) -> bool {
        if let Err(e) = self.udp(id) {
            self.log_once(e);
            return false;
        }
        if let Some(s) = self.sockets.get_mut(&id) {
            s.peer = Some(addr);
        }
        self.push(NetEvent::new(id, "connection"));
        true
    }

    /// Record a TCP connection made outside the lock ([`connect_tcp`]).
    pub fn attach_stream(&mut self, id: u32, stream: TcpStream) -> bool {
        let peer = stream.peer_addr().ok();
        let _ = stream.set_write_timeout(Some(TCP_WRITE_TIMEOUT));
        let _ = stream.set_nodelay(true);
        match self.sockets.get_mut(&id) {
            Some(s) if matches!(s.kind, Kind::TcpIdle) => {
                s.kind = Kind::TcpStream(stream);
                s.peer = peer;
                self.push(NetEvent::new(id, "connection"));
                true
            }
            _ => false,
        }
    }

    /// `socket.send(data)` to the socket's peer: bytes sent or -1.
    pub fn send(&mut self, id: u32, data: &[u8]) -> i32 {
        if data.len() > MAX_DATAGRAM {
            return self.refuse(format!(
                "send(): {} bytes is over the {MAX_DATAGRAM}-byte limit",
                data.len()
            ));
        }
        let Some(s) = self.sockets.get(&id) else {
            return -1;
        };
        let peer = s.peer;
        let tcp = matches!(s.kind, Kind::TcpStream(_));
        if tcp {
            if self.dry_run {
                return self.held_back(data.len());
            }
            let r = match self.sockets.get_mut(&id).map(|s| &mut s.kind) {
                Some(Kind::TcpStream(stream)) => stream.write_all(data),
                _ => return -1,
            };
            return match r {
                Ok(()) => self.sent(data.len()),
                Err(e) => {
                    self.error_event(id, format!("TCP write: {e}"));
                    -1
                }
            };
        }
        match peer {
            Some(addr) => self.send_to(id, data, addr),
            None => {
                self.log_once("send() on a socket without a peer (connect() first)".into());
                -1
            }
        }
    }

    fn held_back(&mut self, n: usize) -> i32 {
        self.stats.dry_run += 1;
        n as i32
    }

    fn sent(&mut self, n: usize) -> i32 {
        self.stats.sent += 1;
        self.stats.sent_bytes += n as u64;
        n as i32
    }

    /// `socket.write(data, ip, port)` (UDP): bytes sent or -1.
    pub fn send_to(&mut self, id: u32, data: &[u8], addr: SocketAddr) -> i32 {
        if data.len() > MAX_DATAGRAM {
            return self.refuse(format!(
                "write(): {} bytes is over the {MAX_DATAGRAM}-byte limit",
                data.len()
            ));
        }
        if !lan_allowed(addr.ip()) {
            return self.refuse(format!("{} is not on the local network: not sent", addr.ip()));
        }
        if self.dry_run {
            return self.held_back(data.len());
        }
        let r = self
            .udp(id)
            .and_then(|s| s.send_to(data, addr).map_err(|e| e.to_string()));
        match r {
            Ok(n) => self.sent(n),
            Err(e) => {
                self.log_once(format!("sending to {addr}: {e}"));
                self.error_event(id, e);
                -1
            }
        }
    }

    /// `socket.listen(port, peer)` (TCP, Lightloom extension): listen on the
    /// local address facing `peer`, accepting `peer` only.
    pub fn listen(&mut self, id: u32, port: u16, peer: IpAddr) -> Option<SocketAddr> {
        if !lan_allowed(peer) {
            self.refuse(format!("listen(): {peer} is not on the local network"));
            return None;
        }
        if !matches!(self.sockets.get(&id).map(|s| &s.kind), Some(Kind::TcpIdle)) {
            self.log_once("listen() needs an unused TCP socket".into());
            return None;
        }
        let bound = local_address_for(peer).and_then(|local| {
            let l = TcpListener::bind(SocketAddr::new(local, port))?;
            l.set_nonblocking(true)?;
            Ok(l)
        });
        match bound {
            Ok(listener) => {
                let at = listener.local_addr().ok();
                if let Some(s) = self.sockets.get_mut(&id) {
                    s.kind = Kind::TcpListen { listener, peer };
                }
                self.push(NetEvent::new(id, "listening"));
                at
            }
            Err(e) => {
                let line = format!("listen() for {peer}: {e}");
                self.log_once(line.clone());
                self.error_event(id, line);
                None
            }
        }
    }

    /// `socket.close()`: the socket is gone; a `close` event follows.
    pub fn close(&mut self, id: u32) {
        if let Some(s) = self.sockets.get_mut(&id) {
            if !matches!(s.kind, Kind::Closed) {
                s.kind = Kind::Closed;
                s.peer = None;
                self.push(NetEvent::new(id, "close"));
            }
        }
    }

    /// `socket.disconnect()`: forget the peer (UDP), close the stream (TCP).
    pub fn disconnect(&mut self, id: u32) {
        let tcp = matches!(self.sockets.get(&id).map(|s| &s.kind), Some(Kind::TcpStream(_)));
        if tcp {
            self.close(id);
        } else if let Some(s) = self.sockets.get_mut(&id) {
            s.peer = None;
        }
    }

    /// `socket.state`.
    pub fn state(&self, id: u32) -> i32 {
        match self.sockets.get(&id) {
            Some(Socket {
                kind: Kind::TcpStream(_),
                ..
            }) => state::CONNECTED,
            Some(Socket {
                kind: Kind::TcpListen { .. },
                ..
            }) => state::LISTENING,
            Some(Socket {
                kind: Kind::Udp(Some(_)),
                peer,
                ..
            }) => {
                if peer.is_some() {
                    state::CONNECTED
                } else {
                    state::BOUND
                }
            }
            _ => state::UNCONNECTED,
        }
    }

    /// `socket.address()`: the local address.
    pub fn local(&self, id: u32) -> Option<SocketAddr> {
        match &self.sockets.get(&id)?.kind {
            Kind::Udp(Some(s)) => s.local_addr().ok(),
            Kind::TcpStream(s) => s.local_addr().ok(),
            Kind::TcpListen { listener, .. } => listener.local_addr().ok(),
            _ => None,
        }
    }

    /// `socket.remoteAddress()`: the peer.
    pub fn remote(&self, id: u32) -> Option<SocketAddr> {
        self.sockets.get(&id)?.peer
    }

    // ------------------------------------------------------------ service sockets

    /// Open a service's discovery sockets: a broadcast socket (ephemeral port)
    /// sending to `broadcast`, and a listener on `listen_port`. Datagrams on
    /// either become `discovery` events. Returns notes about what failed.
    pub fn open_discovery(&mut self, broadcast: Option<SocketAddr>, listen_port: Option<u16>) -> Vec<String> {
        let mut notes = Vec::new();
        if let Some(dest) = broadcast {
            if !lan_allowed(dest.ip()) {
                notes.push(format!(
                    "UdpBroadcastAddress {} is not on the local network",
                    dest.ip()
                ));
            } else {
                let id = self.create(false) as u32;
                match self.udp(id) {
                    Ok(_) => {
                        if let Some(s) = self.sockets.get_mut(&id) {
                            s.discovery = true;
                        }
                        self.broadcast = Some((id, dest));
                    }
                    Err(e) => notes.push(format!("broadcast socket: {e}")),
                }
            }
        }
        if let Some(port) = listen_port {
            let id = self.create(false) as u32;
            match new_udp(port) {
                Ok(sock) => {
                    if let Some(s) = self.sockets.get_mut(&id) {
                        s.kind = Kind::Udp(Some(sock));
                        s.discovery = true;
                    }
                }
                Err(e) => notes.push(format!("Error Binding UdpListenPort [{port}]: {e}")),
            }
        }
        notes
    }

    /// `service.broadcast(payload)`: bytes sent or -1.
    pub fn broadcast(&mut self, data: &[u8]) -> i32 {
        match self.broadcast {
            Some((id, dest)) => self.send_to(id, data, dest),
            None => {
                self.log_once("Failed to broadcast packet. No valid sockets...".into());
                -1
            }
        }
    }

    // ------------------------------------------------------------------ HTTP

    /// Start an asynchronous request: its id (completion comes as an `http` event).
    pub fn start_http(&mut self, req: HttpRequest) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.stats.http_requests += 1;
        let held = self.dry_run && !req.method.eq_ignore_ascii_case("GET");
        if held || self.http.len() >= MAX_ASYNC_HTTP {
            let why = if held {
                self.stats.dry_run += 1;
                format!("dry run: {} {} not sent", req.method, req.url)
            } else {
                format!("more than {MAX_ASYNC_HTTP} requests in flight")
            };
            self.log_once(why.clone());
            let mut ev = NetEvent::new(id, "http");
            ev.http = Some(HttpResponse {
                error: Some(why),
                ..HttpResponse::default()
            });
            self.push(ev);
            return id;
        }
        let (tx, rx) = channel();
        let pins = self.pins.clone();
        let spawned = std::thread::Builder::new()
            .name("lightloom-http".into())
            .spawn(move || {
                let _ = tx.send(http::execute_with(&req, pins.as_ref()));
            });
        if spawned.is_ok() {
            self.http.push((id, rx));
        }
        id
    }

    /// Count a request made synchronously (outside the lock).
    pub fn record_http(&mut self, r: &HttpResponse) {
        self.stats.http_requests += 1;
        if r.status == 0 {
            self.stats.http_errors += 1;
            if let Some(e) = &r.error {
                self.log_once(format!("HTTP: {e}"));
            }
        }
    }

    // ------------------------------------------------------------------ DTLS

    fn dtls_event(&mut self, kind: &'static str, message: Option<String>) {
        let mut ev = NetEvent::new(0, kind);
        ev.message = message;
        self.push(ev);
    }

    /// `dtls.createConnection(host, port, identity, key)`: start the handshake
    /// with `addr` (a `dtlsOpen` or `dtlsError` event follows). An earlier
    /// connection is replaced (closed without a `dtlsClose` event).
    pub fn dtls_connect(&mut self, addr: SocketAddr, psk: Psk) -> bool {
        self.dtls_drop();
        if !lan_allowed(addr.ip()) {
            self.refuse(format!("dtls: {} is not on the local network", addr.ip()));
            self.dtls_event(
                "dtlsError",
                Some(format!("{} is not on the local network", addr.ip())),
            );
            return false;
        }
        if self.dry_run {
            // A dry run never opens device connections.
            self.held_back(0);
            self.log_once(format!("dry run: no DTLS connection to {addr}"));
            self.dtls_event("dtlsError", Some("dry run: not connected".into()));
            return false;
        }
        let (tx, rx) = channel();
        let spawned = std::thread::Builder::new()
            .name("lightloom-dtls".into())
            .spawn(move || {
                let _ = tx.send(DtlsSession::connect(addr, &psk, dtls::HANDSHAKE_TIMEOUT));
            });
        match spawned {
            Ok(_) => {
                self.dtls = DtlsSlot::Connecting(rx);
                true
            }
            Err(e) => {
                self.dtls_event("dtlsError", Some(format!("no handshake thread: {e}")));
                false
            }
        }
    }

    /// `dtls.send(data)`: bytes sent, or -1 without an established connection.
    pub fn dtls_send(&mut self, data: &[u8]) -> i32 {
        if data.len() > MAX_DATAGRAM {
            return self.refuse(format!(
                "dtls.send(): {} bytes is over the {MAX_DATAGRAM}-byte limit",
                data.len()
            ));
        }
        let r = match &mut self.dtls {
            DtlsSlot::Open(s) => s.send(data),
            _ => {
                self.log_once("dtls.send() before the connection is established".into());
                return -1;
            }
        };
        match r {
            Ok(n) => self.sent(n),
            Err(e) => {
                self.stats.errors += 1;
                self.log_once(format!("dtls: {e}"));
                -1
            }
        }
    }

    /// [`dtls_state`] of the connection.
    pub fn dtls_state(&self) -> i32 {
        match self.dtls {
            DtlsSlot::None => dtls_state::NONE,
            DtlsSlot::Connecting(_) => dtls_state::CONNECTING,
            DtlsSlot::Open(_) => dtls_state::OPEN,
        }
    }

    /// Close the connection (close_notify) or abandon the handshake; whether there was one.
    fn dtls_drop(&mut self) -> bool {
        match std::mem::replace(&mut self.dtls, DtlsSlot::None) {
            DtlsSlot::Open(mut s) => {
                s.close();
                true
            }
            // The handshake thread ends on its own; its session closes when dropped.
            DtlsSlot::Connecting(_) => true,
            DtlsSlot::None => false,
        }
    }

    /// `dtls.CloseConnection()`: close_notify, then a `dtlsClose` event.
    pub fn dtls_close(&mut self) {
        if self.dtls_drop() {
            self.dtls_event("dtlsClose", None);
        }
    }

    fn poll_dtls(&mut self) {
        let mut event = None;
        match &mut self.dtls {
            DtlsSlot::Connecting(rx) => match rx.try_recv() {
                Ok(Ok(session)) => {
                    self.stats.dtls_connections += 1;
                    self.dtls = DtlsSlot::Open(Box::new(session));
                    event = Some(("dtlsOpen", None));
                }
                Ok(Err(e)) => {
                    self.dtls = DtlsSlot::None;
                    event = Some(("dtlsError", Some(e)));
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.dtls = DtlsSlot::None;
                    event = Some(("dtlsError", Some("the handshake thread ended".into())));
                }
            },
            DtlsSlot::Open(s) => {
                if let Some(why) = s.poll() {
                    self.dtls = DtlsSlot::None;
                    event = Some(("dtlsClose", Some(why)));
                }
            }
            DtlsSlot::None => {}
        }
        if let Some((kind, message)) = event {
            if let Some(m) = &message {
                self.log_once(format!("dtls: {m}"));
                if kind == "dtlsError" {
                    self.stats.errors += 1;
                }
            }
            self.dtls_event(kind, message);
        }
    }

    // ------------------------------------------------------------------ poll

    /// Everything that arrived since the last poll, in order.
    pub fn poll(&mut self) -> Vec<NetEvent> {
        let mut buf = vec![0u8; 65_536];
        let ids: Vec<u32> = self.sockets.keys().copied().collect();
        for id in ids {
            self.poll_socket(id, &mut buf);
        }
        self.poll_dtls();
        let mut done = Vec::new();
        self.http.retain(|(id, rx)| match rx.try_recv() {
            Ok(r) => {
                done.push((*id, r));
                false
            }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => {
                done.push((
                    *id,
                    HttpResponse {
                        error: Some("request thread ended".into()),
                        ..HttpResponse::default()
                    },
                ));
                false
            }
        });
        self.sockets.retain(|_, s| !matches!(s.kind, Kind::Closed));
        for (id, r) in done {
            if r.status == 0 {
                self.stats.http_errors += 1;
                if let Some(e) = &r.error {
                    self.log_once(format!("HTTP: {e}"));
                }
            }
            let mut ev = NetEvent::new(id, "http");
            ev.http = Some(r);
            self.push(ev);
        }
        self.events.drain(..).collect()
    }

    fn poll_socket(&mut self, id: u32, buf: &mut [u8]) {
        let Some(s) = self.sockets.get_mut(&id) else {
            return;
        };
        let discovery = s.discovery;
        let mut out = Vec::new();
        let mut accepted = Vec::new();
        let mut closed = false;
        match &mut s.kind {
            Kind::Udp(Some(sock)) => {
                for _ in 0..256 {
                    match sock.recv_from(buf) {
                        Ok((n, from)) => {
                            if !lan_allowed(from.ip()) {
                                continue;
                            }
                            out.push((buf[..n].to_vec(), from));
                        }
                        // Windows reports ICMP port unreachable on the next receive.
                        Err(e) if e.kind() == ErrorKind::ConnectionReset => {}
                        Err(_) => break,
                    }
                }
            }
            Kind::TcpStream(stream) => {
                let peer = stream.peer_addr().ok();
                let _ = stream.set_nonblocking(true);
                for _ in 0..64 {
                    match stream.read(buf) {
                        Ok(0) => {
                            closed = true;
                            break;
                        }
                        Ok(n) => {
                            if let Some(p) = peer {
                                out.push((buf[..n].to_vec(), p));
                            }
                        }
                        Err(e) if e.kind() == ErrorKind::Interrupted => {}
                        Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                        Err(_) => {
                            closed = true;
                            break;
                        }
                    }
                }
                let _ = stream.set_nonblocking(false);
            }
            Kind::TcpListen { listener, peer } => {
                for _ in 0..4 {
                    match listener.accept() {
                        Ok((stream, from)) if from.ip() == *peer => accepted.push(stream),
                        Ok(_) => {} // someone else: dropped at once
                        Err(_) => break,
                    }
                }
            }
            _ => {}
        }
        for (data, from) in out {
            self.stats.received += 1;
            let mut ev = NetEvent::new(id, "message");
            ev.text = String::from_utf8_lossy(&data).into_owned();
            ev.data = data;
            ev.address = from.ip().to_string();
            ev.port = from.port();
            ev.discovery = discovery;
            self.push(ev);
        }
        if closed {
            self.close(id);
        }
        for stream in accepted {
            let client = self.create(true);
            if client < 0 {
                continue;
            }
            let client = client as u32;
            let _ = stream.set_nonblocking(false);
            let from = stream.peer_addr().ok();
            let _ = stream.set_write_timeout(Some(TCP_WRITE_TIMEOUT));
            let _ = stream.set_nodelay(true);
            if let Some(s) = self.sockets.get_mut(&client) {
                s.kind = Kind::TcpStream(stream);
                s.peer = from;
            }
            let mut ev = NetEvent::new(id, "connection");
            ev.client = Some(client);
            if let Some(f) = from {
                ev.address = f.ip().to_string();
                ev.port = f.port();
            }
            self.push(ev);
        }
    }
}

/// Connect a TCP stream to a local-network address (blocking, at most [`TCP_CONNECT_TIMEOUT`]).
pub fn connect_tcp(addr: SocketAddr) -> Result<TcpStream, String> {
    if !lan_allowed(addr.ip()) {
        return Err(format!("{} is not on the local network", addr.ip()));
    }
    TcpStream::connect_timeout(&addr, TCP_CONNECT_TIMEOUT).map_err(|e| format!("connecting to {addr}: {e}"))
}

fn to_bytes(values: &[i32]) -> Vec<u8> {
    values.iter().take(MAX_DATAGRAM + 1).map(|&v| v as u8).collect()
}

fn port_arg(port: i32) -> Option<u16> {
    u16::try_from(port).ok()
}

fn addr_json(a: Option<SocketAddr>) -> String {
    match a {
        Some(a) => serde_json::json!({
            "address": a.ip().to_string(),
            "port": a.port(),
            "protocol": if a.is_ipv4() { "IPv4" } else { "IPv6" },
        })
        .to_string(),
        None => serde_json::json!({ "address": "0.0.0.0", "port": 0, "protocol": "IPv4" }).to_string(),
    }
}

/// Add the network natives to `natives` (the object the host module captures).
/// Calls that block (TCP connect, synchronous HTTP, name lookups) suspend
/// `deadline`'s JavaScript budget while they wait.
pub(crate) fn install<'js>(
    ctx: &Ctx<'js>,
    natives: &Object<'js>,
    net: &SharedNet,
    deadline: &Deadline,
) -> rquickjs::Result<()> {
    // As in device.rs: a panic must never unwind through rquickjs.
    macro_rules! native {
        ($name:literal, |$n:ident, $dl:ident $(, $arg:ident : $ty:ty)*| $body:expr) => {{
            let shared = net.clone();
            let watchdog = deadline.clone();
            let f = Function::new(ctx.clone(), move |cx: Ctx<'_>, $($arg: $ty),*| -> rquickjs::Result<_> {
                let $n: &SharedNet = &shared;
                let $dl: &Deadline = &watchdog;
                let _ = ($n, $dl);
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)).map_err(|panic| {
                    let msg = format!("host panic in {}: {}", $name, panic_text(panic.as_ref()));
                    rquickjs::Exception::throw_internal(&cx, &msg)
                })
            })?;
            natives.set($name, f)?;
        }};
    }
    native!("netCreate", |n, d, tcp: bool| lock_net(n).create(tcp));
    native!("netBind", |n, d, id: i32, port: i32| match port_arg(port) {
        Some(p) => lock_net(n).bind(id as u32, p),
        None => false,
    });
    native!("netConnect", |n, d, id: i32, host: String, port: i32| {
        let Some(port) = port_arg(port) else { return false };
        if d.expired() {
            return false;
        }
        let _blocked = d.suspend();
        let addr = match resolve_lan(&host, port) {
            Ok(a) => a,
            Err(e) => {
                lock_net(n).refuse(format!("connect(): {e}"));
                return false;
            }
        };
        let tcp = matches!(
            lock_net(n).sockets.get(&(id as u32)).map(|s| &s.kind),
            Some(Kind::TcpIdle)
        );
        if !tcp {
            return lock_net(n).connect_udp(id as u32, addr);
        }
        if lock_net(n).dry_run {
            // A dry run never opens device connections.
            lock_net(n).held_back(0);
            return false;
        }
        match connect_tcp(addr) {
            Ok(stream) => lock_net(n).attach_stream(id as u32, stream),
            Err(e) => {
                let mut n = lock_net(n);
                n.log_once(e.clone());
                n.error_event(id as u32, e);
                false
            }
        }
    });
    native!("netSend", |n, d, id: i32, data: Vec<i32>| lock_net(n)
        .send(id as u32, &to_bytes(&data)));
    native!("netSendTo", |n,
                          d,
                          id: i32,
                          data: Vec<i32>,
                          host: String,
                          port: i32| {
        let Some(port) = port_arg(port) else { return -1 };
        // Literals resolve without a lookup; names may block briefly.
        let addr = {
            let _blocked = (!d.expired()).then(|| d.suspend());
            resolve_lan(&host, port)
        };
        match addr {
            Ok(a) => lock_net(n).send_to(id as u32, &to_bytes(&data), a),
            Err(e) => lock_net(n).refuse(format!("write(): {e}")),
        }
    });
    native!("netListen", |n, d, id: i32, port: i32, peer: String| {
        let (Some(port), Ok(peer)) = (port_arg(port), peer.trim().parse::<IpAddr>()) else {
            return String::new();
        };
        match lock_net(n).listen(id as u32, port, peer) {
            Some(a) => addr_json(Some(a)),
            None => String::new(),
        }
    });
    native!("netClose", |n, d, id: i32| lock_net(n).close(id as u32));
    native!("netDisconnect", |n, d, id: i32| lock_net(n).disconnect(id as u32));
    native!("netState", |n, d, id: i32| lock_net(n).state(id as u32));
    native!("netAddress", |n, d, id: i32| addr_json(
        lock_net(n).local(id as u32)
    ));
    native!("netRemote", |n, d, id: i32| addr_json(
        lock_net(n).remote(id as u32)
    ));
    native!("netPoll", |n, d| {
        serde_json::to_string(&lock_net(n).poll()).unwrap_or_else(|_| "[]".into())
    });
    native!("netBroadcast", |n, d, data: Vec<i32>| lock_net(n)
        .broadcast(&to_bytes(&data)));
    native!("httpSync", |n, d, json: String| {
        let req: HttpRequest = match serde_json::from_str(&json) {
            Ok(r) => r,
            Err(e) => {
                return serde_json::to_string(&HttpResponse {
                    error: Some(format!("bad request: {e}")),
                    ..HttpResponse::default()
                })
                .unwrap_or_default()
            }
        };
        let (held, pins) = {
            let mut n = lock_net(n);
            let held = n.dry_run && !req.method.eq_ignore_ascii_case("GET");
            if held {
                n.stats.dry_run += 1;
                n.stats.http_requests += 1;
                n.log_once(format!("dry run: {} {} not sent", req.method, req.url));
            }
            (held, n.pins.clone())
        };
        let r = if held {
            HttpResponse {
                error: Some("dry run: not sent".into()),
                ..HttpResponse::default()
            }
        } else if d.expired() {
            HttpResponse {
                error: Some("the plugin call is past its deadline".into()),
                timed_out: true,
                ..HttpResponse::default()
            }
        } else {
            let _blocked = d.suspend();
            let r = http::execute_with(&req, pins.as_ref());
            lock_net(n).record_http(&r);
            r
        };
        serde_json::to_string(&r).unwrap_or_default()
    });
    native!(
        "httpAsync",
        |n, d, json: String| match serde_json::from_str::<HttpRequest>(&json) {
            Ok(req) => lock_net(n).start_http(req) as i32,
            Err(_) => -1,
        }
    );
    native!("dtlsConnect", |n,
                            d,
                            host: String,
                            port: i32,
                            identity: String,
                            key: String| {
        let Some(port) = port_arg(port) else { return false };
        let psk = match Psk::from_hex(&identity, &key) {
            Ok(p) => p,
            Err(e) => {
                let mut n = lock_net(n);
                n.log_once(format!("dtls.createConnection(): {e}"));
                n.dtls_event("dtlsError", Some(e));
                return false;
            }
        };
        let addr = {
            let _blocked = (!d.expired()).then(|| d.suspend());
            resolve_lan(&host, port)
        };
        match addr {
            Ok(a) => lock_net(n).dtls_connect(a, psk),
            Err(e) => {
                let mut n = lock_net(n);
                n.refuse(format!("dtls.createConnection(): {e}"));
                n.dtls_event("dtlsError", Some(e));
                false
            }
        }
    });
    native!("dtlsSend", |n, d, data: Vec<i32>| lock_net(n)
        .dtls_send(&to_bytes(&data)));
    native!("dtlsState", |n, d| lock_net(n).dtls_state());
    native!("dtlsClose", |n, d| lock_net(n).dtls_close());
    native!("netResolve", |n, d, host: String| {
        let _blocked = (!d.expired()).then(|| d.suspend());
        resolve_all(&host)
    });
    Ok(())
}

/// Local-network addresses of `host` (`service.resolve`).
pub fn resolve_all(host: &str) -> Vec<String> {
    use std::net::ToSocketAddrs;
    let host = host.trim().trim_end_matches('.');
    if let Ok(ip) = host.parse::<IpAddr>() {
        return if lan_allowed(ip) {
            vec![ip.to_string()]
        } else {
            Vec::new()
        };
    }
    let mut seen = BTreeSet::new();
    (host, 0u16)
        .to_socket_addrs()
        .map(|it| {
            it.map(|a| a.ip())
                .filter(|ip| lan_allowed(*ip) && seen.insert(*ip))
                .map(|ip| ip.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// How long a poll loop of a service waits between event deliveries.
pub const SERVICE_TICK: Duration = Duration::from_millis(50);

/// Time left until `t`, at least zero.
pub fn until(t: Instant) -> Duration {
    t.saturating_duration_since(Instant::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(role: NetRole, mode: WriteMode) -> NetContext {
        NetContext::new(role, mode)
    }

    fn wait_events(n: &mut NetContext, want: usize) -> Vec<NetEvent> {
        let mut all = Vec::new();
        let end = Instant::now() + Duration::from_secs(3);
        while all.len() < want && Instant::now() < end {
            all.extend(n.poll());
            std::thread::sleep(Duration::from_millis(5));
        }
        all
    }

    #[test]
    fn udp_round_trip_on_loopback() {
        let mut n = ctx(NetRole::Device, WriteMode::Live);
        let peer = UdpSocket::bind("127.0.0.1:0").unwrap();
        let s = n.create(false) as u32;
        assert!(n.bind(s, 0));
        assert_eq!(n.send_to(s, b"hello", peer.local_addr().unwrap()), 5);
        let mut buf = [0u8; 16];
        let (len, from) = peer.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..len], b"hello");
        peer.send_to(b"\x01reply", from).unwrap();
        let evs = wait_events(&mut n, 2);
        assert_eq!(evs[0].kind, "listening");
        let msg = evs.iter().find(|e| e.kind == "message").unwrap();
        assert_eq!(msg.data, b"\x01reply");
        assert_eq!(msg.text, "\u{1}reply");
        assert_eq!(msg.address, "127.0.0.1");
        assert_eq!(n.stats().sent, 1);
        assert!(!n.bind(s, 0), "bind is one-shot");
    }

    #[test]
    fn refuses_the_internet_and_oversized_datagrams() {
        let mut n = ctx(NetRole::Device, WriteMode::Live);
        let s = n.create(false) as u32;
        assert_eq!(n.send_to(s, b"x", "8.8.8.8:53".parse().unwrap()), -1);
        assert_eq!(
            n.send_to(s, &[0; MAX_DATAGRAM + 1], "127.0.0.1:9".parse().unwrap()),
            -1
        );
        assert_eq!(n.stats().refused, 2);
        assert_eq!(n.send(s, b"no peer"), -1);
    }

    #[test]
    fn dry_run_holds_device_sends_back() {
        let mut n = ctx(NetRole::Device, WriteMode::DryRun);
        let peer = UdpSocket::bind("127.0.0.1:0").unwrap();
        peer.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
        let s = n.create(false) as u32;
        assert_eq!(n.send_to(s, b"abc", peer.local_addr().unwrap()), 3);
        assert!(peer.recv_from(&mut [0u8; 8]).is_err());
        assert_eq!((n.stats().sent, n.stats().dry_run), (0, 1));
        // Services are never in dry run.
        assert!(!ctx(NetRole::Service, WriteMode::DryRun).dry_run());
    }

    #[test]
    fn tcp_listen_accepts_only_the_peer() {
        let mut n = ctx(NetRole::Device, WriteMode::Live);
        let l = n.create(true) as u32;
        let at = n.listen(l, 0, "127.0.0.1".parse().unwrap()).unwrap();
        assert!(
            at.ip().is_loopback(),
            "bound to the interface facing the peer, not 0.0.0.0"
        );
        let mut bulb = TcpStream::connect(at).unwrap();
        let evs = wait_events(&mut n, 2);
        let conn = evs.iter().find(|e| e.kind == "connection").unwrap();
        let client = conn.client.unwrap();
        assert_eq!(n.state(client), state::CONNECTED);
        assert_eq!(n.send(client, b"{\"id\":1}\r\n"), 10);
        let mut buf = [0u8; 16];
        let k = bulb.read(&mut buf).unwrap();
        assert_eq!(&buf[..k], b"{\"id\":1}\r\n");
        bulb.write_all(b"ok").unwrap();
        drop(bulb);
        let evs = wait_events(&mut n, 2);
        assert!(evs.iter().any(|e| e.kind == "message" && e.text == "ok"));
        assert!(evs.iter().any(|e| e.kind == "close" && e.socket == client));
        // Listening for an internet peer is refused.
        let other = n.create(true) as u32;
        assert!(n.listen(other, 0, "8.8.8.8".parse().unwrap()).is_none());
    }

    #[test]
    fn discovery_sockets_and_broadcast() {
        let mut n = ctx(NetRole::Service, WriteMode::Live);
        let peer = UdpSocket::bind("127.0.0.1:0").unwrap();
        let notes = n.open_discovery(Some(peer.local_addr().unwrap()), None);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(n.broadcast(b"scan"), 4);
        let mut buf = [0u8; 8];
        let (len, from) = peer.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..len], b"scan");
        peer.send_to(b"here", from).unwrap();
        let evs = wait_events(&mut n, 1);
        assert!(evs[0].discovery);
        assert_eq!(evs[0].text, "here");
        let none = ctx(NetRole::Service, WriteMode::Live).broadcast(b"x");
        assert_eq!(none, -1);
    }

    #[test]
    fn socket_limit() {
        let mut n = ctx(NetRole::Device, WriteMode::Live);
        for _ in 0..MAX_SOCKETS {
            assert!(n.create(false) > 0);
        }
        assert_eq!(n.create(false), -1);
        n.close(1);
        assert!(n.create(true) > 0, "a closed socket frees its slot");
    }
}
