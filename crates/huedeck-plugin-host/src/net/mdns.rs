// SPDX-License-Identifier: GPL-3.0-or-later
//! DNS-SD browsing over mDNS with one-shot queries (RFC 6762 §5.1, RFC 6763).
//!
//! [`MdnsBrowser`] sends PTR questions for the service types a network plugin
//! lists (`MDns`) from an ephemeral UDP port to `224.0.0.251:5353`. Because
//! the source port is not 5353, responders answer by unicast to that port
//! (§6.7), so HueDeck never binds 5353 nor joins the mDNS group. Answers
//! (PTR, and the SRV / TXT / A records responders add) are collected per
//! service instance; a complete instance (port and IPv4 address known) is
//! reported as [`MdnsEvent::Found`], again when its data changes, and as
//! [`MdnsEvent::Lost`] after a goodbye (TTL 0) or once it has not answered for
//! [`EXPIRE_AFTER`]. Instances still missing their SRV or address get a
//! follow-up question.

use std::collections::{BTreeMap, BTreeSet};
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

/// The mDNS group and port.
pub const MDNS_ADDR: SocketAddr = SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(224, 0, 0, 251)), 5353);
/// Delays of the first queries after start, then every [`QUERY_EVERY`].
pub const FIRST_QUERIES: [Duration; 3] = [Duration::ZERO, Duration::from_secs(2), Duration::from_secs(6)];
/// Period of the queries after the first ones.
pub const QUERY_EVERY: Duration = Duration::from_secs(30);
/// An instance that has not answered for this long is lost (three missed queries and a margin).
pub const EXPIRE_AFTER: Duration = Duration::from_secs(95);

const TYPE_A: u16 = 1;
const TYPE_PTR: u16 = 12;
const TYPE_TXT: u16 = 16;
const TYPE_SRV: u16 = 33;
const TYPE_ANY: u16 = 255;
/// Class IN with the unicast-response bit (QU).
const CLASS_IN_QU: u16 = 0x8001;

/// A DNS name in lowercase with a trailing dot.
pub fn normalize(name: &str) -> String {
    let mut n = name.trim().to_ascii_lowercase();
    if !n.ends_with('.') {
        n.push('.');
    }
    n
}

fn push_name(out: &mut Vec<u8>, name: &str) {
    for label in name.trim_end_matches('.').split('.') {
        let l = &label.as_bytes()[..label.len().min(63)];
        out.push(l.len() as u8);
        out.extend_from_slice(l);
    }
    out.push(0);
}

/// A query packet with one question per `(name, type)`, unicast response requested.
pub fn build_query(id: u16, questions: &[(String, u16)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + questions.len() * 32);
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&[0, 0]); // flags: standard query
    out.extend_from_slice(&(questions.len() as u16).to_be_bytes());
    out.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    for (name, qtype) in questions {
        push_name(&mut out, name);
        out.extend_from_slice(&qtype.to_be_bytes());
        out.extend_from_slice(&CLASS_IN_QU.to_be_bytes());
    }
    out
}

/// Record data HueDeck uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RData {
    /// PTR target.
    Ptr(String),
    /// SRV.
    Srv {
        /// Port.
        port: u16,
        /// Target host name.
        target: String,
    },
    /// TXT strings.
    Txt(Vec<String>),
    /// IPv4 address.
    A(Ipv4Addr),
    /// Anything else.
    Other,
}

/// One resource record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// Owner name (normalised).
    pub name: String,
    /// Type.
    pub rtype: u16,
    /// TTL in seconds.
    pub ttl: u32,
    /// Data.
    pub data: RData,
}

fn read_name(buf: &[u8], mut pos: usize) -> Option<(String, usize)> {
    let mut labels: Vec<String> = Vec::new();
    let mut end = None;
    let mut jumps = 0;
    loop {
        let len = *buf.get(pos)? as usize;
        if len == 0 {
            pos += 1;
            break;
        }
        if len & 0xc0 == 0xc0 {
            let ptr = ((len & 0x3f) << 8) | *buf.get(pos + 1)? as usize;
            if end.is_none() {
                end = Some(pos + 2);
            }
            jumps += 1;
            if jumps > 32 || ptr >= buf.len() {
                return None;
            }
            pos = ptr;
            continue;
        }
        if len & 0xc0 != 0 {
            return None;
        }
        let label = buf.get(pos + 1..pos + 1 + len)?;
        labels.push(String::from_utf8_lossy(label).into_owned());
        pos += 1 + len;
        if labels.len() > 64 {
            return None;
        }
    }
    let mut name = labels.join(".");
    name.push('.');
    Some((name.to_ascii_lowercase(), end.unwrap_or(pos)))
}

