// SPDX-License-Identifier: GPL-3.0-or-later
//! SMBus policy: what a plugin may do on a bus, enforced below the JS host.
//!
//! One [`BusHandle`] per bus is shared by everything that uses it: discovery
//! and every device session on that bus. A plugin reaches it only through a
//! [`GuardedBus`], which checks every operation, in this order:
//!
//! 1. the address is a valid 7-bit address (0x03..=0x77);
//! 2. the phase allows it: during `Scan(bus)` the plugin's scan window
//!    (`SmbusAddresses()`), during a device session only the address the
//!    session is bound to;
//! 3. in a device session, the address is in the bus allowlist built from
//!    the scan results;
//! 4. writes and Quick commands to the SPD EEPROM (0x50-0x57) are refused on
//!    every bus; on DIMM buses (chipset, Nuvoton) also those to the SPD
//!    write-protect addresses (0x30-0x37), the DDR5 PMIC (0x48-0x4F) and the
//!    EEPROM area 0x50-0x5F ([`write_denied`]);
//! 5. writes to a register of the plugin's `SmbusDeniedRegisters()` (save to
//!    flash/EEPROM and the like) are refused, whatever the address (for a
//!    register-less write, its first byte is the register);
//! 6. block transfers stay within the bus's `max_block_len`;
//! 7. another session's `TransactionLock` is waited for at most
//!    [`LOCK_WAIT`], then the operation fails;
//! 8. in dry-run mode writes are recorded and reported successful, never
//!    sent (reads still reach the device).
//!
//! `TransactionLock` also reaches the bus ([`SmbusBus::bus_lock`], a
//! [`LOCK_LEASE_MS`] lease): through the I/O broker it holds the global
//! SMBus mutex, so other programs wait for the whole locked sequence.
//!
//! Failures never throw into plugin code: operations return a negative
//! status. On NVIDIA buses that is NvAPI's own status; elsewhere `-1`, as on
//! SignalRGB's chipset buses (research m5-privileged-buses.md §6.3).

use crate::device::{LogSink, WriteMode};
use huedeck_hal::smbus::{validate_address, write_denied, SmbusBus, SmbusBusDescriptor, SmbusBusKind};
use huedeck_hal::HalError;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Longest wait for another session's `TransactionLock`.
pub const LOCK_WAIT: Duration = Duration::from_millis(250);
/// Lease asked of the bus for a `TransactionLock` (the broker releases the
/// global SMBus mutex when it runs out, even if the plugin never unlocks).
pub const LOCK_LEASE_MS: u16 = 100;
/// Dry-run writes kept for the report.
const DRY_RUN_LOG_LIMIT: usize = 64;
/// Distinct refusal messages logged per guard.
const LOG_ONCE_LIMIT: usize = 64;

/// A bus shared by discovery and the sessions on it.
pub type SharedSmbus = Arc<Mutex<Box<dyn SmbusBus>>>;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Allowlist and transaction lock of one bus.
#[derive(Debug, Default)]
struct BusState {
    /// Addresses a scan found a device at: the only ones sessions may use.
    allow: Mutex<BTreeSet<u8>>,
    /// Session holding `TransactionLock`.
    holder: Mutex<Option<u64>>,
    released: Condvar,
}

/// One bus with its policy state; cheap to clone.
#[derive(Clone)]
pub struct BusHandle {
    descriptor: SmbusBusDescriptor,
    bus: SharedSmbus,
    state: Arc<BusState>,
}

impl std::fmt::Debug for BusHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BusHandle")
            .field("name", &self.descriptor.name)
            .field("key", &self.descriptor.key())
            .field("allow", &*lock(&self.state.allow))
            .finish_non_exhaustive()
    }
}

impl BusHandle {
    /// Wrap a bus; its allowlist starts empty.
    pub fn new(bus: Box<dyn SmbusBus>) -> Self {
        let descriptor = bus.descriptor().clone();
        Self {
            descriptor,
            bus: Arc::new(Mutex::new(bus)),
            state: Arc::default(),
        }
    }

    /// The bus description.
    pub fn descriptor(&self) -> &SmbusBusDescriptor {
        &self.descriptor
    }

    /// Let sessions use `addr` (a scan found a device there).
    pub fn allow(&self, addr: u8) {
        lock(&self.state.allow).insert(addr);
    }

