// SPDX-License-Identifier: GPL-3.0-or-later
//! The NvAPI worker thread and its I2C transports.
//!
//! NvAPI GPU handles are plain pointers that must stay on the thread that
//! enumerated them (research §4 step 5), and transfers must not interleave.
//! [`NvapiHub`] therefore starts one OS thread, `lightloom-nvapi`, that creates
//! and owns the transport; every [`NvapiBus`] sends it requests over a channel
//! and waits for the answer. The thread ends (and NvAPI is unloaded) once the
//! hub and every bus are dropped.
//!
//! * A request names its card by identity ([`GpuInfo`]: PCI bus number and
//!   ids), never by enumeration index: after a driver reset that reorders
//!   the GPUs a bus still reaches its own card, or fails ([`GPU_GONE`]).
//! * A request carries a deadline ([`REPLY_TIMEOUT`] after it was sent): one
//!   still queued when its caller has given up is dropped, never sent late.
//! * Every transfer runs under the global SMBus mutex
//!   (`Global\Access_SMBUS.HTP.Method`, [`GlobalLock`]), waited for at most
//!   [`LOCK_TIMEOUT_MS`]; a `TransactionLock` ([`Job::Lock`]) keeps it across
//!   transfers for a lease of at most [`LEASE_MAX`]. NvAPI I2C is another
//!   controller than the chipset SMBus, but holding the mutex keeps Lightloom's
//!   GPU traffic in step with every program that follows the convention
//!   (ADR-0004).

use crate::bus::NvapiBus;
use crate::info::I2cPlan;
use crate::NvapiError;
use lightloom_hal::smbus::{PciIds, SmbusBusDescriptor, SmbusBusKind, SMBUS_BLOCK_MAX};
use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long a request may wait in the worker's queue before it is dropped.
pub const REPLY_TIMEOUT: Duration = Duration::from_millis(500);
/// Extra wait for the answer past [`REPLY_TIMEOUT`]: a request the worker
/// started just before its deadline still reports how it went.
pub const REPLY_GRACE: Duration = Duration::from_millis(250);
/// Longest wait for the global SMBus mutex per transfer or lease.
pub const LOCK_TIMEOUT_MS: u32 = 50;
/// Longest `TransactionLock` lease the worker grants.
pub const LEASE_MAX: Duration = Duration::from_millis(250);
/// `NVAPI_DEVICE_BUSY`: what a transfer reports when another program kept
/// the global SMBus mutex.
pub const BUSY: i32 = -108;
/// `NVAPI_NVIDIA_DEVICE_NOT_FOUND`: the bus's card is no longer enumerated.
pub const GPU_GONE: i32 = -6;

/// One GPU as the transport reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuInfo {
    /// `NvAPI_GPU_GetFullName` ("NVIDIA GeForce RTX 3060"): the bus `Name()`.
    pub name: String,
    /// PCI vendor, device, subsystem vendor and subsystem device.
    pub pci: PciIds,
    /// PCI bus number (`NvAPI_GPU_GetBusId`).
    pub bus: u32,
}

impl GpuInfo {
    /// Whether `other` is the same card: same PCI bus number and ids.
    pub fn same_card(&self, other: &GpuInfo) -> bool {
        self.bus == other.bus && self.pci == other.pci
    }
}

/// Where card `gpu` sits in `gpus`. Exactly one card must match, else
/// [`GPU_GONE`] (it went away, or two cards cannot be told apart).
pub fn locate(gpus: &[GpuInfo], gpu: &GpuInfo) -> Result<usize, NvapiError> {
    let mut found = gpus.iter().enumerate().filter(|(_, g)| g.same_card(gpu));
    match (found.next(), found.next()) {
        (Some((i, _)), None) => Ok(i),
        _ => Err(NvapiError::Status(GPU_GONE)),
    }
}

/// `NvAPI_GPU_GetPCIIdentifiers` packing, as OpenRGB decodes it:
/// `DeviceId = device << 16 | vendor`, `SubSystemId = subdevice << 16 | subvendor`.
pub fn decode_pci(device_id: u32, subsystem_id: u32) -> PciIds {
    PciIds {
        vendor: (device_id & 0xFFFF) as u16,
        device: (device_id >> 16) as u16,
        sub_vendor: (subsystem_id & 0xFFFF) as u16,
        sub_device: (subsystem_id >> 16) as u16,
    }
}