fn u16_at(buf: &[u8], pos: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*buf.get(pos)?, *buf.get(pos + 1)?]))
}

/// The answer, authority and additional records of a response.
pub fn parse_response(buf: &[u8]) -> Option<Vec<Record>> {
    if buf.len() < 12 || buf[2] & 0x80 == 0 {
        return None; // not a response
    }
    let qd = u16_at(buf, 4)? as usize;
    let rr = u16_at(buf, 6)? as usize + u16_at(buf, 8)? as usize + u16_at(buf, 10)? as usize;
    let mut pos = 12;
    for _ in 0..qd {
        pos = read_name(buf, pos)?.1 + 4;
    }
    let mut out = Vec::new();
    for _ in 0..rr.min(256) {
        let (name, p) = read_name(buf, pos)?;
        let rtype = u16_at(buf, p)?;
        let ttl = u32::from_be_bytes(buf.get(p + 4..p + 8)?.try_into().ok()?);
        let len = u16_at(buf, p + 8)? as usize;
        let start = p + 10;
        let rdata = buf.get(start..start + len)?;
        let data = match rtype {
            TYPE_PTR => RData::Ptr(read_name(buf, start)?.0),
            TYPE_SRV if len >= 7 => RData::Srv {
                port: u16_at(buf, start + 4)?,
                target: read_name(buf, start + 6)?.0,
            },
            TYPE_TXT => {
                let mut strings = Vec::new();
                let mut i = 0;
                while i < rdata.len() {
                    let l = rdata[i] as usize;
                    let s = rdata.get(i + 1..i + 1 + l)?;
                    if !s.is_empty() {
                        strings.push(String::from_utf8_lossy(s).into_owned());
                    }
                    i += 1 + l;
                }
                RData::Txt(strings)
            }
            TYPE_A if len == 4 => RData::A(Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3])),
            _ => RData::Other,
        };
        out.push(Record {
            name,
            rtype,
            ttl,
            data,
        });
        pos = start + len;
    }
    Some(out)
}

/// A complete service instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MdnsService {
    /// Full instance name (`WLED-Desk._wled._tcp.local.`).
    pub instance: String,
    /// The instance label (`wled-desk`; names are compared and kept in lowercase).
    pub name: String,
    /// Service type (`_wled._tcp.local.`).
    pub service_type: String,
    /// Host name (`wled-desk.local.`).
    pub hostname: String,
    /// Port.
    pub port: u16,
    /// IPv4 address.
    pub ip: Ipv4Addr,
    /// TXT `key=value` pairs (keys lowercase; a bare key maps to "").
    pub txt: BTreeMap<String, String>,
}

impl MdnsService {
    /// The plugin-facing `Discovered(value)` object (docs/host-api/network-huedeck.md).
    pub fn to_value(&self) -> serde_json::Value {
        let mut v = serde_json::Map::new();
        for (k, val) in &self.txt {
            v.insert(k.clone(), val.clone().into());
        }
        let id = self.txt.get("id").cloned().unwrap_or_else(|| self.name.clone());
        v.insert("id".into(), id.into());
        v.insert("name".into(), self.name.clone().into());
        v.insert("type".into(), self.service_type.clone().into());
        v.insert("hostname".into(), self.hostname.clone().into());
        v.insert("port".into(), self.port.into());
        v.insert("ip".into(), self.ip.to_string().into());
        serde_json::Value::Object(v)
    }
}

/// What a poll found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MdnsEvent {
    /// A complete instance appeared or its data changed.
    Found(MdnsService),
    /// An instance said goodbye or stopped answering.
    Lost(MdnsService),
}

