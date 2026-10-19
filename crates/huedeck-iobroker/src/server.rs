// SPDX-License-Identifier: GPL-3.0-or-later
//! The broker's request handler: one [`BrokerRequest`] in, one
//! [`BrokerResponse`] out, on the broker's single worker thread.
//!
//! Order for a bus operation: protocol version → bus exists → [`Policy`]
//! (ranges, SPD denial, chipset kinds only, rate limit, dry-run) → the global
//! SMBus mutex (bounded wait, `BusContended` when another program keeps it)
//! → on port 0, re-select the port (another program may have moved the
//! primary controller's mux) → the PawnIO transfer → mux put back → mutex
//! released. A `BusLock` keeps the mutex (and the port) across operations
//! until `BusUnlock`, the lease running out, or the client going away; each
//! bus has its own lease, and the mutex stays held while any lease does.
//! Renewals never stretch one continuous hold past [`MAX_HOLD`]: every lease
//! ends by then (each request first ends the leases that ran out), the mutex
//! is let go, and a program waiting for it gets its turn before the next
//! `BusLock` takes it again.

use crate::mutex::{GlobalMutex, ACQUIRE_TIMEOUT_MS};
use crate::pawnio::PawnExec;
use crate::piix4::Piix4Bus;
use crate::policy::{audit, Lease, Policy, MAX_HOLD};
use huedeck_protocol::broker::{
    BrokerError, BrokerOp, BrokerPayload, BrokerRequest, BrokerResponse, BusInfo, HealthReport, ModuleStatus,
    PawnIoEdition, BROKER_PROTOCOL_VERSION,
};
use std::time::Instant;

/// One bus the broker serves.
#[derive(Debug)]
pub struct BrokerBus<E> {
    /// What `ListBuses` reports (its `id` is its index).
    pub info: BusInfo,
    /// The PawnIO handle behind it.
    pub piix4: Piix4Bus<E>,
}

/// The PawnIO facts `Health` reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HealthInfo {
    /// Driver version.
    pub pawnio_version: u32,
    /// Edition.
    pub pawnio_edition: PawnIoEdition,
    /// Code-integrity options.
    pub ci_options: u32,
    /// PIIX4 sleep mode set at start.
    pub sleep_mode: u8,
    /// Loaded modules.
    pub modules: Vec<ModuleStatus>,
}

/// The mux value to put back when a batch on port 0 ends.
#[derive(Clone, Copy, Debug)]
struct Restore {
    bus: usize,
    previous: u8,
}

/// The request handler.
#[derive(Debug)]
pub struct Server<E, M> {
    buses: Vec<BrokerBus<E>>,
    mutex: M,
    mutex_held: bool,
    /// Since when the mutex has been held without a break, while leases hold it.
    held_since: Option<Instant>,
    policy: Policy,
    leases: Vec<Lease>,
    restore: Option<Restore>,
    started: Instant,
    ops_served: u64,
    health: HealthInfo,
}

impl<E: PawnExec, M: GlobalMutex> Server<E, M> {
    /// A server for `buses` (ids are their indexes).
    pub fn new(
        mut buses: Vec<BrokerBus<E>>,
        mutex: M,
        health: HealthInfo,
        dry_run: bool,
        now: Instant,
    ) -> Self {
        for (i, b) in buses.iter_mut().enumerate() {
            b.info.id = i as u8;
        }
        Self {
            buses,
            mutex,
            mutex_held: false,
            held_since: None,
            policy: Policy::new(dry_run, now),
            leases: Vec::new(),
            restore: None,
            started: now,
            ops_served: 0,
            health,
        }
    }

    /// When the first held lease runs out, if any is held.
    pub fn lease_deadline(&self) -> Option<Instant> {
        self.leases.iter().map(|l| l.until).min()
    }

    /// End the leases that have run out at `now`.
    pub fn expire(&mut self, now: Instant) {
        let expired: Vec<u8> = self
            .leases
            .iter()
            .filter(|l| l.expired(now))
            .map(|l| l.bus)
            .collect();
        for bus in expired {
            tracing::warn!(target: "huedeck::audit", bus, "BusLock lease ran out");
            self.end_lease(bus);
        }
    }

    /// The client disconnected: drop its leases.
    pub fn client_gone(&mut self) {
        let held: Vec<u8> = self.leases.iter().map(|l| l.bus).collect();
        for bus in held {
            tracing::warn!(target: "huedeck::audit", bus, "client left holding BusLock");
            self.end_lease(bus);
        }
    }

