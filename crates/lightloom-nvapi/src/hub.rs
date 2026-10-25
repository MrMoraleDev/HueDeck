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
//! * No cross-process mutex: NvAPI I2C is the GPU's own I2C controller,
//!   not the chipset SMBus, and neither OpenRGB nor SignalRGB takes
//!   `Global\Access_SMBUS.HTP.Method` for it. Taking it here made the GPU
//!   wait for Lightloom's own RAM traffic (`NVAPI_DEVICE_BUSY`, -108, and a
//!   rainbow at 13 frames in 4 s with both running, ADR-0004). Transfers
//!   to one GPU are serialised by this worker thread, and a plugin's
//!   `TransactionLock` by the plugin host's per-bus turn queue.

use crate::bus::NvapiBus;
use crate::info::I2cPlan;
use crate::sensors::GpuSensors;
use crate::NvapiError;
use lightloom_hal::smbus::{PciIds, SmbusBusDescriptor, SmbusBusKind, SMBUS_BLOCK_MAX};
use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

/// How long a request may wait in the worker's queue before it is dropped.
pub const REPLY_TIMEOUT: Duration = Duration::from_millis(500);
/// Extra wait for the answer past [`REPLY_TIMEOUT`]: a request the worker
/// started just before its deadline still reports how it went.
pub const REPLY_GRACE: Duration = Duration::from_millis(250);
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
    /// The read-only sensors of card `gpu` ([`crate::sensors`]).
    fn sensors(&mut self, gpu: &GpuInfo) -> Result<GpuSensors, NvapiError> {
        let _ = gpu;
        Err(NvapiError::Unsupported("sensors"))
    }
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
    /// Read the card's sensors (temperature, load, fan, clocks).
    Sensors {
        /// The card.
        gpu: GpuInfo,
    },
}

/// What the worker answers.
#[derive(Debug)]
pub(crate) enum Reply {
    /// The bytes a transfer read (empty for writes).
    Bytes(Vec<u8>),
    /// A card's sensors.
    Sensors(GpuSensors),
}

/// A request to the worker thread.
pub(crate) struct Request {
    pub job: Job,
    /// The caller stops waiting then: a request still queued is dropped.
    pub deadline: Instant,
    pub reply: Sender<Result<Reply, NvapiError>>,
}

/// Owner of the NvAPI worker thread.
#[derive(Debug)]
pub struct NvapiHub {
    gpus: Vec<GpuInfo>,
    tx: Sender<Request>,
}

impl NvapiHub {
    /// Load NvAPI on a new worker thread and enumerate the GPUs.
    #[cfg(windows)]
    pub fn start() -> Result<Self, NvapiError> {
        Self::spawn(crate::api::NvApi::load)
    }

    /// NvAPI exists on Windows only.
    #[cfg(not(windows))]
    pub fn start() -> Result<Self, NvapiError> {
        Err(NvapiError::NotAvailable)
    }

    /// A hub over `transport` (tests: [`FakeNvI2c`]).
    pub fn with_transport<T: I2cTransport + Send + 'static>(transport: T) -> Result<Self, NvapiError> {
        Self::spawn(move || Ok(transport))
    }

    /// Start the worker: `make` runs on it and creates the transport there.
    fn spawn<T, F>(make: F) -> Result<Self, NvapiError>
    where
        T: I2cTransport + 'static,
        F: FnOnce() -> Result<T, NvapiError> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel::<Request>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<Vec<GpuInfo>, NvapiError>>();
        std::thread::Builder::new()
            .name("lightloom-nvapi".into())
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
            .map_err(|_| NvapiError::WorkerGone)?;
        let gpus = ready_rx.recv().map_err(|_| NvapiError::WorkerGone)??;
        Ok(Self { gpus, tx })
    }

    /// The process-wide hub: the running one when somebody still holds it
    /// (the SMBus buses, the sensors), else a new one. One NvAPI worker
    /// thread per process, whoever needs it.
    pub fn shared() -> Result<Arc<Self>, NvapiError> {
        static SHARED: Mutex<Weak<NvapiHub>> = Mutex::new(Weak::new());
        let mut slot = SHARED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(hub) = slot.upgrade() {
            return Ok(hub);
        }
        let hub = Arc::new(Self::start()?);
        *slot = Arc::downgrade(&hub);
        Ok(hub)
    }

    /// The GPUs found.
    pub fn gpus(&self) -> &[GpuInfo] {
        &self.gpus
    }

    /// The worker's queue.
    pub(crate) fn sender(&self) -> &Sender<Request> {
        &self.tx
    }

    /// The GPUs' read-only sensors on this hub's worker.
    pub fn sensors(self: &Arc<Self>) -> crate::sensors::NvapiSensors {
        crate::sensors::NvapiSensors::new(Arc::clone(self))
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

/// The worker loop: one request at a time, in the order they came.
fn serve<T: I2cTransport>(transport: &mut T, rx: &Receiver<Request>) {
    while let Ok(req) = rx.recv() {
        if Instant::now() >= req.deadline {
            // Its caller has given up on it: never sent late.
            tracing::debug!("NvAPI request dropped: its caller timed out");
            let _ = req
                .reply
                .send(Err(NvapiError::Timeout(REPLY_TIMEOUT.as_millis() as u32)));
            continue;
        }
        let result = match req.job {
            Job::Transfer { gpu, addr, plan } => {
                let mut out = vec![0u8; if plan.write { 0 } else { plan.data.len() }];
                transport
                    .transfer(&gpu, addr, &plan, &mut out)
                    .map(|()| Reply::Bytes(out))
            }
            Job::Sensors { gpu } => transport.sensors(&gpu).map(Reply::Sensors),
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
    sensors: BTreeMap<usize, GpuSensors>,
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

    /// GPU `gpu` reports `sensors` (the others report none).
    pub fn with_sensors(mut self, gpu: usize, sensors: GpuSensors) -> Self {
        self.sensors.insert(gpu, sensors);
        self
    }

    /// Every transfer and sensor read fails with NvAPI status `status`.
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

    fn sensors(&mut self, card: &GpuInfo) -> Result<GpuSensors, NvapiError> {
        let gpu = locate(&self.gpus, card)?;
        if let Some(status) = self.fail {
            return Err(NvapiError::Status(status));
        }
        Ok(self.sensors.get(&gpu).cloned().unwrap_or_default())
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
        let r = NvapiHub::spawn(|| -> Result<FakeNvI2c, NvapiError> { Err(NvapiError::NotAvailable) });
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
