// SPDX-License-Identifier: GPL-3.0-or-later
//! Fuzz harness of the broker, the privilege boundary (WP-FUZZ).
//!
//! Shared by the property tests (`tests` below, proptest), the deterministic
//! driver ([`drive`]: a fast run in every `cargo test`, 1M iterations behind
//! `#[ignore]`) and the cargo-fuzz targets in `crates/huedeck-iobroker/fuzz/`
//! (nightly + libFuzzer, Linux). Two entry points take arbitrary bytes:
//!
//! * [`frame`]: the pipe side. Length prefix ([`frame_complete`],
//!   [`read_frame`]: zero, oversized, truncated frames), postcard decoding of
//!   [`BrokerRequest`] (and of [`BrokerResponse`], the client side), encode /
//!   decode round trips, then every frame decoded is served by a [`Rig`].
//! * [`session`]: the bytes drive a sequence of well-formed requests (any op,
//!   `Batch`, `BusLock` / `BusUnlock`, `SpdRead`, bad versions, unknown
//!   buses), simulated time, lease expiry, the client leaving, a contended
//!   mutex and failing transfers, against a [`Rig`] in dry-run or not.
//!
//! A [`Rig`] is a [`Server`] over three fake PawnIO buses (an AMD primary on
//! port 0 with two DDR4 SPDs, an aux on port 1, and a bus of a kind chosen by
//! the input, GPU and SuperIO kinds included) and a fake global mutex. After
//! every step it panics when an invariant breaks:
//!
//! * nothing panics (the fakes also assert the broker never writes an SPD);
//! * no write ever reaches 0x50-0x57, and no transfer reaches 0x30-0x37 or
//!   0x48-0x4F except the EE1004 page select (Send Byte 0x00 to 0x36 / 0x37)
//!   inside an `SpdRead` on a chipset bus;
//! * every other write is to an address of `BROKER_WRITE_ALLOWLIST`, never in
//!   dry-run, never on a non-chipset bus (where nothing is sent at all), and
//!   the registers of every device outside the allowlist never change;
//! * transfers only on the bus the request names; the SPD page is back to 0
//!   and the primary's mux back where another program left it once no lease
//!   holds the mutex; leases and the mutex agree;
//! * the mutex is never held by leases for 250 ms of simulated time or more
//!   (checked before every step) and no lease ends later than that;
//! * a refused request (policy, ranges, version, unknown bus, dry-run, rate
//!   limit, contention) sends nothing and changes no register; a failed one
//!   changes no register; a batch sends exactly the ops it reports done (plus
//!   the one that failed on the bus), at most `MAX_BATCH_OPS`;
//! * transfers stay within the rate limit (2000/s plus the initial burst);
//! * the response echoes the id and fits a frame.

use crate::fake::{FakeMutex, FakePiix4};
use crate::pawnio::NtStatus;
use crate::piix4::Piix4Bus;
use crate::pipe::frame_complete;
use crate::policy::{MAX_HOLD, RATE_LIMIT_PER_SEC};
use crate::server::{BrokerBus, HealthInfo, Server};
use huedeck_protocol::broker::{
    read_frame, BrokerError, BrokerOp, BrokerPayload, BrokerRequest, BrokerResponse, BusInfo, BusKind,
    PawnIoEdition, BROKER_WRITE_ALLOWLIST, MAX_BATCH_OPS, MAX_FRAME_BYTES, SPD_SET_PAGE,
};
use huedeck_protocol::{decode, encode};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

/// Addresses the generator favours: the allowlist, the deny ranges and
/// their edges, out-of-range values.
const ADDRS: [u8; 28] = [
    0x20, 0x21, 0x22, 0x23, 0x27, 0x39, 0x3A, 0x3B, 0x3C, 0x2D, 0x08, 0x30, 0x34, 0x36, 0x37, 0x48, 0x4C,
    0x4F, 0x50, 0x52, 0x57, 0x58, 0x5F, 0x77, 0x02, 0x03, 0x78, 0xFF,
];
/// Where the fake buses have devices (registers 0x00, 0x01, 0x81 set).
const DEVICES: [u8; 12] = [
    0x20, 0x27, 0x39, 0x3C, 0x08, 0x2D, 0x34, 0x48, 0x4C, 0x58, 0x5F, 0x77,
];
/// Failures a fake transfer can be made to return.
const FAILURES: [NtStatus; 5] = [
    NtStatus::NO_SUCH_DEVICE,
    NtStatus::IO_TIMEOUT,
    NtStatus::NOT_SUPPORTED,
    NtStatus::IO_DEVICE_ERROR,
    NtStatus::DEVICE_BUSY,
];
/// Kinds the third bus may have.
const KINDS: [BusKind; 6] = [
    BusKind::Amd,
    BusKind::Intel,
    BusKind::System,
    BusKind::Nvidia,
    BusKind::AmdGpu,
    BusKind::Nuvoton,
];
/// Most steps one [`session`] runs.
pub const MAX_STEPS: usize = 64;

/// How a [`Rig`] starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Config {
    /// The broker runs with `--dry-run`.
    pub dry_run: bool,
    /// The port another program left the primary controller's mux on (0..=4).
    pub primary_mux: u8,
    /// Kind of bus 2.
    pub third_kind: usize,
    /// The SPDs switch page without acknowledging the select.
    pub nack_page_select: bool,
}

