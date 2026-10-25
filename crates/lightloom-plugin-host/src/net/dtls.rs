// SPDX-License-Identifier: GPL-3.0-or-later
//! A DTLS 1.2 client with a pre-shared key, for `device.addFeature("dtls")`
//! (docs/host-api/network-lightloom.md, "DTLS").
//!
//! Exactly what Philips Hue Entertainment streaming needs and nothing more:
//! one cipher suite, `TLS_PSK_WITH_AES_128_GCM_SHA256` (0x00A8, RFC 5487),
//! plain PSK key exchange (RFC 4279 §2), the DTLS 1.2 record and handshake
//! layers (RFC 6347) with the cookie exchange, retransmission of lost
//! flights and reassembly of fragmented handshake messages. No session
//! resumption, renegotiation, extensions or certificates; application data
//! from the server is read and dropped (the Hue bridge sends none).
//!
//! The cryptography is `ring`'s (AES-128-GCM, HMAC-SHA256, SHA-256, the
//! system random source); this module only frames bytes. Why not a DTLS
//! crate: docs/security.md, "DTLS".
//!
//! [`DtlsSession::connect`] runs the handshake (blocking, bounded by its
//! timeout; the host runs it on its own thread), then [`DtlsSession::send`]
//! encrypts one record per datagram.

use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_128_GCM};
use ring::rand::{SecureRandom, SystemRandom};
use ring::{digest, hmac};
use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

/// `TLS_PSK_WITH_AES_128_GCM_SHA256`.
pub const CIPHER_SUITE: u16 = 0x00A8;
/// DTLS 1.2 on the wire.
const VERSION: [u8; 2] = [0xFE, 0xFD];
/// DTLS 1.0, which servers may use in the record layer of their first flight.
const VERSION_10: [u8; 2] = [0xFE, 0xFF];
/// Default bound of the whole handshake.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// First retransmission timer (RFC 6347 §4.2.4.1 suggests 1 s; LAN peers answer in ms).
const FIRST_RETRANSMIT: Duration = Duration::from_millis(500);
/// Largest plaintext of one record (RFC 5246: 2^14).
pub const MAX_PLAINTEXT: usize = 1 << 14;
/// Longest PSK identity and key accepted.
const MAX_PSK: usize = 256;

const CT_CCS: u8 = 20;
const CT_ALERT: u8 = 21;
const CT_HANDSHAKE: u8 = 22;
const CT_APPDATA: u8 = 23;

const HS_CLIENT_HELLO: u8 = 1;
const HS_SERVER_HELLO: u8 = 2;
const HS_HELLO_VERIFY: u8 = 3;
const HS_CERTIFICATE: u8 = 11;
const HS_SERVER_KEY_EXCHANGE: u8 = 12;
const HS_CERTIFICATE_REQUEST: u8 = 13;
const HS_SERVER_HELLO_DONE: u8 = 14;
const HS_CLIENT_KEY_EXCHANGE: u8 = 16;
const HS_FINISHED: u8 = 20;

/// Record header: type, version, epoch, 48-bit sequence number, length.
const RECORD_HEADER: usize = 13;
/// Handshake header: type, length, message_seq, fragment offset and length.
const HS_HEADER: usize = 12;
/// AES-GCM explicit nonce + tag.
const GCM_OVERHEAD: usize = 8 + 16;

/// A pre-shared key: the identity sent in the clear and the secret.
#[derive(Clone, PartialEq, Eq)]
pub struct Psk {
    /// PSK identity (Hue: the application key / username, or the application id).
    pub identity: Vec<u8>,
    /// The key (Hue: the `clientkey`, hex-decoded: 16 bytes).
    pub key: Vec<u8>,
}

impl std::fmt::Debug for Psk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Psk")
            .field("identity", &String::from_utf8_lossy(&self.identity))
            .field("key", &format_args!("<{} bytes>", self.key.len()))
            .finish()
    }
}

impl Psk {
    /// `identity` as UTF-8 and `key_hex` as hexadecimal (SignalRGB's
    /// `dtls.createConnection(host, port, identity, keyHex)`).
    pub fn from_hex(identity: &str, key_hex: &str) -> Result<Self, String> {
        let key = decode_hex(key_hex.trim()).ok_or("the PSK is not an even number of hex digits")?;
        if key.is_empty() || key.len() > MAX_PSK {
            return Err(format!("the PSK must be 1..{MAX_PSK} bytes"));
        }
        if identity.is_empty() || identity.len() > MAX_PSK {
            return Err(format!("the PSK identity must be 1..{MAX_PSK} bytes"));
        }
        Ok(Self {
            identity: identity.as_bytes().to_vec(),
            key,
        })
    }
}

/// Hexadecimal text as bytes (`None` when it is not hex).
pub fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

// ------------------------------------------------------------------ crypto

/// The TLS 1.2 PRF with SHA-256 (RFC 5246 §5): `P_SHA256(secret, label + seed)`.
pub fn prf(secret: &[u8], label: &[u8], seed: &[u8], len: usize) -> Vec<u8> {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    let mut label_seed = label.to_vec();
    label_seed.extend_from_slice(seed);
    let mut out = Vec::with_capacity(len + 32);
    let mut a = hmac::sign(&key, &label_seed).as_ref().to_vec(); // A(1)
    while out.len() < len {
        let mut ctx = hmac::Context::with_key(&key);
        ctx.update(&a);
        ctx.update(&label_seed);
        out.extend_from_slice(ctx.sign().as_ref());
        a = hmac::sign(&key, &a).as_ref().to_vec();
    }
    out.truncate(len);
    out
}

/// The PSK premaster secret (RFC 4279 §2): `N`, N zero bytes, `N`, the key.
fn premaster(psk: &[u8]) -> Vec<u8> {
    let n = (psk.len() as u16).to_be_bytes();
    let mut out = Vec::with_capacity(4 + 2 * psk.len());
    out.extend_from_slice(&n);
    out.extend(std::iter::repeat_n(0u8, psk.len()));
    out.extend_from_slice(&n);
    out.extend_from_slice(psk);
    out
}