/// What the worker thread talks to: NvAPI itself, or [`FakeNvI2c`] in tests.
pub trait I2cTransport {
    /// The GPUs found.
    fn gpus(&mut self) -> Result<Vec<GpuInfo>, NvapiError>;
    /// One transfer to `addr` on card `gpu`, looked up at every call
    /// ([`GPU_GONE`] when it is not there); a read fills `out`
    /// (`plan.data.len()` bytes).
    fn transfer(&mut self, gpu: &GpuInfo, addr: u8, plan: &I2cPlan, out: &mut [u8])
        -> Result<(), NvapiError>;
}

/// The cross-process lock the worker takes around transfers: the global
/// SMBus mutex on Windows, [`NoLock`] or [`FakeLock`] in tests. A Win32
/// mutex belongs to a thread: it is created, taken and released on the
/// worker thread only.
pub trait GlobalLock {
    /// Take it, waiting at most `timeout_ms`; `Err` when another program kept it.
    fn acquire(&mut self, timeout_ms: u32) -> Result<(), NvapiError>;
    /// Release it if held.
    fn release(&mut self);
}

/// No cross-process lock (tests).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoLock;

impl GlobalLock for NoLock {
    fn acquire(&mut self, _: u32) -> Result<(), NvapiError> {
        Ok(())
    }

    fn release(&mut self) {}
}

/// What a request asks of the worker.
#[derive(Debug)]
pub(crate) enum Job {
    /// One transfer.
    Transfer {
        /// The card.
        gpu: GpuInfo,
        /// 7-bit address.
        addr: u8,
        /// The transfer.
        plan: I2cPlan,
    },
    /// Keep the global lock across transfers for `lease` (at most [`LEASE_MAX`]).
    Lock {
        /// Lease asked for.
        lease: Duration,
    },
    /// End a [`Job::Lock`].
    Unlock,
}

/// A request to the worker thread.
pub(crate) struct Request {
    pub job: Job,
    /// The caller stops waiting then: a request still queued is dropped.
    pub deadline: Instant,
    pub reply: Sender<Result<Vec<u8>, NvapiError>>,
}

/// Owner of the NvAPI worker thread.
#[derive(Debug)]
pub struct NvapiHub {
    gpus: Vec<GpuInfo>,
    tx: Sender<Request>,
}

impl NvapiHub {
    /// Load NvAPI on a new worker thread, open the global SMBus mutex there
    /// and enumerate the GPUs. Without the mutex no GPU is driven.
    #[cfg(windows)]
    pub fn start() -> Result<Self, NvapiError> {
        Self::spawn(|| {
            let lock = crate::mutex::SmbusMutex::create().map_err(NvapiError::GlobalMutex)?;
            Ok((crate::api::NvApi::load()?, lock))
        })
    }

    /// NvAPI exists on Windows only.
    #[cfg(not(windows))]
    pub fn start() -> Result<Self, NvapiError> {
        Err(NvapiError::NotAvailable)
    }

    /// A hub over `transport` without a cross-process lock (tests: [`FakeNvI2c`]).
    pub fn with_transport<T: I2cTransport + Send + 'static>(transport: T) -> Result<Self, NvapiError> {
        Self::with_transport_and_lock(transport, NoLock)
    }

    /// A hub over `transport` that takes `lock` around transfers (tests).
    pub fn with_transport_and_lock<T, L>(transport: T, lock: L) -> Result<Self, NvapiError>
    where
        T: I2cTransport + Send + 'static,
        L: GlobalLock + Send + 'static,
    {
        Self::spawn(move || Ok((transport, lock)))
    }

    /// Start the worker: `make` runs on it and creates the transport and the lock there.
    fn spawn<T, L, F>(make: F) -> Result<Self, NvapiError>
    where
        T: I2cTransport + 'static,
        L: GlobalLock + 'static,
        F: FnOnce() -> Result<(T, L), NvapiError> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel::<Request>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<Vec<GpuInfo>, NvapiError>>();
        std::thread::Builder::new()
            .name("lightloom-nvapi".into())
            .spawn(move || {
                let (mut transport, mut lock) = match make() {
                    Ok(t) => t,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let gpus = transport.gpus();
                let ok = gpus.is_ok();
                let _ = ready_tx.send(gpus);
                if ok {
                    serve(&mut transport, &mut lock, &rx);
                }
                lock.release();
            })
            .map_err(|_| NvapiError::WorkerGone)?;
        let gpus = ready_rx.recv().map_err(|_| NvapiError::WorkerGone)??;
        Ok(Self { gpus, tx })
    }

    /// The GPUs found.
    pub fn gpus(&self) -> &[GpuInfo] {
        &self.gpus
    }

    /// One SMBus bus per GPU (port 1 of each).
    pub fn buses(&self) -> Vec<NvapiBus> {
        self.gpus.iter().map(|g| self.bus_for(g.clone())).collect()
    }

    /// The bus of card `gpu`, enumerated or not (tests: a card that went away).
    pub(crate) fn bus_for(&self, gpu: GpuInfo) -> NvapiBus {
        let descriptor = SmbusBusDescriptor {
            name: gpu.name.clone(),
            kind: SmbusBusKind::Nvidia,
            pci: gpu.pci,
            instance: format!("pci{:02x}", gpu.bus),
            max_block_len: SMBUS_BLOCK_MAX,
        };
        NvapiBus::new(descriptor, gpu, self.tx.clone())
    }
}

