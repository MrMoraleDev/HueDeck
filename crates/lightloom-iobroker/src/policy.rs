// SPDX-License-Identifier: GPL-3.0-or-later
//! Static rules of the broker, applied to every request before it reaches
//! PawnIO (research m5-privileged-buses.md §6.4):
//!
//! * argument ranges for the bus kind ([`BrokerOp::validate_for`]: address
//!   0x03..=0x77, blocks 1..=32 on chipset buses);
//! * writes and Quick commands to the DIMMs' own parts denied
//!   ([`BrokerOp::touches_spd`]): SPD EEPROM and EEPROM area (0x50-0x5F),
//!   write-protect addresses (0x30-0x37), DDR5 PMIC (0x48-0x4F);
//! * writes and Quick commands anywhere else outside
//!   [`BROKER_WRITE_ALLOWLIST`](lightloom_protocol::broker::BROKER_WRITE_ALLOWLIST) denied ([`BrokerOp::write_allowed`]): the
//!   addresses of the first-party chipset plugins only, so a process that
//!   reaches the pipe cannot write clock generators, VRM controllers or EC
//!   parts through this LocalSystem service;
//! * only chipset buses: GPU I2C stays in the user process, SuperIO is not served;
//! * `SpdRead` only at the SPD addresses 0x50-0x57, 1..=32 bytes in one page
//!   ([`BrokerOp::validate`]); its EE1004 page select (0x36 / 0x37) is a step
//!   inside the broker, never an op of its own;
//! * at most [`RATE_LIMIT_PER_SEC`] bus transfers per second (token bucket;
//!   an `SpdRead` costs its bytes plus two);
//! * dry-run: writes refused with `DryRun` (reported, never silent);
//! * an audit line per write (`target: "lightloom::audit"`).
//!
//! The per-plugin, per-scan address allowlist lives in the client (it knows
//! which plugin asks and what its scan found); the broker's static rules hold
//! whatever the client sends. Every refusal happens before the mutex.

use lightloom_protocol::broker::{BrokerError, BrokerOp, BusInfo, BUS_LOCK_MAX_MS};
use std::time::{Duration, Instant};

/// Operations per second (ADR-0004).
pub const RATE_LIMIT_PER_SEC: u32 = 2000;

/// Token bucket: [`RATE_LIMIT_PER_SEC`] per second, bursts up to the same.
#[derive(Clone, Debug)]
pub struct RateLimiter {
    rate: f64,
    tokens: f64,
    last: Instant,
}

impl RateLimiter {
    /// A full bucket of `per_sec` tokens.
    pub fn new(per_sec: u32, now: Instant) -> Self {
        Self {
            rate: f64::from(per_sec),
            tokens: f64::from(per_sec),
            last: now,
        }
    }

    /// Take one token at `now`.
    pub fn admit(&mut self, now: Instant) -> bool {
        self.admit_n(1, now)
    }

    /// Take `n` tokens at `now` (none when fewer are left).
    pub fn admit_n(&mut self, n: u32, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now.max(self.last);
        self.tokens = (self.tokens + elapsed * self.rate).min(self.rate);
        let n = f64::from(n);
        if self.tokens >= n {
            self.tokens -= n;
            true
        } else {
            false
        }
    }
}

/// Longest continuous hold of the global SMBus mutex for leases, renewals
/// and leases on other buses included: once reached, the broker lets go (a
/// program waiting for the mutex gets it) before it takes the next lease.
pub const MAX_HOLD: Duration = Duration::from_millis(BUS_LOCK_MAX_MS as u64);

/// A `BusLock`: the global SMBus mutex is held for the client until `until`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lease {
    /// Bus the client locked.
    pub bus: u8,
    /// When the broker lets go by itself.
    pub until: Instant,
}

impl Lease {
    /// A lease of `lease_ms` (capped at [`BUS_LOCK_MAX_MS`]) from `now`.
    pub fn new(bus: u8, lease_ms: u16, now: Instant) -> Self {
        Self {
            bus,
            until: now + Duration::from_millis(u64::from(lease_ms.min(BUS_LOCK_MAX_MS))),
        }
    }

    /// The same lease ending no later than `cap`.
    pub fn capped(self, cap: Instant) -> Self {
        Self {
            until: self.until.min(cap),
            ..self
        }
    }

