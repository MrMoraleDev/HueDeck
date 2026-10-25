// SPDX-License-Identifier: GPL-3.0-or-later
//! Scripted in-memory backends and the transaction recorder used by golden
//! tests, the plugin simulator and the conformance record/replay harness.
//!
//! * [`MockHid`] exposes a list of scripted [`HidEndpointInfo`]s; each opened
//!   handle returns pre-scripted reads/feature reports and records every
//!   operation into a shared [`TransactionRecorder`].
//! * [`MockSmbus`] is a register-map SMBus bus that records transactions too.
//! * [`MockUsb`] is a scripted raw USB backend (bulk IN data per endpoint,
//!   control IN answers, timeouts, unplug) that records every transfer.
//! * [`MockNotifier`] keeps the system notifications it was asked to show.
//! * [`MockLampArray`] is a fake LampArray provider (plug, unplug,
//!   availability, recorded colour updates).
//!
//! Golden tests compare `recorder.snapshot()` (or just `recorder.writes()`)
//! against checked-in fixtures.

use crate::error::{HalError, HalResult};
use crate::hid::{HidBackend, HidEndpointInfo, HidHandle, ReportLengths};
use crate::smbus::{validate_address, PciIds, SmbusBus, SmbusBusDescriptor, SmbusBusKind};
use crate::usb::{UsbBackend, UsbDeviceInfo, UsbHandle, UsbInterfaceInfo};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// What a recorded transaction did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransactionKind {
    /// Endpoint opened.
    Open,
    /// Output report written (`bytes` = payload incl. report id).
    Write,
    /// Input report read (`bytes` = what the mock returned).
    Read,
    /// Feature report sent.
    SendFeature,
    /// Feature report fetched (`bytes` = what the mock returned).
    GetFeature,
    /// Input report fetched via GET_REPORT (`bytes` = returned).
    GetInputReport,
    /// Handle dropped.
    Close,
    /// SMBus Quick command (`bytes` = `[addr]`).
    SmbusWriteQuick,
    /// SMBus write (`bytes` = `[addr, reg, data…]`; `reg` omitted for no-reg
    /// ops; a Block Write is `[addr, reg, len, data…]`, count byte included).
    SmbusWrite,
    /// SMBus read (`bytes` = `[addr, reg, data…]`).
    SmbusRead,
    /// SMBus bus lock taken (`bytes` = lease in ms, LE) or released (`bytes` empty).
    SmbusLock,
    /// Raw USB interface claimed (`bytes` = `[interface]`).
    UsbClaim,
    /// Bulk OUT transfer (`bytes` = `[endpoint, data…]`).
    BulkOut,
    /// Bulk IN transfer (`bytes` = `[endpoint, data the mock returned…]`).
    BulkIn,
    /// Control OUT transfer (`bytes` = the 8-byte setup packet, then the data).
    ControlOut,
    /// Control IN transfer (`bytes` = the setup packet, then the data returned).
    ControlIn,
}

/// One recorded operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    /// Operation.
    pub kind: TransactionKind,
    /// Endpoint path or bus name the operation targeted.
    pub target: String,
    /// Payload (see [`TransactionKind`] for what it holds).
    pub bytes: Vec<u8>,
    /// Time since the recorder was created.
    pub ts: Duration,
}

/// Shared, cloneable log of [`Transaction`]s.
#[derive(Clone, Debug)]
pub struct TransactionRecorder {
    inner: Arc<Mutex<Vec<Transaction>>>,
    epoch: Instant,
}

impl Default for TransactionRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl TransactionRecorder {
    /// New, empty recorder.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::new())),
            epoch: Instant::now(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Transaction>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Append one transaction.
    pub fn record(&self, kind: TransactionKind, target: impl Into<String>, bytes: impl Into<Vec<u8>>) {
        let ts = self.epoch.elapsed();
        self.lock().push(Transaction {
            kind,
            target: target.into(),
            bytes: bytes.into(),
            ts,
        });
    }

    /// Copy of everything recorded so far.
    pub fn snapshot(&self) -> Vec<Transaction> {
        self.lock().clone()
    }

    /// Number of recorded transactions.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether nothing was recorded.
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// Forget everything recorded so far.
    pub fn clear(&self) {
        self.lock().clear();
    }

    /// Payloads of every transaction of `kind`, in order.
    pub fn payloads(&self, kind: TransactionKind) -> Vec<Vec<u8>> {
        self.lock()
            .iter()
            .filter(|t| t.kind == kind)
            .map(|t| t.bytes.clone())
            .collect()
    }

    /// Payloads of every HID output-report write, in order.
    pub fn writes(&self) -> Vec<Vec<u8>> {
        self.payloads(TransactionKind::Write)
    }

    /// Payloads of every HID feature-report send, in order.
    pub fn feature_sends(&self) -> Vec<Vec<u8>> {
        self.payloads(TransactionKind::SendFeature)
    }
}

/// Mutable state behind one scripted endpoint.
#[derive(Debug, Default)]
pub struct MockEndpointState {
    /// Static description handed to `enumerate()`.
    pub info: HidEndpointInfo,
    /// Reports returned by successive `read_timeout` calls.
    pub scripted_reads: VecDeque<Vec<u8>>,
    /// Reports returned by successive `get_input_report` calls.
    pub scripted_input_reports: VecDeque<Vec<u8>>,
    /// Feature reports by report id; updated by `send_feature_report`.
    pub feature_reports: BTreeMap<u8, Vec<u8>>,
    /// When set, every write fails with [`HalError::Backend`] (simulates `-1`).
    pub fail_writes: bool,
    /// Number of times the endpoint was opened.
    pub open_count: usize,
    /// Unplugged: absent from `enumerate()`, `open` fails, I/O fails with
    /// [`HalError::Disconnected`].
    pub unplugged: bool,
    /// Bumped by every unplug: handles opened before keep failing after a
    /// replug, as stale Windows handles do.
    pub generation: u64,
    /// `read_timeout` without a scripted report waits out its timeout (up to
    /// 5 s), as a real device that sends nothing does; otherwise it returns at once.
    pub block_reads: bool,
    /// What `report_descriptor()` returns; unsupported when `None`.
    pub report_descriptor: Option<Vec<u8>>,
}

type SharedEndpoint = Arc<Mutex<MockEndpointState>>;

/// Scripted HID backend.
#[derive(Debug, Default)]
pub struct MockHid {
    endpoints: Vec<SharedEndpoint>,
    recorder: TransactionRecorder,
}

impl MockHid {
    /// Empty backend with a fresh recorder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Empty backend that logs into an existing recorder.
    pub fn with_recorder(recorder: TransactionRecorder) -> Self {
        Self {
            endpoints: Vec::new(),
            recorder,
        }
    }

    /// Build an endpoint description with a synthetic `mock://` path.
    pub fn endpoint_info(
        vid: u16,
        pid: u16,
        serial: &str,
        interface_number: i32,
        usage_page: u16,
        usage: u16,
        collection: u16,
        report_lengths: ReportLengths,
    ) -> HidEndpointInfo {
        HidEndpointInfo {
            vid,
            pid,
            serial: serial.to_string(),
            manufacturer: "HueDeck Mock".into(),
            product: format!("Mock {vid:04x}:{pid:04x}"),
            path: format!("mock://{vid:04x}:{pid:04x}/{serial}/mi{interface_number}/col{collection}"),
            interface_number,
            usage_page,
            usage,
            collection,
            report_lengths,
            feature_reports: Vec::new(),
        }
    }

    /// Add an endpoint (builder style).
    pub fn add_endpoint(mut self, info: HidEndpointInfo) -> Self {
        self.push_endpoint(info);
        self
    }

    /// Add an endpoint in place.
    pub fn push_endpoint(&mut self, info: HidEndpointInfo) {
        self.endpoints.push(Arc::new(Mutex::new(MockEndpointState {
            info,
            ..Default::default()
        })));
    }

    /// The shared recorder.
    pub fn recorder(&self) -> TransactionRecorder {
        self.recorder.clone()
    }