/// One step of a session.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    /// Simulated time passing before the action.
    pub advance: Duration,
    /// What happens.
    pub action: Action,
}

/// What a [`Step`] does.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// A request reaches the broker.
    Request(BrokerRequest),
    /// The raw payload of a frame reaches the broker (decoded as the pipe does).
    Payload(Vec<u8>),
    /// The pipe loop ends the leases that ran out.
    Expire,
    /// The client disconnects.
    ClientGone,
    /// Another program holds the global mutex (or lets go).
    Contend(bool),
    /// Transfers on bus `bus` fail with this status from now on (or stop failing).
    Fail {
        /// Bus index (0..3).
        bus: u8,
        /// Status, `None` to stop.
        status: Option<NtStatus>,
    },
}

/// Counts of a run (a coverage hint in the driver's report).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Requests the server answered.
    pub requests: u64,
    /// Answered `Ok` (a batch counts even when it stopped early).
    pub ok: u64,
    /// Answered with an error.
    pub refused: u64,
    /// Transfers sent to the fake buses.
    pub transfers: u64,
    /// Frames whose payload decoded as a request.
    pub decoded: u64,
    /// `BusLock` leases granted.
    pub leases: u64,
}

impl std::ops::AddAssign for Stats {
    fn add_assign(&mut self, o: Self) {
        self.requests += o.requests;
        self.ok += o.ok;
        self.refused += o.refused;
        self.transfers += o.transfers;
        self.decoded += o.decoded;
        self.leases += o.leases;
    }
}

/// A server over fake buses, with the invariants checked after every step.
pub struct Rig {
    server: Server<FakePiix4, FakeMutex>,
    fakes: Vec<(FakePiix4, BusKind)>,
    mutex: FakeMutex,
    config: Config,
    t0: Instant,
    now: Instant,
    /// When the mutex was last taken, while it is held across steps.
    held_from: Option<Instant>,
    /// Registers of the devices outside the write allowlist, per bus.
    untouchable: Vec<BTreeMap<u8, BTreeMap<u8, u8>>>,
    stats: Stats,
}

fn ddr4_spd(seed: u8) -> Vec<u8> {
    (0..512u16).map(|i| (i as u8) ^ seed).collect()
}

fn fake_with_devices(fake: FakePiix4) -> FakePiix4 {
    DEVICES.iter().fold(fake, |f, &addr| {
        f.with_device(addr, [(0x00, addr), (0x01, 0x22), (0x81, 0x42)])
    })
}

fn bus_info(kind: BusKind) -> BusInfo {
    BusInfo {
        id: 0,
        name: format!("fake {kind:?}"),
        kind,
        vendor: 0x1022,
        device: 0x790B,
        sub_vendor: 0x1022,
        sub_device: 0x790B,
        max_block_len: 32,
    }
}

fn untouchable(fake: &FakePiix4) -> BTreeMap<u8, BTreeMap<u8, u8>> {
    let mut devices = fake.devices();
    devices.retain(|addr, _| !BROKER_WRITE_ALLOWLIST.contains(addr));
    devices
}

/// Refusals: nothing is sent on the bus.
fn is_refusal(e: &BrokerError) -> bool {
    !matches!(
        e,
        BrokerError::Nack | BrokerError::Timeout | BrokerError::Backend { .. } | BrokerError::Unsupported
    )
}

impl Rig {
    /// A fresh rig.
    pub fn new(config: Config) -> Self {
        let t0 = Instant::now();
        let mut primary = fake_with_devices(FakePiix4::new())
            .with_spd(0x50, &ddr4_spd(0))
            .with_spd(0x52, &ddr4_spd(0x5A))
            .on_port(config.primary_mux.min(4));
        if config.nack_page_select {
            primary = primary.nack_page_select();
        }
        // Buses 1 and 2 have no SPD: a device sits at 0x36 and 0x50, so a
        // write that got through would show in its registers.
        let plain = || {
            fake_with_devices(FakePiix4::new())
                .with_device(0x36, [(0, 0)])
                .with_device(0x50, [(0, 0)])
                .on_port(1)
        };
        let third = KINDS[config.third_kind % KINDS.len()];
        let fakes = vec![(primary, BusKind::Amd), (plain(), BusKind::Amd), (plain(), third)];
        let buses = fakes
            .iter()
            .enumerate()
            .map(|(i, (fake, kind))| BrokerBus {
                info: bus_info(*kind),
                piix4: Piix4Bus::new(fake.clone(), u8::from(i != 0)).expect("port 0 or 1"),
            })
            .collect();
        let mutex = FakeMutex::new();
        let health = HealthInfo {
            pawnio_version: 0x0002_0200,
            pawnio_edition: PawnIoEdition::Official,
            ci_options: 0,
            sleep_mode: 0,
            modules: Vec::new(),
        };
        let server = Server::new(buses, mutex.clone(), health, config.dry_run, t0);
        let untouchable = fakes.iter().map(|(f, _)| untouchable(f)).collect();
        Self {
            server,
            fakes,
            mutex,
            config,
            t0,
            now: t0,
            held_from: None,
            untouchable,
            stats: Stats::default(),
        }
    }