/// The worker loop: one request at a time. A lease keeps the global lock
/// between requests until it is unlocked or runs out.
fn serve<T: I2cTransport, L: GlobalLock>(transport: &mut T, lock: &mut L, rx: &Receiver<Request>) {
    let mut lease: Option<Instant> = None;
    loop {
        let next = match lease {
            Some(until) => rx.recv_timeout(until.saturating_duration_since(Instant::now())),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        let req = match next {
            Ok(req) => req,
            Err(RecvTimeoutError::Timeout) => {
                tracing::warn!("GPU I2C TransactionLock lease ran out");
                lease = None;
                lock.release();
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => return,
        };
        let now = Instant::now();
        if lease.is_some_and(|until| now >= until) {
            lease = None;
            lock.release();
        }
        if now >= req.deadline {
            // Its caller has given up on it: never sent late.
            tracing::debug!("NvAPI request dropped: its caller timed out");
            let _ = req
                .reply
                .send(Err(NvapiError::Timeout(REPLY_TIMEOUT.as_millis() as u32)));
            continue;
        }
        let result = match req.job {
            Job::Transfer { gpu, addr, plan } => {
                let held = lease.is_some();
                let taken = if held {
                    Ok(())
                } else {
                    lock.acquire(LOCK_TIMEOUT_MS)
                };
                taken.and_then(|()| {
                    let mut out = vec![0u8; if plan.write { 0 } else { plan.data.len() }];
                    let r = transport.transfer(&gpu, addr, &plan, &mut out).map(|()| out);
                    if !held {
                        lock.release();
                    }
                    r
                })
            }
            Job::Lock { lease: wanted } => {
                let taken = if lease.is_some() {
                    Ok(())
                } else {
                    lock.acquire(LOCK_TIMEOUT_MS)
                };
                taken.map(|()| {
                    lease = Some(now + wanted.min(LEASE_MAX));
                    Vec::new()
                })
            }
            Job::Unlock => {
                if lease.take().is_some() {
                    lock.release();
                }
                Ok(Vec::new())
            }
        };
        let _ = req.reply.send(result);
    }
}

/// One transfer as [`FakeNvI2c`] saw it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeTransfer {
    /// Index of the card among the fake's GPUs.
    pub gpu: usize,
    /// 7-bit address.
    pub addr: u8,
    /// The transfer.
    pub plan: I2cPlan,
    /// Name of the thread that ran it.
    pub thread: String,
}

/// A register-map stand-in for NvAPI: devices answer at the addresses given
/// to [`FakeNvI2c::add_device`]; a write stores its data from the register
/// on, a read returns the registers from there (0xFF where unset). An absent
/// device answers `NVAPI_ERROR` (-1), NvAPI's way of saying nobody ACKed.
#[derive(Clone, Debug, Default)]
pub struct FakeNvI2c {
    gpus: Vec<GpuInfo>,
    devices: BTreeMap<(usize, u8), BTreeMap<u8, u8>>,
    fail: Option<i32>,
    log: Arc<Mutex<Vec<FakeTransfer>>>,
}

impl FakeNvI2c {
    /// A fake with the given GPUs.
    pub fn new(gpus: Vec<GpuInfo>) -> Self {
        Self {
            gpus,
            ..Self::default()
        }
    }

    /// A device at `addr` on GPU `gpu` (an index into the GPUs given to
    /// [`FakeNvI2c::new`]) with initial registers.
    pub fn add_device(mut self, gpu: usize, addr: u8, registers: impl IntoIterator<Item = (u8, u8)>) -> Self {
        self.devices.insert((gpu, addr), registers.into_iter().collect());
        self
    }

    /// Every transfer fails with NvAPI status `status`.
    pub fn fail_with(mut self, status: i32) -> Self {
        self.fail = Some(status);
        self
    }

    /// The transfers seen so far (shared with clones).
    pub fn log(&self) -> Arc<Mutex<Vec<FakeTransfer>>> {
        Arc::clone(&self.log)
    }
}

impl I2cTransport for FakeNvI2c {
    fn gpus(&mut self) -> Result<Vec<GpuInfo>, NvapiError> {
        Ok(self.gpus.clone())
    }

    fn transfer(
        &mut self,
        card: &GpuInfo,
        addr: u8,
        plan: &I2cPlan,
        out: &mut [u8],
    ) -> Result<(), NvapiError> {
        let gpu = locate(&self.gpus, card)?;
        self.log
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(FakeTransfer {
                gpu,
                addr,
                plan: plan.clone(),
                thread: std::thread::current().name().unwrap_or_default().to_string(),
            });
        if let Some(status) = self.fail {
            return Err(NvapiError::Status(status));
        }
        let regs = self.devices.get_mut(&(gpu, addr)).ok_or(NvapiError::Status(-1))?;
        let base = plan.reg.unwrap_or(0);
        if plan.write {
            for (i, b) in plan.data.iter().enumerate() {
                regs.insert(base.wrapping_add(i as u8), *b);
            }
        } else {
            for (i, slot) in out.iter_mut().enumerate() {
                *slot = *regs.get(&base.wrapping_add(i as u8)).unwrap_or(&0xFF);
            }
        }
        Ok(())
    }
}

/// A [`GlobalLock`] double: counts acquisitions and releases, and can play
/// another program that keeps the mutex.
#[derive(Clone, Debug, Default)]
pub struct FakeLock {
    state: Arc<Mutex<FakeLockState>>,
}

#[derive(Debug, Default)]
struct FakeLockState {
    held: bool,
    acquired: usize,
    released: usize,
    contended: bool,
}

impl FakeLock {
    /// A free lock.
    pub fn new() -> Self {
        Self::default()
    }

    /// Another program keeps (or lets go of) the mutex.
    pub fn contend(&self, on: bool) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).contended = on;
    }

    /// `(acquired, released)` so far.
    pub fn counts(&self) -> (usize, usize) {
        let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        (s.acquired, s.released)
    }

    /// Whether the worker holds it now.
    pub fn held(&self) -> bool {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).held
    }
}