    /// Shared state of the endpoint at `path`, for scripting from tests.
    pub fn endpoint(&self, path: &str) -> Option<SharedEndpoint> {
        self.endpoints.iter().find(|e| lock(e).info.path == path).cloned()
    }

    /// Queue an input report for the next `read_timeout` on `path`.
    pub fn script_read(&self, path: &str, report: impl Into<Vec<u8>>) -> HalResult<()> {
        let ep = self
            .endpoint(path)
            .ok_or_else(|| HalError::NotFound(path.to_string()))?;
        lock(&ep).scripted_reads.push_back(report.into());
        Ok(())
    }

    /// Queue a report for the next `get_input_report` on `path`.
    pub fn script_input_report(&self, path: &str, report: impl Into<Vec<u8>>) -> HalResult<()> {
        let ep = self
            .endpoint(path)
            .ok_or_else(|| HalError::NotFound(path.to_string()))?;
        lock(&ep).scripted_input_reports.push_back(report.into());
        Ok(())
    }

    /// Unplug the endpoint at `path` (see [`MockEndpointState::unplugged`]).
    pub fn unplug(&self, path: &str) -> HalResult<()> {
        let ep = self
            .endpoint(path)
            .ok_or_else(|| HalError::NotFound(path.to_string()))?;
        let mut s = lock(&ep);
        s.unplugged = true;
        s.generation += 1;
        Ok(())
    }

    /// Plug the endpoint at `path` back in (handles opened before stay stale).
    pub fn replug(&self, path: &str) -> HalResult<()> {
        let ep = self
            .endpoint(path)
            .ok_or_else(|| HalError::NotFound(path.to_string()))?;
        lock(&ep).unplugged = false;
        Ok(())
    }

    /// Preload the feature report for id `report[0]` on `path`.
    pub fn set_feature_report(&self, path: &str, report: impl Into<Vec<u8>>) -> HalResult<()> {
        let report = report.into();
        let id = *report
            .first()
            .ok_or_else(|| HalError::InvalidArgument("empty feature report".into()))?;
        let ep = self
            .endpoint(path)
            .ok_or_else(|| HalError::NotFound(path.to_string()))?;
        lock(&ep).feature_reports.insert(id, report);
        Ok(())
    }
}

fn lock(ep: &SharedEndpoint) -> MutexGuard<'_, MockEndpointState> {
    ep.lock().unwrap_or_else(|e| e.into_inner())
}

impl HidBackend for MockHid {
    fn enumerate(&self) -> HalResult<Vec<HidEndpointInfo>> {
        Ok(self
            .endpoints
            .iter()
            .filter_map(|e| {
                let s = lock(e);
                (!s.unplugged).then(|| s.info.clone())
            })
            .collect())
    }

    fn open(&self, path: &str) -> HalResult<Box<dyn HidHandle>> {
        let state = self
            .endpoint(path)
            .ok_or_else(|| HalError::NotFound(path.to_string()))?;
        let (info, generation) = {
            let mut s = lock(&state);
            if s.unplugged {
                return Err(HalError::NotFound(path.to_string()));
            }
            s.open_count += 1;
            (s.info.clone(), s.generation)
        };
        self.recorder.record(TransactionKind::Open, path, Vec::new());
        Ok(Box::new(MockHidHandle {
            state,
            recorder: self.recorder.clone(),
            info,
            generation,
        }))
    }
}

/// Handle returned by [`MockHid::open`].
#[derive(Debug)]
pub struct MockHidHandle {
    state: SharedEndpoint,
    recorder: TransactionRecorder,
    info: HidEndpointInfo,
    generation: u64,
}

impl MockHidHandle {
    /// `Disconnected` once the device was unplugged after this handle was opened.
    fn connected(&self) -> HalResult<MutexGuard<'_, MockEndpointState>> {
        let s = lock(&self.state);
        if s.unplugged || s.generation != self.generation {
            return Err(HalError::Disconnected);
        }
        Ok(s)
    }

    fn copy_out(src: &[u8], dst: &mut [u8]) -> usize {
        let n = src.len().min(dst.len());
        dst[..n].copy_from_slice(&src[..n]);
        n
    }
}

impl HidHandle for MockHidHandle {
    fn info(&self) -> &HidEndpointInfo {
        &self.info
    }

    fn write(&mut self, data: &[u8]) -> HalResult<usize> {
        if self.connected()?.fail_writes {
            return Err(HalError::Backend("mock: write failure scripted".into()));
        }
        self.recorder
            .record(TransactionKind::Write, &*self.info.path, data);
        Ok(data.len())
    }

    fn read_timeout(&mut self, buf: &mut [u8], timeout_ms: i32) -> HalResult<usize> {
        let (next, block) = {
            let mut s = self.connected()?;
            (s.scripted_reads.pop_front(), s.block_reads)
        };
        if next.is_none() && block {
            let ms = u64::try_from(timeout_ms).unwrap_or(0).min(5000);
            std::thread::sleep(std::time::Duration::from_millis(ms));
        }
        match next {
            Some(report) => {
                let n = Self::copy_out(&report, buf);
                self.recorder
                    .record(TransactionKind::Read, &*self.info.path, &buf[..n]);
                Ok(n)
            }
            None => Ok(0),
        }
    }

    fn send_feature_report(&mut self, data: &[u8]) -> HalResult<usize> {
        let id = *data
            .first()
            .ok_or_else(|| HalError::InvalidArgument("empty feature report".into()))?;
        self.connected()?.feature_reports.insert(id, data.to_vec());
        self.recorder
            .record(TransactionKind::SendFeature, &*self.info.path, data);
        Ok(data.len())
    }

    fn get_feature_report(&mut self, buf: &mut [u8]) -> HalResult<usize> {
        let id = *buf
            .first()
            .ok_or_else(|| HalError::InvalidArgument("empty buffer".into()))?;
        let stored = self.connected()?.feature_reports.get(&id).cloned();
        let report = stored.ok_or_else(|| HalError::Backend(format!("mock: no feature report {id:#04x}")))?;
        let n = Self::copy_out(&report, buf);
        self.recorder
            .record(TransactionKind::GetFeature, &*self.info.path, &buf[..n]);
        Ok(n)
    }

    fn get_input_report(&mut self, buf: &mut [u8]) -> HalResult<usize> {
        let next = self.connected()?.scripted_input_reports.pop_front();
        let report = next.ok_or_else(|| HalError::Backend("mock: no input report scripted".into()))?;
        let n = Self::copy_out(&report, buf);
        self.recorder
            .record(TransactionKind::GetInputReport, &*self.info.path, &buf[..n]);
        Ok(n)
    }

    fn report_descriptor(&mut self) -> HalResult<Vec<u8>> {
        self.connected()?
            .report_descriptor
            .clone()
            .ok_or(HalError::Unsupported("report descriptor"))
    }
}

impl Drop for MockHidHandle {
    fn drop(&mut self) {
        self.recorder
            .record(TransactionKind::Close, &*self.info.path, Vec::new());
    }
}

/// Register-map SMBus bus. Every `(addr, reg)` cell is a byte; devices exist
/// only at the addresses given to [`MockSmbus::add_device`] or
/// [`MockSmbus::add_indexed_device`].
#[derive(Debug)]
pub struct MockSmbus {
    descriptor: SmbusBusDescriptor,
    devices: BTreeMap<u8, BTreeMap<u8, u8>>,
    indexed: BTreeMap<u8, IndexedDevice>,
    fail_block: BTreeSet<u8>,
    /// DDR4 SPD EEPROMs (EE1004, 512 bytes) by address.
    spd: BTreeMap<u8, Vec<u8>>,
    /// The EE1004 page every DDR4 SPD on the bus has selected.
    spd_page: u8,
    /// Sizes of the batches [`SmbusBus::run_batch`] was given.
    batches: Arc<Mutex<Vec<usize>>>,
    recorder: TransactionRecorder,
}