    /// Counts so far.
    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// Run one step and check every invariant.
    pub fn step(&mut self, step: &Step) {
        self.now += step.advance;
        // The pipe loop ends the leases that ran out before it serves anything.
        self.server.expire(self.now);
        self.check_hold();
        match &step.action {
            Action::Request(req) => self.request(req),
            Action::Payload(bytes) => {
                if let Ok(req) = decode::<BrokerRequest>(bytes) {
                    self.stats.decoded += 1;
                    self.request(&req);
                }
            }
            Action::Expire => self.server.expire(self.now),
            Action::ClientGone => {
                self.server.client_gone();
                assert_eq!(self.server.lease_deadline(), None, "leases outlive the client");
                assert!(!self.mutex.held(), "the mutex outlives the client");
            }
            Action::Contend(on) => self.mutex.contend(*on),
            Action::Fail { bus, status } => {
                if let Some((fake, _)) = self.fakes.get(usize::from(*bus)) {
                    fake.set_failing(*status);
                }
            }
        }
        self.check_state();
    }

    /// The mutex is never held by leases for [`MAX_HOLD`] or longer.
    fn check_hold(&self) {
        if self.mutex.held() {
            let from = self.held_from.expect("held since a step");
            assert!(
                self.now.saturating_duration_since(from) < MAX_HOLD,
                "global SMBus mutex held {:?} (limit {MAX_HOLD:?})",
                self.now - from
            );
        }
    }

    /// State that must hold between requests.
    fn check_state(&mut self) {
        let deadline = self.server.lease_deadline();
        assert_eq!(
            deadline.is_some(),
            self.mutex.held(),
            "leases and the mutex disagree"
        );
        let (acquired, released) = self.mutex.counts();
        assert_eq!(acquired - released, u32::from(self.mutex.held()));
        if let (Some(deadline), Some(from)) = (deadline, self.held_from) {
            assert!(
                deadline <= from + MAX_HOLD,
                "a lease ends {:?} after the mutex was taken",
                deadline - from
            );
        }
        if !self.mutex.held() {
            self.held_from = None;
            let mux = self.config.primary_mux;
            if (2..=4).contains(&mux) {
                assert_eq!(self.fakes[0].0.port(), mux, "primary mux not put back");
            }
        }
        for (i, (fake, _)) in self.fakes.iter().enumerate() {
            assert_eq!(fake.spd_page(), 0, "SPD page left selected");
            assert_eq!(
                untouchable(fake),
                self.untouchable[i],
                "a device outside the allowlist changed"
            );
        }
        // Rate limit: 2000 transfers per second plus the initial full bucket
        // (an SpdRead is charged at least the transfers it makes).
        let secs = self.now.duration_since(self.t0).as_secs_f64();
        let limit = f64::from(RATE_LIMIT_PER_SEC) * (1.0 + secs) + 1.0;
        assert!(
            (self.stats.transfers as f64) <= limit,
            "{} transfers in {secs} s",
            self.stats.transfers
        );
    }

    fn request(&mut self, req: &BrokerRequest) {
        let before: Vec<(usize, BTreeMap<u8, BTreeMap<u8, u8>>)> = self
            .fakes
            .iter()
            .map(|(f, _)| (f.call_count(), f.devices()))
            .collect();
        let acquired_before = self.mutex.counts().0;
        let resp = self.server.handle(req, self.now);
        self.stats.requests += 1;
        assert_eq!(resp.id, req.id, "response id");
        if encode(req).is_ok_and(|b| b.len() <= MAX_FRAME_BYTES) {
            let out = encode(&resp).expect("responses encode");
            assert!(out.len() <= MAX_FRAME_BYTES, "response of {} bytes", out.len());
        }
        if self.mutex.counts().0 != acquired_before {
            self.held_from = Some(self.now);
        }

        // Every transfer sent while serving it.
        let mut sent = 0usize;
        for (i, ((fake, kind), (calls_before, _))) in self.fakes.iter().zip(&before).enumerate() {
            for (function, cells) in fake.calls_from(*calls_before) {
                if function != "ioctl_smbus_xfer" {
                    continue;
                }
                sent += 1;
                assert!(kind.is_chipset(), "transfer on a {kind:?} bus: {req:?}");
                assert_eq!(req.op.bus(), Some(i as u8), "transfer on bus {i}: {req:?}");
                let (addr, write, cmd, proto) = (cells[0] as u8, cells[1] == 0, cells[2], cells[3]);
                if !write {
                    continue;
                }
                assert!(
                    !(0x50..=0x57).contains(&addr),
                    "SPD written: {cells:?} by {req:?}"
                );
                if (0x30..=0x37).contains(&addr) || (0x48..=0x4F).contains(&addr) {
                    assert!(
                        SPD_SET_PAGE.contains(&addr)
                            && proto == 1
                            && cmd == 0
                            && matches!(req.op, BrokerOp::SpdRead { .. }),
                        "write to 0x{addr:02x} outside an SPD page select: {cells:?} by {req:?}"
                    );
                    continue;
                }
                assert!(
                    BROKER_WRITE_ALLOWLIST.contains(&addr),
                    "write to 0x{addr:02x} outside the allowlist: {req:?}"
                );
                assert!(!self.config.dry_run, "write sent in dry-run: {req:?}");
            }
        }
        self.stats.transfers += sent as u64;
        let devices_unchanged = || {
            self.fakes
                .iter()
                .zip(&before)
                .all(|((f, _), (_, devices))| f.devices() == *devices)
        };

        match &resp.result {
            Err(e) => {
                self.stats.refused += 1;
                if is_refusal(e) {
                    assert_eq!(sent, 0, "refused with {e:?} but sent: {req:?}");
                    assert_eq!(
                        self.mutex.counts().0,
                        acquired_before,
                        "refused after the mutex: {e:?}"
                    );
                }
                assert!(
                    devices_unchanged(),
                    "failed with {e:?} but changed a device: {req:?}"
                );
            }
            Ok(payload) => {
                self.stats.ok += 1;
                if let BrokerOp::BusLock { .. } = req.op {
                    self.stats.leases += 1;
                    assert!(self.mutex.held(), "lease granted without the mutex");
                }
                match (payload, &req.op) {
                    (BrokerPayload::Batch { results, error }, BrokerOp::Batch { ops, .. }) => {
                        assert!(results.len() <= ops.len() && ops.len() <= MAX_BATCH_OPS);
                        match error {
                            None => assert_eq!(results.len(), ops.len(), "batch ended early silently"),
                            Some(e) if is_refusal(e) => {
                                assert_eq!(sent, results.len(), "refused op of a batch sent: {e:?}")
                            }
                            Some(BrokerError::Unsupported) => assert!(
                                sent == results.len() || sent == results.len() + 1,
                                "batch sent {sent} for {} results",
                                results.len()
                            ),
                            Some(_) => assert_eq!(sent, results.len() + 1, "batch transfers"),
                        }
                    }
                    (BrokerPayload::Batch { .. }, op) => panic!("batch answer to {op:?}"),
                    (_, BrokerOp::Batch { .. }) => panic!("batch answered with {payload:?}"),
                    (BrokerPayload::Block(bytes), BrokerOp::SpdRead { len, .. }) => {
                        assert_eq!(bytes.len(), usize::from(*len))
                    }
                    _ => {}
                }
                if !req.op.is_write() && !matches!(req.op, BrokerOp::Batch { .. }) {
                    assert!(devices_unchanged(), "a read changed a device: {req:?}");
                }
            }
        }
    }
}