    /// The allowlist.
    pub fn allowed(&self) -> BTreeSet<u8> {
        lock(&self.state.allow).clone()
    }
}

/// What a guard lets the plugin reach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    /// `Scan(bus)`: any address of the plugin's scan window.
    Scan {
        /// `SmbusAddresses()`, valid addresses only.
        window: BTreeSet<u8>,
    },
    /// A device session: the one address it is bound to.
    Run {
        /// The device's address.
        addr: u8,
    },
}

/// Counters of one guard.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct SmbusStats {
    /// Operations that reached the bus (or the dry-run log).
    pub ops: u64,
    /// Writes among them.
    pub writes: u64,
    /// Operations the bus failed.
    pub errors: u64,
    /// Operations the policy refused.
    pub denied: u64,
    /// Text of the latest failure or refusal.
    pub last_error: Option<String>,
    /// Dry-run writes as `[addr, reg, data…]` (no register for register-less
    /// writes; a Block Write with its count byte), the first 64.
    pub dry_run_log: Vec<Vec<u8>>,
}

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

/// A plugin's view of a bus.
pub struct GuardedBus {
    handle: BusHandle,
    phase: Phase,
    session: u64,
    mode: WriteMode,
    /// Registers never written (`SmbusDeniedRegisters()`).
    denied_registers: BTreeSet<u8>,
    stats: SmbusStats,
    logged: BTreeSet<String>,
    log_sink: Option<LogSink>,
}

impl std::fmt::Debug for GuardedBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuardedBus")
            .field("bus", &self.handle.descriptor.name)
            .field("phase", &self.phase)
            .field("session", &self.session)
            .field("mode", &self.mode)
            .field("stats", &self.stats)
            .finish_non_exhaustive()
    }
}

/// Status of a refused or failed operation.
pub type Status = i32;

/// What an operation does to the bus.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Access {
    Read,
    Write,
}

impl GuardedBus {
    /// A guard on `handle` for one phase, with its own session id.
    pub fn new(handle: BusHandle, phase: Phase, mode: WriteMode) -> Self {
        let phase = match phase {
            Phase::Scan { window } => Phase::Scan {
                window: window
                    .into_iter()
                    .filter(|a| validate_address(*a).is_ok())
                    .collect(),
            },
            run => run,
        };
        Self {
            handle,
            phase,
            session: NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
            mode,
            denied_registers: BTreeSet::new(),
            stats: SmbusStats::default(),
            logged: BTreeSet::new(),
            log_sink: None,
        }
    }

    /// Wrap into the shared form the JS natives use.
    pub fn shared(self) -> Arc<Mutex<GuardedBus>> {
        Arc::new(Mutex::new(self))
    }

    /// Refuse every write to these registers (the plugin's `SmbusDeniedRegisters()`).
    pub fn deny_registers(&mut self, registers: impl IntoIterator<Item = u8>) {
        self.denied_registers.extend(registers);
    }

    /// Forward refusal messages to `sink` as well (logged once each).
    pub fn set_log_sink(&mut self, sink: Option<LogSink>) {
        self.log_sink = sink;
    }

    /// The bus.
    pub fn descriptor(&self) -> &SmbusBusDescriptor {
        &self.handle.descriptor
    }

    /// The phase.
    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    /// The bound address of a session (the scan has none).
    pub fn address(&self) -> Option<u8> {
        match self.phase {
            Phase::Run { addr } => Some(addr),
            Phase::Scan { .. } => None,
        }
    }

    /// Counters so far.
    pub fn stats(&self) -> &SmbusStats {
        &self.stats
    }

    fn log_once(&mut self, text: String) {
        tracing::warn!(target: "huedeck::smbus", bus = %self.handle.descriptor.name, "{text}");
        if self.logged.len() < LOG_ONCE_LIMIT && self.logged.insert(text.clone()) {
            if let Some(sink) = &self.log_sink {
                sink(&text);
            }
        }
    }

    fn deny(&mut self, what: &str, why: String) -> Status {
        self.stats.denied += 1;
        let text = format!("bus: {what} refused: {why}");
        self.stats.last_error = Some(text.clone());
        self.log_once(text);
        -1
    }

    /// The status a plugin sees for `e`: NvAPI's own on NVIDIA buses, else -1.
    fn status_of(&self, e: &HalError) -> Status {
        if self.handle.descriptor.kind == SmbusBusKind::Nvidia {
            e.plugin_status()
        } else {
            -1
        }
    }