/// A DRAM RGB controller with the ENE-style indexed register file (Crucial
/// Ballistix, ENE/Aura DRAM): Write Word to command 0x00 sets the 16-bit
/// register pointer (sent byte-swapped, so the pointer's high byte goes
/// first on the wire), Read Byte 0x81 reads the pointed register, Write Byte
/// 0x01 writes it and a Block Write to 0x03 writes from it. The pointer does
/// not advance. Plain commands (0xA0..0xAF identification bytes, Receive
/// Byte) read the direct map.
#[derive(Debug, Default)]
struct IndexedDevice {
    pointer: u16,
    direct: BTreeMap<u8, u8>,
    memory: BTreeMap<u16, u8>,
}

/// ENE-style commands (see [`MockSmbus::add_indexed_device`]).
const INDEX_POINTER: u8 = 0x00;
const INDEX_WRITE: u8 = 0x01;
const INDEX_BLOCK: u8 = 0x03;
const INDEX_READ: u8 = 0x81;

impl MockSmbus {
    /// New bus of `kind` named `name`, logging into `recorder`.
    pub fn new(name: &str, kind: SmbusBusKind, recorder: TransactionRecorder) -> Self {
        Self {
            descriptor: SmbusBusDescriptor {
                name: name.to_string(),
                kind,
                pci: Default::default(),
                instance: String::new(),
                max_block_len: crate::smbus::SMBUS_BLOCK_MAX,
            },
            devices: BTreeMap::new(),
            indexed: BTreeMap::new(),
            fail_block: BTreeSet::new(),
            spd: BTreeMap::new(),
            spd_page: 0,
            batches: Arc::default(),
            recorder,
        }
    }

    /// A DDR4 SPD EEPROM (JEDEC EE1004) at `addr` (0x50..=0x57) holding
    /// `contents` (padded with 0xFF to 512 bytes). Plain reads at `addr` see
    /// the page selected on the bus (page 0 at first); a Send Byte to 0x36 /
    /// 0x37 inside [`SmbusBus::spd_read`] switches it, as on the wire, and
    /// is recorded as an `SmbusWrite` of `[0x36 or 0x37, 0x00]`.
    pub fn add_ddr4_spd(&mut self, addr: u8, contents: &[u8]) {
        let mut bytes = vec![0xFF; usize::from(crate::smbus::SPD_DDR4_LEN)];
        let n = contents.len().min(bytes.len());
        bytes[..n].copy_from_slice(&contents[..n]);
        self.spd.insert(addr, bytes);
    }

    /// The sizes of the batches handed to [`SmbusBus::run_batch`] so far
    /// (shared: keep it before the bus is boxed). Each op of a batch is
    /// still recorded as its own transaction.
    pub fn batch_log(&self) -> Arc<Mutex<Vec<usize>>> {
        Arc::clone(&self.batches)
    }

    /// Leave page `page` selected on the bus's DDR4 SPDs (another program did).
    pub fn set_spd_page(&mut self, page: u8) {
        self.spd_page = page & 1;
    }

    /// The EE1004 page currently selected.
    pub fn spd_page(&self) -> u8 {
        self.spd_page
    }

    fn spd_byte(&self, addr: u8, reg: u8) -> Option<u8> {
        self.spd
            .get(&addr)
            .map(|b| b[usize::from(self.spd_page) * 256 + usize::from(reg)])
    }

    /// EE1004 Set Page Address: a Send Byte (data 0x00) to 0x36 or 0x37.
    fn spd_set_page(&mut self, page: u8) -> HalResult<()> {
        let at = crate::smbus::SPD_SET_PAGE[usize::from(page & 1)];
        self.rec(TransactionKind::SmbusWrite, vec![at, 0x00]);
        if self.spd.is_empty() {
            // No EE1004 on the bus: nothing acknowledges.
            return Err(HalError::Bus { status: -1 });
        }
        self.spd_page = page & 1;
        Ok(())
    }

    /// The bus's controller PCI ids (`bus.Vendor()` and friends).
    pub fn with_pci(mut self, pci: PciIds) -> Self {
        self.descriptor.pci = pci;
        self
    }

    /// The bus instance (PIIX4 port base, GPU PCI bus).
    pub fn with_instance(mut self, instance: &str) -> Self {
        self.descriptor.instance = instance.to_string();
        self
    }

    /// Block writes to `addr` fail with a bus error (status -1) and change nothing.
    pub fn fail_block_writes(mut self, addr: u8) -> Self {
        self.fail_block.insert(addr);
        self
    }

    /// Make a device answer at `addr` with the given initial registers.
    pub fn add_device(&mut self, addr: u8, registers: impl IntoIterator<Item = (u8, u8)>) {
        self.devices.insert(addr, registers.into_iter().collect());
    }

    /// Make an ENE-style indexed controller (Crucial / ENE DRAM RGB) answer
    /// at `addr`: `direct` holds the plain command registers (identification
    /// bytes 0xA0..0xAF), `memory` the 16-bit register file behind the
    /// pointer. Writes land in the register file, recorded as usual.
    pub fn add_indexed_device(
        &mut self,
        addr: u8,
        direct: impl IntoIterator<Item = (u8, u8)>,
        memory: impl IntoIterator<Item = (u16, u8)>,
    ) {
        self.indexed.insert(
            addr,
            IndexedDevice {
                pointer: 0,
                direct: direct.into_iter().collect(),
                memory: memory.into_iter().collect(),
            },
        );
    }

    /// Current register map of the device at `addr`.
    pub fn registers(&self, addr: u8) -> Option<&BTreeMap<u8, u8>> {
        self.devices.get(&addr)
    }

    /// One register of the indexed device at `addr`.
    pub fn indexed_register(&self, addr: u8, reg: u16) -> Option<u8> {
        self.indexed.get(&addr).and_then(|d| d.memory.get(&reg).copied())
    }

    fn device(&mut self, addr: u8) -> HalResult<&mut BTreeMap<u8, u8>> {
        validate_address(addr)?;
        self.devices.get_mut(&addr).ok_or(HalError::Bus { status: -1 })
    }

    fn indexed(&mut self, addr: u8) -> Option<&mut IndexedDevice> {
        self.indexed.get_mut(&addr)
    }

    fn rec(&self, kind: TransactionKind, bytes: Vec<u8>) {
        self.recorder.record(kind, &*self.descriptor.name, bytes);
    }
}

impl SmbusBus for MockSmbus {
    fn descriptor(&self) -> &SmbusBusDescriptor {
        &self.descriptor
    }

    fn write_quick(&mut self, addr: u8) -> HalResult<()> {
        if !self.descriptor.kind.supports_write_quick() {
            return Err(HalError::Unsupported("WriteQuick on NvAPI bus"));
        }
        self.rec(TransactionKind::SmbusWriteQuick, vec![addr]);
        if self.indexed.contains_key(&addr) {
            return Ok(());
        }
        self.device(addr).map(|_| ())
    }

    fn read_byte(&mut self, addr: u8, reg: u8) -> HalResult<u8> {
        if let Some(v) = self.spd_byte(addr, reg) {
            self.rec(TransactionKind::SmbusRead, vec![addr, reg, v]);
            return Ok(v);
        }
        let v = match self.indexed(addr) {
            Some(d) if reg == INDEX_READ => *d.memory.get(&d.pointer).unwrap_or(&0xFF),
            Some(d) => *d.direct.get(&reg).unwrap_or(&0xFF),
            None => *self.device(addr)?.get(&reg).unwrap_or(&0xFF),
        };
        self.rec(TransactionKind::SmbusRead, vec![addr, reg, v]);
        Ok(v)
    }

    fn write_byte(&mut self, addr: u8, reg: u8, value: u8) -> HalResult<()> {
        match self.indexed(addr) {
            Some(d) if reg == INDEX_WRITE => {
                d.memory.insert(d.pointer, value);
            }
            Some(d) => {
                d.direct.insert(reg, value);
            }
            None => {
                self.device(addr)?.insert(reg, value);
            }
        }
        self.rec(TransactionKind::SmbusWrite, vec![addr, reg, value]);
        Ok(())
    }