/// Reads the fuzz input; past its end every byte is 0.
pub struct Bytes<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Bytes<'a> {
    /// Over `data`.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }

    /// Whether every byte was used.
    pub fn done(&self) -> bool {
        self.at >= self.data.len()
    }

    fn u8(&mut self) -> u8 {
        let b = self.data.get(self.at).copied().unwrap_or(0);
        self.at += 1;
        b
    }

    fn u16(&mut self) -> u16 {
        u16::from_le_bytes([self.u8(), self.u8()])
    }

    fn take(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.u8()).collect()
    }

    fn addr(&mut self) -> u8 {
        match self.u8() {
            b @ 0..=0xDF => ADDRS[usize::from(b) % ADDRS.len()],
            _ => self.u8(),
        }
    }

    fn bus(&mut self) -> u8 {
        match self.u8() % 16 {
            0..=6 => 0,
            7..=10 => 1,
            11..=13 => 2,
            14 => 3,
            _ => self.u8(),
        }
    }

    fn len(&mut self) -> u8 {
        match self.u8() {
            b @ 0..=0xBF => b % 33,
            b @ 0xC0..=0xEF => b - 0xC0 + 30,
            _ => self.u8(),
        }
    }

    /// A session config.
    pub fn config(&mut self) -> Config {
        let b = self.u8();
        Config {
            dry_run: b & 1 != 0,
            primary_mux: [0, 0, 1, 2, 3, 4, 0, 2][usize::from(b >> 1) & 7],
            third_kind: usize::from(b >> 4) % KINDS.len(),
            nack_page_select: self.u8() & 0x0F == 0,
        }
    }

    /// An op (a batch holds ops of `depth + 1`; nesting stops at 2).
    pub fn op(&mut self, depth: u8) -> BrokerOp {
        let kind = self.u8();
        let (bus, addr) = (self.bus(), self.addr());
        match kind % 17 {
            0 => BrokerOp::SmbusWriteQuick { bus, addr },
            1 => BrokerOp::SmbusReadByte {
                bus,
                addr,
                reg: self.u8(),
            },
            2 | 3 => BrokerOp::SmbusWriteByte {
                bus,
                addr,
                reg: self.u8(),
                value: self.u8(),
            },
            4 => BrokerOp::SmbusReadWord {
                bus,
                addr,
                reg: self.u8(),
            },
            5 => BrokerOp::SmbusWriteWord {
                bus,
                addr,
                reg: self.u8(),
                value: self.u16(),
            },
            6 => {
                let (reg, n) = (self.u8(), self.len());
                BrokerOp::SmbusWriteBlock {
                    bus,
                    addr,
                    reg,
                    data: self.take(usize::from(n)),
                }
            }
            7 => BrokerOp::SmbusReadBlock {
                bus,
                addr,
                reg: self.u8(),
                len: self.len(),
            },
            8 => {
                let n = self.len();
                BrokerOp::SmbusWriteBlockNoReg {
                    bus,
                    addr,
                    data: self.take(usize::from(n)),
                }
            }
            9 => BrokerOp::SmbusReadBlockNoReg {
                bus,
                addr,
                len: self.len(),
            },
            10 => BrokerOp::SmbusReadByteNoReg { bus, addr },
            11 | 12 => {
                let offset = match self.u8() {
                    b @ 0..=0x7F => u16::from(b) * 4,
                    _ => self.u16(),
                };
                BrokerOp::SpdRead {
                    bus,
                    addr,
                    offset,
                    len: self.len(),
                }
            }
            13 | 14 if depth < 2 => {
                let n = match self.u8() {
                    b @ 0..=0xEF => b % 9,
                    b => (b - 0xF0) + 28,
                };
                let ops = (0..n)
                    .map(|_| {
                        let mut op = self.op(depth + 1);
                        // Mostly ops of the batch's own bus.
                        if self.u8() < 0xF0 {
                            set_bus(&mut op, bus);
                        }
                        op
                    })
                    .collect();
                BrokerOp::Batch { bus, ops }
            }
            15 => BrokerOp::BusLock {
                bus,
                lease_ms: match self.u8() {
                    b @ 0..=0xEF => [0, 1, 2, 10, 50, 100, 200, 249, 250, 251][usize::from(b) % 10],
                    _ => self.u16(),
                },
            },
            16 => BrokerOp::BusUnlock { bus },
            _ if kind & 0x80 != 0 => BrokerOp::ListBuses,
            _ => BrokerOp::Health,
        }
    }

    /// A step.
    pub fn step(&mut self) -> Step {
        let t = self.u8();
        let advance = match t >> 6 {
            0 => Duration::ZERO,
            1 => Duration::from_micros(u64::from(t & 63) * 50),
            2 => Duration::from_millis(u64::from(t & 63)),
            _ => Duration::from_millis(u64::from(t & 63) * 10),
        };
        let action = match self.u8() {
            0..=199 => Action::Request(BrokerRequest::new(u64::from(self.u8()), self.op(0))),
            200..=209 => {
                let mut req = BrokerRequest::new(7, self.op(0));
                req.version = self.u16();
                Action::Request(req)
            }
            210..=219 => {
                // A well-formed frame with a few bytes changed.
                let mut bytes = encode(&BrokerRequest::new(9, self.op(0))).expect("encodes");
                for _ in 0..(self.u8() % 4) {
                    let at = usize::from(self.u16()) % bytes.len();
                    bytes[at] ^= self.u8();
                }
                Action::Payload(bytes)
            }
            220..=225 => {
                let n = self.u8();
                Action::Payload(self.take(usize::from(n % 48)))
            }
            226..=235 => Action::Expire,
            236..=239 => Action::ClientGone,
            240..=245 => Action::Contend(self.u8() & 1 == 1),
            _ => {
                let bus = self.u8() % 3;
                let s = self.u8();
                Action::Fail {
                    bus,
                    status: (s & 1 == 1).then(|| FAILURES[usize::from(s >> 1) % FAILURES.len()]),
                }
            }
        };
        Step { advance, action }
    }
}

