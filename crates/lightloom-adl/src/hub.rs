// SPDX-License-Identifier: GPL-3.0-or-later
//! The ADL worker thread and its I2C transports.
//!
//! An ADL context is not documented as thread-safe, and transfers must not
//! interleave. [`AdlHub`] therefore starts one OS thread, `lightloom-adl`, that
//! creates and owns the transport; every [`AdlBus`] sends it requests over a
//! channel and waits for the answer. The thread ends (and the ADL context is
//! destroyed) once the hub and every bus are dropped.
//!
//! * A request names its card by identity ([`GpuInfo`]: PCI bus number and
//!   ids), never by ADL adapter index: at every transfer the transport looks
//!   the card up among the adapters it enumerated, so after a driver reset
//!   that renumbers the adapters a bus still reaches its own card, or fails
//!   ([`GPU_GONE`]).
//! * A request carries a deadline ([`REPLY_TIMEOUT`] after it was sent): one
//!   still queued when its caller has given up is dropped, never sent late.
//! * No cross-process mutex (see the crate docs): transfers to one GPU are
//!   serialised by this worker thread, and a plugin's `TransactionLock` by
//!   the plugin host's per-bus turn queue.

use crate::bus::AdlBus;
use crate::info::I2cPlan;
use crate::status::ADL_ERR_INVALID_ADL_IDX;
use crate::AdlError;
use lightloom_hal::smbus::{PciIds, SmbusBusDescriptor, SmbusBusKind, SMBUS_BLOCK_MAX};
use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long a request may wait in the worker's queue before it is dropped.
pub const REPLY_TIMEOUT: Duration = Duration::from_millis(500);
/// Extra wait for the answer past [`REPLY_TIMEOUT`]: a request the worker
/// started just before its deadline still reports how it went.
pub const REPLY_GRACE: Duration = Duration::from_millis(250);
/// `ADL_ERR_INVALID_ADL_IDX`: the bus's card is no longer enumerated.
pub const GPU_GONE: i32 = ADL_ERR_INVALID_ADL_IDX;
/// Prefix of the bus `Name()`. It matches the bus name SignalRGB gives AMD
/// GPU buses (docs/host-api/smbus.md §10), which GPU plugins strip to get the
/// card's name; the transport behind it is ADL, not ADLX.
pub const BUS_NAME_PREFIX: &str = "AMD ADLX Bus - ";

/// One GPU as the transport reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuInfo {
    /// The adapter name ("AMD Radeon RX 6700 XT").
    pub name: String,
    /// PCI vendor, device, subsystem vendor and subsystem device (from the PNP string).
    pub pci: PciIds,
    /// PCI bus number.
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
pub fn locate(gpus: &[GpuInfo], gpu: &GpuInfo) -> Result<usize, AdlError> {
    let mut found = gpus.iter().enumerate().filter(|(_, g)| g.same_card(gpu));
    match (found.next(), found.next()) {
        (Some((i, _)), None) => Ok(i),
        _ => Err(AdlError::Status(GPU_GONE)),
    }
}

/// What the worker thread talks to: ADL itself, or [`FakeAdlI2c`] in tests.
pub trait I2cTransport {
    /// The GPUs found.
    fn gpus(&mut self) -> Result<Vec<GpuInfo>, AdlError>;
    /// One transfer to `addr` on card `gpu`, looked up at every call
    /// ([`GPU_GONE`] when it is not there); a read fills `out`
    /// (`plan.data.len()` bytes).
    fn transfer(&mut self, gpu: &GpuInfo, addr: u8, plan: &I2cPlan, out: &mut [u8]) -> Result<(), AdlError>;
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
}

/// A request to the worker thread.
pub(crate) struct Request {
    pub job: Job,
    /// The caller stops waiting then: a request still queued is dropped.
    pub deadline: Instant,
    pub reply: Sender<Result<Vec<u8>, AdlError>>,
}

/// Owner of the ADL worker thread.
#[derive(Debug)]
pub struct AdlHub {
    gpus: Vec<GpuInfo>,
    tx: Sender<Request>,
}

impl AdlHub {
    /// Load ADL on a new worker thread and enumerate the AMD GPUs.
    /// [`AdlError::NotAvailable`] without an AMD driver.
    #[cfg(windows)]
    pub fn start() -> Result<Self, AdlError> {
        Self::spawn(crate::api::Adl::load)
    }

    /// ADL is used on Windows only.
    #[cfg(not(windows))]
    pub fn start() -> Result<Self, AdlError> {
        Err(AdlError::NotAvailable)
    }

    /// A hub over `transport` (tests: [`FakeAdlI2c`]).
    pub fn with_transport<T: I2cTransport + Send + 'static>(transport: T) -> Result<Self, AdlError> {
        Self::spawn(move || Ok(transport))
    }

    /// Start the worker: `make` runs on it and creates the transport there.
    fn spawn<T, F>(make: F) -> Result<Self, AdlError>
    where
        T: I2cTransport + 'static,
        F: FnOnce() -> Result<T, AdlError> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel::<Request>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<Vec<GpuInfo>, AdlError>>();
        std::thread::Builder::new()
            .name("lightloom-adl".into())
            .spawn(move || {
                let mut transport = match make() {
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
                    serve(&mut transport, &rx);
                }
            })
            .map_err(|_| AdlError::WorkerGone)?;
        let gpus = ready_rx.recv().map_err(|_| AdlError::WorkerGone)??;
        Ok(Self { gpus, tx })
    }

    /// The GPUs found.
    pub fn gpus(&self) -> &[GpuInfo] {
        &self.gpus
    }

    /// One SMBus bus per GPU (line 1 of each).
    pub fn buses(&self) -> Vec<AdlBus> {
        self.gpus.iter().map(|g| self.bus_for(g.clone())).collect()
    }

    /// The bus of card `gpu`, enumerated or not (tests: a card that went away).
    pub(crate) fn bus_for(&self, gpu: GpuInfo) -> AdlBus {
        let descriptor = SmbusBusDescriptor {
            name: format!("{BUS_NAME_PREFIX}{}", gpu.name),
            kind: SmbusBusKind::AmdGpu,
            pci: gpu.pci,
            instance: format!("pci{:02x}", gpu.bus),
            max_block_len: SMBUS_BLOCK_MAX,
        };
        AdlBus::new(descriptor, gpu, self.tx.clone())
    }
}