    fn read_word(&mut self, addr: u8, reg: u8) -> HalResult<u16> {
        let dev = self.device(addr)?;
        let lo = *dev.get(&reg).unwrap_or(&0xFF);
        let hi = *dev.get(&reg.wrapping_add(1)).unwrap_or(&0xFF);
        self.rec(TransactionKind::SmbusRead, vec![addr, reg, lo, hi]);
        Ok(u16::from_le_bytes([lo, hi]))
    }

    fn write_word(&mut self, addr: u8, reg: u8, value: u16) -> HalResult<()> {
        let [lo, hi] = value.to_le_bytes();
        match self.indexed(addr) {
            // The pointer arrives byte-swapped: its high byte is the word's low byte.
            Some(d) if reg == INDEX_POINTER => d.pointer = u16::from_be_bytes([lo, hi]),
            Some(_) => return Err(HalError::Bus { status: -1 }),
            None => {
                let dev = self.device(addr)?;
                dev.insert(reg, lo);
                dev.insert(reg.wrapping_add(1), hi);
            }
        }
        self.rec(TransactionKind::SmbusWrite, vec![addr, reg, lo, hi]);
        Ok(())
    }

    fn write_block(&mut self, addr: u8, reg: u8, data: &[u8]) -> HalResult<()> {
        if data.len() > self.descriptor.max_block_len {
            return Err(HalError::InvalidArgument(format!(
                "block of {} > max {}",
                data.len(),
                self.descriptor.max_block_len
            )));
        }
        if self.fail_block.contains(&addr) {
            return Err(HalError::Bus { status: -1 });
        }
        match self.indexed(addr) {
            Some(d) if reg == INDEX_BLOCK => {
                for (i, b) in data.iter().enumerate() {
                    d.memory.insert(d.pointer.wrapping_add(i as u16), *b);
                }
            }
            Some(_) => return Err(HalError::Bus { status: -1 }),
            None => {
                let dev = self.device(addr)?;
                for (i, b) in data.iter().enumerate() {
                    dev.insert(reg.wrapping_add(i as u8), *b);
                }
            }
        }
        let mut bytes = vec![addr, reg, data.len() as u8];
        bytes.extend_from_slice(data);
        self.rec(TransactionKind::SmbusWrite, bytes);
        Ok(())
    }

    fn read_block(&mut self, addr: u8, reg: u8, buf: &mut [u8]) -> HalResult<usize> {
        let dev = self.device(addr)?;
        for (i, slot) in buf.iter_mut().enumerate() {
            *slot = *dev.get(&reg.wrapping_add(i as u8)).unwrap_or(&0xFF);
        }
        let mut bytes = vec![addr, reg];
        bytes.extend_from_slice(buf);
        self.rec(TransactionKind::SmbusRead, bytes);
        Ok(buf.len())
    }

    fn write_block_no_reg(&mut self, addr: u8, data: &[u8]) -> HalResult<()> {
        // Register-less writes are modelled as a write starting at register 0.
        let dev = self.device(addr)?;
        for (i, b) in data.iter().enumerate() {
            dev.insert(i as u8, *b);
        }
        let mut bytes = vec![addr];
        bytes.extend_from_slice(data);
        self.rec(TransactionKind::SmbusWrite, bytes);
        Ok(())
    }

    fn read_block_no_reg(&mut self, addr: u8, buf: &mut [u8]) -> HalResult<usize> {
        let dev = self.device(addr)?;
        for (i, slot) in buf.iter_mut().enumerate() {
            *slot = *dev.get(&(i as u8)).unwrap_or(&0xFF);
        }
        let mut bytes = vec![addr];
        bytes.extend_from_slice(buf);
        self.rec(TransactionKind::SmbusRead, bytes);
        Ok(buf.len())
    }

    fn read_byte_no_reg(&mut self, addr: u8) -> HalResult<u8> {
        if !self.descriptor.kind.supports_read_byte_no_reg() {
            return Err(HalError::Unsupported("ReadByteWithoutRegister on this bus kind"));
        }
        let v = match self.indexed(addr) {
            Some(d) => *d.direct.get(&0).unwrap_or(&0xFF),
            None => *self.device(addr)?.get(&0).unwrap_or(&0xFF),
        };
        self.rec(TransactionKind::SmbusRead, vec![addr, v]);
        Ok(v)
    }

    fn bus_lock(&mut self, lease_ms: u16) -> HalResult<()> {
        self.rec(TransactionKind::SmbusLock, lease_ms.to_le_bytes().to_vec());
        Ok(())
    }

    fn bus_unlock(&mut self) -> HalResult<()> {
        self.rec(TransactionKind::SmbusLock, Vec::new());
        Ok(())
    }

    fn run_batch(&mut self, ops: &[crate::smbus::SmbusOp]) -> crate::smbus::SmbusBatch {
        self.batches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(ops.len());
        crate::smbus::run_one_by_one(self, ops)
    }

    fn spd_read(&mut self, addr: u8, offset: u16, buf: &mut [u8]) -> HalResult<usize> {
        if !self.descriptor.kind.is_chipset() {
            return Err(HalError::Unsupported("SPD read on this bus"));
        }
        crate::smbus::validate_spd_read(addr, offset, buf.len())?;
        let page = (offset / crate::smbus::SPD_PAGE_LEN) as u8;
        self.spd_set_page(page)?;
        let mut result = Ok(buf.len());
        for (i, slot) in buf.iter_mut().enumerate() {
            match self.read_byte(addr, (offset % crate::smbus::SPD_PAGE_LEN) as u8 + i as u8) {
                Ok(v) => *slot = v,
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }
        if page != 0 {
            // Page 0 again, whatever the reads did.
            self.spd_set_page(0)?;
        }
        result
    }
}

/// Mutable state behind one scripted raw USB device.
#[derive(Debug, Default)]
pub struct MockUsbDeviceState {
    /// Static description handed to `enumerate()`.
    pub info: UsbDeviceInfo,
    /// Data returned by successive bulk IN transfers, per endpoint address.
    pub bulk_in: BTreeMap<u8, VecDeque<Vec<u8>>>,
    /// Data returned by successive control IN transfers.
    pub control_in: VecDeque<Vec<u8>>,
    /// Every OUT transfer fails with [`HalError::Backend`].
    pub fail_out: bool,
    /// Every OUT transfer times out (a device that stopped reading).
    pub out_timeout: bool,
    /// An IN transfer without scripted data waits out its timeout (up to
    /// 5 s) before it fails with [`HalError::Timeout`]; otherwise it fails at once.
    pub block_reads: bool,
    /// Number of times the device was opened.
    pub open_count: usize,
    /// Unplugged: absent from `enumerate()`, `open` fails, I/O fails with
    /// [`HalError::Disconnected`].
    pub unplugged: bool,
    /// Bumped by every unplug: handles opened before stay stale.
    pub generation: u64,
}

type SharedUsbDevice = Arc<Mutex<MockUsbDeviceState>>;

/// Scripted raw USB backend. Handles route transfers like the Windows
/// backend does ([`UsbDeviceInfo::claimable_for_endpoint`],
/// [`UsbDeviceInfo::claimable_for_control`]): an interface another driver
/// owns is never claimed. Every operation is recorded: `Open`/`Close`,
/// `UsbClaim` (`[interface]`), `BulkOut`/`BulkIn` (`[endpoint, data…]`) and
/// `ControlOut`/`ControlIn` (the 8-byte setup packet, then the data).
#[derive(Debug, Default)]
pub struct MockUsb {
    devices: Vec<SharedUsbDevice>,
    recorder: TransactionRecorder,
}

fn lock_usb(d: &SharedUsbDevice) -> MutexGuard<'_, MockUsbDeviceState> {
    d.lock().unwrap_or_else(|e| e.into_inner())
}