    /// Answer one request.
    pub fn handle(&mut self, req: &BrokerRequest, now: Instant) -> BrokerResponse {
        self.expire(now);
        let result = if req.version != BROKER_PROTOCOL_VERSION {
            Err(BrokerError::UnsupportedVersion(req.version))
        } else {
            self.dispatch(&req.op, now)
        };
        BrokerResponse { id: req.id, result }
    }

    fn dispatch(&mut self, op: &BrokerOp, now: Instant) -> Result<BrokerPayload, BrokerError> {
        op.validate()?;
        match op {
            BrokerOp::Health => Ok(BrokerPayload::Health(self.health_report(now))),
            BrokerOp::ListBuses => Ok(BrokerPayload::Buses(
                self.buses.iter().map(|b| b.info.clone()).collect(),
            )),
            BrokerOp::BusLock { bus, lease_ms } => {
                let index = self.bus_index(*bus)?;
                if !self.leased(index) {
                    self.begin(index)?;
                }
                let cap = *self.held_since.get_or_insert(now) + MAX_HOLD;
                self.leases.retain(|l| l.bus != *bus);
                self.leases.push(Lease::new(*bus, *lease_ms, now).capped(cap));
                Ok(BrokerPayload::Ack)
            }
            BrokerOp::BusUnlock { bus } => {
                self.bus_index(*bus)?;
                self.end_lease(*bus);
                Ok(BrokerPayload::Ack)
            }
            _ => {
                let index = self.bus_index(op.bus().unwrap_or(u8::MAX))?;
                let info = self.buses[index].info.clone();
                self.policy.check(op, &info, now)?;
                // Operations the PIIX4 module cannot do are refused before the mutex.
                crate::piix4::xfer_input(op)?;
                let leased = self.leased(index);
                if !leased {
                    self.begin(index)?;
                }
                let result = self.buses[index].piix4.transfer(op);
                if !leased {
                    self.end(index);
                }
                self.ops_served += 1;
                match &result {
                    Ok(_) => audit(&info, op, "ok"),
                    Err(e) => audit(&info, op, &format!("failed: {e}")),
                }
                result
            }
        }
    }

    fn leased(&self, index: usize) -> bool {
        self.leases.iter().any(|l| usize::from(l.bus) == index)
    }

    fn bus_index(&self, bus: u8) -> Result<usize, BrokerError> {
        let index = usize::from(bus);
        if index < self.buses.len() {
            Ok(index)
        } else {
            Err(BrokerError::UnknownBus(bus))
        }
    }