/// The worker loop: one request at a time, in the order they came.
fn serve<T: I2cTransport>(transport: &mut T, rx: &Receiver<Request>) {
    while let Ok(req) = rx.recv() {
        if Instant::now() >= req.deadline {
            // Its caller has given up on it: never sent late.
            tracing::debug!("ADL request dropped: its caller timed out");
            let _ = req
                .reply
                .send(Err(AdlError::Timeout(REPLY_TIMEOUT.as_millis() as u32)));
            continue;
        }
        let result = match req.job {
            Job::Transfer { gpu, addr, plan } => {
                let mut out = vec![0u8; if plan.write { 0 } else { plan.data.len() }];
                transport.transfer(&gpu, addr, &plan, &mut out).map(|()| out)
            }
        };
        let _ = req.reply.send(result);
    }
}

/// One transfer as [`FakeAdlI2c`] saw it.
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

/// A register-map stand-in for ADL that models the wire: devices answer at
/// the addresses given to [`FakeAdlI2c::add_device`]; a write's first byte is
/// the register and the rest is stored from there (a zero-length write only
/// checks the device answers); a read returns the registers from its offset
/// (0xFF where unset). An absent device answers `ADL_ERR` (-1), ADL's way of
/// saying nobody ACKed.
#[derive(Clone, Debug, Default)]
pub struct FakeAdlI2c {
    gpus: Vec<GpuInfo>,
    devices: BTreeMap<(usize, u8), BTreeMap<u8, u8>>,
    fail: Option<i32>,
    log: Arc<Mutex<Vec<FakeTransfer>>>,
}

impl FakeAdlI2c {
    /// A fake with the given GPUs.
    pub fn new(gpus: Vec<GpuInfo>) -> Self {
        Self {
            gpus,
            ..Self::default()
        }
    }

    /// A device at `addr` on GPU `gpu` (an index into the GPUs given to
    /// [`FakeAdlI2c::new`]) with initial registers.
    pub fn add_device(mut self, gpu: usize, addr: u8, registers: impl IntoIterator<Item = (u8, u8)>) -> Self {
        self.devices.insert((gpu, addr), registers.into_iter().collect());
        self
    }

    /// Every transfer fails with ADL status `status`.
    pub fn fail_with(mut self, status: i32) -> Self {
        self.fail = Some(status);
        self
    }

    /// The transfers seen so far (shared with clones).
    pub fn log(&self) -> Arc<Mutex<Vec<FakeTransfer>>> {
        Arc::clone(&self.log)
    }
}

impl I2cTransport for FakeAdlI2c {
    fn gpus(&mut self) -> Result<Vec<GpuInfo>, AdlError> {
        Ok(self.gpus.clone())
    }

    fn transfer(&mut self, card: &GpuInfo, addr: u8, plan: &I2cPlan, out: &mut [u8]) -> Result<(), AdlError> {
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
            return Err(AdlError::Status(status));
        }
        let regs = self.devices.get_mut(&(gpu, addr)).ok_or(AdlError::Status(-1))?;
        if plan.write {
            if let Some((&reg, data)) = plan.data.split_first() {
                for (i, b) in data.iter().enumerate() {
                    regs.insert(reg.wrapping_add(i as u8), *b);
                }
            }
        } else {
            for (i, slot) in out.iter_mut().enumerate() {
                *slot = *regs.get(&plan.offset.wrapping_add(i as u8)).unwrap_or(&0xFF);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(bus: u32) -> GpuInfo {
        GpuInfo {
            name: "AMD Radeon RX 6700 XT".into(),
            pci: PciIds {
                vendor: 0x1002,
                device: 0x73DF,
                sub_vendor: 0x1DA2,
                sub_device: 0xE445,
            },
            bus,
        }
    }

    #[test]
    fn a_transport_that_fails_to_start_is_reported() {
        let r = AdlHub::spawn(|| -> Result<FakeAdlI2c, AdlError> { Err(AdlError::NotAvailable) });
        assert_eq!(r.unwrap_err(), AdlError::NotAvailable);
    }

    #[test]
    fn cards_are_found_by_identity_not_position() {
        let (a, b) = (card(3), card(7));
        assert_eq!(locate(&[a.clone(), b.clone()], &b), Ok(1));
        assert_eq!(locate(&[b.clone(), a.clone()], &b), Ok(0), "reordered");
        assert_eq!(
            locate(std::slice::from_ref(&a), &b),
            Err(AdlError::Status(GPU_GONE)),
            "gone"
        );
        assert_eq!(
            locate(&[a.clone(), a.clone()], &a),
            Err(AdlError::Status(GPU_GONE)),
            "two cards that cannot be told apart"
        );
    }
}