impl MockUsb {
    /// Empty backend with a fresh recorder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Empty backend that logs into an existing recorder (shared with a [`MockHid`]).
    pub fn with_recorder(recorder: TransactionRecorder) -> Self {
        Self {
            devices: Vec::new(),
            recorder,
        }
    }

    /// An interface WinUSB owns, with its endpoints.
    pub fn winusb_interface(number: u8, endpoints: &[u8]) -> UsbInterfaceInfo {
        UsbInterfaceInfo {
            number,
            class: 0xFF,
            subclass: 0,
            protocol: 0,
            driver: "WinUSB".into(),
            accessible: true,
            endpoints: endpoints.to_vec(),
        }
    }

    /// An interface another driver owns (`HidUsb`, `usbaudio`, `USBSTOR`, ...).
    pub fn foreign_interface(number: u8, class: u8, driver: &str, endpoints: &[u8]) -> UsbInterfaceInfo {
        UsbInterfaceInfo {
            number,
            class,
            subclass: 0,
            protocol: 0,
            driver: driver.into(),
            accessible: false,
            endpoints: endpoints.to_vec(),
        }
    }

    /// A device description with a synthetic `mockusb://` path and instance
    /// id `USB\VID_xxxx&PID_xxxx\<serial>` (what a HID group's USB parent is
    /// matched against). `driver_bound` follows the interfaces.
    pub fn device_info(vid: u16, pid: u16, serial: &str, interfaces: Vec<UsbInterfaceInfo>) -> UsbDeviceInfo {
        UsbDeviceInfo {
            vid,
            pid,
            serial: serial.to_string(),
            manufacturer: "HueDeck Mock".into(),
            product: format!("Mock USB {vid:04x}:{pid:04x}"),
            path: format!("mockusb://{vid:04x}:{pid:04x}/{serial}"),
            bus_number: 1,
            address: 1,
            driver_bound: interfaces.iter().any(|i| i.accessible),
            instance_id: format!("USB\\VID_{vid:04X}&PID_{pid:04X}\\{serial}"),
            interfaces,
        }
    }

    /// Add a device (builder style).
    pub fn add_device(mut self, info: UsbDeviceInfo) -> Self {
        self.push_device(info);
        self
    }

    /// Add a device in place.
    pub fn push_device(&mut self, info: UsbDeviceInfo) {
        self.devices.push(Arc::new(Mutex::new(MockUsbDeviceState {
            info,
            ..Default::default()
        })));
    }

    /// The shared recorder.
    pub fn recorder(&self) -> TransactionRecorder {
        self.recorder.clone()
    }

    /// Shared state of the device at `path`, for scripting from tests.
    pub fn device(&self, path: &str) -> Option<SharedUsbDevice> {
        self.devices
            .iter()
            .find(|d| lock_usb(d).info.path == path)
            .cloned()
    }

    fn with_device<T>(&self, path: &str, f: impl FnOnce(&mut MockUsbDeviceState) -> T) -> HalResult<T> {
        let d = self
            .device(path)
            .ok_or_else(|| HalError::NotFound(path.to_string()))?;
        let mut s = lock_usb(&d);
        Ok(f(&mut s))
    }

    /// Queue data for the next bulk IN transfer on `endpoint` of `path`.
    pub fn script_bulk_in(&self, path: &str, endpoint: u8, data: impl Into<Vec<u8>>) -> HalResult<()> {
        let data = data.into();
        self.with_device(path, |s| s.bulk_in.entry(endpoint).or_default().push_back(data))
    }

    /// Queue data for the next control IN transfer on `path`.
    pub fn script_control_in(&self, path: &str, data: impl Into<Vec<u8>>) -> HalResult<()> {
        let data = data.into();
        self.with_device(path, |s| s.control_in.push_back(data))
    }

    /// Unplug the device at `path` (see [`MockUsbDeviceState::unplugged`]).
    pub fn unplug(&self, path: &str) -> HalResult<()> {
        self.with_device(path, |s| {
            s.unplugged = true;
            s.generation += 1;
        })
    }

    /// Plug the device at `path` back in (handles opened before stay stale).
    pub fn replug(&self, path: &str) -> HalResult<()> {
        self.with_device(path, |s| s.unplugged = false)
    }
}

impl UsbBackend for MockUsb {
    fn enumerate(&self) -> HalResult<Vec<UsbDeviceInfo>> {
        Ok(self
            .devices
            .iter()
            .filter_map(|d| {
                let s = lock_usb(d);
                (!s.unplugged).then(|| s.info.clone())
            })
            .collect())
    }

    fn open(&self, path: &str) -> HalResult<Box<dyn UsbHandle>> {
        let state = self
            .device(path)
            .ok_or_else(|| HalError::NotFound(path.to_string()))?;
        let (info, generation) = {
            let mut s = lock_usb(&state);
            if s.unplugged {
                return Err(HalError::NotFound(path.to_string()));
            }
            s.open_count += 1;
            (s.info.clone(), s.generation)
        };
        self.recorder.record(TransactionKind::Open, path, Vec::new());
        Ok(Box::new(MockUsbHandle {
            state,
            recorder: self.recorder.clone(),
            info,
            generation,
            claimed: BTreeSet::new(),
        }))
    }
}

/// Handle returned by [`MockUsb::open`].
#[derive(Debug)]
pub struct MockUsbHandle {
    state: SharedUsbDevice,
    recorder: TransactionRecorder,
    info: UsbDeviceInfo,
    generation: u64,
    claimed: BTreeSet<u8>,
}

impl MockUsbHandle {
    fn connected(&self) -> HalResult<MutexGuard<'_, MockUsbDeviceState>> {
        let s = lock_usb(&self.state);
        if s.unplugged || s.generation != self.generation {
            return Err(HalError::Disconnected);
        }
        Ok(s)
    }

    fn claim(&mut self, interface: u8) -> HalResult<()> {
        self.info.claimable_interface(interface)?;
        drop(self.connected()?);
        if self.claimed.insert(interface) {
            self.recorder
                .record(TransactionKind::UsbClaim, &*self.info.path, vec![interface]);
        }
        Ok(())
    }

    /// What an OUT transfer runs into: a scripted timeout or failure.
    fn out_allowed(&self, timeout_ms: u32) -> HalResult<()> {
        let s = self.connected()?;
        if s.out_timeout {
            return Err(HalError::Timeout(timeout_ms));
        }
        if s.fail_out {
            return Err(HalError::Backend("mock: OUT transfer failure scripted".into()));
        }
        Ok(())
    }
}

/// An IN transfer found nothing to return: [`HalError::Timeout`], after
/// waiting the timeout out (at most 5 s) when `block`.
fn starve(block: bool, timeout_ms: u32) -> HalError {
    if block {
        std::thread::sleep(Duration::from_millis(u64::from(timeout_ms.min(5000))));
    }
    HalError::Timeout(timeout_ms)
}

/// The setup packet of a control transfer, little-endian, then `data`.
fn setup_packet(bm: u8, request: u8, value: u16, index: u16, length: usize, data: &[u8]) -> Vec<u8> {
    let length = u16::try_from(length).unwrap_or(u16::MAX);
    let mut out = vec![bm, request];
    out.extend_from_slice(&value.to_le_bytes());
    out.extend_from_slice(&index.to_le_bytes());
    out.extend_from_slice(&length.to_le_bytes());
    out.extend_from_slice(data);
    out
}

impl UsbHandle for MockUsbHandle {
    fn info(&self) -> &UsbDeviceInfo {
        &self.info
    }

    fn claim_interface(&mut self, interface: u8) -> HalResult<()> {
        self.claim(interface)
    }

