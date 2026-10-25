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
//!    register-less write, its first byte is the register), except inside
//!    the plugin's `SaveToDevice(request)` call that the user asked for
//!    ([`GuardedBus::set_save_window`], [`crate::save`]);
//! 6. block transfers stay within the bus's `max_block_len` (a register-less
//!    write within [`SmbusBusDescriptor::max_raw_write_len`]: up to 140 bytes on
//!    NvAPI buses, for MSI's per-LED GPU frame);
//! 7. the bus is taken in turn: a session's `TransactionLock`, and every
//!    lone operation of a session that holds none, waits in one FIFO queue
//!    per bus (sessions on one bus never starve each other) for at most
//!    [`LOCK_WAIT`]; past that the operation is not sent (`-1`, counted as
//!    `contended`, not as an error: the frame is skipped and sent again);
//! 8. in dry-run mode writes are recorded and reported successful, never
//!    sent (reads still reach the device; `bus.IsDryRun()` tells the plugin).
//!
//! Plugins that declare `SmbusBatchWrites()` (Lightloom extension) get their
//! writes inside a `TransactionLock` queued after the checks above and sent
//! together ([`SmbusBus::run_batch`]: through the I/O broker one request and
//! one hold of the global SMBus mutex) at the next read (which ends the
//! batch and returns its value), at `TransactionUnlock()` (which returns
//! `0` or the status of the first queued write that failed; the ops after
//! it are not sent), when [`MAX_QUEUED_WRITES`] are waiting, and when the
//! lock is let go for the plugin. A queued write answers `0` at once.
//!
//! The host's own SPD check of RAM plugins ([`crate::spd`]) reads the bus
//! through [`BusHandle::with_bus`], never through a guard; a plugin's guard
//! only carries its result ([`GuardedBus::set_spd_slots`], `bus.SpdSlots()`).
//!
//! `TransactionLock` also reaches the bus ([`SmbusBus::bus_lock`], a
//! [`LOCK_LEASE_MS`] lease): through the I/O broker it holds the global
//! SMBus mutex, so other programs wait for the whole locked sequence.
//!
//! Failures never throw into plugin code: operations return a negative
//! status. On NVIDIA buses that is NvAPI's own status; elsewhere `-1`, as on
//! SignalRGB's chipset buses (research m5-privileged-buses.md §6.3).