    /// Step 5 of the module docs: a write to `reg` (`None`: no register).
    fn check_register(&mut self, what: &str, reg: Option<u8>) -> Result<(), Status> {
        match reg {
            Some(reg) if self.denied_registers.contains(&reg) => Err(self.deny(
                what,
                format!("register 0x{reg:02x} is in the plugin's SmbusDeniedRegisters() (never written)"),
            )),
            _ => Ok(()),
        }
    }

    /// Steps 1-4 and 7 of the module docs.
    fn admit(&mut self, what: &str, addr: u8, access: Access) -> Result<(), Status> {
        if validate_address(addr).is_err() {
            return Err(self.deny(what, format!("0x{addr:02x} is not a 7-bit SMBus address")));
        }
        match &self.phase {
            Phase::Scan { window } if !window.contains(&addr) => {
                return Err(self.deny(
                    what,
                    format!("0x{addr:02x} is outside the plugin's SmbusAddresses()"),
                ));
            }
            Phase::Run { addr: bound } if *bound != addr => {
                let bound = *bound;
                return Err(self.deny(
                    what,
                    format!("0x{addr:02x} is not this device's address 0x{bound:02x}"),
                ));
            }
            Phase::Run { .. } if !lock(&self.handle.state.allow).contains(&addr) => {
                return Err(self.deny(what, format!("0x{addr:02x} was not found by a scan of this bus")));
            }
            _ => {}
        }
        if access == Access::Write && write_denied(self.handle.descriptor.kind, addr) {
            return Err(self.deny(
                what,
                format!("0x{addr:02x} is an SPD / EEPROM / write-protect / PMIC address (never written)"),
            ));
        }
        if !self.wait_for_foreign_lock() {
            return Err(self.deny(what, "another device holds the bus's TransactionLock".into()));
        }
        Ok(())
    }