    fn bulk_transfer(&mut self, endpoint: u8, data: &mut [u8], timeout_ms: u32) -> HalResult<usize> {
        let iface = self.info.claimable_for_endpoint(endpoint)?.number;
        self.claim(iface)?;
        if endpoint & crate::usb::ENDPOINT_IN == 0 {
            self.out_allowed(timeout_ms)?;
            let mut bytes = vec![endpoint];
            bytes.extend_from_slice(data);
            self.recorder
                .record(TransactionKind::BulkOut, &*self.info.path, bytes);
            return Ok(data.len());
        }
        let (next, block) = {
            let mut s = self.connected()?;
            let next = s.bulk_in.get_mut(&endpoint).and_then(VecDeque::pop_front);
            (next, s.block_reads)
        };
        let Some(next) = next else {
            return Err(starve(block, timeout_ms));
        };
        let n = next.len().min(data.len());
        data[..n].copy_from_slice(&next[..n]);
        let mut bytes = vec![endpoint];
        bytes.extend_from_slice(&data[..n]);
        self.recorder
            .record(TransactionKind::BulkIn, &*self.info.path, bytes);
        Ok(n)
    }

    fn control_transfer(
        &mut self,
        bm_request_type: u8,
        b_request: u8,
        w_value: u16,
        w_index: u16,
        data: &mut [u8],
        timeout_ms: u32,
    ) -> HalResult<usize> {
        let iface = self.info.claimable_for_control(bm_request_type, w_index)?.number;
        self.claim(iface)?;
        if bm_request_type & crate::usb::ENDPOINT_IN == 0 {
            self.out_allowed(timeout_ms)?;
            let bytes = setup_packet(bm_request_type, b_request, w_value, w_index, data.len(), data);
            self.recorder
                .record(TransactionKind::ControlOut, &*self.info.path, bytes);
            return Ok(data.len());
        }
        let (next, block) = {
            let mut s = self.connected()?;
            (s.control_in.pop_front(), s.block_reads)
        };
        let Some(next) = next else {
            return Err(starve(block, timeout_ms));
        };
        let n = next.len().min(data.len());
        data[..n].copy_from_slice(&next[..n]);
        let bytes = setup_packet(
            bm_request_type,
            b_request,
            w_value,
            w_index,
            data.len(),
            &data[..n],
        );
        self.recorder
            .record(TransactionKind::ControlIn, &*self.info.path, bytes);
        Ok(n)
    }
}

impl Drop for MockUsbHandle {
    fn drop(&mut self) {
        self.recorder
            .record(TransactionKind::Close, &*self.info.path, Vec::new());
    }
}

/// A [`crate::notify::Notifier`] that keeps what it is asked to show.
#[derive(Debug, Default)]
pub struct MockNotifier {
    shown: Mutex<Vec<crate::notify::Notification>>,
}

impl MockNotifier {
    /// The notifications shown so far.
    pub fn shown(&self) -> Vec<crate::notify::Notification> {
        self.shown.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl crate::notify::Notifier for MockNotifier {
    fn notify(&self, notification: &crate::notify::Notification) -> HalResult<()> {
        self.shown
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(notification.clone());
        Ok(())
    }
}

#[derive(Debug)]
struct LampState {
    devices: Vec<crate::lamparray::LampArrayInfo>,
    available: BTreeMap<String, bool>,
    writes: BTreeMap<String, Vec<Vec<[u8; 4]>>>,
    releases: BTreeMap<String, u32>,
    containers: BTreeMap<String, String>,
    generation: u64,
    identity: bool,
}

/// A fake LampArray provider ([`crate::lamparray::LampArrayBackend`]): devices
/// come and go, availability flips (another app took control) and every
/// colour update is recorded per device id.
#[derive(Clone, Debug)]
pub struct MockLampArray {
    state: Arc<Mutex<LampState>>,
}

impl Default for MockLampArray {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(LampState {
                devices: Vec::new(),
                available: BTreeMap::new(),
                writes: BTreeMap::new(),
                releases: BTreeMap::new(),
                containers: BTreeMap::new(),
                generation: 0,
                identity: true,
            })),
        }
    }
}

impl MockLampArray {
    /// No devices, with package identity.
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, LampState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A LampArray of `lamps` RGB lamps in a row, 10 mm apart, on a
    /// `lamps * 10 mm` x 10 mm bounding box.
    pub fn row(id: &str, vid: u16, pid: u16, lamps: u32) -> crate::lamparray::LampArrayInfo {
        crate::lamparray::LampArrayInfo {
            id: id.into(),
            name: format!("Mock LampArray {id}"),
            vid,
            pid,
            container_id: None,
            kind: crate::lamparray::LampArrayKind::Peripheral,
            bounding_box_um: [lamps.max(1) * 10_000, 10_000, 1_000],
            min_update_interval_us: 0,
            lamps: (0..lamps)
                .map(|i| crate::lamparray::LampInfo {
                    index: i,
                    position_um: [(i * 10_000 + 5_000) as i32, 5_000, 0],
                    is_rgb: true,
                })
                .collect(),
        }
    }

    /// Plug a device in (replacing one with the same id).
    pub fn add(&self, info: crate::lamparray::LampArrayInfo) {
        let mut s = self.lock();
        s.devices.retain(|d| d.id != info.id);
        s.devices.push(info);
        s.generation += 1;
    }

    /// Unplug a device.
    pub fn remove(&self, id: &str) {
        let mut s = self.lock();
        s.devices.retain(|d| d.id != id);
        s.generation += 1;
    }

    /// `IsAvailable` of a device (default `true`).
    pub fn set_available(&self, id: &str, available: bool) {
        self.lock().available.insert(id.into(), available);
    }

    /// Whether the process has package identity (default `true`).
    pub fn set_identity(&self, identity: bool) {
        self.lock().identity = identity;
    }

    /// The container id [`crate::lamparray::LampArrayBackend::container_of`]
    /// returns for a HID interface path.
    pub fn set_container(&self, interface_path: &str, container: &str) {
        self.lock()
            .containers
            .insert(interface_path.to_ascii_lowercase(), container.into());
    }

    /// Every colour update sent to device `id`, oldest first.
    pub fn writes(&self, id: &str) -> Vec<Vec<[u8; 4]>> {
        self.lock().writes.get(id).cloned().unwrap_or_default()
    }

    /// How often device `id` was released.
    pub fn releases(&self, id: &str) -> u32 {
        self.lock().releases.get(id).copied().unwrap_or(0)
    }
}

struct MockLampHandle {
    state: Arc<Mutex<LampState>>,
    info: crate::lamparray::LampArrayInfo,
}

impl MockLampHandle {
    fn lock(&self) -> MutexGuard<'_, LampState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl crate::lamparray::LampArrayHandle for MockLampHandle {
    fn info(&self) -> &crate::lamparray::LampArrayInfo {
        &self.info
    }

    fn available(&self) -> bool {
        self.lock().available.get(&self.info.id).copied().unwrap_or(true)
    }

    fn set_colors(&mut self, rgba: &[[u8; 4]]) -> HalResult<()> {
        if rgba.len() != self.info.lamps.len() {
            return Err(HalError::InvalidArgument(format!(
                "{} colours for {} lamps",
                rgba.len(),
                self.info.lamps.len()
            )));
        }
        let id = self.info.id.clone();
        let mut s = self.lock();
        if !s.devices.iter().any(|d| d.id == id) {
            return Err(HalError::Disconnected);
        }
        s.writes.entry(id).or_default().push(rgba.to_vec());
        Ok(())
    }

    fn release(&mut self) -> HalResult<()> {
        let id = self.info.id.clone();
        *self.lock().releases.entry(id).or_default() += 1;
        Ok(())
    }
}

impl crate::lamparray::LampArrayBackend for MockLampArray {
    fn enumerate(&self) -> HalResult<Vec<crate::lamparray::LampArrayInfo>> {
        Ok(self.lock().devices.clone())
    }

    fn open(&self, id: &str) -> HalResult<Box<dyn crate::lamparray::LampArrayHandle>> {
        let info = self
            .lock()
            .devices
            .iter()
            .find(|d| d.id == id)
            .cloned()
            .ok_or_else(|| HalError::NotFound(id.into()))?;
        Ok(Box::new(MockLampHandle {
            state: Arc::clone(&self.state),
            info,
        }))
    }

    fn generation(&self) -> u64 {
        self.lock().generation
    }

