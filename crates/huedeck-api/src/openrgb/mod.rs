// SPDX-License-Identifier: GPL-3.0-or-later
//! OpenRGB SDK server: OpenRGB SDK clients (OpenRGB in client mode, its
//! plugins, Artemis, Aurora, openrgb-python, OpenRGB.NET ...) see HueDeck's
//! running devices as RGBControllers and drive their LEDs directly.
//!
//! Written from the protocol facts in `docs/research/m6-windows-integrations.md`
//! §2; see `docs/integrations/openrgb-sdk.md` for the user side.
//!
//! * Opt-in (Settings → OpenRGB SDK server, `settings.openrgbSdk`), loopback
//!   only, port [`crate::OPENRGB_SDK_PORT`]. A busy port (OpenRGB's own
//!   server, usually) is logged and tried again every [`RETRY`].
//! * Protocol versions 0 to [`wire::SERVER_PROTOCOL`] (4).
//! * One controller per running device with LEDs; one zone per canvas
//!   endpoint (the device body, each subdevice, each component on a channel);
//!   one mode, "Direct".
//! * UPDATELEDS / UPDATEZONELEDS / UPDATESINGLELED write the colours into the
//!   daemon's [`DirectLeds`], which win over the canvas (and the forced
//!   colour) for those endpoints while the client is connected. When the
//!   client disconnects, or the server stops, they return to the canvas; a
//!   server that stops refreshing them (hung) lets them expire after
//!   [`DIRECT_TTL`].
//! * RESIZEZONE is refused (channel lengths come from the components
//!   assigned in the dashboard); profiles are not supported (empty list,
//!   save/load/delete ignored); the plugin list is empty.
//! * When the device list changes, clients that negotiated version 1 or
//!   later get DEVICE_LIST_UPDATED, and their LED writes are ignored until
//!   they ask for the controller count again (their indexes are stale).

pub mod wire;

use crate::model::{DeviceView, Snapshot};
use crate::server::Backend;
use huedeck_engine::{DirectLeds, Rgb};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch, Semaphore};
use wire::{pid, Controller, Header, Mode, Zone, HEADER_LEN};

/// How long direct colours outlive the last sign of life of their client.
pub const DIRECT_TTL: Duration = Duration::from_secs(5);
/// How often a connection refreshes its colours and looks for device changes.
pub const TICK: Duration = Duration::from_secs(1);
/// How often a busy port is tried again.
pub const RETRY: Duration = Duration::from_secs(30);
/// Most clients served at once.
pub const MAX_CLIENTS: usize = 32;

/// One controller as a connection sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Device key.
    pub key: String,
    /// Zones: endpoint uid and LED count.
    pub zones: Vec<(String, usize)>,
    /// What REQUEST_CONTROLLER_DATA sends.
    pub controller: Controller,
}

impl Entry {
    fn led_count(&self) -> usize {
        self.zones.iter().map(|z| z.1).sum()
    }
}

/// The controllers for `snapshot`: running devices with at least one LED, in
/// snapshot order.
pub fn controllers(snapshot: &Snapshot) -> Vec<Entry> {
    snapshot
        .devices
        .iter()
        .filter(|d| d.state == "running")
        .filter_map(|d| controller(d, &snapshot.version))
        .collect()
}

fn controller(d: &DeviceView, version: &str) -> Option<Entry> {
    let mut zones = Vec::new();
    let mut leds = Vec::new();
    let mut ozones = Vec::new();
    for e in d.endpoints.iter().filter(|e| e.leds > 0) {
        let n = e.leds.min(u16::MAX as usize);
        let name = match (&e.channel, e.kind.as_str()) {
            (Some(ch), _) => format!("{ch}: {}", e.name),
            (None, "device") => "Main".to_string(),
            _ => e.name.clone(),
        };
        leds.extend((1..=n).map(|i| format!("{name} LED {i}")));
        ozones.push(Zone {
            name,
            ty: if n == 1 {
                wire::ZONE_TYPE_SINGLE
            } else {
                wire::ZONE_TYPE_LINEAR
            },
            leds_min: n as u32,
            leds_max: n as u32,
            leds_count: n as u32,
        });
        zones.push((e.uid.clone(), n));
    }
    if zones.is_empty() {
        return None;
    }
    let colors = vec![[0, 0, 0]; leds.len()];
    Some(Entry {
        key: d.key.clone(),
        zones,
        controller: Controller {
            ty: wire::device_type(&d.device_type),
            name: d.name.clone(),
            vendor: d.plugin.clone(),
            description: format!("{} (HueDeck plugin {})", d.name, d.plugin),
            version: format!("HueDeck {version}"),
            serial: d.key.clone(),
            location: format!("HueDeck: {}", d.key),
            modes: vec![Mode::direct()],
            active_mode: 0,
            zones: ozones,
            leds,
            colors,
        },
    })
}