#[derive(Debug, Default)]
struct Instance {
    service_type: String,
    label: String,
    port: Option<u16>,
    target: Option<String>,
    txt: BTreeMap<String, String>,
    last_seen: Option<Instant>,
    reported: Option<MdnsService>,
}

/// Collects instances of the browsed types from responses.
#[derive(Debug)]
pub struct MdnsCache {
    types: BTreeSet<String>,
    instances: BTreeMap<String, Instance>,
    hosts: BTreeMap<String, Ipv4Addr>,
}

impl MdnsCache {
    /// A cache for `types` (`_wled._tcp.local.`, trailing dot optional).
    pub fn new(types: &[String]) -> Self {
        Self {
            types: types.iter().map(|t| normalize(t)).collect(),
            instances: BTreeMap::new(),
            hosts: BTreeMap::new(),
        }
    }

    /// The browsed types.
    pub fn types(&self) -> impl Iterator<Item = &String> {
        self.types.iter()
    }

    /// Take in the records of one response; returns the changes.
    pub fn ingest(&mut self, records: &[Record], now: Instant) -> Vec<MdnsEvent> {
        let mut events = Vec::new();
        let mut goodbye = BTreeSet::new();
        for r in records {
            if let (TYPE_PTR, RData::Ptr(target)) = (r.rtype, &r.data) {
                if self.types.contains(&r.name) && target.ends_with(&format!(".{}", r.name)) {
                    if r.ttl == 0 {
                        goodbye.insert(target.clone());
                        continue;
                    }
                    let inst = self.instances.entry(target.clone()).or_default();
                    inst.service_type = r.name.clone();
                    inst.label = target[..target.len() - r.name.len() - 1].to_string();
                    inst.last_seen = Some(now);
                }
            }
        }
        for r in records {
            match &r.data {
                RData::A(ip) => {
                    self.hosts.insert(r.name.clone(), *ip);
                }
                RData::Srv { port, target } => {
                    if let Some(inst) = self.instances.get_mut(&r.name) {
                        inst.port = Some(*port);
                        inst.target = Some(target.clone());
                        inst.last_seen = Some(now);
                    }
                }
                RData::Txt(strings) => {
                    if let Some(inst) = self.instances.get_mut(&r.name) {
                        inst.txt = strings
                            .iter()
                            .map(|s| match s.split_once('=') {
                                Some((k, v)) => (k.to_ascii_lowercase(), v.to_string()),
                                None => (s.to_ascii_lowercase(), String::new()),
                            })
                            .collect();
                    }
                }
                _ => {}
            }
        }
        for name in goodbye {
            if let Some(inst) = self.instances.remove(&name) {
                if let Some(s) = inst.reported {
                    events.push(MdnsEvent::Lost(s));
                }
            }
        }
        for (instance, inst) in &mut self.instances {
            let (Some(port), Some(target)) = (inst.port, &inst.target) else {
                continue;
            };
            let Some(ip) = self.hosts.get(target) else {
                continue;
            };
            let service = MdnsService {
                instance: instance.clone(),
                name: inst.label.clone(),
                service_type: inst.service_type.clone(),
                hostname: target.clone(),
                port,
                ip: *ip,
                txt: inst.txt.clone(),
            };
            if inst.reported.as_ref() != Some(&service) {
                inst.reported = Some(service.clone());
                events.push(MdnsEvent::Found(service));
            }
        }
        events
    }

    /// Instances not heard from for [`EXPIRE_AFTER`] are dropped (reported when they had been found).
    pub fn expire(&mut self, now: Instant) -> Vec<MdnsEvent> {
        let stale: Vec<String> = self
            .instances
            .iter()
            .filter(|(_, i)| i.last_seen.is_none_or(|t| now.duration_since(t) >= EXPIRE_AFTER))
            .map(|(k, _)| k.clone())
            .collect();
        stale
            .into_iter()
            .filter_map(|k| self.instances.remove(&k)?.reported.map(MdnsEvent::Lost))
            .collect()
    }