    fn has_identity(&self) -> bool {
        self.lock().identity
    }

    fn container_of(&self, interface_path: &str) -> Option<String> {
        self.lock()
            .containers
            .get(&interface_path.to_ascii_lowercase())
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_lamparray_records_and_follows_plugs() {
        use crate::lamparray::LampArrayBackend;
        let m = MockLampArray::new();
        assert_eq!(m.generation(), 0);
        m.add(MockLampArray::row("a", 1, 2, 3));
        assert_eq!(m.generation(), 1);
        let mut h = m.open("a").unwrap();
        assert!(h.available());
        h.set_colors(&[[1, 2, 3, 255]; 3]).unwrap();
        assert!(h.set_colors(&[[0; 4]; 2]).is_err());
        m.set_available("a", false);
        assert!(!h.available());
        m.remove("a");
        assert!(matches!(h.set_colors(&[[0; 4]; 3]), Err(HalError::Disconnected)));
        h.release().unwrap();
        assert_eq!((m.writes("a").len(), m.releases("a")), (1, 1));
        assert!(m.open("a").is_err());
    }

    #[test]
    fn ddr4_spd_pages_switch_on_the_wire() {
        let rec = TransactionRecorder::new();
        let mut bus = MockSmbus::new("AMD PIIX4 Bus - 0x0b00", SmbusBusKind::Amd, rec.clone());
        let mut spd = vec![0u8; 512];
        spd[2] = 0x0C;
        spd[320] = 0x85;
        spd[321] = 0x9B;
        bus.add_ddr4_spd(0x51, &spd);
        assert_eq!(bus.read_byte(0x51, 2).unwrap(), 0x0C);
        let mut id = [0u8; 2];
        assert_eq!(bus.spd_read(0x51, 320, &mut id).unwrap(), 2);
        assert_eq!(id, [0x85, 0x9B]);
        assert_eq!(bus.spd_page(), 0, "page 0 again");
        assert_eq!(
            rec.payloads(TransactionKind::SmbusWrite),
            vec![vec![0x37, 0x00], vec![0x36, 0x00]]
        );
        // Page 1 left selected by someone else: plain reads see it.
        bus.set_spd_page(1);
        assert_eq!(bus.read_byte(0x51, 0x40).unwrap(), 0x85);
        let mut ty = [0u8; 1];
        bus.spd_read(0x51, 2, &mut ty).unwrap();
        assert_eq!((ty[0], bus.spd_page()), (0x0C, 0));
        assert!(bus.spd_read(0x51, 250, &mut [0u8; 8]).is_err(), "within one page");
        let mut gpu = MockSmbus::new("GPU", SmbusBusKind::Nvidia, rec);
        assert!(matches!(
            gpu.spd_read(0x50, 0, &mut ty),
            Err(HalError::Unsupported(_))
        ));
    }

    #[test]
    fn unplugged_devices_vanish_and_their_old_handles_stay_stale() {
        let info = MockHid::endpoint_info(1, 2, "U", 0, 0xFF00, 1, 0, ReportLengths::default());
        let mock = MockHid::new().add_endpoint(info.clone());
        let mut old = mock.open(&info.path).unwrap();
        assert_eq!(old.write(&[0, 1]).unwrap(), 2);
        mock.unplug(&info.path).unwrap();
        assert!(mock.enumerate().unwrap().is_empty());
        assert!(matches!(old.write(&[0, 1]), Err(HalError::Disconnected)));
        assert!(mock.open(&info.path).is_err());
        mock.replug(&info.path).unwrap();
        assert_eq!(mock.enumerate().unwrap().len(), 1);
        assert!(matches!(old.write(&[0, 1]), Err(HalError::Disconnected)), "stale");
        let mut fresh = mock.open(&info.path).unwrap();
        assert_eq!(fresh.write(&[0, 2]).unwrap(), 2);
    }

    fn backend() -> MockHid {
        MockHid::new().add_endpoint(MockHid::endpoint_info(
            0x1462,
            0x7C91,
            "",
            HidEndpointInfo::UNKNOWN_INTERFACE,
            0xFF00,
            0x01,
            0,
            ReportLengths {
                input: 65,
                output: 65,
                feature: 185,
            },
        ))
    }

    #[test]
    fn golden_recording_of_a_feature_report() {
        let hid = backend();
        let rec = hid.recorder();
        let path = hid.enumerate().unwrap()[0].path.clone();
        assert_eq!(path, "mock://1462:7c91//mi-1/col0");

        let mut h = hid.open(&path).unwrap();
        let mut packet = vec![0u8; 185];
        packet[0] = 0x52;
        packet[1] = 0xFF;
        assert_eq!(h.send_feature_report(&packet).unwrap(), 185);

        let mut back = vec![0u8; 185];
        back[0] = 0x52;
        assert_eq!(h.get_feature_report(&mut back).unwrap(), 185);
        assert_eq!(back, packet);
        drop(h);

        let log = rec.snapshot();
        let kinds: Vec<_> = log.iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            vec![
                TransactionKind::Open,
                TransactionKind::SendFeature,
                TransactionKind::GetFeature,
                TransactionKind::Close
            ]
        );
        assert_eq!(rec.feature_sends(), vec![packet]);
        assert!(log.windows(2).all(|w| w[0].ts <= w[1].ts));
    }

    #[test]
    fn scripted_reads_then_timeout() {
        let hid = backend();
        let path = hid.enumerate().unwrap()[0].path.clone();
        hid.script_read(&path, vec![0x00, 0x21, 1, 2, 3]).unwrap();
        let mut h = hid.open(&path).unwrap();
        let mut buf = [0u8; 32];
        assert_eq!(h.read_timeout(&mut buf, 10).unwrap(), 5);
        assert_eq!(&buf[..5], &[0x00, 0x21, 1, 2, 3]);
        assert_eq!(h.read_timeout(&mut buf, 10).unwrap(), 0);
        assert_eq!(hid.recorder().payloads(TransactionKind::Read).len(), 1);
    }

    #[test]
    fn write_failure_and_missing_feature() {
        let hid = backend();
        let path = hid.enumerate().unwrap()[0].path.clone();
        lock(&hid.endpoint(&path).unwrap()).fail_writes = true;
        let mut h = hid.open(&path).unwrap();
        let err = h.write(&[0, 1, 2]).unwrap_err();
        assert_eq!(err.plugin_status(), -1);
        let mut buf = [0x07u8; 8];
        assert!(h.get_feature_report(&mut buf).is_err());
        assert!(hid.open("mock://nope").is_err());
    }

    #[test]
    fn smbus_register_map() {
        let rec = TransactionRecorder::new();
        let mut bus = MockSmbus::new("AMD PIIX4 Bus - 0x0b00", SmbusBusKind::Amd, rec.clone());
        bus.add_device(0x39, [(0xA0, 0x00), (0xA1, 0x01)]);
        assert!(bus.write_quick(0x39).is_ok());
        assert_eq!(bus.write_quick(0x3A).unwrap_err().plugin_status(), -1);
        assert_eq!(bus.read_byte(0x39, 0xA1).unwrap(), 0x01);
        bus.write_word(0x39, 0x00, 0x80F8).unwrap();
        assert_eq!(bus.read_word(0x39, 0x00).unwrap(), 0x80F8);
        bus.write_block(0x39, 0x10, &[1, 2, 3]).unwrap();
        let mut back = [0u8; 3];
        assert_eq!(bus.read_block(0x39, 0x10, &mut back).unwrap(), 3);
        assert_eq!(back, [1, 2, 3]);
        assert_eq!(
            bus.read_byte_no_reg(0x39).unwrap(),
            0xF8,
            "receive byte on PIIX4 (register 0)"
        );
        assert!(bus.read_byte_no_reg(0x3A).is_err(), "no device");
        assert_eq!(
            rec.payloads(TransactionKind::SmbusWriteQuick),
            vec![vec![0x39], vec![0x3A]]
        );
        assert!(
            rec.payloads(TransactionKind::SmbusWrite)
                .contains(&vec![0x39, 0x10, 3, 1, 2, 3]),
            "block writes carry their count byte"
        );
        let nuvoton = &mut MockSmbus::new("Nuvoton", SmbusBusKind::Nuvoton, rec.clone());
        nuvoton.add_device(0x39, []);
        assert!(nuvoton.read_byte_no_reg(0x39).is_err());
    }