/// One direction's AES-128-GCM key and implicit nonce part.
struct Direction {
    key: LessSafeKey,
    salt: [u8; 4],
}

impl Direction {
    fn new(key: &[u8], salt: &[u8]) -> Self {
        let unbound = UnboundKey::new(&AES_128_GCM, key).expect("16-byte AES key");
        let mut s = [0u8; 4];
        s.copy_from_slice(salt);
        Self {
            key: LessSafeKey::new(unbound),
            salt: s,
        }
    }
}

/// Client and server write keys of one connection.
struct Keys {
    client: Direction,
    server: Direction,
}

/// Master secret and keys from the premaster secret and the two randoms.
fn derive(psk: &[u8], client_random: &[u8; 32], server_random: &[u8; 32]) -> ([u8; 48], Keys) {
    let mut seed = client_random.to_vec();
    seed.extend_from_slice(server_random);
    let ms = prf(&premaster(psk), b"master secret", &seed, 48);
    let mut master = [0u8; 48];
    master.copy_from_slice(&ms);
    let mut seed = server_random.to_vec();
    seed.extend_from_slice(client_random);
    // AES-128-GCM: no MAC keys; 16-byte keys, 4-byte implicit nonces (RFC 5288 §3).
    let block = prf(&master, b"key expansion", &seed, 40);
    let keys = Keys {
        client: Direction::new(&block[0..16], &block[32..36]),
        server: Direction::new(&block[16..32], &block[36..40]),
    };
    (master, keys)
}

fn seq_num(epoch: u16, seq: u64) -> u64 {
    (u64::from(epoch) << 48) | (seq & 0xFFFF_FFFF_FFFF)
}

fn aad(epoch: u16, seq: u64, content_type: u8, version: [u8; 2], len: usize) -> [u8; 13] {
    let mut a = [0u8; 13];
    a[..8].copy_from_slice(&seq_num(epoch, seq).to_be_bytes());
    a[8] = content_type;
    a[9..11].copy_from_slice(&version);
    a[11..13].copy_from_slice(&(len as u16).to_be_bytes());
    a
}

/// Encrypt `plain` as the fragment of record (`content_type`, `epoch`, `seq`):
/// explicit nonce (the record's sequence number), ciphertext, tag.
fn seal(dir: &Direction, content_type: u8, epoch: u16, seq: u64, plain: &[u8]) -> Vec<u8> {
    let explicit = seq_num(epoch, seq).to_be_bytes();
    let mut nonce = [0u8; 12];
    nonce[..4].copy_from_slice(&dir.salt);
    nonce[4..].copy_from_slice(&explicit);
    let mut buf = plain.to_vec();
    dir.key
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad(epoch, seq, content_type, VERSION, plain.len())),
            &mut buf,
        )
        .expect("AES-GCM seal");
    let mut out = explicit.to_vec();
    out.extend_from_slice(&buf);
    out
}

/// Decrypt the fragment of a protected record; `None` when it does not authenticate.
fn open(dir: &Direction, rec: &Record<'_>) -> Option<Vec<u8>> {
    let frag = rec.fragment;
    if frag.len() < GCM_OVERHEAD {
        return None;
    }
    let mut nonce = [0u8; 12];
    nonce[..4].copy_from_slice(&dir.salt);
    nonce[4..].copy_from_slice(&frag[..8]);
    let plain_len = frag.len() - GCM_OVERHEAD;
    let mut buf = frag[8..].to_vec();
    let plain = dir
        .key
        .open_in_place(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad(rec.epoch, rec.seq, rec.content_type, rec.version, plain_len)),
            &mut buf,
        )
        .ok()?;
    Some(plain.to_vec())
}

// ------------------------------------------------------------------ framing

/// One record of a datagram.
struct Record<'a> {
    content_type: u8,
    version: [u8; 2],
    epoch: u16,
    seq: u64,
    fragment: &'a [u8],
}

/// The records of a datagram (a truncated tail is dropped).
fn records(datagram: &[u8]) -> Vec<Record<'_>> {
    let mut out = Vec::new();
    let mut rest = datagram;
    while rest.len() >= RECORD_HEADER {
        let len = usize::from(u16::from_be_bytes([rest[11], rest[12]]));
        if rest.len() < RECORD_HEADER + len {
            break;
        }
        let mut seq = [0u8; 8];
        seq[2..].copy_from_slice(&rest[5..11]);
        out.push(Record {
            content_type: rest[0],
            version: [rest[1], rest[2]],
            epoch: u16::from_be_bytes([rest[3], rest[4]]),
            seq: u64::from_be_bytes(seq),
            fragment: &rest[RECORD_HEADER..RECORD_HEADER + len],
        });
        rest = &rest[RECORD_HEADER + len..];
    }
    out
}

fn record_bytes(content_type: u8, epoch: u16, seq: u64, fragment: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(RECORD_HEADER + fragment.len());
    out.push(content_type);
    out.extend_from_slice(&VERSION);
    out.extend_from_slice(&epoch.to_be_bytes());
    out.extend_from_slice(&seq.to_be_bytes()[2..]);
    out.extend_from_slice(&(fragment.len() as u16).to_be_bytes());
    out.extend_from_slice(fragment);
    out
}

fn u24(n: usize) -> [u8; 3] {
    let b = (n as u32).to_be_bytes();
    [b[1], b[2], b[3]]
}

fn read_u24(b: &[u8]) -> usize {
    (usize::from(b[0]) << 16) | (usize::from(b[1]) << 8) | usize::from(b[2])
}

/// A whole handshake message with its DTLS header (one fragment): the form
/// that is sent and that goes into the transcript (RFC 6347 §4.2.6).
fn handshake_message(msg_type: u8, message_seq: u16, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HS_HEADER + body.len());
    out.push(msg_type);
    out.extend_from_slice(&u24(body.len()));
    out.extend_from_slice(&message_seq.to_be_bytes());
    out.extend_from_slice(&u24(0));
    out.extend_from_slice(&u24(body.len()));
    out.extend_from_slice(body);
    out
}