    /// Whether it has run out at `now`.
    pub fn expired(&self, now: Instant) -> bool {
        now >= self.until
    }
}

/// The rules plus their state.
#[derive(Clone, Debug)]
pub struct Policy {
    /// Writes are refused with `DryRun`.
    pub dry_run: bool,
    limiter: RateLimiter,
}

impl Policy {
    /// Rules at `now`.
    pub fn new(dry_run: bool, now: Instant) -> Self {
        Self {
            dry_run,
            limiter: RateLimiter::new(RATE_LIMIT_PER_SEC, now),
        }
    }

    /// Check a bus operation on `bus`; counts against the rate limit.
    pub fn check(&mut self, op: &BrokerOp, bus: &BusInfo, now: Instant) -> Result<(), BrokerError> {
        if !bus.kind.is_chipset() {
            return Err(BrokerError::Unsupported);
        }
        op.validate_for(bus.kind)?;
        if op.touches_spd(bus.kind) {
            let addr = op.addr().unwrap_or(0);
            audit(bus, op, "DENIED (SPD / EEPROM / write-protect / PMIC)");
            return Err(BrokerError::AddressDenied { addr });
        }
        if !op.write_allowed() {
            let addr = op.addr().unwrap_or(0);
            audit(bus, op, "DENIED (not in the broker's write allowlist)");
            return Err(BrokerError::AddressDenied { addr });
        }
        if !self.limiter.admit_n(op.transfers(), now) {
            return Err(BrokerError::RateLimited);
        }
        if self.dry_run && op.is_write() {
            audit(bus, op, "dry run, not sent");
            return Err(BrokerError::DryRun);
        }
        Ok(())
    }
}