    #[test]
    fn indexed_device_reads_and_writes_through_the_pointer() {
        let rec = TransactionRecorder::new();
        let mut bus = MockSmbus::new("AMD PIIX4 Bus - 0x0b00", SmbusBusKind::Amd, rec.clone());
        bus.add_indexed_device(0x39, [(0xA0, 0x00), (0xA1, 0x01)], [(0x1025, b'M')]);
        assert_eq!(bus.read_byte_no_reg(0x39).unwrap(), 0xFF);
        assert_eq!(bus.read_byte(0x39, 0xA1).unwrap(), 0x01);
        // Pointer 0x1025 goes out byte-swapped: word 0x2510.
        bus.write_word(0x39, 0x00, 0x2510).unwrap();
        assert_eq!(bus.read_byte(0x39, 0x81).unwrap(), b'M');
        bus.write_word(0x39, 0x00, 0x0083).unwrap();
        bus.write_block(0x39, 0x03, &[1, 2, 3]).unwrap();
        assert_eq!(bus.indexed_register(0x39, 0x8302), Some(3));
        bus.write_word(0x39, 0x00, 0xEE82).unwrap();
        bus.write_byte(0x39, 0x01, 0x07).unwrap();
        assert_eq!(bus.indexed_register(0x39, 0x82EE), Some(0x07));
        assert!(bus.write_quick(0x39).is_ok());
        assert!(bus.read_byte(0x3A, 0).is_err(), "nothing else answers");
        bus.bus_lock(100).unwrap();
        bus.bus_unlock().unwrap();
        assert_eq!(
            rec.payloads(TransactionKind::SmbusLock),
            vec![vec![100, 0], vec![]]
        );
    }

    #[test]
    fn block_write_failure_is_scriptable() {
        let rec = TransactionRecorder::new();
        let mut bus = MockSmbus::new("AMD PIIX4 Bus - 0x0b00", SmbusBusKind::Amd, rec.clone())
            .with_instance("0b00")
            .with_pci(PciIds {
                vendor: 0x1022,
                device: 0x790B,
                sub_vendor: 0x1022,
                sub_device: 0x790B,
            })
            .fail_block_writes(0x39);
        bus.add_device(0x39, []);
        assert_eq!(bus.descriptor().key(), "1022:790b:1022:790b@0b00");
        let err = bus.write_block(0x39, 0x03, &[1; 8]).unwrap_err();
        assert_eq!(err.plugin_status(), -1);
        assert!(bus.write_byte(0x39, 0x01, 7).is_ok(), "byte writes still work");
        assert_eq!(
            rec.payloads(TransactionKind::SmbusWrite),
            vec![vec![0x39, 0x01, 7]]
        );
    }

    fn usb() -> (MockUsb, String) {
        let info = MockUsb::device_info(
            0x0CF2,
            0xA103,
            "S1",
            vec![
                MockUsb::foreign_interface(0, 3, "HidUsb", &[0x81]),
                MockUsb::winusb_interface(1, &[0x02, 0x82]),
            ],
        );
        let path = info.path.clone();
        (MockUsb::new().add_device(info), path)
    }

    #[test]
    fn usb_bulk_and_control_transfers_are_recorded() {
        let (mock, path) = usb();
        let rec = mock.recorder();
        mock.script_bulk_in(&path, 0x82, vec![9, 8, 7]).unwrap();
        mock.script_control_in(&path, vec![0x12, 0x34]).unwrap();
        let mut h = mock.open(&path).unwrap();
        assert_eq!(h.bulk_transfer(0x02, &mut [1, 2, 3], 100).unwrap(), 3);
        let mut buf = [0u8; 64];
        assert_eq!(h.bulk_transfer(0x82, &mut buf, 100).unwrap(), 3);
        assert_eq!(buf[..3], [9, 8, 7]);
        assert_eq!(
            h.control_transfer(0x40, 0x80, 0, 0x0010, &mut [0xAA], 1000)
                .unwrap(),
            1
        );
        let mut buf = [0u8; 4];
        assert_eq!(
            h.control_transfer(0xC0, 0x81, 0, 0xD800, &mut buf, 1000).unwrap(),
            2
        );
        drop(h);
        let kinds: Vec<TransactionKind> = rec.snapshot().iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            vec![
                TransactionKind::Open,
                TransactionKind::UsbClaim,
                TransactionKind::BulkOut,
                TransactionKind::BulkIn,
                TransactionKind::ControlOut,
                TransactionKind::ControlIn,
                TransactionKind::Close,
            ]
        );
        assert_eq!(rec.payloads(TransactionKind::UsbClaim), vec![vec![1]]);
        assert_eq!(rec.payloads(TransactionKind::BulkOut), vec![vec![0x02, 1, 2, 3]]);
        assert_eq!(rec.payloads(TransactionKind::BulkIn), vec![vec![0x82, 9, 8, 7]]);
        assert_eq!(
            rec.payloads(TransactionKind::ControlOut),
            vec![vec![0x40, 0x80, 0, 0, 0x10, 0, 1, 0, 0xAA]]
        );
        assert_eq!(
            rec.payloads(TransactionKind::ControlIn),
            vec![vec![0xC0, 0x81, 0, 0, 0x00, 0xD8, 4, 0, 0x12, 0x34]]
        );
    }

    #[test]
    fn usb_in_without_data_times_out_and_foreign_interfaces_stay_unclaimed() {
        let (mock, path) = usb();
        let mut h = mock.open(&path).unwrap();
        let mut buf = [0u8; 8];
        assert!(matches!(
            h.bulk_transfer(0x82, &mut buf, 250),
            Err(HalError::Timeout(250))
        ));
        assert!(matches!(
            h.control_transfer(0xC0, 1, 0, 0, &mut buf, 30),
            Err(HalError::Timeout(30))
        ));
        // The HID interface's endpoint and a class request to it: refused, never claimed.
        assert!(matches!(
            h.bulk_transfer(0x81, &mut buf, 10),
            Err(HalError::NeedsDriver(_))
        ));
        assert!(matches!(
            h.control_transfer(0x21, 9, 0x0200, 0, &mut buf, 10),
            Err(HalError::NeedsDriver(_))
        ));
        assert!(matches!(h.claim_interface(0), Err(HalError::NeedsDriver(_))));
        assert_eq!(mock.recorder().payloads(TransactionKind::UsbClaim), vec![vec![1]]);
        lock_usb(&mock.device(&path).unwrap()).out_timeout = true;
        assert!(matches!(
            h.bulk_transfer(0x02, &mut [1], 40),
            Err(HalError::Timeout(40))
        ));
    }

    #[test]
    fn unplugged_usb_devices_vanish_and_their_handles_go_stale() {
        let (mock, path) = usb();
        let mut old = mock.open(&path).unwrap();
        mock.unplug(&path).unwrap();
        assert!(mock.enumerate().unwrap().is_empty());
        assert!(matches!(mock.open(&path), Err(HalError::NotFound(_))));
        assert!(matches!(
            old.bulk_transfer(0x02, &mut [1], 10),
            Err(HalError::Disconnected)
        ));
        mock.replug(&path).unwrap();
        assert!(matches!(
            old.bulk_transfer(0x02, &mut [1], 10),
            Err(HalError::Disconnected)
        ));
        assert_eq!(
            mock.open(&path)
                .unwrap()
                .bulk_transfer(0x02, &mut [1], 10)
                .unwrap(),
            1
        );
        assert_eq!(
            mock.enumerate().unwrap()[0].instance_id,
            "USB\\VID_0CF2&PID_A103\\S1"
        );
    }
}