    /// Questions to ask: PTR for every type, plus SRV/TXT for instances
    /// without a port and A for host names without an address.
    pub fn questions(&self) -> Vec<(String, u16)> {
        let mut q: Vec<(String, u16)> = self.types.iter().map(|t| (t.clone(), TYPE_PTR)).collect();
        for (name, inst) in &self.instances {
            match &inst.target {
                None => q.push((name.clone(), TYPE_ANY)),
                Some(t) if !self.hosts.contains_key(t) => q.push((t.clone(), TYPE_A)),
                Some(_) => {}
            }
        }
        q.truncate(32);
        q
    }
}

/// A browser: the cache plus its socket and query schedule.
#[derive(Debug)]
pub struct MdnsBrowser {
    cache: MdnsCache,
    socket: Option<UdpSocket>,
    started: Instant,
    queries: u32,
    next_query: Instant,
    next_id: u16,
    error: Option<String>,
}

impl MdnsBrowser {
    /// Open an ephemeral socket and browse `types`. A socket that cannot be
    /// opened leaves the browser idle ([`MdnsBrowser::error`]).
    pub fn new(types: &[String]) -> Self {
        let socket = UdpSocket::bind("0.0.0.0:0").and_then(|s| {
            s.set_nonblocking(true)?;
            s.set_multicast_ttl_v4(255)?;
            Ok(s)
        });
        let now = Instant::now();
        let (socket, error) = match socket {
            Ok(s) => (Some(s), None),
            Err(e) => (None, Some(format!("mDNS socket: {e}"))),
        };
        Self {
            cache: MdnsCache::new(types),
            socket,
            started: now,
            queries: 0,
            next_query: now,
            next_id: 1,
            error,
        }
    }