    /// Wait while another session holds the transaction lock (at most [`LOCK_WAIT`]).
    fn wait_for_foreign_lock(&self) -> bool {
        let state = &self.handle.state;
        let deadline = Instant::now() + LOCK_WAIT;
        let mut holder = lock(&state.holder);
        while holder.is_some_and(|h| h != self.session) {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            holder = state
                .released
                .wait_timeout(holder, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        true
    }

    fn check_block(&mut self, what: &str, len: usize) -> Result<(), Status> {
        let max = self.handle.descriptor.max_block_len;
        if len == 0 || len > max {
            return Err(self.deny(what, format!("{len} bytes (this bus moves 1..={max})")));
        }
        Ok(())
    }

    /// Run one admitted operation on the bus, counting it.
    fn io<T>(
        &mut self,
        what: &str,
        f: impl FnOnce(&mut dyn SmbusBus) -> huedeck_hal::HalResult<T>,
    ) -> Result<T, Status> {
        self.stats.ops += 1;
        let r = {
            let mut bus = lock(&self.handle.bus);
            f(bus.as_mut())
        };
        r.map_err(|e| {
            self.stats.errors += 1;
            self.stats.last_error = Some(format!("bus: {what} failed: {e}"));
            self.status_of(&e)
        })
    }

    /// A write to register `reg` (`None` for a Quick command): sent, or
    /// recorded in dry-run mode.
    fn write(
        &mut self,
        what: &str,
        addr: u8,
        reg: Option<u8>,
        record: Vec<u8>,
        f: impl FnOnce(&mut dyn SmbusBus) -> huedeck_hal::HalResult<()>,
    ) -> Result<(), Status> {
        self.admit(what, addr, Access::Write)?;
        self.check_register(what, reg)?;
        self.stats.writes += 1;
        if self.mode == WriteMode::DryRun {
            self.stats.ops += 1;
            if self.stats.dry_run_log.len() < DRY_RUN_LOG_LIMIT {
                self.stats.dry_run_log.push(record);
            }
            return Ok(());
        }
        self.io(what, f)
    }

    /// `WriteQuick(addr)` (counts as a write).
    pub fn write_quick(&mut self, addr: u8) -> Result<(), Status> {
        self.write("WriteQuick", addr, None, vec![addr], |b| b.write_quick(addr))
    }

    /// `ReadByte(addr, reg)`.
    pub fn read_byte(&mut self, addr: u8, reg: u8) -> Result<u8, Status> {
        self.admit("ReadByte", addr, Access::Read)?;
        self.io("ReadByte", |b| b.read_byte(addr, reg))
    }

    /// `WriteByte(addr, reg, value)`.
    pub fn write_byte(&mut self, addr: u8, reg: u8, value: u8) -> Result<(), Status> {
        self.write("WriteByte", addr, Some(reg), vec![addr, reg, value], |b| {
            b.write_byte(addr, reg, value)
        })
    }

    /// `ReadWord(addr, reg)`.
    pub fn read_word(&mut self, addr: u8, reg: u8) -> Result<u16, Status> {
        self.admit("ReadWord", addr, Access::Read)?;
        self.io("ReadWord", |b| b.read_word(addr, reg))
    }

    /// `WriteWord(addr, reg, word)` (LSB first).
    pub fn write_word(&mut self, addr: u8, reg: u8, value: u16) -> Result<(), Status> {
        let [lo, hi] = value.to_le_bytes();
        self.write("WriteWord", addr, Some(reg), vec![addr, reg, lo, hi], |b| {
            b.write_word(addr, reg, value)
        })
    }

    /// `WriteBlock(addr, reg, data)`: an SMBus Block Write.
    pub fn write_block(&mut self, addr: u8, reg: u8, data: &[u8]) -> Result<(), Status> {
        self.check_block("WriteBlock", data.len())?;
        let mut record = vec![addr, reg, data.len() as u8];
        record.extend_from_slice(data);
        self.write("WriteBlock", addr, Some(reg), record, |b| {
            b.write_block(addr, reg, data)
        })
    }

    /// `ReadBlockBytes(addr, reg, len)`: exactly `len` bytes after the register.
    pub fn read_block(&mut self, addr: u8, reg: u8, len: usize) -> Result<Vec<u8>, Status> {
        self.check_block("ReadBlockBytes", len)?;
        self.admit("ReadBlockBytes", addr, Access::Read)?;
        let mut buf = vec![0u8; len];
        let n = self.io("ReadBlockBytes", |b| b.read_block(addr, reg, &mut buf))?;
        buf.truncate(n);
        Ok(buf)
    }

    /// `WriteBlockWithoutRegister(addr, data)`.
    pub fn write_block_no_reg(&mut self, addr: u8, data: &[u8]) -> Result<(), Status> {
        self.check_block("WriteBlockWithoutRegister", data.len())?;
        let mut record = vec![addr];
        record.extend_from_slice(data);
        self.write(
            "WriteBlockWithoutRegister",
            addr,
            data.first().copied(),
            record,
            |b| b.write_block_no_reg(addr, data),
        )
    }

    /// `ReadBlockWithoutRegister(addr, len)`.
    pub fn read_block_no_reg(&mut self, addr: u8, len: usize) -> Result<Vec<u8>, Status> {
        self.check_block("ReadBlockWithoutRegister", len)?;
        self.admit("ReadBlockWithoutRegister", addr, Access::Read)?;
        let mut buf = vec![0u8; len];
        let n = self.io("ReadBlockWithoutRegister", |b| {
            b.read_block_no_reg(addr, &mut buf)
        })?;
        buf.truncate(n);
        Ok(buf)
    }

    /// `ReadByteWithoutRegister(addr)`: SMBus Receive Byte.
    pub fn read_byte_no_reg(&mut self, addr: u8) -> Result<u8, Status> {
        self.admit("ReadByteWithoutRegister", addr, Access::Read)?;
        self.io("ReadByteWithoutRegister", |b| b.read_byte_no_reg(addr))
    }

    /// `TransactionLock()`: hold the bus for this session (waits at most
    /// [`LOCK_WAIT`]), and ask the bus itself for a lease.
    pub fn lock(&mut self) -> bool {
        let state = Arc::clone(&self.handle.state);
        let deadline = Instant::now() + LOCK_WAIT;
        let mut holder = lock(&state.holder);
        loop {
            match *holder {
                None => {
                    *holder = Some(self.session);
                    drop(holder);
                    return self.lease_bus();
                }
                Some(h) if h == self.session => return true,
                Some(_) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        drop(holder);
                        self.deny("TransactionLock", "another device holds the bus".into());
                        return false;
                    }
                    holder = state
                        .released
                        .wait_timeout(holder, left)
                        .unwrap_or_else(|e| e.into_inner())
                        .0;
                }
            }
        }
    }