use crate::device::{LogSink, WriteMode};
use crate::spd::SpdSlot;
use lightloom_hal::smbus::{
    validate_address, write_denied, SmbusBus, SmbusBusDescriptor, SmbusBusKind, SmbusOp, SmbusReply,
};
use lightloom_hal::HalError;
use std::collections::{BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Longest wait for the bus while other sessions on it hold or wait for it
/// (then the operation, usually the frame, is skipped).
pub const LOCK_WAIT: Duration = Duration::from_millis(100);
/// Writes queued under `SmbusBatchWrites()` before they are sent anyway.
pub const MAX_QUEUED_WRITES: usize = 32;
/// Plugin log lines a scan outcome keeps.
pub const KEPT_LOG_LINES: usize = 32;
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

/// Who has the bus, and who waits for it, in order.
#[derive(Debug, Default)]
struct Turns {
    /// Session holding the bus (`TransactionLock`, or one lone operation).
    holder: Option<u64>,
    /// Sessions waiting, first come first served.
    waiting: VecDeque<u64>,
}

/// Allowlist and turn queue of one bus.
#[derive(Debug, Default)]
struct BusState {
    /// Addresses a scan found a device at: the only ones sessions may use.
    allow: Mutex<BTreeSet<u8>>,
    turns: Mutex<Turns>,
    released: Condvar,
}

impl BusState {
    /// Give `session` the bus, waiting in line at most `wait`. A session
    /// letting go hands the bus to the first in line, so nobody jumps the
    /// queue (fair). `true` when `session` holds it.
    fn take(&self, session: u64, wait: Duration) -> bool {
        let deadline = Instant::now() + wait;
        let mut t = lock(&self.turns);
        if t.holder == Some(session) {
            return true;
        }
        if t.holder.is_none() && t.waiting.is_empty() {
            t.holder = Some(session);
            return true;
        }
        t.waiting.push_back(session);
        loop {
            if t.holder == Some(session) {
                return true;
            }
            // Nobody holds it and this session is first in line (a holder
            // went away without handing over): take it.
            if t.holder.is_none() && t.waiting.front() == Some(&session) {
                t.waiting.pop_front();
                t.holder = Some(session);
                return true;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                t.waiting.retain(|s| *s != session);
                return false;
            }
            t = self
                .released
                .wait_timeout(t, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }

    /// Hand the bus to the next in line if `session` holds it; whether it did.
    fn give(&self, session: u64) -> bool {
        let mut t = lock(&self.turns);
        if t.holder != Some(session) {
            return false;
        }
        t.holder = t.waiting.pop_front();
        self.released.notify_all();
        true
    }

    fn holder(&self) -> Option<u64> {
        lock(&self.turns).holder
    }
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

    /// Run `f` on the bus itself, outside every plugin guard: for the host's
    /// own reads (the SPD check, [`crate::spd`]). Not reachable from plugins.
    pub(crate) fn with_bus<R>(&self, f: impl FnOnce(&mut dyn SmbusBus) -> R) -> R {
        let mut bus = lock(&self.bus);
        f(bus.as_mut())
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
    /// Operations not sent because other sessions kept the bus past
    /// [`LOCK_WAIT`] (the frame is skipped; not a failure).
    pub contended: u64,
    /// Batches sent ([`SmbusBus::run_batch`], `SmbusBatchWrites()`).
    pub batches: u64,
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
    /// Inside a user-requested `SaveToDevice(request)`: the denied registers may be written.
    save_window: bool,
    stats: SmbusStats,
    logged: BTreeSet<String>,
    log_sink: Option<LogSink>,
    /// The SPD slots a RAM plugin's scan is told about (`bus.SpdSlots()`).
    spd_slots: Option<Vec<SpdSlot>>,
    /// `SmbusBatchWrites()`: queue writes inside `TransactionLock`.
    batch_writes: bool,
    /// Writes waiting for the batch (only while this session holds the bus).
    queue: Vec<SmbusOp>,
    /// Status of the first queued write that failed, for `TransactionUnlock()`.
    queue_status: Option<Status>,
    /// Plugin log lines to keep (the scan's outcome), when set.
    kept_log: Option<Arc<Mutex<Vec<String>>>>,
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
            save_window: false,
            stats: SmbusStats::default(),
            logged: BTreeSet::new(),
            log_sink: None,
            spd_slots: None,
            batch_writes: false,
            queue: Vec::new(),
            queue_status: None,
            kept_log: None,
        }
    }

    /// Queue writes inside `TransactionLock` and send them as one batch
    /// (the plugin's `SmbusBatchWrites()`; device sessions only).
    pub fn set_batch_writes(&mut self, on: bool) {
        self.batch_writes = on && matches!(self.phase, Phase::Run { .. });
    }

    /// Keep the plugin's `bus.log()` lines and refusal messages in `log`
    /// (at most [`KEPT_LOG_LINES`]): the scan outcome shows them.
    pub fn keep_log(&mut self, log: Arc<Mutex<Vec<String>>>) {
        self.kept_log = Some(log);
    }

    /// A `bus.log()` line of the plugin.
    pub fn plugin_log(&self, text: &str) {
        tracing::info!(target: "lightloom::plugin", bus = %self.handle.descriptor.name, "{text}");
        self.keep(text);
    }

    fn keep(&self, text: &str) {
        if let Some(log) = &self.kept_log {
            let mut log = lock(log);
            if log.len() < KEPT_LOG_LINES {
                log.push(text.to_string());
            }
        }
    }

    /// Writes are recorded, never sent (`bus.IsDryRun()`).
    pub fn dry_run(&self) -> bool {
        self.mode == WriteMode::DryRun
    }

    /// The DIMM slots this scan may rely on (the SPD check's matches),
    /// shown to the plugin by `bus.SpdSlots()`.
    pub fn set_spd_slots(&mut self, slots: Vec<SpdSlot>) {
        self.spd_slots = Some(slots);
    }

    /// See [`GuardedBus::set_spd_slots`] (`None`: no SPD check ran).
    pub fn spd_slots(&self) -> Option<&[SpdSlot]> {
        self.spd_slots.as_deref()
    }

    /// Wrap into the shared form the JS natives use.
    pub fn shared(self) -> Arc<Mutex<GuardedBus>> {
        Arc::new(Mutex::new(self))
    }

    /// Refuse every write to these registers (the plugin's `SmbusDeniedRegisters()`).
    pub fn deny_registers(&mut self, registers: impl IntoIterator<Item = u8>) {
        self.denied_registers.extend(registers);
    }

    /// Open (`true`) or close the save window: while it is open, writes to the
    /// `SmbusDeniedRegisters()` go through (and are logged). The session opens it
    /// for the duration of one user-requested `SaveToDevice(request)` call only.
    pub fn set_save_window(&mut self, open: bool) {
        self.save_window = open;
    }

    /// Whether the save window is open.
    pub fn save_window(&self) -> bool {
        self.save_window
    }

    /// Writes go through or are recorded (`DryRun`): a preview runs dry.
    pub fn set_write_mode(&mut self, mode: WriteMode) {
        self.mode = mode;
    }

    /// The write mode.
    pub fn write_mode(&self) -> WriteMode {
        self.mode
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
        tracing::warn!(target: "lightloom::smbus", bus = %self.handle.descriptor.name, "{text}");
        if self.logged.len() < LOG_ONCE_LIMIT && self.logged.insert(text.clone()) {
            if let Some(sink) = &self.log_sink {
                sink(&text);
            }
            self.keep(&text);
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
            Some(reg) if self.denied_registers.contains(&reg) && self.save_window => {
                let text =
                    format!("bus: {what} to register 0x{reg:02x} (SmbusDeniedRegisters) inside SaveToDevice");
                tracing::warn!(target: "lightloom::smbus", bus = %self.handle.descriptor.name, "{text}");
                if let Some(sink) = &self.log_sink {
                    sink(&text);
                }
                Ok(())
            }
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
        Ok(())
    }

    /// Other sessions kept the bus past [`LOCK_WAIT`]: not sent.
    fn contended(&mut self, what: &str) -> Status {
        self.stats.contended += 1;
        let text = format!(
            "bus: {what} not sent: other devices on this bus kept it {} ms",
            LOCK_WAIT.as_millis()
        );
        self.stats.last_error = Some(text.clone());
        self.log_once(text);
        -1
    }

    fn check_block(&mut self, what: &str, len: usize) -> Result<(), Status> {
        let max = self.handle.descriptor.max_block_len;
        self.check_len(what, len, max)
    }

    fn check_len(&mut self, what: &str, len: usize, max: usize) -> Result<(), Status> {
        if len == 0 || len > max {
            return Err(self.deny(what, format!("{len} bytes (this bus moves 1..={max})")));
        }
        Ok(())
    }

    /// Run one admitted operation on the bus, counting it: in this
    /// session's turn (taken for the operation alone when the session holds
    /// no `TransactionLock`), after the writes queued before it.
    fn io<T>(
        &mut self,
        what: &str,
        f: impl FnOnce(&mut dyn SmbusBus) -> lightloom_hal::HalResult<T>,
    ) -> Result<T, Status> {
        let state = Arc::clone(&self.handle.state);
        let held = state.holder() == Some(self.session);
        if !held && !state.take(self.session, LOCK_WAIT) {
            return Err(self.contended(what));
        }
        let r = self.flush().and_then(|()| {
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
        });
        if !held {
            state.give(self.session);
        }
        r
    }

    /// Whether writes are queued now (`SmbusBatchWrites()`, inside this
    /// session's `TransactionLock`).
    fn queues(&self) -> bool {
        self.batch_writes && self.has_lock()
    }

    /// Queue `op` (its checks passed); sent at once when the queue is full.
    fn enqueue(&mut self, op: SmbusOp) -> Result<(), Status> {
        self.queue.push(op);
        if self.queue.len() >= MAX_QUEUED_WRITES {
            // A failure is reported by TransactionUnlock(), as for any queued write.
            let _ = self.flush();
        }
        Ok(())
    }

    /// Send the queued writes as one batch; `Err` with the status of the
    /// first that failed (the rest are dropped, never sent).
    fn flush(&mut self) -> Result<(), Status> {
        self.send_batch(None).map(drop)
    }

    /// Send the queued writes and then `read` (if any) as one batch; the
    /// read's reply. A failed queued write fails the read (not sent).
    fn send_batch(&mut self, read: Option<SmbusOp>) -> Result<Option<SmbusReply>, Status> {
        if self.queue.is_empty() && read.is_none() {
            return Ok(None);
        }
        let mut ops = std::mem::take(&mut self.queue);
        let queued = ops.len();
        ops.extend(read);
        let out = lock(&self.handle.bus).run_batch(&ops);
        self.stats.batches += 1;
        self.stats.ops += out.replies.len() as u64 + u64::from(out.error.is_some());
        match out.error {
            None if out.replies.len() == ops.len() => Ok(if queued < ops.len() {
                out.replies.last().copied()
            } else {
                None
            }),
            error => {
                let at = out.replies.len();
                let status = error.as_ref().map_or(-1, |e| self.status_of(e));
                let why = error.map_or_else(|| "no answer".to_string(), |e| e.to_string());
                let what = if at < queued {
                    "queued write"
                } else {
                    "read after queued writes"
                };
                let text = format!(
                    "bus: {what} failed: {why} (op {} of a batch of {}; the rest not sent)",
                    at + 1,
                    ops.len()
                );
                self.stats.errors += 1;
                self.stats.last_error = Some(text.clone());
                self.log_once(text);
                if at < queued {
                    self.queue_status.get_or_insert(status);
                }
                Err(status)
            }
        }
    }

    /// A read that can end a batch: with writes queued, sent after them as
    /// one batch; otherwise alone.
    fn read_op<T>(
        &mut self,
        what: &str,
        op: SmbusOp,
        reply: impl Fn(SmbusReply) -> Option<T>,
        alone: impl FnOnce(&mut dyn SmbusBus) -> lightloom_hal::HalResult<T>,
    ) -> Result<T, Status> {
        if self.queue.is_empty() {
            return self.io(what, alone);
        }
        match self.send_batch(Some(op))? {
            Some(r) => reply(r).ok_or(-1),
            None => Err(-1),
        }
    }

    /// A write to register `reg` (`None` for a Quick command): sent, or
    /// recorded in dry-run mode.
    fn write(
        &mut self,
        what: &str,
        (addr, reg): (u8, Option<u8>),
        record: Vec<u8>,
        op: SmbusOp,
        f: impl FnOnce(&mut dyn SmbusBus) -> lightloom_hal::HalResult<()>,
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
        if self.queues() {
            return self.enqueue(op);
        }
        self.io(what, f)
    }

    /// `WriteQuick(addr)` (counts as a write).
    pub fn write_quick(&mut self, addr: u8) -> Result<(), Status> {
        self.write(
            "WriteQuick",
            (addr, None),
            vec![addr],
            SmbusOp::WriteQuick { addr },
            |b| b.write_quick(addr),
        )
    }

    /// `ReadByte(addr, reg)`.
    pub fn read_byte(&mut self, addr: u8, reg: u8) -> Result<u8, Status> {
        self.admit("ReadByte", addr, Access::Read)?;
        self.read_op(
            "ReadByte",
            SmbusOp::ReadByte { addr, reg },
            |r| match r {
                SmbusReply::Byte(v) => Some(v),
                _ => None,
            },
            |b| b.read_byte(addr, reg),
        )
    }

    /// `WriteByte(addr, reg, value)`.
    pub fn write_byte(&mut self, addr: u8, reg: u8, value: u8) -> Result<(), Status> {
        self.write(
            "WriteByte",
            (addr, Some(reg)),
            vec![addr, reg, value],
            SmbusOp::WriteByte { addr, reg, value },
            |b| b.write_byte(addr, reg, value),
        )
    }

    /// `ReadWord(addr, reg)`.
    pub fn read_word(&mut self, addr: u8, reg: u8) -> Result<u16, Status> {
        self.admit("ReadWord", addr, Access::Read)?;
        self.read_op(
            "ReadWord",
            SmbusOp::ReadWord { addr, reg },
            |r| match r {
                SmbusReply::Word(v) => Some(v),
                _ => None,
            },
            |b| b.read_word(addr, reg),
        )
    }

    /// `WriteWord(addr, reg, word)` (LSB first).
    pub fn write_word(&mut self, addr: u8, reg: u8, value: u16) -> Result<(), Status> {
        let [lo, hi] = value.to_le_bytes();
        self.write(
            "WriteWord",
            (addr, Some(reg)),
            vec![addr, reg, lo, hi],
            SmbusOp::WriteWord { addr, reg, value },
            |b| b.write_word(addr, reg, value),
        )
    }

    /// `WriteBlock(addr, reg, data)`: an SMBus Block Write.
    pub fn write_block(&mut self, addr: u8, reg: u8, data: &[u8]) -> Result<(), Status> {
        self.check_block("WriteBlock", data.len())?;
        let mut record = vec![addr, reg, data.len() as u8];
        record.extend_from_slice(data);
        self.write(
            "WriteBlock",
            (addr, Some(reg)),
            record,
            SmbusOp::WriteBlock {
                addr,
                reg,
                data: data.to_vec(),
            },
            |b| b.write_block(addr, reg, data),
        )
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
        let max = self.handle.descriptor.max_raw_write_len();
        self.check_len("WriteBlockWithoutRegister", data.len(), max)?;
        let mut record = vec![addr];
        record.extend_from_slice(data);
        self.write(
            "WriteBlockWithoutRegister",
            (addr, data.first().copied()),
            record,
            SmbusOp::WriteBlockNoReg {
                addr,
                data: data.to_vec(),
            },
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
        self.read_op(
            "ReadByteWithoutRegister",
            SmbusOp::ReadByteNoReg { addr },
            |r| match r {
                SmbusReply::Byte(v) => Some(v),
                _ => None,
            },
            |b| b.read_byte_no_reg(addr),
        )
    }

    /// `TransactionLock()`: hold the bus for this session (waiting in line
    /// at most [`LOCK_WAIT`]), and ask the bus itself for a lease.
    pub fn lock(&mut self) -> bool {
        let state = Arc::clone(&self.handle.state);
        if state.holder() == Some(self.session) {
            return true;
        }
        if !state.take(self.session, LOCK_WAIT) {
            self.contended("TransactionLock");
            return false;
        }
        self.lease_bus()
    }

    /// The bus-side half of [`GuardedBus::lock`]; on failure the session lock
    /// is given back.
    fn lease_bus(&mut self) -> bool {
        let r = lock(&self.handle.bus).bus_lock(LOCK_LEASE_MS);
        match r {
            Ok(()) => true,
            Err(e) => {
                self.stats.errors += 1;
                self.handle.state.give(self.session);
                self.log_once(format!("bus: TransactionLock failed: {e}"));
                self.stats.last_error = Some(format!("bus: TransactionLock failed: {e}"));
                false
            }
        }
    }

    /// `TransactionUnlock()`: send the queued writes and let go of the bus.
    /// `0`, or the status of the first queued write that failed since the
    /// lock was taken (Lightloom extension; SignalRGB returns nothing).
    pub fn unlock(&mut self) -> Status {
        self.force_unlock();
        self.queue_status.take().unwrap_or(0)
    }

    /// `HasTransactionLock()`.
    pub fn has_lock(&self) -> bool {
        self.handle.state.holder() == Some(self.session)
    }

    /// Send the queued writes and release the lock if this session holds
    /// it; whether it did.
    pub fn force_unlock(&mut self) -> bool {
        if !self.has_lock() {
            self.queue.clear();
            return false;
        }
        let _ = self.flush();
        if let Err(e) = lock(&self.handle.bus).bus_unlock() {
            // The bus lease runs out by itself; nothing else to undo.
            tracing::debug!(target: "lightloom::smbus", bus = %self.handle.descriptor.name, "bus unlock: {e}");
        }
        self.handle.state.give(self.session)
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
    use lightloom_hal::mock::{MockSmbus, TransactionKind, TransactionRecorder};

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
    fn long_raw_writes_only_on_nvapi_and_never_to_a_denied_register() {
        let (h, rec) = handle(SmbusBusKind::Nvidia, &[0x68]);
        h.allow(0x68);
        let mut g = run(&h, 0x68);
        g.deny_registers([0x3F]);
        let mut frame = vec![0x49];
        frame.extend(std::iter::repeat_n(0x40, 136));
        assert_eq!(g.write_block_no_reg(0x68, &frame), Ok(()), "MSI per-LED frame");
        frame[0] = 0x3F;
        assert_eq!(g.write_block_no_reg(0x68, &frame), Err(-1), "first byte is the register");
        assert_eq!(g.write_block_no_reg(0x68, &[0x49; 141]), Err(-1), "over 140 bytes");
        assert_eq!(g.write_block(0x68, 0x49, &[0; 33]), Err(-1), "Block Write stays at 32");
        assert_eq!(g.read_block(0x68, 0x49, 33), Err(-1));
        assert_eq!(g.read_block_no_reg(0x68, 33), Err(-1));
        let sent = rec.payloads(TransactionKind::SmbusWrite);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].len(), 1 + 137, "address, then the frame");

        let (h, rec) = handle(SmbusBusKind::Amd, &[0x39]);
        h.allow(0x39);
        let mut g = run(&h, 0x39);
        assert_eq!(g.write_block_no_reg(0x39, &[0; 33]), Err(-1), "chipset buses keep 32");
        assert!(rec.payloads(TransactionKind::SmbusWrite).is_empty());
    }

    #[test]
    fn denied_registers_open_only_inside_the_save_window() {
        let (h, rec) = handle(SmbusBusKind::Nvidia, &[0x68]);
        h.allow(0x68);
        let mut g = run(&h, 0x68);
        g.deny_registers([0x3F]);
        assert_eq!(g.write_byte(0x68, 0x3F, 1), Err(-1));
        g.set_save_window(true);
        assert!(g.save_window());
        assert_eq!(g.write_byte(0x68, 0x3F, 1), Ok(()));
        g.set_save_window(false);
        assert_eq!(g.write_byte(0x68, 0x3F, 1), Err(-1), "closed again");
        assert_eq!(
            rec.payloads(TransactionKind::SmbusWrite),
            vec![vec![0x68, 0x3F, 1]]
        );
        // The SPD and the policy's address checks stay closed inside the window.
        g.set_save_window(true);
        assert_eq!(g.write_byte(0x69, 0x3F, 1), Err(-1));
        g.set_write_mode(WriteMode::DryRun);
        assert_eq!(g.write_byte(0x68, 0x3F, 1), Ok(()));
        assert_eq!(
            rec.payloads(TransactionKind::SmbusWrite).len(),
            1,
            "a dry run sends nothing"
        );
        assert_eq!(g.stats().dry_run_log, vec![vec![0x68, 0x3F, 1]]);
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
    fn turns_are_first_come_first_served() {
        let (h, _) = handle(SmbusBusKind::Amd, &[0x39, 0x3A]);
        h.allow(0x39);
        h.allow(0x3A);
        let mut a = run(&h, 0x39);
        assert!(a.lock());
        // B queues for the bus while A holds it.
        let waiter = {
            let h = h.clone();
            std::thread::spawn(move || {
                let mut b = run(&h, 0x3A);
                let got = b.lock();
                let held = b.has_lock();
                b.unlock();
                (got, held)
            })
        };
        while lock(&h.state.turns).waiting.is_empty() {
            std::thread::yield_now();
        }
        // A lets go and at once wants the bus again: B is first in line.
        a.unlock();
        assert!(!a.has_lock(), "handed to the waiting session");
        assert!(a.lock(), "A gets it back once B is done");
        assert_eq!(waiter.join().unwrap(), (true, true));
        a.unlock();
    }

    #[test]
    fn a_bus_kept_too_long_skips_the_operation_without_an_error() {
        let (h, rec) = handle(SmbusBusKind::Amd, &[0x39, 0x3A]);
        h.allow(0x39);
        h.allow(0x3A);
        let mut a = run(&h, 0x39);
        let mut b = run(&h, 0x3A);
        assert!(a.lock());
        let t = Instant::now();
        assert!(!b.lock());
        assert!(t.elapsed() >= LOCK_WAIT && t.elapsed() < LOCK_WAIT * 5);
        assert_eq!(b.write_byte(0x3A, 1, 1), Err(-1), "a lone op waits in line too");
        assert_eq!(
            (b.stats().contended, b.stats().errors, b.stats().denied),
            (2, 0, 0)
        );
        assert!(
            lock(&h.state.turns).waiting.is_empty(),
            "a session that gave up left the line"
        );
        a.unlock();
        assert!(b.write_byte(0x3A, 1, 1).is_ok());
        assert_eq!(rec.payloads(TransactionKind::SmbusWrite), vec![vec![0x3A, 1, 1]]);
    }

    fn batching(h: &BusHandle, addr: u8) -> GuardedBus {
        let mut g = run(h, addr);
        g.set_batch_writes(true);
        g
    }

    #[test]
    fn batched_writes_wait_for_a_read_or_the_unlock() {
        let rec = TransactionRecorder::new();
        let mut bus = MockSmbus::new("bus", SmbusBusKind::Amd, rec.clone());
        bus.add_device(0x39, [(0x81, 0x42)]);
        let batches = bus.batch_log();
        let h = BusHandle::new(Box::new(bus));
        h.allow(0x39);
        let mut g = batching(&h, 0x39);
        // Outside a lock: sent at once, one by one.
        assert!(g.write_byte(0x39, 1, 1).is_ok());
        assert_eq!(rec.payloads(TransactionKind::SmbusWrite).len(), 1);
        assert!(g.lock());
        assert_eq!(g.write_word(0x39, 0, 0x0083), Ok(()));
        assert_eq!(g.write_block(0x39, 3, &[7; 8]), Ok(()));
        assert_eq!(rec.payloads(TransactionKind::SmbusWrite).len(), 1, "queued");
        // The policy still answers at once: refused writes are never queued.
        assert_eq!(g.write_byte(0x50, 0, 0), Err(-1));
        // A read ends the batch and returns its value.
        assert_eq!(g.read_byte(0x39, 0x81), Ok(0x42));
        assert_eq!(rec.payloads(TransactionKind::SmbusWrite).len(), 3);
        assert_eq!(g.write_byte(0x39, 2, 2), Ok(()));
        assert_eq!(g.unlock(), 0, "TransactionUnlock() sends the rest");
        assert_eq!(rec.payloads(TransactionKind::SmbusWrite).len(), 4);
        assert_eq!(*batches.lock().unwrap(), vec![3, 1]);
        assert_eq!(g.stats().batches, 2);
        assert_eq!((g.stats().errors, g.stats().denied), (0, 1));
    }

    #[test]
    fn a_failed_batched_write_is_reported_by_the_unlock() {
        let rec = TransactionRecorder::new();
        let mut bus = MockSmbus::new("bus", SmbusBusKind::Amd, rec.clone()).fail_block_writes(0x39);
        bus.add_device(0x39, []);
        let h = BusHandle::new(Box::new(bus));
        h.allow(0x39);
        let mut g = batching(&h, 0x39);
        assert!(g.lock());
        assert_eq!(g.write_byte(0x39, 1, 1), Ok(()));
        assert_eq!(g.write_block(0x39, 3, &[1, 2]), Ok(()), "answered at once");
        assert_eq!(g.write_byte(0x39, 2, 2), Ok(()));
        assert_eq!(g.unlock(), -1);
        assert_eq!(
            rec.payloads(TransactionKind::SmbusWrite),
            vec![vec![0x39, 1, 1]],
            "stopped at the failed block write"
        );
        assert_eq!(g.stats().errors, 1);
        assert!(!g.has_lock());
        // The next locked sequence starts clean.
        assert!(g.lock());
        assert_eq!(g.write_byte(0x39, 3, 3), Ok(()));
        assert_eq!(g.unlock(), 0);
        // A failed queued write fails the read after it (never sent).
        assert!(g.lock());
        assert_eq!(g.write_block(0x39, 3, &[1]), Ok(()));
        assert_eq!(g.read_byte(0x39, 0), Err(-1));
        assert_eq!(g.unlock(), -1);
    }

    #[test]
    fn nothing_is_queued_in_dry_run_scans_or_for_plugins_that_did_not_ask() {
        let (h, rec) = handle(SmbusBusKind::Amd, &[0x39]);
        h.allow(0x39);
        let mut plain = run(&h, 0x39);
        assert!(plain.lock());
        assert!(plain.write_byte(0x39, 1, 1).is_ok());
        assert_eq!(rec.payloads(TransactionKind::SmbusWrite).len(), 1, "sent at once");
        assert_eq!(plain.unlock(), 0);
        let mut dry = GuardedBus::new(h.clone(), Phase::Run { addr: 0x39 }, WriteMode::DryRun);
        dry.set_batch_writes(true);
        assert!(dry.dry_run());
        assert!(dry.lock());
        assert!(dry.write_byte(0x39, 2, 2).is_ok());
        assert_eq!(dry.unlock(), 0);
        assert_eq!(dry.stats().dry_run_log, vec![vec![0x39, 2, 2]]);
        let mut scan_guard = scan(&h, &[0x39]);
        scan_guard.set_batch_writes(true);
        assert!(scan_guard.lock());
        assert!(scan_guard.write_byte(0x39, 3, 3).is_ok());
        assert_eq!(
            rec.payloads(TransactionKind::SmbusWrite).len(),
            2,
            "Scan(bus) writes are never queued"
        );
        scan_guard.unlock();
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
    struct Contended(lightloom_hal::SmbusBusDescriptor);

    impl SmbusBus for Contended {
        fn descriptor(&self) -> &SmbusBusDescriptor {
            &self.0
        }
        fn write_quick(&mut self, _: u8) -> lightloom_hal::HalResult<()> {
            Ok(())
        }
        fn read_byte(&mut self, _: u8, _: u8) -> lightloom_hal::HalResult<u8> {
            Ok(0)
        }
        fn write_byte(&mut self, _: u8, _: u8, _: u8) -> lightloom_hal::HalResult<()> {
            Ok(())
        }
        fn read_word(&mut self, _: u8, _: u8) -> lightloom_hal::HalResult<u16> {
            Ok(0)
        }
        fn write_word(&mut self, _: u8, _: u8, _: u16) -> lightloom_hal::HalResult<()> {
            Ok(())
        }
        fn write_block(&mut self, _: u8, _: u8, _: &[u8]) -> lightloom_hal::HalResult<()> {
            Ok(())
        }
        fn read_block(&mut self, _: u8, _: u8, _: &mut [u8]) -> lightloom_hal::HalResult<usize> {
            Ok(0)
        }
        fn write_block_no_reg(&mut self, _: u8, _: &[u8]) -> lightloom_hal::HalResult<()> {
            Ok(())
        }
        fn read_block_no_reg(&mut self, _: u8, _: &mut [u8]) -> lightloom_hal::HalResult<usize> {
            Ok(0)
        }
        fn read_byte_no_reg(&mut self, _: u8) -> lightloom_hal::HalResult<u8> {
            Ok(0)
        }
        fn bus_lock(&mut self, _: u16) -> lightloom_hal::HalResult<()> {
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