/// A handshake fragment of a record.
struct Fragment<'a> {
    msg_type: u8,
    length: usize,
    message_seq: u16,
    offset: usize,
    data: &'a [u8],
}

fn fragments(plain: &[u8]) -> Result<Vec<Fragment<'_>>, String> {
    let mut out = Vec::new();
    let mut rest = plain;
    while !rest.is_empty() {
        if rest.len() < HS_HEADER {
            return Err("truncated handshake header".into());
        }
        let length = read_u24(&rest[1..4]);
        let offset = read_u24(&rest[6..9]);
        let flen = read_u24(&rest[9..12]);
        if rest.len() < HS_HEADER + flen || offset + flen > length || length > MAX_PLAINTEXT * 4 {
            return Err("bad handshake fragment".into());
        }
        out.push(Fragment {
            msg_type: rest[0],
            length,
            message_seq: u16::from_be_bytes([rest[4], rest[5]]),
            offset,
            data: &rest[HS_HEADER..HS_HEADER + flen],
        });
        rest = &rest[HS_HEADER + flen..];
    }
    Ok(out)
}

/// Handshake messages being put together from fragments, by message_seq.
#[derive(Default)]
struct Reassembly {
    partial: BTreeMap<u16, (u8, Vec<u8>, Vec<bool>)>,
}

impl Reassembly {
    /// Add a fragment; the whole message `(type, body)` once every byte is there.
    fn add(&mut self, f: &Fragment<'_>) -> Option<(u8, Vec<u8>)> {
        let entry = self
            .partial
            .entry(f.message_seq)
            .or_insert_with(|| (f.msg_type, vec![0; f.length], vec![false; f.length]));
        if entry.0 != f.msg_type || entry.1.len() != f.length {
            return None;
        }
        entry.1[f.offset..f.offset + f.data.len()].copy_from_slice(f.data);
        entry.2[f.offset..f.offset + f.data.len()].fill(true);
        if entry.2.iter().all(|&b| b) {
            let (t, body, _) = self.partial.remove(&f.message_seq)?;
            Some((t, body))
        } else {
            None
        }
    }
}

/// Alert description names (RFC 5246 §7.2, RFC 4279 §2).
fn alert_name(code: u8) -> &'static str {
    match code {
        0 => "close_notify",
        10 => "unexpected_message",
        20 => "bad_record_mac",
        40 => "handshake_failure",
        47 => "illegal_parameter",
        50 => "decode_error",
        51 => "decrypt_error",
        70 => "protocol_version",
        71 => "insufficient_security",
        80 => "internal_error",
        115 => "unknown_psk_identity",
        _ => "alert",
    }
}

// ------------------------------------------------------------------ client

/// What the client sends in one flight: (content type, epoch, plaintext).
type Flight = Vec<(u8, u16, Vec<u8>)>;

/// An established DTLS connection.
pub struct DtlsSession {
    socket: UdpSocket,
    peer: SocketAddr,
    keys: Keys,
    /// Next epoch-1 sequence number of the client.
    write_seq: u64,
    closed: bool,
}

impl std::fmt::Debug for DtlsSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DtlsSession")
            .field("peer", &self.peer)
            .field("write_seq", &self.write_seq)
            .field("closed", &self.closed)
            .finish_non_exhaustive()
    }
}

/// The handshake state of the client.
struct Handshake<'a> {
    socket: &'a UdpSocket,
    client_random: [u8; 32],
    /// Next client message_seq.
    send_seq: u16,
    /// Next epoch-0 record sequence number.
    epoch0_seq: u64,
    /// Next epoch-1 record sequence number.
    epoch1_seq: u64,
    /// Next server message_seq expected (None before the first server message).
    recv_seq: Option<u16>,
    transcript: Vec<u8>,
    reassembly: Reassembly,
    /// Server messages that arrived ahead of order.
    ahead: BTreeMap<u16, (u8, Vec<u8>)>,
    flight: Flight,
}

fn now_random(rng: &SystemRandom) -> Result<[u8; 32], String> {
    let mut r = [0u8; 32];
    rng.fill(&mut r).map_err(|_| "no system random source")?;
    Ok(r)
}

impl Handshake<'_> {
    fn client_hello(&self, cookie: &[u8]) -> Vec<u8> {
        let mut b = Vec::with_capacity(64 + cookie.len());
        b.extend_from_slice(&VERSION);
        b.extend_from_slice(&self.client_random);
        b.push(0); // session id
        b.push(cookie.len() as u8);
        b.extend_from_slice(cookie);
        b.extend_from_slice(&2u16.to_be_bytes());
        b.extend_from_slice(&CIPHER_SUITE.to_be_bytes());
        b.push(1); // compression methods
        b.push(0); // null
                   // An empty extensions block: optional in TLS 1.2, but some stacks
                   // (webrtc-dtls) do not parse a ClientHello without one.
        b.extend_from_slice(&0u16.to_be_bytes());
        b
    }

    /// Send the current flight (again): every record gets a fresh sequence number.
    fn send_flight(&mut self, keys: Option<&Keys>, peer: SocketAddr) -> Result<(), String> {
        let mut datagram = Vec::new();
        for (content_type, epoch, plain) in &self.flight {
            let rec = if *epoch == 0 {
                let r = record_bytes(*content_type, 0, self.epoch0_seq, plain);
                self.epoch0_seq += 1;
                r
            } else {
                let k = keys.ok_or("epoch 1 without keys")?;
                let frag = seal(&k.client, *content_type, 1, self.epoch1_seq, plain);
                let r = record_bytes(*content_type, 1, self.epoch1_seq, &frag);
                self.epoch1_seq += 1;
                r
            };
            datagram.extend_from_slice(&rec);
        }
        self.socket
            .send_to(&datagram, peer)
            .map_err(|e| format!("sending to {peer}: {e}"))?;
        Ok(())
    }

    /// Queue a handshake message of the next flight (and add it to the transcript).
    fn push_handshake(&mut self, msg_type: u8, body: &[u8], epoch: u16) {
        let msg = handshake_message(msg_type, self.send_seq, body);
        self.send_seq += 1;
        self.transcript.extend_from_slice(&msg);
        self.flight.push((CT_HANDSHAKE, epoch, msg));
    }
}