/// Point a transfer op at `bus`.
pub fn set_bus(op: &mut BrokerOp, to: u8) {
    match op {
        BrokerOp::SmbusWriteQuick { bus, .. }
        | BrokerOp::SmbusReadByte { bus, .. }
        | BrokerOp::SmbusWriteByte { bus, .. }
        | BrokerOp::SmbusReadWord { bus, .. }
        | BrokerOp::SmbusWriteWord { bus, .. }
        | BrokerOp::SmbusWriteBlock { bus, .. }
        | BrokerOp::SmbusReadBlock { bus, .. }
        | BrokerOp::SmbusWriteBlockNoReg { bus, .. }
        | BrokerOp::SmbusReadBlockNoReg { bus, .. }
        | BrokerOp::SmbusReadByteNoReg { bus, .. }
        | BrokerOp::SpdRead { bus, .. }
        | BrokerOp::Batch { bus, .. }
        | BrokerOp::BusLock { bus, .. }
        | BrokerOp::BusUnlock { bus } => *bus = to,
        BrokerOp::ListBuses | BrokerOp::Health => {}
    }
}

/// Run `steps` on a fresh rig; panics on a broken invariant.
pub fn run(config: Config, steps: &[Step]) -> Stats {
    let mut rig = Rig::new(config);
    for step in steps {
        rig.step(step);
    }
    rig.step(&Step {
        advance: Duration::ZERO,
        action: Action::ClientGone,
    });
    rig.stats()
}

/// Fuzz target: the bytes drive a session of requests (see the module docs).
pub fn session(data: &[u8]) -> Stats {
    let mut bytes = Bytes::new(data);
    let config = bytes.config();
    let mut steps = Vec::new();
    while !bytes.done() && steps.len() < MAX_STEPS {
        steps.push(bytes.step());
    }
    run(config, &steps)
}

/// Encode and decode a decoded value again: the same value.
macro_rules! round_trips {
    ($value:expr, $ty:ty) => {{
        let bytes = encode($value).expect("a decoded value encodes");
        let back: $ty = decode(&bytes).expect("an encoded value decodes");
        assert_eq!(&back, $value, "round trip");
    }};
}