    /// The bus-side half of [`GuardedBus::lock`]; on failure the session lock
    /// is given back.
    fn lease_bus(&mut self) -> bool {
        let r = lock(&self.handle.bus).bus_lock(LOCK_LEASE_MS);
        match r {
            Ok(()) => true,
            Err(e) => {
                self.stats.errors += 1;
                self.release_holder();
                self.log_once(format!("bus: TransactionLock failed: {e}"));
                self.stats.last_error = Some(format!("bus: TransactionLock failed: {e}"));
                false
            }
        }
    }

    /// `TransactionUnlock()`.
    pub fn unlock(&mut self) {
        self.force_unlock();
    }

    /// `HasTransactionLock()`.
    pub fn has_lock(&self) -> bool {
        *lock(&self.handle.state.holder) == Some(self.session)
    }

    /// Release the lock if this session holds it; whether it did.
    pub fn force_unlock(&mut self) -> bool {
        if *lock(&self.handle.state.holder) != Some(self.session) {
            return false;
        }
        if let Err(e) = lock(&self.handle.bus).bus_unlock() {
            // The bus lease runs out by itself; nothing else to undo.
            tracing::debug!(target: "huedeck::smbus", bus = %self.handle.descriptor.name, "bus unlock: {e}");
        }
        self.release_holder()
    }

    fn release_holder(&self) -> bool {
        let state = &self.handle.state;
        let mut holder = lock(&state.holder);
        if *holder == Some(self.session) {
            *holder = None;
            state.released.notify_all();
            true
        } else {
            false
        }
    }
}