    /// Why the browser is idle, if it is.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Send the queries that are due, read the answers, expire old instances.
    pub fn poll(&mut self, now: Instant) -> Vec<MdnsEvent> {
        let Some(socket) = &self.socket else {
            return Vec::new();
        };
        if now >= self.next_query {
            let packet = build_query(self.next_id, &self.cache.questions());
            self.next_id = self.next_id.wrapping_add(1).max(1);
            if let Err(e) = socket.send_to(&packet, MDNS_ADDR) {
                self.error = Some(format!("mDNS query: {e}"));
            }
            self.queries += 1;
            self.next_query = match FIRST_QUERIES.get(self.queries as usize) {
                Some(d) => self.started + *d,
                None => now + QUERY_EVERY,
            };
        }
        let mut events = Vec::new();
        let mut buf = [0u8; 9000];
        for _ in 0..64 {
            match socket.recv_from(&mut buf) {
                Ok((n, from)) => {
                    // Answers come from the local network only (responders send from 5353).
                    if !super::policy::lan_allowed(from.ip()) {
                        continue;
                    }
                    if let Some(records) = parse_response(&buf[..n]) {
                        let found = self.cache.ingest(&records, now);
                        // A new instance missing data: ask again soon.
                        if self.cache.questions().len() > self.cache.types.len() {
                            self.next_query = self.next_query.min(now + Duration::from_millis(500));
                        }
                        events.extend(found);
                    }
                }
                Err(_) => break,
            }
        }
        events.extend(self.cache.expire(now));
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 1035 §4.1 by hand: header, then `_wled._tcp.local.` PTR QU.
    #[test]
    fn query_packet_golden() {
        let q = build_query(0x1234, &[("_wled._tcp.local.".into(), TYPE_PTR)]);
        let mut expected = vec![0x12, 0x34, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        expected.extend_from_slice(b"\x05_wled\x04_tcp\x05local\x00");
        expected.extend_from_slice(&[0x00, 0x0c, 0x80, 0x01]);
        assert_eq!(q, expected);
    }

    fn rr(out: &mut Vec<u8>, name: &str, rtype: u16, ttl: u32, rdata: &[u8]) {
        push_name(out, name);
        out.extend_from_slice(&rtype.to_be_bytes());
        out.extend_from_slice(&0x8001u16.to_be_bytes());
        out.extend_from_slice(&ttl.to_be_bytes());
        out.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
        out.extend_from_slice(rdata);
    }

    fn name_bytes(name: &str) -> Vec<u8> {
        let mut v = Vec::new();
        push_name(&mut v, name);
        v
    }

    /// A WLED-like answer: PTR + SRV + TXT + A, the instance label in mixed case.
    fn wled_response(ttl: u32) -> Vec<u8> {
        let mut p = vec![0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 3];
        rr(
            &mut p,
            "_wled._tcp.local",
            TYPE_PTR,
            ttl,
            &name_bytes("WLED-Desk._wled._tcp.local"),
        );
        let mut srv = vec![0, 0, 0, 0, 0, 80];
        srv.extend(name_bytes("wled-desk.local"));
        rr(&mut p, "WLED-Desk._wled._tcp.local", TYPE_SRV, ttl, &srv);
        rr(
            &mut p,
            "WLED-Desk._wled._tcp.local",
            TYPE_TXT,
            ttl,
            b"\x0cmac=aabbccdd\x06ID=x1y",
        );
        rr(&mut p, "wled-desk.local", TYPE_A, ttl, &[192, 168, 1, 50]);
        p
    }

    #[test]
    fn finds_and_loses_an_instance() {
        let mut cache = MdnsCache::new(&["_wled._tcp.local.".into()]);
        let t0 = Instant::now();
        let records = parse_response(&wled_response(120)).unwrap();
        assert_eq!(records.len(), 4);
        let ev = cache.ingest(&records, t0);
        let MdnsEvent::Found(s) = &ev[0] else {
            panic!("{ev:?}")
        };
        assert_eq!(s.name, "wled-desk");
        assert_eq!((s.port, s.ip), (80, Ipv4Addr::new(192, 168, 1, 50)));
        assert_eq!(s.hostname, "wled-desk.local.");
        let v = s.to_value();
        assert_eq!(v["id"], "x1y");
        assert_eq!(v["mac"], "aabbccdd");
        assert_eq!(v["ip"], "192.168.1.50");
        // Same data again: nothing new.
        assert!(cache.ingest(&records, t0).is_empty());
        assert!(cache.expire(t0 + Duration::from_secs(60)).is_empty());
        let lost = cache.expire(t0 + EXPIRE_AFTER);
        assert!(matches!(&lost[..], [MdnsEvent::Lost(_)]));
        // Goodbye packets drop a found instance at once.
        cache.ingest(&records, t0);
        let bye = cache.ingest(&parse_response(&wled_response(0)).unwrap(), t0);
        assert!(matches!(&bye[..], [MdnsEvent::Lost(_)]), "{bye:?}");
    }

    #[test]
    fn asks_for_missing_records() {
        let mut cache = MdnsCache::new(&["_hue._tcp.local".into()]);
        let mut p = vec![0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 0];
        rr(
            &mut p,
            "_hue._tcp.local",
            TYPE_PTR,
            120,
            &name_bytes("Hue Bridge - 1A2B3C._hue._tcp.local"),
        );
        assert!(cache
            .ingest(&parse_response(&p).unwrap(), Instant::now())
            .is_empty());
        assert_eq!(
            cache.questions(),
            vec![
                ("_hue._tcp.local.".into(), TYPE_PTR),
                ("hue bridge - 1a2b3c._hue._tcp.local.".into(), TYPE_ANY)
            ]
        );
    }

    #[test]
    fn compressed_names_and_garbage() {
        // Name pointer back to offset 12.
        let mut p = vec![0, 0, 0x84, 0, 0, 0, 0, 2, 0, 0, 0, 0];
        rr(&mut p, "a.local", TYPE_A, 10, &[10, 0, 0, 1]);
        p.extend_from_slice(&[0xc0, 12, 0, 1, 0x80, 1, 0, 0, 0, 10, 0, 4, 10, 0, 0, 2]);
        let r = parse_response(&p).unwrap();
        assert_eq!(r[1].name, "a.local.");
        assert_eq!(r[1].data, RData::A(Ipv4Addr::new(10, 0, 0, 2)));
        // A pointer loop or truncation is rejected, never a hang or panic.
        let mut l = vec![0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 0];
        l.extend_from_slice(&[0xc0, 12]);
        assert!(parse_response(&l).is_none());
        assert!(parse_response(&[0; 5]).is_none());
        assert!(
            parse_response(&build_query(1, &[("x.".into(), 1)])).is_none(),
            "queries are ignored"
        );
    }
}