/// What decides whether two device lists differ for a client.
fn signature(list: &[Entry]) -> Vec<(&str, &[(String, usize)])> {
    list.iter()
        .map(|e| (e.key.as_str(), e.zones.as_slice()))
        .collect()
}

// --------------------------------------------------------------- supervisor

/// Serve the SDK on `127.0.0.1:port` while the `openrgbSdk` setting is on,
/// until `stop` turns true. `taken_elsewhere(port)` tells whether another
/// program listens on the port even though binding would succeed (Windows
/// address sharing); such a port counts as busy.
pub async fn serve_when_enabled(
    backend: Arc<dyn Backend>,
    port: u16,
    mut stop: watch::Receiver<bool>,
    taken_elsewhere: Arc<dyn Fn(u16) -> bool + Send + Sync>,
) {
    let Some(direct) = backend.direct() else {
        tracing::warn!("the daemon does not support direct LED colours: no OpenRGB SDK server");
        return;
    };
    let mut running: Option<(watch::Sender<bool>, tokio::task::JoinHandle<()>)> = None;
    let mut warned = false;
    let mut next_try = tokio::time::Instant::now();
    loop {
        let wanted = backend.snapshot().settings.openrgb_sdk;
        if !wanted {
            warned = false;
            next_try = tokio::time::Instant::now();
            if let Some((tx, task)) = running.take() {
                let _ = tx.send(true);
                let _ = task.await;
                tracing::info!("OpenRGB SDK server stopped");
            }
        } else if running.is_none() && tokio::time::Instant::now() >= next_try {
            let bound = if taken_elsewhere(port) {
                Err(std::io::Error::new(
                    std::io::ErrorKind::AddrInUse,
                    "another program listens on it",
                ))
            } else {
                TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await
            };
            match bound {
                Ok(listener) => {
                    tracing::info!("OpenRGB SDK server on 127.0.0.1:{port}");
                    warned = false;
                    let (tx, rx) = watch::channel(false);
                    let task = tokio::spawn(serve(listener, Arc::clone(&backend), Arc::clone(&direct), rx));
                    running = Some((tx, task));
                }
                Err(e) => {
                    if !warned {
                        tracing::warn!(
                            "OpenRGB SDK server: port {port} is busy ({e}). Another program (OpenRGB's own \
                             SDK server?) uses it; HueDeck tries again every {} s. Stop that server to use \
                             HueDeck's.",
                            RETRY.as_secs()
                        );
                        warned = true;
                    }
                    next_try = tokio::time::Instant::now() + RETRY;
                }
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(TICK) => {}
            _ = stopped(&mut stop) => break,
        }
    }
    if let Some((tx, task)) = running.take() {
        let _ = tx.send(true);
        let _ = task.await;
    }
}

/// Accept SDK clients on `listener` until `stop` turns true; every
/// connection ends with it.
pub async fn serve(
    listener: TcpListener,
    backend: Arc<dyn Backend>,
    direct: Arc<DirectLeds>,
    mut stop: watch::Receiver<bool>,
) {
    let slots = Arc::new(Semaphore::new(MAX_CLIENTS));
    let mut tasks = tokio::task::JoinSet::new();
    let conn_stop = stop.clone();
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, peer)) => {
                    let Ok(permit) = Arc::clone(&slots).try_acquire_owned() else {
                        tracing::warn!("OpenRGB SDK: refusing {peer}, {MAX_CLIENTS} clients already connected");
                        continue;
                    };
                    let conn = Connection::new(Arc::clone(&backend), Arc::clone(&direct));
                    let stop = conn_stop.clone();
                    tasks.spawn(async move {
                        conn.run(stream, stop).await;
                        drop(permit);
                    });
                }
                Err(e) => {
                    tracing::debug!("OpenRGB SDK: accept failed: {e}");
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            },
            _ = stopped(&mut stop) => break,
        }
        while tasks.try_join_next().is_some() {}
    }
    while tasks.join_next().await.is_some() {}
}