    /// Take the mutex (unless held) and select the bus's port.
    fn begin(&mut self, index: usize) -> Result<(), BrokerError> {
        if !self.mutex_held {
            self.mutex.acquire(ACQUIRE_TIMEOUT_MS)?;
            self.mutex_held = true;
        }
        if self.buses[index].piix4.port() == 0 {
            match self.buses[index].piix4.select_port() {
                Ok(previous) => {
                    if previous != 0 {
                        self.restore = Some(Restore { bus: index, previous });
                    }
                }
                Err(e) => {
                    self.release_if_unleased();
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    /// Put the mux back and release the mutex (unless a lease keeps it).
    fn end(&mut self, index: usize) {
        if let Some(r) = self.restore.filter(|r| r.bus == index) {
            self.restore = None;
            if let Err(e) = self.buses[index].piix4.restore_port(r.previous) {
                tracing::warn!("PIIX4 port {} not restored: {e}", r.previous);
            }
        }
        self.release_if_unleased();
    }

    fn release_if_unleased(&mut self) {
        if self.leases.is_empty() {
            self.held_since = None;
            if self.mutex_held {
                self.mutex.release();
                self.mutex_held = false;
            }
        }
    }

    /// End the lease of `bus`, if held: its port put back, the mutex
    /// released when no other lease keeps it.
    fn end_lease(&mut self, bus: u8) {
        let before = self.leases.len();
        self.leases.retain(|l| l.bus != bus);
        if self.leases.len() != before {
            self.end(usize::from(bus));
        }
    }

    fn health_report(&self, now: Instant) -> HealthReport {
        HealthReport {
            version: env!("CARGO_PKG_VERSION").to_string(),
            protocol: BROKER_PROTOCOL_VERSION,
            uptime_secs: now.saturating_duration_since(self.started).as_secs(),
            dry_run: self.policy.dry_run,
            ops_served: self.ops_served,
            modules: self.health.modules.clone(),
            pawnio_version: self.health.pawnio_version,
            pawnio_edition: self.health.pawnio_edition,
            ci_options: self.health.ci_options,
            sleep_mode: self.health.sleep_mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{FakeMutex, FakePiix4};
    use huedeck_protocol::broker::BusKind;
    use std::time::Duration;

    fn info(name: &str) -> BusInfo {
        BusInfo {
            id: 9,
            name: name.into(),
            kind: BusKind::Amd,
            vendor: 0x1022,
            device: 0x790B,
            sub_vendor: 0x1022,
            sub_device: 0x790B,
            max_block_len: 32,
        }
    }

    fn health() -> HealthInfo {
        HealthInfo {
            pawnio_version: 0x0002_0200,
            pawnio_edition: PawnIoEdition::Official,
            ci_options: 0xF401,
            sleep_mode: 1,
            modules: vec![ModuleStatus {
                name: "SmbusPIIX4".into(),
                sha256: crate::pins::SMBUS_PIIX4.sha256.into(),
                loaded: true,
            }],
        }
    }

    struct Rig {
        server: Server<FakePiix4, FakeMutex>,
        primary: FakePiix4,
        mutex: FakeMutex,
        t0: Instant,
    }

    fn rig(primary: FakePiix4, dry_run: bool) -> Rig {
        let mutex = FakeMutex::new();
        let t0 = Instant::now();
        let buses = vec![
            BrokerBus {
                info: info("AMD PIIX4 Bus - 0x0b00"),
                piix4: Piix4Bus::new(primary.clone(), 0).unwrap(),
            },
            BrokerBus {
                info: info("AMD PIIX4 Bus - 0x0b20"),
                piix4: Piix4Bus::new(FakePiix4::new().on_port(1), 1).unwrap(),
            },
        ];
        Rig {
            server: Server::new(buses, mutex.clone(), health(), dry_run, t0),
            primary,
            mutex,
            t0,
        }
    }

    fn call(r: &mut Rig, op: BrokerOp) -> Result<BrokerPayload, BrokerError> {
        r.server.handle(&BrokerRequest::new(1, op), r.t0).result
    }

    fn xfers(fake: &FakePiix4) -> usize {
        fake.calls()
            .iter()
            .filter(|(f, _)| f == "ioctl_smbus_xfer")
            .count()
    }

    #[test]
    fn list_buses() {
        let mut r = rig(FakePiix4::new(), false);
        let Ok(BrokerPayload::Buses(buses)) = call(&mut r, BrokerOp::ListBuses) else {
            panic!("bus list");
        };
        let names: Vec<(u8, &str)> = buses.iter().map(|b| (b.id, b.name.as_str())).collect();
        assert_eq!(
            names,
            vec![(0, "AMD PIIX4 Bus - 0x0b00"), (1, "AMD PIIX4 Bus - 0x0b20")]
        );
        assert!(r.primary.calls().is_empty(), "listing touches no hardware");
    }

    #[test]
    fn write_byte_round_trip() {
        let mut r = rig(FakePiix4::new().with_device(0x39, []), false);
        let write = BrokerOp::SmbusWriteByte {
            bus: 0,
            addr: 0x39,
            reg: 0x01,
            value: 0xCF,
        };
        assert_eq!(call(&mut r, write), Ok(BrokerPayload::Ack));
        let read = BrokerOp::SmbusReadByte {
            bus: 0,
            addr: 0x39,
            reg: 0x01,
        };
        assert_eq!(call(&mut r, read), Ok(BrokerPayload::Byte(0xCF)));
        assert_eq!(r.mutex.counts(), (2, 2), "the mutex is held per operation");
        assert!(!r.mutex.held());
        assert_eq!(
            call(&mut r, BrokerOp::SmbusReadByteNoReg { bus: 0, addr: 0x3A }),
            Err(BrokerError::Nack)
        );
        let Ok(BrokerPayload::Health(h)) = call(&mut r, BrokerOp::Health) else {
            panic!("health");
        };
        assert_eq!(h.ops_served, 3);
    }

    #[test]
    fn spd_write_refused() {
        let mut r = rig(FakePiix4::new().with_device(0x50, []), false);
        let op = BrokerOp::SmbusWriteByte {
            bus: 0,
            addr: 0x50,
            reg: 0,
            value: 0,
        };
        assert_eq!(call(&mut r, op), Err(BrokerError::AddressDenied { addr: 0x50 }));
        assert_eq!(
            call(&mut r, BrokerOp::SmbusWriteQuick { bus: 1, addr: 0x36 }),
            Err(BrokerError::AddressDenied { addr: 0x36 })
        );
        assert_eq!(
            call(
                &mut r,
                BrokerOp::SmbusWriteByte {
                    bus: 1,
                    addr: 0x2D,
                    reg: 0,
                    value: 0
                }
            ),
            Err(BrokerError::AddressDenied { addr: 0x2D }),
            "outside the write allowlist"
        );
        assert!(r.primary.calls().is_empty());
        assert_eq!(r.mutex.counts(), (0, 0), "refused before the mutex");
    }

    #[test]
    fn renewals_never_hold_the_mutex_past_the_limit() {
        let mut r = rig(FakePiix4::new().with_device(0x39, []), false);
        let lock = |r: &mut Rig, bus, at| {
            r.server
                .handle(
                    &BrokerRequest::new(1, BrokerOp::BusLock { bus, lease_ms: 200 }),
                    at,
                )
                .result
        };
        let t0 = r.t0;
        let ms = |n| t0 + Duration::from_millis(n);
        let (t100, t200, t260) = (ms(100), ms(200), ms(260));
        assert_eq!(lock(&mut r, 0, t0), Ok(BrokerPayload::Ack));
        assert_eq!(lock(&mut r, 0, t100), Ok(BrokerPayload::Ack), "renewal");
        assert_eq!(lock(&mut r, 1, t200), Ok(BrokerPayload::Ack), "another bus");
        assert_eq!(
            r.server.lease_deadline(),
            Some(t0 + MAX_HOLD),
            "every lease ends with the first hold"
        );
        assert_eq!(r.mutex.counts(), (1, 0));
        r.server.expire(t0 + MAX_HOLD);
        assert!(!r.mutex.held(), "let go at the limit");
        assert_eq!(lock(&mut r, 0, t260), Ok(BrokerPayload::Ack));
        assert_eq!(r.mutex.counts(), (2, 1), "taken again afresh");
        assert_eq!(r.server.lease_deadline(), Some(t260 + Duration::from_millis(200)));
    }

    #[test]
    fn lease_holds_mutex_across_ops() {
        let mut r = rig(FakePiix4::new().with_device(0x39, []), false);
        assert_eq!(
            call(
                &mut r,
                BrokerOp::BusLock {
                    bus: 0,
                    lease_ms: 100
                }
            ),
            Ok(BrokerPayload::Ack)
        );
        assert!(r.mutex.held());
        for value in 0..3 {
            let op = BrokerOp::SmbusWriteByte {
                bus: 0,
                addr: 0x39,
                reg: 0x01,
                value,
            };
            assert_eq!(call(&mut r, op), Ok(BrokerPayload::Ack));
        }
        assert_eq!(r.mutex.counts(), (1, 0));
        assert_eq!(
            call(&mut r, BrokerOp::BusUnlock { bus: 0 }),
            Ok(BrokerPayload::Ack)
        );
        assert_eq!(r.mutex.counts(), (1, 1));
        // A lease left behind runs out by itself.
        assert_eq!(
            call(&mut r, BrokerOp::BusLock { bus: 0, lease_ms: 50 }),
            Ok(BrokerPayload::Ack)
        );
        assert_eq!(r.server.lease_deadline(), Some(r.t0 + Duration::from_millis(50)));
        r.server.expire(r.t0 + Duration::from_millis(49));
        assert!(r.mutex.held());
        r.server.expire(r.t0 + Duration::from_millis(50));
        assert!(!r.mutex.held());
        assert_eq!(r.server.lease_deadline(), None);
        // And so does one whose client left.
        assert_eq!(
            call(&mut r, BrokerOp::BusLock { bus: 1, lease_ms: 50 }),
            Ok(BrokerPayload::Ack)
        );
        r.server.client_gone();
        assert!(!r.mutex.held());
        assert_eq!(r.mutex.counts(), (3, 3));
    }

    #[test]
    fn leases_on_two_buses_share_the_mutex() {
        let mut r = rig(FakePiix4::new().with_device(0x39, []), false);
        assert_eq!(
            call(
                &mut r,
                BrokerOp::BusLock {
                    bus: 0,
                    lease_ms: 100
                }
            ),
            Ok(BrokerPayload::Ack)
        );
        assert_eq!(
            call(&mut r, BrokerOp::BusLock { bus: 1, lease_ms: 30 }),
            Ok(BrokerPayload::Ack)
        );
        assert_eq!(r.server.lease_deadline(), Some(r.t0 + Duration::from_millis(30)));
        assert_eq!(
            call(&mut r, BrokerOp::BusUnlock { bus: 0 }),
            Ok(BrokerPayload::Ack)
        );
        assert!(r.mutex.held(), "bus 1 still holds it");
        assert_eq!(
            call(&mut r, BrokerOp::BusUnlock { bus: 0 }),
            Ok(BrokerPayload::Ack),
            "idempotent"
        );
        assert!(r.mutex.held());
        r.server.expire(r.t0 + Duration::from_millis(30));
        assert!(!r.mutex.held());
        assert_eq!(r.mutex.counts(), (1, 1));
    }

    #[test]
    fn contended_mutex_sends_nothing() {
        let mut r = rig(FakePiix4::new().with_device(0x39, []), false);
        r.mutex.contend(true);
        let op = BrokerOp::SmbusReadByte {
            bus: 0,
            addr: 0x39,
            reg: 0,
        };
        assert_eq!(call(&mut r, op), Err(BrokerError::BusContended));
        assert_eq!(
            call(&mut r, BrokerOp::BusLock { bus: 0, lease_ms: 10 }),
            Err(BrokerError::BusContended)
        );
        assert_eq!(xfers(&r.primary), 0);
        assert!(r.primary.calls().is_empty());
    }

    #[test]
    fn port_0_is_reselected_and_the_mux_put_back() {
        let mut r = rig(FakePiix4::new().with_device(0x39, []).on_port(2), false);
        let op = BrokerOp::SmbusReadByteNoReg { bus: 0, addr: 0x39 };
        assert_eq!(call(&mut r, op), Ok(BrokerPayload::Byte(0xFF)));
        let calls: Vec<(String, u64)> = r.primary.calls().into_iter().map(|(f, i)| (f, i[0])).collect();
        assert_eq!(
            calls,
            vec![
                ("ioctl_piix4_port_sel".into(), 0),
                ("ioctl_smbus_xfer".into(), 0x39),
                ("ioctl_piix4_port_sel".into(), 2),
            ]
        );
    }

    #[test]
    fn unsupported_version() {
        let mut r = rig(FakePiix4::new(), false);
        let mut req = BrokerRequest::new(5, BrokerOp::Health);
        req.version = 1;
        let resp = r.server.handle(&req, r.t0);
        assert_eq!(resp.id, 5);
        assert_eq!(resp.result, Err(BrokerError::UnsupportedVersion(1)));
    }

    #[test]
    fn health_round_trips_and_writes_are_refused() {
        let mut r = rig(FakePiix4::new().with_device(0x39, []), true);
        let req = BrokerRequest::new(7, BrokerOp::Health);
        let bytes = huedeck_protocol::encode(&req).unwrap();
        let decoded: BrokerRequest = huedeck_protocol::decode(&bytes).unwrap();
        let resp = r.server.handle(&decoded, r.t0);
        assert_eq!(resp.id, 7);
        let Ok(BrokerPayload::Health(h)) = resp.result else {
            panic!("health");
        };
        assert!(h.dry_run);
        assert_eq!(
            (h.protocol, h.pawnio_version, h.sleep_mode),
            (BROKER_PROTOCOL_VERSION, 0x0002_0200, 1)
        );
        let write = BrokerOp::SmbusWriteByte {
            bus: 0,
            addr: 0x39,
            reg: 0,
            value: 1,
        };
        assert_eq!(
            call(&mut r, write),
            Err(BrokerError::DryRun),
            "dry run is reported"
        );
        assert_eq!(
            call(
                &mut r,
                BrokerOp::SmbusReadByte {
                    bus: 0,
                    addr: 0x00,
                    reg: 0
                }
            ),
            Err(BrokerError::AddressOutOfRange { addr: 0 })
        );
        assert_eq!(
            call(
                &mut r,
                BrokerOp::SmbusReadByte {
                    bus: 7,
                    addr: 0x39,
                    reg: 0
                }
            ),
            Err(BrokerError::UnknownBus(7))
        );
        assert_eq!(
            call(
                &mut r,
                BrokerOp::SmbusReadBlock {
                    bus: 0,
                    addr: 0x39,
                    reg: 0,
                    len: 4
                }
            ),
            Err(BrokerError::Unsupported),
            "no block reads on chipset buses"
        );
        assert!(r.primary.calls().is_empty());
        assert_eq!(r.mutex.counts(), (0, 0));
    }
}