impl GlobalLock for FakeLock {
    fn acquire(&mut self, _: u32) -> Result<(), NvapiError> {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if s.contended {
            return Err(NvapiError::Status(BUSY));
        }
        if !s.held {
            s.held = true;
            s.acquired += 1;
        }
        Ok(())
    }

    fn release(&mut self) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if s.held {
            s.held = false;
            s.released += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(bus: u32) -> GpuInfo {
        GpuInfo {
            name: "NVIDIA GeForce RTX 3060".into(),
            pci: decode_pci(0x2504_10DE, 0x3903_1462),
            bus,
        }
    }

    #[test]
    fn pci_identifiers_unpack_like_openrgb() {
        assert_eq!(
            decode_pci(0x2504_10DE, 0x3903_1462),
            PciIds {
                vendor: 0x10DE,
                device: 0x2504,
                sub_vendor: 0x1462,
                sub_device: 0x3903
            }
        );
    }

    #[test]
    fn a_transport_that_fails_to_start_is_reported() {
        let r =
            NvapiHub::spawn(|| -> Result<(FakeNvI2c, NoLock), NvapiError> { Err(NvapiError::NotAvailable) });
        assert_eq!(r.unwrap_err(), NvapiError::NotAvailable);
    }

    #[test]
    fn cards_are_found_by_identity_not_position() {
        let (a, b) = (card(1), card(2));
        assert_eq!(locate(&[a.clone(), b.clone()], &b), Ok(1));
        assert_eq!(locate(&[b.clone(), a.clone()], &b), Ok(0), "reordered");
        assert_eq!(
            locate(std::slice::from_ref(&a), &b),
            Err(NvapiError::Status(GPU_GONE)),
            "gone"
        );
        assert_eq!(
            locate(&[a.clone(), a.clone()], &a),
            Err(NvapiError::Status(GPU_GONE)),
            "two cards that cannot be told apart"
        );
    }
}