/// Resolves once `stop` turns true (or its sender is gone).
async fn stopped(stop: &mut watch::Receiver<bool>) {
    let _ = stop.wait_for(|s| *s).await;
}

// --------------------------------------------------------------- connection

/// One client.
pub struct Connection {
    backend: Arc<dyn Backend>,
    direct: Arc<DirectLeds>,
    /// Writer id in [`DirectLeds`].
    owner: u64,
    /// Negotiated protocol version.
    proto: u32,
    client: String,
    /// The controllers the client's indexes refer to.
    list: Vec<Entry>,
    /// DEVICE_LIST_UPDATED was sent: indexes are stale until the next count.
    stale: bool,
}

impl Connection {
    /// A connection writing into `direct`.
    pub fn new(backend: Arc<dyn Backend>, direct: Arc<DirectLeds>) -> Self {
        let owner = direct.new_owner();
        let list = controllers(&backend.snapshot());
        Self {
            backend,
            direct,
            owner,
            proto: 0,
            client: String::new(),
            list,
            stale: false,
        }
    }

    /// Serve `stream` until the client leaves (or misbehaves) or `stop`
    /// turns true; the client's colours are released either way.
    pub async fn run(mut self, stream: TcpStream, mut stop: watch::Receiver<bool>) {
        let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_default();
        let _ = stream.set_nodelay(true);
        let (mut rd, mut wr) = stream.into_split();
        let (tx, mut rx) = mpsc::channel::<(Header, Vec<u8>)>(64);
        let reader = tokio::spawn(async move {
            loop {
                let mut h = [0u8; HEADER_LEN];
                if rd.read_exact(&mut h).await.is_err() {
                    return;
                }
                let header = match Header::decode(&h) {
                    Ok(h) => h,
                    Err(e) => {
                        tracing::debug!("OpenRGB SDK: dropping a client: {e}");
                        return;
                    }
                };
                let mut payload = vec![0u8; header.len as usize];
                if rd.read_exact(&mut payload).await.is_err() {
                    return;
                }
                if tx.send((header, payload)).await.is_err() {
                    return;
                }
            }
        });
        tracing::debug!("OpenRGB SDK: client connected from {peer}");
        let mut tick = tokio::time::interval(TICK);
        loop {
            let out = tokio::select! {
                packet = rx.recv() => match packet {
                    Some((h, payload)) => self.handle(h, &payload),
                    None => break,
                },
                _ = tick.tick() => self.on_tick(),
                _ = stopped(&mut stop) => break,
            };
            if let Some(bytes) = out {
                if wr.write_all(&bytes).await.is_err() {
                    break;
                }
            }
        }
        reader.abort();
        self.direct.release(self.owner);
        let who = if self.client.is_empty() {
            peer
        } else {
            format!("{} ({peer})", self.client)
        };
        tracing::info!("OpenRGB SDK: client {who} disconnected; its devices follow the canvas again");
    }

    /// Keep the colours alive; tell a versioned client the device list changed.
    pub fn on_tick(&mut self) -> Option<Vec<u8>> {
        self.direct.touch(self.owner, DIRECT_TTL);
        if self.stale || self.proto < 1 {
            return None;
        }
        let now = controllers(&self.backend.snapshot());
        if signature(&now) == signature(&self.list) {
            return None;
        }
        self.stale = true;
        Some(wire::packet(0, pid::DEVICE_LIST_UPDATED, &[]))
    }