/// Fuzz target: the bytes are what arrives on the pipe (see the module docs).
pub fn frame(data: &[u8]) -> Stats {
    // The length prefix: `frame_complete` (the lease loop's peek) and
    // `read_frame` agree on every input.
    let complete = frame_complete(
        &data[..data.len().min(4)],
        data.len().min(u32::MAX as usize) as u32,
    );
    let read = read_frame(&mut &data[..]);
    if data.len() < 4 {
        assert!(!complete);
        assert!(matches!(read, Ok(None)), "short header: {read:?}");
    } else {
        let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if len == 0 || len > MAX_FRAME_BYTES {
            assert!(complete, "a bad length is read (and refused) at once");
            assert_eq!(
                read.expect_err("bad length").kind(),
                std::io::ErrorKind::InvalidData
            );
        } else if data.len() >= 4 + len {
            assert!(complete);
            assert_eq!(read.expect("whole frame"), Some(data[4..4 + len].to_vec()));
        } else {
            assert!(!complete, "a truncated frame is not waited on as complete");
            assert_eq!(
                read.expect_err("truncated").kind(),
                std::io::ErrorKind::UnexpectedEof
            );
        }
    }

    let mut rig = Rig::new(Config {
        primary_mux: 2,
        ..Config::default()
    });
    let serve = |rig: &mut Rig, payload: &[u8]| {
        if let Ok(req) = decode::<BrokerRequest>(payload) {
            round_trips!(&req, BrokerRequest);
            let _ = req.op.validate();
            rig.step(&Step {
                advance: Duration::from_micros(100),
                action: Action::Request(req),
            });
        }
        // The client decodes the broker's answers with the same code.
        if let Ok(resp) = decode::<BrokerResponse>(payload) {
            round_trips!(&resp, BrokerResponse);
        }
    };
    // As one stream of frames, as the pipe reads it.
    let mut stream = data;
    while let Ok(Some(payload)) = read_frame(&mut stream) {
        serve(&mut rig, &payload);
    }
    // And as one bare payload.
    if data.len() <= MAX_FRAME_BYTES {
        serve(&mut rig, data);
    }
    rig.stats()
}

/// SplitMix64: the deterministic driver's generator.
#[derive(Clone, Debug)]
pub struct SplitMix64(u64);

impl SplitMix64 {
    /// Seeded.
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// Next value.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn fill(&mut self, n: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(n + 8);
        while out.len() < n {
            out.extend(self.next_u64().to_le_bytes());
        }
        out.truncate(n);
        out
    }
}

