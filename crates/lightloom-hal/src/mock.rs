// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Scripted in-memory backends and the transaction recorder used by golden
//! tests, the plugin simulator and the conformance record/replay harness.
//!
//! * [`MockHid`] exposes a list of scripted [`HidEndpointInfo`]s; each opened
//!   handle returns pre-scripted reads/feature reports and records every
//!   operation into a shared [`TransactionRecorder`].
//! * [`MockSmbus`] is a register-map SMBus bus that records transactions too.
//!
//! Golden tests compare `recorder.snapshot()` (or just `recorder.writes()`)
//! against checked-in fixtures.

use crate::error::{HalError, HalResult};
use crate::hid::{HidBackend, HidEndpointInfo, HidHandle, ReportLengths};
use crate::smbus::{validate_address, SmbusBus, SmbusBusDescriptor, SmbusBusKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
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
    /// SMBus write (`bytes` = `[addr, reg, data…]`; `reg` omitted for no-reg ops).
    SmbusWrite,
    /// SMBus read (`bytes` = `[addr, reg, data…]`).
    SmbusRead,
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
        Self { inner: Arc::new(Mutex::new(Vec::new())), epoch: Instant::now() }
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Transaction>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Append one transaction.
    pub fn record(&self, kind: TransactionKind, target: impl Into<String>, bytes: impl Into<Vec<u8>>) {
        let ts = self.epoch.elapsed();
        self.lock().push(Transaction { kind, target: target.into(), bytes: bytes.into(), ts });
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
        self.lock().iter().filter(|t| t.kind == kind).map(|t| t.bytes.clone()).collect()
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
        Self { endpoints: Vec::new(), recorder }
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
            manufacturer: "Lightloom Mock".into(),
            product: format!("Mock {vid:04x}:{pid:04x}"),
            path: format!("mock://{vid:04x}:{pid:04x}/{serial}/mi{interface_number}/col{collection}"),
            interface_number,
            usage_page,
            usage,
            collection,
            report_lengths,
        }
    }

    /// Add an endpoint (builder style).
    pub fn add_endpoint(mut self, info: HidEndpointInfo) -> Self {
        self.push_endpoint(info);
        self
    }

    /// Add an endpoint in place.
    pub fn push_endpoint(&mut self, info: HidEndpointInfo) {
        self.endpoints.push(Arc::new(Mutex::new(MockEndpointState { info, ..Default::default() })));
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
        let ep = self.endpoint(path).ok_or_else(|| HalError::NotFound(path.to_string()))?;
        lock(&ep).scripted_reads.push_back(report.into());
        Ok(())
    }

    /// Queue a report for the next `get_input_report` on `path`.
    pub fn script_input_report(&self, path: &str, report: impl Into<Vec<u8>>) -> HalResult<()> {
        let ep = self.endpoint(path).ok_or_else(|| HalError::NotFound(path.to_string()))?;
        lock(&ep).scripted_input_reports.push_back(report.into());
        Ok(())
    }

    /// Preload the feature report for id `report[0]` on `path`.
    pub fn set_feature_report(&self, path: &str, report: impl Into<Vec<u8>>) -> HalResult<()> {
        let report = report.into();
        let id = *report.first().ok_or_else(|| HalError::InvalidArgument("empty feature report".into()))?;
        let ep = self.endpoint(path).ok_or_else(|| HalError::NotFound(path.to_string()))?;
        lock(&ep).feature_reports.insert(id, report);
        Ok(())
    }
}

fn lock(ep: &SharedEndpoint) -> MutexGuard<'_, MockEndpointState> {
    ep.lock().unwrap_or_else(|e| e.into_inner())
}

impl HidBackend for MockHid {
    fn enumerate(&self) -> HalResult<Vec<HidEndpointInfo>> {
        Ok(self.endpoints.iter().map(|e| lock(e).info.clone()).collect())
    }

    fn open(&self, path: &str) -> HalResult<Box<dyn HidHandle>> {
        let state = self.endpoint(path).ok_or_else(|| HalError::NotFound(path.to_string()))?;
        let info = {
            let mut s = lock(&state);
            s.open_count += 1;
            s.info.clone()
        };
        self.recorder.record(TransactionKind::Open, path, Vec::new());
        Ok(Box::new(MockHidHandle { state, recorder: self.recorder.clone(), info }))
    }
}

/// Handle returned by [`MockHid::open`].
#[derive(Debug)]
pub struct MockHidHandle {
    state: SharedEndpoint,
    recorder: TransactionRecorder,
    info: HidEndpointInfo,
}