    /// Answer one packet (`None`: no reply).
    pub fn handle(&mut self, h: Header, p: &[u8]) -> Option<Vec<u8>> {
        match h.id {
            pid::REQUEST_CONTROLLER_COUNT => {
                self.list = controllers(&self.backend.snapshot());
                self.stale = false;
                Some(wire::packet(
                    0,
                    pid::REQUEST_CONTROLLER_COUNT,
                    &wire::u32_payload(self.list.len() as u32),
                ))
            }
            pid::REQUEST_CONTROLLER_DATA => {
                let asked = wire::In::new(p).u32().unwrap_or(0);
                let v = asked.min(wire::SERVER_PROTOCOL);
                let Some(e) = self.list.get(h.dev as usize) else {
                    tracing::debug!("OpenRGB SDK: no controller {}", h.dev);
                    return None;
                };
                Some(wire::packet(
                    h.dev,
                    pid::REQUEST_CONTROLLER_DATA,
                    &e.controller.encode(v),
                ))
            }
            pid::REQUEST_PROTOCOL_VERSION => {
                let client = wire::In::new(p).u32().unwrap_or(0);
                self.proto = client.min(wire::SERVER_PROTOCOL);
                Some(wire::packet(
                    0,
                    pid::REQUEST_PROTOCOL_VERSION,
                    &wire::u32_payload(wire::SERVER_PROTOCOL),
                ))
            }
            pid::SET_CLIENT_NAME => {
                self.client = wire::parse_c_string(p).chars().take(128).collect();
                tracing::info!(
                    "OpenRGB SDK: client \"{}\" connected (protocol {})",
                    self.client,
                    self.proto
                );
                None
            }
            pid::REQUEST_PROFILE_LIST => Some(wire::packet(0, h.id, &wire::empty_list())),
            pid::REQUEST_PLUGIN_LIST => Some(wire::packet(0, h.id, &wire::empty_list())),
            pid::REQUEST_SAVE_PROFILE | pid::REQUEST_LOAD_PROFILE | pid::REQUEST_DELETE_PROFILE => {
                tracing::debug!("OpenRGB SDK: profiles are not supported (packet {})", h.id);
                None
            }
            pid::RESIZEZONE => {
                if let Ok((zone, size)) = wire::parse_resize_zone(p) {
                    tracing::info!(
                        "OpenRGB SDK: refusing to resize zone {zone} of controller {} to {size} LEDs: \
                         set channel components in HueDeck's dashboard",
                        h.dev
                    );
                }
                None
            }
            pid::UPDATELEDS => {
                if let Ok(colors) = wire::parse_update_leds(p) {
                    self.write_leds(h.dev, 0, &colors);
                }
                None
            }
            pid::UPDATEZONELEDS => {
                if let Ok((zone, colors)) = wire::parse_update_zone_leds(p) {
                    self.write_zone(h.dev, zone as usize, &colors);
                }
                None
            }
            pid::UPDATESINGLELED => {
                if let Ok((led, color)) = wire::parse_update_single_led(p) {
                    if let Ok(led) = usize::try_from(led) {
                        self.write_leds(h.dev, led, &[color]);
                    }
                }
                None
            }
            pid::SETCUSTOMMODE => None,
            pid::UPDATEMODE | pid::SAVEMODE => {
                if let Ok(mode) = wire::parse_mode_index(p) {
                    if mode != 0 {
                        tracing::debug!("OpenRGB SDK: only the Direct mode (0) exists, not {mode}");
                    }
                }
                None
            }
            // DEVICE_LIST_UPDATED echoes, rescans, plugin packets, newer packets.
            _ => None,
        }
    }

    fn entry(&self, dev: u32) -> Option<&Entry> {
        if self.stale {
            return None;
        }
        self.list.get(dev as usize)
    }

    /// Controller-wide LEDs from `start`, split across its zones.
    fn write_leds(&self, dev: u32, start: usize, colors: &[[u8; 3]]) {
        let Some(e) = self.entry(dev) else {
            return;
        };
        if start >= e.led_count() {
            return;
        }
        let mut zone_start = 0;
        for (uid, len) in &e.zones {
            let zone_end = zone_start + len;
            let from = start.max(zone_start);
            let to = (start + colors.len()).min(zone_end);
            if from < to {
                let part: Vec<Rgb> = colors[from - start..to - start]
                    .iter()
                    .map(|c| Rgb::new(c[0], c[1], c[2]))
                    .collect();
                self.direct
                    .write(self.owner, uid, *len, from - zone_start, &part, DIRECT_TTL);
            }
            zone_start = zone_end;
        }
    }

    fn write_zone(&self, dev: u32, zone: usize, colors: &[[u8; 3]]) {
        let Some(e) = self.entry(dev) else {
            return;
        };
        let Some((uid, len)) = e.zones.get(zone) else {
            return;
        };
        let part: Vec<Rgb> = colors.iter().map(|c| Rgb::new(c[0], c[1], c[2])).collect();
        self.direct.write(self.owner, uid, *len, 0, &part, DIRECT_TTL);
    }
}

#[cfg(test)]
mod tests;