/// One audit line for a write (or a refused one), and for an SPD read (its
/// page select writes 0x36 / 0x37).
pub fn audit(bus: &BusInfo, op: &BrokerOp, outcome: &str) {
    if op.is_write() || matches!(op, BrokerOp::SpdRead { .. }) || outcome.starts_with("DENIED") {
        tracing::info!(target: "lightloom::audit", bus = %bus.name, "{op:?}: {outcome}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_protocol::broker::BusKind;

    fn bus(kind: BusKind) -> BusInfo {
        BusInfo {
            id: 0,
            name: "AMD PIIX4 Bus - 0x0b00".into(),
            kind,
            vendor: 0x1022,
            device: 0x790B,
            sub_vendor: 0x1022,
            sub_device: 0x790B,
            max_block_len: 32,
        }
    }

    fn read(addr: u8) -> BrokerOp {
        BrokerOp::SmbusReadByte { bus: 0, addr, reg: 0 }
    }

    #[test]
    fn rate_limit_trips() {
        let t0 = Instant::now();
        let mut p = Policy::new(false, t0);
        let b = bus(BusKind::Amd);
        for _ in 0..RATE_LIMIT_PER_SEC {
            assert!(p.check(&read(0x39), &b, t0).is_ok());
        }
        assert_eq!(p.check(&read(0x39), &b, t0), Err(BrokerError::RateLimited));
        // Half a millisecond refills one token at 2000/s.
        assert!(p.check(&read(0x39), &b, t0 + Duration::from_micros(600)).is_ok());
        assert_eq!(
            p.check(&read(0x39), &b, t0 + Duration::from_micros(600)),
            Err(BrokerError::RateLimited)
        );
    }

    #[test]
    fn lease_expires() {
        let t0 = Instant::now();
        let lease = Lease::new(0, 100, t0);
        assert!(!lease.expired(t0 + Duration::from_millis(99)));
        assert!(lease.expired(t0 + Duration::from_millis(100)));
        let capped = Lease::new(0, u16::MAX, t0);
        assert_eq!(
            capped.until,
            t0 + Duration::from_millis(u64::from(BUS_LOCK_MAX_MS))
        );
        let cut = Lease::new(0, 100, t0).capped(t0 + Duration::from_millis(30));
        assert_eq!(cut.until, t0 + Duration::from_millis(30));
        assert_eq!(Lease::new(0, 10, t0).capped(t0 + MAX_HOLD), Lease::new(0, 10, t0));
    }

    #[test]
    fn spd_write_denied_quick_too() {
        let t0 = Instant::now();
        let mut p = Policy::new(false, t0);
        let b = bus(BusKind::Amd);
        for op in [
            BrokerOp::SmbusWriteQuick { bus: 0, addr: 0x50 },
            BrokerOp::SmbusWriteQuick { bus: 0, addr: 0x36 },
            BrokerOp::SmbusWriteByte {
                bus: 0,
                addr: 0x57,
                reg: 0,
                value: 0,
            },
            BrokerOp::SmbusWriteBlock {
                bus: 0,
                addr: 0x30,
                reg: 0,
                data: vec![1],
            },
        ] {
            assert!(
                matches!(p.check(&op, &b, t0), Err(BrokerError::AddressDenied { .. })),
                "{op:?}"
            );
        }
        assert!(p.check(&read(0x50), &b, t0).is_ok(), "SPD reads are fine");
    }

    #[test]
    fn writes_outside_the_allowlist_denied() {
        let t0 = Instant::now();
        let mut p = Policy::new(false, t0);
        let b = bus(BusKind::Amd);
        let write = |addr| BrokerOp::SmbusWriteWord {
            bus: 1,
            addr,
            reg: 0,
            value: 0,
        };
        // Clock generator, EC, DDR5 PMIC / RCD, an ENE address: never written.
        for addr in [0x08, 0x2D, 0x48, 0x4C, 0x5A, 0x69, 0x77] {
            assert_eq!(
                p.check(&write(addr), &b, t0),
                Err(BrokerError::AddressDenied { addr }),
                "0x{addr:02x}"
            );
            assert_eq!(
                p.check(&BrokerOp::SmbusWriteQuick { bus: 1, addr }, &b, t0),
                Err(BrokerError::AddressDenied { addr })
            );
            assert!(p.check(&read(addr), &b, t0).is_ok(), "reads are not limited");
        }
        for addr in [0x20, 0x23, 0x27, 0x39, 0x3C] {
            assert!(p.check(&write(addr), &b, t0).is_ok(), "0x{addr:02x}");
        }
    }

    #[test]
    fn spd_read_only_at_spd_addresses_and_charged_per_transfer() {
        let t0 = Instant::now();
        let mut p = Policy::new(false, t0);
        let b = bus(BusKind::Amd);
        let spd = |addr| BrokerOp::SpdRead {
            bus: 0,
            addr,
            offset: 320,
            len: 2,
        };
        assert!(p.check(&spd(0x50), &b, t0).is_ok());
        for addr in [0x36, 0x37, 0x39] {
            assert_eq!(
                p.check(&spd(addr), &b, t0),
                Err(BrokerError::AddressDenied { addr })
            );
        }
        let mut p = Policy::new(true, t0);
        assert!(p.check(&spd(0x57), &b, t0).is_ok(), "served in dry-run");
        assert_eq!(
            p.check(&spd(0x50), &bus(BusKind::Nvidia), t0),
            Err(BrokerError::Unsupported)
        );
        let mut p = Policy::new(false, t0);
        for _ in 0..(RATE_LIMIT_PER_SEC / 4) {
            assert!(p.check(&spd(0x50), &b, t0).is_ok());
        }
        assert_eq!(
            p.check(&spd(0x50), &b, t0),
            Err(BrokerError::RateLimited),
            "4 transfers each"
        );
    }

    #[test]
    fn nvidia_kind_not_served_by_broker() {
        let t0 = Instant::now();
        let mut p = Policy::new(false, t0);
        for kind in [BusKind::Nvidia, BusKind::AmdGpu, BusKind::Nuvoton] {
            assert_eq!(
                p.check(&read(0x68), &bus(kind), t0),
                Err(BrokerError::Unsupported)
            );
        }
    }

    #[test]
    fn dry_run_refuses_writes_only() {
        let t0 = Instant::now();
        let mut p = Policy::new(true, t0);
        let b = bus(BusKind::Amd);
        assert!(p.check(&read(0x39), &b, t0).is_ok());
        let write = BrokerOp::SmbusWriteByte {
            bus: 0,
            addr: 0x39,
            reg: 1,
            value: 1,
        };
        assert_eq!(p.check(&write, &b, t0), Err(BrokerError::DryRun));
        let block = BrokerOp::SmbusWriteBlock {
            bus: 0,
            addr: 0x39,
            reg: 3,
            data: vec![0; 33],
        };
        assert_eq!(
            p.check(&block, &b, t0),
            Err(BrokerError::LengthOutOfRange { len: 33 })
        );
    }
}