impl MockHidHandle {
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
        if lock(&self.state).fail_writes {
            return Err(HalError::Backend("mock: write failure scripted".into()));
        }
        self.recorder.record(TransactionKind::Write, &*self.info.path, data);
        Ok(data.len())
    }

    fn read_timeout(&mut self, buf: &mut [u8], _timeout_ms: i32) -> HalResult<usize> {
        let next = lock(&self.state).scripted_reads.pop_front();
        match next {
            Some(report) => {
                let n = Self::copy_out(&report, buf);
                self.recorder.record(TransactionKind::Read, &*self.info.path, &buf[..n]);
                Ok(n)
            }
            None => Ok(0),
        }
    }

    fn send_feature_report(&mut self, data: &[u8]) -> HalResult<usize> {
        let id = *data.first().ok_or_else(|| HalError::InvalidArgument("empty feature report".into()))?;
        lock(&self.state).feature_reports.insert(id, data.to_vec());
        self.recorder.record(TransactionKind::SendFeature, &*self.info.path, data);
        Ok(data.len())
    }

    fn get_feature_report(&mut self, buf: &mut [u8]) -> HalResult<usize> {
        let id = *buf.first().ok_or_else(|| HalError::InvalidArgument("empty buffer".into()))?;
        let stored = lock(&self.state).feature_reports.get(&id).cloned();
        let report = stored.ok_or_else(|| HalError::Backend(format!("mock: no feature report {id:#04x}")))?;
        let n = Self::copy_out(&report, buf);
        self.recorder.record(TransactionKind::GetFeature, &*self.info.path, &buf[..n]);
        Ok(n)
    }

    fn get_input_report(&mut self, buf: &mut [u8]) -> HalResult<usize> {
        let next = lock(&self.state).scripted_input_reports.pop_front();
        let report = next.ok_or_else(|| HalError::Backend("mock: no input report scripted".into()))?;
        let n = Self::copy_out(&report, buf);
        self.recorder.record(TransactionKind::GetInputReport, &*self.info.path, &buf[..n]);
        Ok(n)
    }
}

impl Drop for MockHidHandle {
    fn drop(&mut self) {
        self.recorder.record(TransactionKind::Close, &*self.info.path, Vec::new());
    }
}

/// Register-map SMBus bus. Every `(addr, reg)` cell is a byte; devices exist
/// only at the addresses given to [`MockSmbus::add_device`].
#[derive(Debug)]
pub struct MockSmbus {
    descriptor: SmbusBusDescriptor,
    devices: BTreeMap<u8, BTreeMap<u8, u8>>,
    recorder: TransactionRecorder,
}

impl MockSmbus {
    /// New bus of `kind` named `name`, logging into `recorder`.
    pub fn new(name: &str, kind: SmbusBusKind, recorder: TransactionRecorder) -> Self {
        Self {
            descriptor: SmbusBusDescriptor {
                name: name.to_string(),
                kind,
                pci: Default::default(),
                max_block_len: crate::smbus::SMBUS_BLOCK_MAX,
            },
            devices: BTreeMap::new(),
            recorder,
        }
    }

    /// Make a device answer at `addr` with the given initial registers.
    pub fn add_device(&mut self, addr: u8, registers: impl IntoIterator<Item = (u8, u8)>) {
        self.devices.insert(addr, registers.into_iter().collect());
    }

    /// Current register map of the device at `addr`.
    pub fn registers(&self, addr: u8) -> Option<&BTreeMap<u8, u8>> {
        self.devices.get(&addr)
    }

    fn device(&mut self, addr: u8) -> HalResult<&mut BTreeMap<u8, u8>> {
        validate_address(addr)?;
        self.devices.get_mut(&addr).ok_or(HalError::Bus { status: -1 })
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
        self.device(addr).map(|_| ())
    }

    fn read_byte(&mut self, addr: u8, reg: u8) -> HalResult<u8> {
        let v = *self.device(addr)?.get(&reg).unwrap_or(&0xFF);
        self.rec(TransactionKind::SmbusRead, vec![addr, reg, v]);
        Ok(v)
    }

    fn write_byte(&mut self, addr: u8, reg: u8, value: u8) -> HalResult<()> {
        self.device(addr)?.insert(reg, value);
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
        let dev = self.device(addr)?;
        dev.insert(reg, lo);
        dev.insert(reg.wrapping_add(1), hi);
        self.rec(TransactionKind::SmbusWrite, vec![addr, reg, lo, hi]);
        Ok(())
    }

    fn write_block(&mut self, addr: u8, reg: u8, data: &[u8]) -> HalResult<()> {
        if data.len() > self.descriptor.max_block_len {
            return Err(HalError::InvalidArgument(format!("block of {} > max {}", data.len(), self.descriptor.max_block_len)));
        }
        let dev = self.device(addr)?;
        for (i, b) in data.iter().enumerate() {
            dev.insert(reg.wrapping_add(i as u8), *b);
        }
        let mut bytes = vec![addr, reg];
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
            return Err(HalError::Unsupported("ReadByteWithoutRegister outside NvAPI"));
        }
        let v = *self.device(addr)?.get(&0).unwrap_or(&0xFF);
        self.rec(TransactionKind::SmbusRead, vec![addr, v]);
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend() -> MockHid {
        MockHid::new().add_endpoint(MockHid::endpoint_info(
            0x1462,
            0x7C91,
            "",
            HidEndpointInfo::UNKNOWN_INTERFACE,
            0xFF00,
            0x01,
            0,
            ReportLengths { input: 65, output: 65, feature: 185 },
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
        assert!(bus.read_byte_no_reg(0x39).is_err());
        assert_eq!(rec.payloads(TransactionKind::SmbusWriteQuick), vec![vec![0x39], vec![0x3A]]);
    }
}