/// The deterministic driver: `iterations` inputs from `seed`, each fed to
/// [`session`] or [`frame`] (random bytes, or a stream of well-formed frames
/// with a few bytes changed). Panics with the iteration and its input on
/// the first broken invariant.
pub fn drive(iterations: u64, seed: u64) -> Stats {
    let mut rng = SplitMix64::new(seed);
    let mut stats = Stats::default();
    for i in 0..iterations {
        let mode = rng.next_u64() % 4;
        let len = (rng.next_u64() % 512) as usize;
        let mut input = rng.fill(len);
        if mode == 3 {
            // Frames of requests from a session, then mutated.
            let mut bytes = Bytes::new(&input);
            let mut stream = Vec::new();
            while !bytes.done() {
                if let Action::Request(req) = bytes.step().action {
                    let payload = encode(&req).expect("encodes");
                    stream.extend((payload.len() as u32).to_le_bytes());
                    stream.extend(payload);
                }
            }
            if !stream.is_empty() {
                for _ in 0..rng.next_u64() % 4 {
                    let at = (rng.next_u64() as usize) % stream.len();
                    stream[at] ^= rng.next_u64() as u8;
                }
            }
            input = stream;
        }
        let run = || match mode {
            0 | 1 => session(&input),
            _ => frame(&input),
        };
        match std::panic::catch_unwind(run) {
            Ok(s) => stats += s,
            Err(e) => {
                eprintln!("fuzz iteration {i} (seed {seed:#x}, mode {mode}) failed on input {input:02x?}");
                std::panic::resume_unwind(e);
            }
        }
    }
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{Lease, Policy, RateLimiter};
    use huedeck_protocol::broker::{BROKER_PROTOCOL_VERSION, CHIPSET_BLOCK_MAX, MAX_BLOCK_LEN};
    use proptest::prelude::*;

    /// Iterations of the default driver run (`HUEDECK_FUZZ_ITERS` overrides).
    const FAST_ITERATIONS: u64 = 5_000;

    fn iterations(default: u64) -> u64 {
        std::env::var("HUEDECK_FUZZ_ITERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    #[test]
    fn driver_fast_run() {
        let stats = drive(iterations(FAST_ITERATIONS), 0x4855_4544_4543_4B21);
        assert!(
            stats.requests > 0 && stats.ok > 0 && stats.refused > 0,
            "{stats:?}"
        );
        assert!(
            stats.transfers > 0 && stats.decoded > 0 && stats.leases > 0,
            "{stats:?}"
        );
    }

    /// The WP-FUZZ acceptance run: `cargo test --release -p huedeck-iobroker
    /// --lib fuzz::tests::driver_one_million -- --ignored --nocapture`.
    #[test]
    #[ignore = "1M iterations: run in release mode on demand"]
    fn driver_one_million() {
        let started = std::time::Instant::now();
        let stats = drive(iterations(1_000_000), 0x5750_2D46_555A_5A21);
        eprintln!("{stats:?} in {:?}", started.elapsed());
    }

    #[test]
    fn empty_and_tiny_inputs() {
        for data in [&[][..], &[0], &[0xFF; 3], &[0, 0, 0, 0], &[1, 0, 0, 0]] {
            session(data);
            frame(data);
        }
    }

    /// Found by this harness: a frame of nested batches overflowed the
    /// stack while decoding (now refused by `one_level`); served on a small
    /// stack to prove the decode does not recurse.
    #[test]
    fn nested_batch_frame_is_refused_without_recursion() {
        let mut payload = vec![BROKER_PROTOCOL_VERSION as u8, 0];
        for _ in 0..1300 {
            payload.extend([11, 0, 1]);
        }
        payload.push(15);
        let mut data = (payload.len() as u32).to_le_bytes().to_vec();
        data.extend(payload);
        std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(move || assert_eq!(frame(&data).decoded, 0))
            .unwrap()
            .join()
            .unwrap();
    }

    // ---- proptest strategies ----

    fn addr() -> impl Strategy<Value = u8> {
        prop_oneof![4 => proptest::sample::select(ADDRS.to_vec()), 1 => any::<u8>()]
    }

    fn bus() -> impl Strategy<Value = u8> {
        prop_oneof![6 => Just(0u8), 3 => Just(1u8), 2 => Just(2u8), 1 => any::<u8>()]
    }

    fn data() -> impl Strategy<Value = Vec<u8>> {
        prop_oneof![
            4 => proptest::collection::vec(any::<u8>(), 1..=CHIPSET_BLOCK_MAX),
            1 => proptest::collection::vec(any::<u8>(), 0..=MAX_BLOCK_LEN + 2),
        ]
    }

    fn transfer() -> BoxedStrategy<BrokerOp> {
        prop_oneof![
            (bus(), addr()).prop_map(|(bus, addr)| BrokerOp::SmbusWriteQuick { bus, addr }),
            (bus(), addr(), any::<u8>()).prop_map(|(bus, addr, reg)| BrokerOp::SmbusReadByte {
                bus,
                addr,
                reg
            }),
            (bus(), addr(), any::<u8>(), any::<u8>()).prop_map(|(bus, addr, reg, value)| {
                BrokerOp::SmbusWriteByte {
                    bus,
                    addr,
                    reg,
                    value,
                }
            }),
            (bus(), addr(), any::<u8>()).prop_map(|(bus, addr, reg)| BrokerOp::SmbusReadWord {
                bus,
                addr,
                reg
            }),
            (bus(), addr(), any::<u8>(), any::<u16>()).prop_map(|(bus, addr, reg, value)| {
                BrokerOp::SmbusWriteWord {
                    bus,
                    addr,
                    reg,
                    value,
                }
            }),
            (bus(), addr(), any::<u8>(), data())
                .prop_map(|(bus, addr, reg, data)| { BrokerOp::SmbusWriteBlock { bus, addr, reg, data } }),
            (bus(), addr(), any::<u8>(), any::<u8>())
                .prop_map(|(bus, addr, reg, len)| { BrokerOp::SmbusReadBlock { bus, addr, reg, len } }),
            (bus(), addr(), data()).prop_map(|(bus, addr, data)| BrokerOp::SmbusWriteBlockNoReg {
                bus,
                addr,
                data
            }),
            (bus(), addr(), any::<u8>()).prop_map(|(bus, addr, len)| BrokerOp::SmbusReadBlockNoReg {
                bus,
                addr,
                len
            }),
            (bus(), addr()).prop_map(|(bus, addr)| BrokerOp::SmbusReadByteNoReg { bus, addr }),
        ]
        .boxed()
    }

    fn op() -> BoxedStrategy<BrokerOp> {
        prop_oneof![
            8 => transfer(),
            2 => (bus(), addr(), prop_oneof![0u16..600, any::<u16>()], 0u8..40)
                .prop_map(|(bus, addr, offset, len)| BrokerOp::SpdRead { bus, addr, offset, len }),
            2 => (bus(), proptest::collection::vec((transfer(), any::<bool>()), 0..=MAX_BATCH_OPS + 2))
                .prop_map(|(bus, ops)| BrokerOp::Batch {
                    bus,
                    ops: ops
                        .into_iter()
                        .map(|(mut op, same_bus)| {
                            if same_bus {
                                set_bus(&mut op, bus);
                            }
                            op
                        })
                        .collect(),
                }),
            2 => (bus(), prop_oneof![0u16..=260, any::<u16>()]).prop_map(|(bus, lease_ms)| BrokerOp::BusLock { bus, lease_ms }),
            1 => bus().prop_map(|bus| BrokerOp::BusUnlock { bus }),
            1 => Just(BrokerOp::ListBuses),
            1 => Just(BrokerOp::Health),
        ]
        .boxed()
    }

    fn step() -> BoxedStrategy<Step> {
        let advance = prop_oneof![
            3 => Just(Duration::ZERO),
            3 => (0u64..2_000).prop_map(Duration::from_micros),
            2 => (0u64..300).prop_map(Duration::from_millis),
        ];
        let action = prop_oneof![
            20 => (any::<u64>(), op()).prop_map(|(id, op)| Action::Request(BrokerRequest::new(id, op))),
            1 => (any::<u16>(), op()).prop_map(|(version, op)| Action::Request(BrokerRequest { version, id: 3, op })),
            1 => proptest::collection::vec(any::<u8>(), 0..64).prop_map(Action::Payload),
            2 => Just(Action::Expire),
            1 => Just(Action::ClientGone),
            1 => any::<bool>().prop_map(Action::Contend),
            1 => (0u8..3, proptest::option::of(proptest::sample::select(FAILURES.to_vec())))
                .prop_map(|(bus, status)| Action::Fail { bus, status }),
        ];
        (advance, action)
            .prop_map(|(advance, action)| Step { advance, action })
            .boxed()
    }

    fn config() -> impl Strategy<Value = Config> {
        (any::<bool>(), 0u8..=4, 0usize..KINDS.len(), any::<bool>()).prop_map(
            |(dry_run, primary_mux, third_kind, nack_page_select)| Config {
                dry_run,
                primary_mux,
                third_kind,
                nack_page_select,
            },
        )
    }

    fn kind() -> impl Strategy<Value = BusKind> {
        proptest::sample::select(KINDS.to_vec())
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

        /// (2) Server: arbitrary well-formed request sequences keep every invariant.
        #[test]
        fn sessions_keep_the_invariants(config in config(), steps in proptest::collection::vec(step(), 0..48)) {
            run(config, &steps);
        }

        /// (1) Decoding: arbitrary bytes, and encoded requests with bytes changed.
        #[test]
        fn frames_of_arbitrary_bytes(data in proptest::collection::vec(any::<u8>(), 0..300)) {
            frame(&data);
            session(&data);
        }

        #[test]
        fn mutated_requests_decode_safely(
            op in op(),
            id in any::<u64>(),
            flips in proptest::collection::vec((any::<usize>(), any::<u8>()), 0..4),
            cut in any::<usize>(),
        ) {
            let req = BrokerRequest::new(id, op);
            let mut payload = encode(&req).unwrap();
            prop_assert!(payload.len() <= MAX_FRAME_BYTES);
            prop_assert_eq!(decode::<BrokerRequest>(&payload).unwrap(), req);
            for (at, x) in flips {
                let at = at % payload.len();
                payload[at] ^= x;
            }
            let mut framed = (payload.len() as u32).to_le_bytes().to_vec();
            framed.extend(&payload);
            frame(&framed);
            frame(&framed[..cut % (framed.len() + 1)]);
        }

        /// (3) Policy: whatever it lets through respects the static rules.
        #[test]
        fn policy_admits_only_what_the_rules_allow(op in op(), kind in kind(), dry_run in any::<bool>()) {
            let t0 = Instant::now();
            let mut policy = Policy::new(dry_run, t0);
            let info = bus_info(kind);
            let verdict = policy.check(&op, &info, t0);
            if op.touches_spd(kind) || !op.write_allowed() || (dry_run && op.is_write()) {
                prop_assert!(verdict.is_err(), "{:?} admitted", op);
            }
            if verdict.is_ok() {
                prop_assert!(kind.is_chipset());
                prop_assert!(op.validate_for(kind).is_ok());
                if op.is_write() {
                    prop_assert!(!dry_run, "write admitted in dry-run");
                    let a = op.addr().unwrap();
                    prop_assert!(BROKER_WRITE_ALLOWLIST.contains(&a));
                    prop_assert!(!(0x30..=0x37).contains(&a) && !(0x48..=0x5F).contains(&a));
                }
                if let BrokerOp::SpdRead { addr, offset, len, .. } = op {
                    prop_assert!((0x50..=0x57).contains(&addr));
                    prop_assert!(len >= 1 && usize::from(len) <= CHIPSET_BLOCK_MAX);
                    prop_assert!(offset % 256 + u16::from(len) <= 256 && offset < 512);
                }
                if let Some(a) = op.addr() {
                    prop_assert!((0x03..=0x77).contains(&a));
                }
            }
        }

        /// (3) The rate limiter never admits more than its rate allows.
        #[test]
        fn rate_limiter_bound(steps in proptest::collection::vec((0u64..5_000, 1u32..40), 1..400)) {
            let t0 = Instant::now();
            let mut limiter = RateLimiter::new(RATE_LIMIT_PER_SEC, t0);
            let (mut now, mut admitted) = (t0, 0u64);
            for (micros, n) in steps {
                now += Duration::from_micros(micros);
                if limiter.admit_n(n, now) {
                    admitted += u64::from(n);
                }
                let secs = (now - t0).as_secs_f64();
                prop_assert!(admitted as f64 <= f64::from(RATE_LIMIT_PER_SEC) * (1.0 + secs) + 1e-6);
            }
        }

        /// (3) A lease never runs past 250 ms, nor past its cap.
        #[test]
        fn leases_are_bounded(lease_ms in any::<u16>(), cap_ms in 0u64..1_000) {
            let t0 = Instant::now();
            let lease = Lease::new(0, lease_ms, t0);
            prop_assert!(lease.until - t0 <= MAX_HOLD);
            let capped = lease.capped(t0 + Duration::from_millis(cap_ms));
            prop_assert!(capped.until <= t0 + Duration::from_millis(cap_ms) && capped.until <= lease.until);
        }
    }

    #[test]
    fn allowlist_and_deny_ranges_are_disjoint() {
        for &a in BROKER_WRITE_ALLOWLIST {
            assert!(
                !(0x30..=0x37).contains(&a) && !(0x48..=0x5F).contains(&a),
                "0x{a:02x}"
            );
        }
    }
}