/// What one received datagram brought during the handshake.
enum Got {
    /// Whole handshake messages in order: (type, message_seq, body).
    Messages(Vec<(u8, u16, Vec<u8>)>),
    /// Only old (already processed) messages: the peer lost our last flight.
    Duplicate,
    /// A fatal alert.
    Alert(u8),
}

impl DtlsSession {
    /// Connect to `peer` and run the handshake (blocking, at most `timeout`).
    pub fn connect(peer: SocketAddr, psk: &Psk, timeout: Duration) -> Result<Self, String> {
        let bind: SocketAddr = match peer.ip() {
            IpAddr::V4(_) => (Ipv4Addr::UNSPECIFIED, 0).into(),
            IpAddr::V6(_) => (Ipv6Addr::UNSPECIFIED, 0).into(),
        };
        let socket = UdpSocket::bind(bind).map_err(|e| format!("opening a UDP socket: {e}"))?;
        socket
            .connect(peer)
            .map_err(|e| format!("connecting to {peer}: {e}"))?;
        let rng = SystemRandom::new();
        let mut hs = Handshake {
            socket: &socket,
            client_random: now_random(&rng)?,
            send_seq: 0,
            epoch0_seq: 0,
            epoch1_seq: 0,
            recv_seq: None,
            transcript: Vec::new(),
            reassembly: Reassembly::default(),
            ahead: BTreeMap::new(),
            flight: Vec::new(),
        };
        let deadline = Instant::now() + timeout;
        let hello = hs.client_hello(&[]);
        hs.push_handshake(HS_CLIENT_HELLO, &hello, 0);
        hs.send_flight(None, peer)?;
        let mut keys: Option<Keys> = None;
        let mut master = [0u8; 48];
        let mut server_random = [0u8; 32];
        // What the client waits for next.
        #[derive(PartialEq)]
        enum Wait {
            Hello,
            HelloDone,
            Finished,
        }
        let mut wait = Wait::Hello;
        let mut rto = FIRST_RETRANSMIT;
        let mut resend_at = Instant::now() + rto;
        let mut buf = vec![0u8; 65_536];
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(format!("DTLS handshake with {peer} timed out"));
            }
            if now >= resend_at {
                hs.send_flight(keys.as_ref(), peer)?;
                rto = (rto * 2).min(Duration::from_secs(4));
                resend_at = now + rto;
                continue;
            }
            let wait_for = resend_at.min(deadline) - now;
            socket
                .set_read_timeout(Some(wait_for.max(Duration::from_millis(1))))
                .map_err(|e| e.to_string())?;
            let n = match socket.recv(&mut buf) {
                Ok(n) => n,
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => continue,
                // ICMP port unreachable (Windows reports it on the next receive).
                Err(e) if e.kind() == ErrorKind::ConnectionReset => continue,
                Err(e) if e.kind() == ErrorKind::ConnectionRefused => {
                    return Err(format!("{peer} refused the connection"))
                }
                Err(e) => return Err(format!("receiving from {peer}: {e}")),
            };
            let got = receive(&mut hs, keys.as_ref(), &buf[..n])?;
            let messages = match got {
                Got::Alert(code) => {
                    return Err(format!(
                        "{peer} ended the handshake: {} ({code})",
                        alert_name(code)
                    ))
                }
                Got::Duplicate => {
                    // The server repeats its flight: ours was lost.
                    if wait != Wait::Hello {
                        hs.send_flight(keys.as_ref(), peer)?;
                        resend_at = Instant::now() + rto;
                    }
                    continue;
                }
                Got::Messages(m) => m,
            };
            for (msg_type, message_seq, body) in messages {
                let whole = handshake_message(msg_type, message_seq, &body);
                match (&wait, msg_type) {
                    (Wait::Hello, HS_HELLO_VERIFY) => {
                        if body.len() < 3 || body.len() < 3 + usize::from(body[2]) {
                            return Err("bad HelloVerifyRequest".into());
                        }
                        let cookie = body[3..3 + usize::from(body[2])].to_vec();
                        // The first ClientHello and the HelloVerifyRequest are not
                        // part of the transcript (RFC 6347 §4.2.1).
                        hs.transcript.clear();
                        hs.flight.clear();
                        let hello = hs.client_hello(&cookie);
                        hs.push_handshake(HS_CLIENT_HELLO, &hello, 0);
                        hs.send_flight(None, peer)?;
                        rto = FIRST_RETRANSMIT;
                        resend_at = Instant::now() + rto;
                    }
                    (Wait::Hello, HS_SERVER_HELLO) => {
                        hs.transcript.extend_from_slice(&whole);
                        server_random = parse_server_hello(&body)?;
                        wait = Wait::HelloDone;
                    }
                    (Wait::HelloDone, HS_SERVER_KEY_EXCHANGE) => {
                        // A PSK identity hint: Lightloom sends the configured identity anyway.
                        hs.transcript.extend_from_slice(&whole);
                    }
                    (Wait::HelloDone, HS_CERTIFICATE | HS_CERTIFICATE_REQUEST) => {
                        return Err("the server asked for a certificate exchange (not a PSK server)".into());
                    }
                    (Wait::HelloDone, HS_SERVER_HELLO_DONE) => {
                        hs.transcript.extend_from_slice(&whole);
                        let (ms, k) = derive(&psk.key, &hs.client_random, &server_random);
                        master = ms;
                        keys = Some(k);
                        hs.flight.clear();
                        let mut cke = (psk.identity.len() as u16).to_be_bytes().to_vec();
                        cke.extend_from_slice(&psk.identity);
                        hs.push_handshake(HS_CLIENT_KEY_EXCHANGE, &cke, 0);
                        hs.flight.push((CT_CCS, 0, vec![1]));
                        let hash = digest::digest(&digest::SHA256, &hs.transcript);
                        let verify = prf(&master, b"client finished", hash.as_ref(), 12);
                        hs.push_handshake(HS_FINISHED, &verify, 1);
                        hs.send_flight(keys.as_ref(), peer)?;
                        rto = FIRST_RETRANSMIT;
                        resend_at = Instant::now() + rto;
                        wait = Wait::Finished;
                    }
                    (Wait::Finished, HS_FINISHED) => {
                        let hash = digest::digest(&digest::SHA256, &hs.transcript);
                        let expected = prf(&master, b"server finished", hash.as_ref(), 12);
                        if !constant_time_eq(&expected, &body) {
                            return Err("the server's Finished does not verify (wrong PSK?)".into());
                        }
                        let _ = socket.set_read_timeout(None);
                        socket.set_nonblocking(true).map_err(|e| e.to_string())?;
                        let write_seq = hs.epoch1_seq;
                        drop(hs);
                        return Ok(Self {
                            socket,
                            peer,
                            keys: keys.ok_or("no keys")?,
                            write_seq,
                            closed: false,
                        });
                    }
                    (_, t) => {
                        return Err(format!("unexpected handshake message {t}"));
                    }
                }
            }
        }
    }

    /// The server's address.
    pub fn peer(&self) -> SocketAddr {
        self.peer
    }

    /// The connection was closed (by [`close`](Self::close) or the server).
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Encrypt `data` as one application-data record and send it: bytes of
    /// application data sent.
    pub fn send(&mut self, data: &[u8]) -> Result<usize, String> {
        if self.closed {
            return Err("the DTLS connection is closed".into());
        }
        if data.len() > MAX_PLAINTEXT {
            return Err(format!(
                "{} bytes is over the {MAX_PLAINTEXT}-byte record limit",
                data.len()
            ));
        }
        let frag = seal(&self.keys.client, CT_APPDATA, 1, self.write_seq, data);
        let rec = record_bytes(CT_APPDATA, 1, self.write_seq, &frag);
        self.write_seq += 1;
        match self.socket.send(&rec) {
            Ok(_) => Ok(data.len()),
            Err(e) => Err(format!("sending to {}: {e}", self.peer)),
        }
    }

    /// Read what the server sent (non-blocking): `Some(reason)` once it closed
    /// the connection (alert) or the socket failed.
    pub fn poll(&mut self) -> Option<String> {
        if self.closed {
            return None;
        }
        let mut buf = [0u8; 2048];
        for _ in 0..64 {
            match self.socket.recv(&mut buf) {
                Ok(n) => {
                    for rec in records(&buf[..n]) {
                        if rec.epoch != 1 || rec.content_type != CT_ALERT {
                            continue; // application data and stray handshake records
                        }
                        if let Some(plain) = open(&self.keys.server, &rec) {
                            if plain.len() == 2 && (plain[0] == 2 || plain[1] == 0) {
                                self.closed = true;
                                return Some(format!(
                                    "the peer closed the connection: {} ({})",
                                    alert_name(plain[1]),
                                    plain[1]
                                ));
                            }
                        }
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == ErrorKind::ConnectionReset => {
                    self.closed = true;
                    return Some(format!("{} is unreachable", self.peer));
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => {
                    self.closed = true;
                    return Some(format!("receiving from {}: {e}", self.peer));
                }
            }
        }
        None
    }

    /// Send `close_notify` and close.
    pub fn close(&mut self) {
        if self.closed {
            return;
        }
        let frag = seal(&self.keys.client, CT_ALERT, 1, self.write_seq, &[1, 0]);
        let rec = record_bytes(CT_ALERT, 1, self.write_seq, &frag);
        self.write_seq += 1;
        let _ = self.socket.send(&rec);
        self.closed = true;
    }
}

impl Drop for DtlsSession {
    fn drop(&mut self) {
        self.close();
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn parse_server_hello(body: &[u8]) -> Result<[u8; 32], String> {
    if body.len() < 38 {
        return Err("short ServerHello".into());
    }
    if body[..2] != VERSION {
        return Err(format!(
            "the server picked version {:02x}{:02x}, not DTLS 1.2",
            body[0], body[1]
        ));
    }
    let mut random = [0u8; 32];
    random.copy_from_slice(&body[2..34]);
    let sid = usize::from(body[34]);
    let at = 35 + sid;
    if body.len() < at + 3 {
        return Err("short ServerHello".into());
    }
    let suite = u16::from_be_bytes([body[at], body[at + 1]]);
    if suite != CIPHER_SUITE {
        return Err(format!("the server picked cipher suite {suite:#06x}"));
    }
    if body[at + 2] != 0 {
        return Err("the server picked compression".into());
    }
    Ok(random)
}

/// Process one datagram of the server's handshake flights.
fn receive(hs: &mut Handshake<'_>, keys: Option<&Keys>, datagram: &[u8]) -> Result<Got, String> {
    let mut whole: Vec<(u8, u16, Vec<u8>)> = Vec::new();
    let mut old = false;
    for rec in records(datagram) {
        if rec.version != VERSION && rec.version != VERSION_10 {
            continue;
        }
        let plain = match rec.epoch {
            0 => rec.fragment.to_vec(),
            1 => match keys.and_then(|k| open(&k.server, &rec)) {
                Some(p) => p,
                None => continue, // not ours, or ahead of our keys
            },
            _ => continue,
        };
        match rec.content_type {
            CT_ALERT => {
                if plain.len() == 2 && plain[0] == 2 {
                    return Ok(Got::Alert(plain[1]));
                }
            }
            CT_HANDSHAKE => {
                for f in fragments(&plain)? {
                    if let Some(next) = hs.recv_seq {
                        if f.message_seq < next {
                            old = true;
                            continue;
                        }
                    }
                    if let Some((t, body)) = hs.reassembly.add(&f) {
                        whole.push((t, f.message_seq, body));
                    }
                }
            }
            _ => {} // ChangeCipherSpec: the Finished in epoch 1 is what counts
        }
    }
    // Put complete messages in message_seq order; the first server message
    // (HelloVerifyRequest or ServerHello) sets where the sequence starts.
    for (t, seq, body) in whole {
        hs.ahead.insert(seq, (t, body));
    }
    let mut out = Vec::new();
    loop {
        let next = match hs.recv_seq {
            Some(n) => n,
            None => match hs
                .ahead
                .iter()
                .find(|(_, (t, _))| matches!(*t, HS_HELLO_VERIFY | HS_SERVER_HELLO))
            {
                Some((&s, _)) => s,
                None => break,
            },
        };
        let Some((t, body)) = hs.ahead.remove(&next) else {
            break;
        };
        hs.recv_seq = Some(next + 1);
        out.push((t, next, body));
    }
    if out.is_empty() && old {
        return Ok(Got::Duplicate);
    }
    Ok(Got::Messages(out))
}

// ------------------------------------------------------------------ test server

/// A loopback DTLS 1.2 PSK server for tests: one client, the cookie
/// exchange, `TLS_PSK_WITH_AES_128_GCM_SHA256`, application data handed to a
/// channel. Never part of a release build (feature `test-support`).
#[cfg(any(test, feature = "test-support"))]
pub mod test_server {
    use super::*;
    use std::sync::mpsc::{channel, Receiver};
    use std::thread::JoinHandle;

    /// How the test server behaves.
    #[derive(Clone, Debug)]
    pub struct ServerOptions {
        /// The identity it expects (others get `unknown_psk_identity`).
        pub identity: Vec<u8>,
        /// Its key.
        pub key: Vec<u8>,
        /// Send a HelloVerifyRequest first.
        pub cookie: bool,
        /// Drop this many of the first datagrams it receives (retransmission tests).
        pub drop_first: usize,
        /// Split the ServerHello into two fragments (reassembly tests).
        pub fragment_hello: bool,
        /// Send a ServerKeyExchange with this identity hint.
        pub hint: Option<Vec<u8>>,
        /// Stop after this long.
        pub lifetime: Duration,
    }

    impl Default for ServerOptions {
        fn default() -> Self {
            Self {
                identity: b"identity".to_vec(),
                key: vec![0x11; 16],
                cookie: true,
                drop_first: 0,
                fragment_hello: false,
                hint: None,
                lifetime: Duration::from_secs(20),
            }
        }
    }

    /// What the server saw.
    #[derive(Debug, PartialEq, Eq)]
    pub enum ServerEvent {
        /// The handshake completed.
        Connected,
        /// It failed (and the server sent this alert, when any).
        Failed(String),
        /// One application-data record.
        Data(Vec<u8>),
        /// The client sent close_notify.
        Closed,
    }

    /// A running test server.
    pub struct TestServer {
        /// Where it listens (127.0.0.1).
        pub addr: SocketAddr,
        /// What happened, in order.
        pub events: Receiver<ServerEvent>,
        /// The server thread.
        pub thread: JoinHandle<()>,
    }

    /// Start a server on an ephemeral loopback port.
    pub fn start(opts: ServerOptions) -> TestServer {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("bind loopback");
        let addr = socket.local_addr().expect("local addr");
        let (tx, events) = channel();
        let thread = std::thread::spawn(move || {
            let end = Instant::now() + opts.lifetime;
            let _ = serve(&socket, &opts, end, &|e| {
                let _ = tx.send(e);
            });
        });
        TestServer { addr, events, thread }
    }

    fn serve(socket: &UdpSocket, o: &ServerOptions, end: Instant, emit: &dyn Fn(ServerEvent)) -> Option<()> {
        let rng = SystemRandom::new();
        let cookie: Vec<u8> = {
            let mut c = [0u8; 16];
            rng.fill(&mut c).ok()?;
            c.to_vec()
        };
        let server_random = now_random(&rng).ok()?;
        let mut buf = vec![0u8; 65_536];
        let mut dropped = 0;
        let mut seq0 = 0u64;
        let mut seq1 = 0u64;
        let mut next_msg = 0u16;
        let mut transcript = Vec::new();
        let mut client_random = [0u8; 32];
        let mut keys: Option<Keys> = None;
        let mut master = [0u8; 48];
        let mut last_flight: Vec<u8> = Vec::new();
        let mut identity_ok = false;
        let mut connected = false;
        let mut hello_seen = false;
        let mut peer: Option<SocketAddr> = None;
        let send = |datagram: &[u8], to: SocketAddr| {
            let _ = socket.send_to(datagram, to);
        };
        loop {
            let left = end.checked_duration_since(Instant::now())?;
            socket
                .set_read_timeout(Some(left.max(Duration::from_millis(1))))
                .ok()?;
            let (n, from) = match socket.recv_from(&mut buf) {
                Ok(x) => x,
                Err(e) if e.kind() == ErrorKind::ConnectionReset => continue,
                Err(_) => return None,
            };
            if dropped < o.drop_first {
                dropped += 1;
                continue;
            }
            peer.get_or_insert(from);
            let datagram = buf[..n].to_vec();
            for rec in records(&datagram) {
                let plain = match rec.epoch {
                    0 => rec.fragment.to_vec(),
                    1 => match keys.as_ref().and_then(|k| open(&k.client, &rec)) {
                        Some(p) => p,
                        None => {
                            emit(ServerEvent::Failed("a record did not authenticate".into()));
                            let alert = record_bytes(CT_ALERT, 0, seq0, &[2, 20]);
                            send(&alert, from);
                            return None;
                        }
                    },
                    _ => continue,
                };
                match rec.content_type {
                    CT_APPDATA if connected => emit(ServerEvent::Data(plain)),
                    CT_ALERT => {
                        emit(ServerEvent::Closed);
                        return Some(());
                    }
                    CT_HANDSHAKE => {
                        let frags = fragments(&plain).ok()?;
                        for f in frags {
                            if f.offset != 0 || f.data.len() != f.length {
                                continue; // the client never fragments
                            }
                            let whole = handshake_message(f.msg_type, f.message_seq, f.data);
                            match f.msg_type {
                                HS_CLIENT_HELLO => {
                                    let body = f.data;
                                    let sid = usize::from(*body.get(34)?);
                                    let clen = usize::from(*body.get(35 + sid)?);
                                    let got_cookie = body.get(36 + sid..36 + sid + clen)?;
                                    if hello_seen {
                                        send(&last_flight, from); // a retransmitted hello
                                        continue;
                                    }
                                    if o.cookie && got_cookie != cookie.as_slice() {
                                        let mut hvr = VERSION.to_vec();
                                        hvr.push(cookie.len() as u8);
                                        hvr.extend_from_slice(&cookie);
                                        let msg = handshake_message(HS_HELLO_VERIFY, 0, &hvr);
                                        send(&record_bytes(CT_HANDSHAKE, 0, seq0, &msg), from);
                                        seq0 += 1;
                                        next_msg = 1;
                                        continue;
                                    }
                                    hello_seen = true;
                                    client_random.copy_from_slice(body.get(2..34)?);
                                    transcript.extend_from_slice(&whole);
                                    let mut sh = VERSION.to_vec();
                                    sh.extend_from_slice(&server_random);
                                    sh.push(0);
                                    sh.extend_from_slice(&CIPHER_SUITE.to_be_bytes());
                                    sh.push(0);
                                    sh.extend_from_slice(&0u16.to_be_bytes()); // no extensions
                                    let mut msgs = Vec::new();
                                    let hello = handshake_message(HS_SERVER_HELLO, next_msg, &sh);
                                    transcript.extend_from_slice(&hello);
                                    if o.fragment_hello {
                                        // Two fragments of the ServerHello, second one first.
                                        let split = sh.len() / 2;
                                        for (off, part) in [(split, &sh[split..]), (0, &sh[..split])] {
                                            let mut m = vec![HS_SERVER_HELLO];
                                            m.extend_from_slice(&u24(sh.len()));
                                            m.extend_from_slice(&next_msg.to_be_bytes());
                                            m.extend_from_slice(&u24(off));
                                            m.extend_from_slice(&u24(part.len()));
                                            m.extend_from_slice(part);
                                            msgs.push(m);
                                        }
                                    } else {
                                        msgs.push(hello);
                                    }
                                    next_msg += 1;
                                    if let Some(hint) = &o.hint {
                                        let mut ske = (hint.len() as u16).to_be_bytes().to_vec();
                                        ske.extend_from_slice(hint);
                                        let m = handshake_message(HS_SERVER_KEY_EXCHANGE, next_msg, &ske);
                                        transcript.extend_from_slice(&m);
                                        msgs.push(m);
                                        next_msg += 1;
                                    }
                                    let done = handshake_message(HS_SERVER_HELLO_DONE, next_msg, &[]);
                                    transcript.extend_from_slice(&done);
                                    msgs.push(done);
                                    next_msg += 1;
                                    last_flight.clear();
                                    for m in msgs {
                                        last_flight.extend_from_slice(&record_bytes(
                                            CT_HANDSHAKE,
                                            0,
                                            seq0,
                                            &m,
                                        ));
                                        seq0 += 1;
                                    }
                                    send(&last_flight, from);
                                }
                                HS_CLIENT_KEY_EXCHANGE => {
                                    let len =
                                        usize::from(u16::from_be_bytes([*f.data.first()?, *f.data.get(1)?]));
                                    let identity = f.data.get(2..2 + len)?;
                                    if keys.is_none() {
                                        transcript.extend_from_slice(&whole);
                                        identity_ok = identity == o.identity.as_slice();
                                        let (ms, k) = derive(&o.key, &client_random, &server_random);
                                        master = ms;
                                        keys = Some(k);
                                    }
                                    if !identity_ok {
                                        emit(ServerEvent::Failed("unknown PSK identity".into()));
                                        send(&record_bytes(CT_ALERT, 0, seq0, &[2, 115]), from);
                                        return None;
                                    }
                                }
                                HS_FINISHED => {
                                    let k = keys.as_ref()?;
                                    if !connected {
                                        let hash = digest::digest(&digest::SHA256, &transcript);
                                        let expected = prf(&master, b"client finished", hash.as_ref(), 12);
                                        if !constant_time_eq(&expected, f.data) {
                                            emit(ServerEvent::Failed(
                                                "client Finished does not verify".into(),
                                            ));
                                            send(&record_bytes(CT_ALERT, 0, seq0, &[2, 51]), from);
                                            return None;
                                        }
                                        transcript.extend_from_slice(&whole);
                                        let hash = digest::digest(&digest::SHA256, &transcript);
                                        let verify = prf(&master, b"server finished", hash.as_ref(), 12);
                                        let fin = handshake_message(HS_FINISHED, next_msg, &verify);
                                        let mut flight = record_bytes(CT_CCS, 0, seq0, &[1]);
                                        seq0 += 1;
                                        let frag = seal(&k.server, CT_HANDSHAKE, 1, seq1, &fin);
                                        flight.extend_from_slice(&record_bytes(CT_HANDSHAKE, 1, seq1, &frag));
                                        seq1 += 1;
                                        last_flight = flight;
                                        connected = true;
                                        emit(ServerEvent::Connected);
                                    }
                                    send(&last_flight, from);
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_server::{start, ServerEvent, ServerOptions};
    use super::*;

    fn psk() -> Psk {
        Psk::from_hex("identity", &"11".repeat(16)).unwrap()
    }

    fn next(events: &std::sync::mpsc::Receiver<ServerEvent>) -> ServerEvent {
        events
            .recv_timeout(Duration::from_secs(10))
            .expect("a server event")
    }

    #[test]
    fn prf_matches_openssl() {
        // `openssl kdf -keylen 100 -kdfopt digest:SHA256 -kdfopt hexsecret:9bbe436ba940f017b17652849a71db35
        //  -kdfopt seed:"test label" -kdfopt hexseed:a0ba9f936cda311827a6f796ffd5198c TLS1-PRF`
        // (OpenSSL 3.2.4; the TLS 1.2 PRF test vector of the IETF TLS working group).
        let secret = decode_hex("9bbe436ba940f017b17652849a71db35").unwrap();
        let seed = decode_hex("a0ba9f936cda311827a6f796ffd5198c").unwrap();
        let out = prf(&secret, b"test label", &seed, 100);
        let expected = decode_hex(concat!(
            "e3f229ba727be17b8d122620557cd453c2aab21d07c3d495329b52d4e61edb5a",
            "6b301791e90d35c9c9a46b4e14baf9af0fa022f7077def17abfd3797c0564bab",
            "4fbc91666e9def9b97fce34f796789baa48082d122ee42c5a72e5a5110fff701",
            "87347b66"
        ))
        .unwrap();
        assert_eq!(out, expected);
    }

    #[test]
    fn premaster_and_hex() {
        assert_eq!(premaster(&[0xAA, 0xBB]), vec![0, 2, 0, 0, 0, 2, 0xAA, 0xBB]);
        assert_eq!(decode_hex("0aFf"), Some(vec![0x0a, 0xff]));
        assert_eq!(decode_hex("abc"), None);
        assert_eq!(decode_hex("zz"), None);
        assert!(Psk::from_hex("id", "").is_err());
        assert!(Psk::from_hex("", "00").is_err());
        assert!(Psk::from_hex("id", "0g").is_err());
        assert!(!format!("{:?}", psk()).contains("11"), "the key is not printed");
    }

    #[test]
    fn records_round_trip() {
        let (_, keys) = derive(&[1; 16], &[2; 32], &[3; 32]);
        let frag = seal(&keys.client, CT_APPDATA, 1, 7, b"hello");
        let datagram = record_bytes(CT_APPDATA, 1, 7, &frag);
        let recs = records(&datagram);
        assert_eq!(recs.len(), 1);
        assert_eq!(
            (recs[0].epoch, recs[0].seq, recs[0].fragment.len()),
            (1, 7, 5 + GCM_OVERHEAD)
        );
        assert_eq!(open(&keys.client, &recs[0]).unwrap(), b"hello");
        assert!(open(&keys.server, &recs[0]).is_none(), "other direction's key");
        let mut tampered = datagram.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 1;
        assert!(open(&keys.client, &records(&tampered)[0]).is_none());
    }

    #[test]
    fn handshake_and_stream_with_a_cookie() {
        let srv = start(ServerOptions::default());
        let mut s = DtlsSession::connect(srv.addr, &psk(), HANDSHAKE_TIMEOUT).unwrap();
        assert_eq!(next(&srv.events), ServerEvent::Connected);
        assert_eq!(s.send(b"HueStream frame").unwrap(), 15);
        assert_eq!(s.send(&[0u8; 200]).unwrap(), 200);
        assert_eq!(next(&srv.events), ServerEvent::Data(b"HueStream frame".to_vec()));
        assert_eq!(next(&srv.events), ServerEvent::Data(vec![0u8; 200]));
        assert!(s.poll().is_none());
        s.close();
        assert_eq!(next(&srv.events), ServerEvent::Closed);
        assert!(s.send(b"x").is_err());
    }

    #[test]
    fn handshake_without_cookie_with_hint_and_fragments() {
        let srv = start(ServerOptions {
            cookie: false,
            hint: Some(b"hint".to_vec()),
            fragment_hello: true,
            ..ServerOptions::default()
        });
        let mut s = DtlsSession::connect(srv.addr, &psk(), HANDSHAKE_TIMEOUT).unwrap();
        assert_eq!(next(&srv.events), ServerEvent::Connected);
        s.send(b"x").unwrap();
        assert_eq!(next(&srv.events), ServerEvent::Data(b"x".to_vec()));
    }

    #[test]
    fn lost_datagrams_are_retransmitted() {
        let srv = start(ServerOptions {
            drop_first: 2,
            ..ServerOptions::default()
        });
        let mut s = DtlsSession::connect(srv.addr, &psk(), Duration::from_secs(10)).unwrap();
        assert_eq!(next(&srv.events), ServerEvent::Connected);
        s.send(b"y").unwrap();
        assert_eq!(next(&srv.events), ServerEvent::Data(b"y".to_vec()));
    }

    #[test]
    fn wrong_key_or_identity_fails() {
        let srv = start(ServerOptions::default());
        let bad = Psk::from_hex("identity", &"22".repeat(16)).unwrap();
        let e = DtlsSession::connect(srv.addr, &bad, HANDSHAKE_TIMEOUT).unwrap_err();
        assert!(e.contains("decrypt_error") || e.contains("bad_record_mac"), "{e}");
        assert!(matches!(next(&srv.events), ServerEvent::Failed(_)));
        let srv = start(ServerOptions::default());
        let stranger = Psk::from_hex("stranger", &"11".repeat(16)).unwrap();
        let e = DtlsSession::connect(srv.addr, &stranger, HANDSHAKE_TIMEOUT).unwrap_err();
        assert!(e.contains("unknown_psk_identity"), "{e}");
    }

    #[test]
    fn a_silent_peer_times_out() {
        let quiet = UdpSocket::bind("127.0.0.1:0").unwrap();
        let t = Instant::now();
        let e = DtlsSession::connect(quiet.local_addr().unwrap(), &psk(), Duration::from_millis(700))
            .unwrap_err();
        assert!(e.contains("timed out") || e.contains("refused"), "{e}");
        assert!(t.elapsed() < Duration::from_secs(3));
        let mut got = [0u8; 512];
        quiet.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
        let (n, _) = quiet.recv_from(&mut got).unwrap();
        assert_eq!(got[0], CT_HANDSHAKE, "a ClientHello went out");
        assert_eq!(got[RECORD_HEADER], HS_CLIENT_HELLO);
        assert!(n > RECORD_HEADER + HS_HEADER + 38);
    }
}