impl Drop for GuardedBus {
    fn drop(&mut self) {
        self.force_unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use huedeck_hal::mock::{MockSmbus, TransactionKind, TransactionRecorder};

    fn handle(kind: SmbusBusKind, addrs: &[u8]) -> (BusHandle, TransactionRecorder) {
        let rec = TransactionRecorder::new();
        let mut bus = MockSmbus::new("bus", kind, rec.clone());
        for a in addrs {
            bus.add_device(*a, []);
        }
        (BusHandle::new(Box::new(bus)), rec)
    }

    fn scan(h: &BusHandle, window: &[u8]) -> GuardedBus {
        GuardedBus::new(
            h.clone(),
            Phase::Scan {
                window: window.iter().copied().collect(),
            },
            WriteMode::Live,
        )
    }

    fn run(h: &BusHandle, addr: u8) -> GuardedBus {
        GuardedBus::new(h.clone(), Phase::Run { addr }, WriteMode::Live)
    }

    #[test]
    fn scan_phase_refuses_addresses_outside_window() {
        let (h, rec) = handle(SmbusBusKind::Amd, &[0x39, 0x3A]);
        let mut g = scan(&h, &[0x39]);
        assert!(g.write_quick(0x39).is_ok());
        assert_eq!(g.write_quick(0x3A), Err(-1));
        assert_eq!(g.read_byte(0x3A, 0), Err(-1));
        assert_eq!(g.stats().denied, 2);
        assert_eq!(rec.payloads(TransactionKind::SmbusWriteQuick), vec![vec![0x39]]);
    }

    #[test]
    fn run_phase_refuses_other_addresses() {
        let (h, rec) = handle(SmbusBusKind::Nvidia, &[0x68, 0x69]);
        let mut g = run(&h, 0x68);
        assert_eq!(g.read_byte(0x68, 0x22), Err(-1), "0x68 not scanned yet");
        h.allow(0x68);
        h.allow(0x69);
        assert_eq!(g.read_byte(0x68, 0x22), Ok(0xFF));
        assert_eq!(g.write_byte(0x69, 0x30, 1), Err(-1), "bound to 0x68");
        assert!(rec.payloads(TransactionKind::SmbusWrite).is_empty());
    }

    #[test]
    fn spd_and_write_protect_writes_denied_on_chipset_buses() {
        let (h, rec) = handle(SmbusBusKind::Amd, &[0x50, 0x36, 0x4A, 0x5C]);
        let mut g = scan(&h, &[0x50, 0x36, 0x4A, 0x5C]);
        assert_eq!(g.write_byte(0x50, 0, 1), Err(-1));
        assert_eq!(g.write_quick(0x36), Err(-1), "quick counts as a write");
        assert_eq!(g.write_block(0x50, 0, &[1, 2]), Err(-1));
        assert_eq!(g.write_word(0x36, 0, 1), Err(-1));
        assert_eq!(g.write_byte(0x4A, 0, 1), Err(-1), "DDR5 PMIC");
        assert_eq!(g.write_quick(0x5C), Err(-1), "EEPROM area");
        assert_eq!(g.stats().denied, 6);
        assert!(rec.snapshot().is_empty());
    }

    #[test]
    fn denied_registers_are_never_written() {
        let (h, rec) = handle(SmbusBusKind::Nvidia, &[0x68]);
        h.allow(0x68);
        let mut g = run(&h, 0x68);
        g.deny_registers([0x3F]);
        assert_eq!(g.write_byte(0x68, 0x3F, 1), Err(-1), "MSI_GPU_REG_SAVE");
        assert_eq!(g.write_word(0x68, 0x3F, 1), Err(-1));
        assert_eq!(g.write_block(0x68, 0x3F, &[1]), Err(-1));
        assert_eq!(
            g.write_block_no_reg(0x68, &[0x3F, 1]),
            Err(-1),
            "first byte is the register"
        );
        assert_eq!(g.read_byte(0x68, 0x3F), Ok(0xFF), "reads are fine");
        assert!(g.write_byte(0x68, 0x30, 1).is_ok());
        assert_eq!(g.stats().denied, 4);
        assert_eq!(rec.payloads(TransactionKind::SmbusWrite).len(), 1);
    }

    #[test]
    fn spd_read_allowed() {
        let (h, _) = handle(SmbusBusKind::Intel, &[0x50]);
        let mut g = scan(&h, &[0x50]);
        assert_eq!(g.read_byte(0x50, 2), Ok(0xFF));
    }

    #[test]
    fn nvidia_bus_keeps_dimm_addresses_but_never_the_spd() {
        let (h, _) = handle(SmbusBusKind::Nvidia, &[0x50, 0x36, 0x48]);
        let mut g = scan(&h, &[0x50, 0x36, 0x48]);
        assert_eq!(
            g.write_byte(0x50, 0, 1),
            Err(-1),
            "SPD range: denied on every bus"
        );
        assert_eq!(g.write_byte(0x36, 0, 1), Ok(()));
        assert_eq!(g.write_byte(0x48, 0, 1), Ok(()));
        assert_eq!(g.write_byte(0x51, 0, 1), Err(-1), "still the scan window");
        assert_eq!(
            g.write_quick(0x36),
            Err(-1),
            "no Quick on NvAPI buses (the bus refuses it)"
        );
        assert_eq!(g.stats().errors, 1);
        assert_eq!(g.stats().denied, 2);
    }

    #[test]
    fn transaction_lock_blocks_other_session_then_times_out() {
        let (h, _) = handle(SmbusBusKind::Amd, &[0x39, 0x3A]);
        h.allow(0x39);
        h.allow(0x3A);
        let mut a = run(&h, 0x39);
        let mut b = run(&h, 0x3A);
        assert!(a.lock() && a.has_lock());
        let t = Instant::now();
        assert_eq!(b.write_byte(0x3A, 1, 1), Err(-1));
        assert!(t.elapsed() >= LOCK_WAIT);
        assert!(!b.lock());
        assert!(a.write_byte(0x39, 1, 1).is_ok(), "the holder keeps working");
        a.unlock();
        assert!(!a.has_lock());
        assert!(b.write_byte(0x3A, 1, 1).is_ok());
        // A lock left behind is released when its guard goes away.
        assert!(b.lock());
        drop(b);
        assert!(a.lock());
        assert!(a.force_unlock() && !a.force_unlock());
    }

    #[test]
    fn transaction_lock_leases_the_bus() {
        let (h, rec) = handle(SmbusBusKind::Amd, &[0x39]);
        h.allow(0x39);
        let mut g = run(&h, 0x39);
        assert!(g.lock());
        assert!(g.lock(), "re-entrant, no second lease");
        assert!(g.write_byte(0x39, 1, 1).is_ok());
        g.unlock();
        g.unlock();
        let leases: Vec<Vec<u8>> = rec
            .snapshot()
            .into_iter()
            .filter(|t| t.kind == TransactionKind::SmbusLock)
            .map(|t| t.bytes)
            .collect();
        assert_eq!(leases, vec![LOCK_LEASE_MS.to_le_bytes().to_vec(), vec![]]);
    }

    /// A bus whose lease always fails (a broker answering BusContended).
    struct Contended(huedeck_hal::SmbusBusDescriptor);

    impl SmbusBus for Contended {
        fn descriptor(&self) -> &SmbusBusDescriptor {
            &self.0
        }
        fn write_quick(&mut self, _: u8) -> huedeck_hal::HalResult<()> {
            Ok(())
        }
        fn read_byte(&mut self, _: u8, _: u8) -> huedeck_hal::HalResult<u8> {
            Ok(0)
        }
        fn write_byte(&mut self, _: u8, _: u8, _: u8) -> huedeck_hal::HalResult<()> {
            Ok(())
        }
        fn read_word(&mut self, _: u8, _: u8) -> huedeck_hal::HalResult<u16> {
            Ok(0)
        }
        fn write_word(&mut self, _: u8, _: u8, _: u16) -> huedeck_hal::HalResult<()> {
            Ok(())
        }
        fn write_block(&mut self, _: u8, _: u8, _: &[u8]) -> huedeck_hal::HalResult<()> {
            Ok(())
        }
        fn read_block(&mut self, _: u8, _: u8, _: &mut [u8]) -> huedeck_hal::HalResult<usize> {
            Ok(0)
        }
        fn write_block_no_reg(&mut self, _: u8, _: &[u8]) -> huedeck_hal::HalResult<()> {
            Ok(())
        }
        fn read_block_no_reg(&mut self, _: u8, _: &mut [u8]) -> huedeck_hal::HalResult<usize> {
            Ok(0)
        }
        fn read_byte_no_reg(&mut self, _: u8) -> huedeck_hal::HalResult<u8> {
            Ok(0)
        }
        fn bus_lock(&mut self, _: u16) -> huedeck_hal::HalResult<()> {
            Err(HalError::Timeout(50))
        }
    }

    #[test]
    fn failed_bus_lease_gives_the_lock_back() {
        let h = BusHandle::new(Box::new(Contended(SmbusBusDescriptor {
            name: "AMD PIIX4 Bus - 0x0b00".into(),
            kind: SmbusBusKind::Amd,
            pci: Default::default(),
            instance: "0b00".into(),
            max_block_len: 32,
        })));
        h.allow(0x39);
        let mut a = run(&h, 0x39);
        assert!(!a.lock());
        assert!(!a.has_lock());
        assert_eq!(a.stats().errors, 1);
        let mut b = run(&h, 0x39);
        assert!(b.write_byte(0x39, 1, 1).is_ok(), "no session lock left behind");
    }

    #[test]
    fn dry_run_records_without_writing() {
        let (h, rec) = handle(SmbusBusKind::Nvidia, &[0x68]);
        h.allow(0x68);
        let mut g = GuardedBus::new(h.clone(), Phase::Run { addr: 0x68 }, WriteMode::DryRun);
        assert_eq!(g.write_byte(0x68, 0x30, 0x12), Ok(()));
        assert_eq!(g.write_block(0x68, 0x03, &[1, 2]), Ok(()));
        assert_eq!(g.read_byte(0x68, 0x30), Ok(0xFF), "reads still reach the device");
        assert_eq!(
            g.stats().dry_run_log,
            vec![vec![0x68, 0x30, 0x12], vec![0x68, 0x03, 2, 1, 2]]
        );
        assert!(rec.payloads(TransactionKind::SmbusWrite).is_empty());
    }

    #[test]
    fn block_over_max_is_refused() {
        let (h, rec) = handle(SmbusBusKind::Amd, &[0x39]);
        let mut g = scan(&h, &[0x39]);
        assert_eq!(g.write_block(0x39, 3, &[0; 33]), Err(-1));
        assert_eq!(g.write_block(0x39, 3, &[]), Err(-1));
        assert_eq!(g.read_block(0x39, 3, 40), Err(-1));
        assert!(g.write_block(0x39, 3, &[0; 32]).is_ok());
        assert_eq!(rec.payloads(TransactionKind::SmbusWrite).len(), 1);
    }
}
