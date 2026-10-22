// SPDX-License-Identifier: GPL-3.0-or-later
//! [`NvapiBus`]: port 1 of one NVIDIA GPU as a [`SmbusBus`].
//!
//! Every operation becomes one [`I2cPlan`] sent to the hub's worker thread,
//! addressed to the card by identity. Errors keep NvAPI's own status
//! (`HalError::Bus { status }`), which the plugin host hands to plugins
//! unchanged on NVIDIA buses (research §6.3). `TransactionLock` needs nothing
//! here: the plugin host's per-bus turn queue keeps other sessions off the
//! bus, and no other program shares NvAPI's I2C controller under a mutex.

use crate::hub::{GpuInfo, Job, Request, REPLY_GRACE, REPLY_TIMEOUT};
use crate::info::{I2cOp, I2cPlan};
use crate::NvapiError;
use huedeck_hal::smbus::{validate_address, SmbusBus, SmbusBusDescriptor};
use huedeck_hal::{HalError, HalResult};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Instant;

/// One GPU's RGB I2C port.
#[derive(Debug)]
pub struct NvapiBus {
    descriptor: SmbusBusDescriptor,
    gpu: GpuInfo,
    tx: Sender<Request>,
}

impl From<NvapiError> for HalError {
    fn from(e: NvapiError) -> Self {
        match e {
            NvapiError::Status(status) => HalError::Bus { status },
            NvapiError::Unsupported(what) => HalError::Unsupported(what),
            NvapiError::Length(n) => HalError::InvalidArgument(format!("transfer of {n} bytes")),
            NvapiError::Timeout(ms) => HalError::Timeout(ms),
            other => HalError::Backend(other.to_string()),
        }
    }
}

impl NvapiBus {
    pub(crate) fn new(descriptor: SmbusBusDescriptor, gpu: GpuInfo, tx: Sender<Request>) -> Self {
        Self { descriptor, gpu, tx }
    }

    /// Send `job` to the worker and wait for its answer. The worker drops it
    /// unsent once [`REPLY_TIMEOUT`] has passed; the wait lasts a little
    /// longer ([`REPLY_GRACE`]) so a job started just before still reports.
    fn ask(&self, job: Job) -> HalResult<Vec<u8>> {
        let (reply, answer) = mpsc::channel();
        self.tx
            .send(Request {
                job,
                deadline: Instant::now() + REPLY_TIMEOUT,
                reply,
            })
            .map_err(|_| HalError::from(NvapiError::WorkerGone))?;
        match answer.recv_timeout(REPLY_TIMEOUT + REPLY_GRACE) {
            Ok(r) => Ok(r?),
            Err(RecvTimeoutError::Timeout) => {
                Err(NvapiError::Timeout(REPLY_TIMEOUT.as_millis() as u32).into())
            }
            Err(RecvTimeoutError::Disconnected) => Err(NvapiError::WorkerGone.into()),
        }
    }

    /// Run `op` on the worker; the bytes read (empty for writes).
    fn run(&self, addr: u8, op: I2cOp) -> HalResult<Vec<u8>> {
        validate_address(addr)?;
        let plan = I2cPlan::of(&op)?;
        self.ask(Job::Transfer {
            gpu: self.gpu.clone(),
            addr,
            plan,
        })
    }

    fn byte(&self, addr: u8, op: I2cOp) -> HalResult<u8> {
        self.run(addr, op)?
            .first()
            .copied()
            .ok_or(HalError::Bus { status: -1 })
    }
}

impl SmbusBus for NvapiBus {
    fn descriptor(&self) -> &SmbusBusDescriptor {
        &self.descriptor
    }

    fn write_quick(&mut self, _addr: u8) -> HalResult<()> {
        Err(HalError::Unsupported("WriteQuick on NvAPI bus"))
    }

    fn read_byte(&mut self, addr: u8, reg: u8) -> HalResult<u8> {
        self.byte(addr, I2cOp::ReadByte { reg })
    }

    fn write_byte(&mut self, addr: u8, reg: u8, value: u8) -> HalResult<()> {
        self.run(addr, I2cOp::WriteByte { reg, val: value }).map(drop)
    }

    fn read_word(&mut self, addr: u8, reg: u8) -> HalResult<u16> {
        let b = self.run(addr, I2cOp::ReadWord { reg })?;
        match b.as_slice() {
            [lo, hi] => Ok(u16::from_le_bytes([*lo, *hi])),
            _ => Err(HalError::Bus { status: -1 }),
        }
    }

    fn write_word(&mut self, addr: u8, reg: u8, value: u16) -> HalResult<()> {
        self.run(addr, I2cOp::WriteWord { reg, val: value }).map(drop)
    }

    fn write_block(&mut self, addr: u8, reg: u8, data: &[u8]) -> HalResult<()> {
        self.run(
            addr,
            I2cOp::WriteBlock {
                reg,
                data: data.to_vec(),
            },
        )
        .map(drop)
    }

    fn read_block(&mut self, addr: u8, reg: u8, buf: &mut [u8]) -> HalResult<usize> {
        let b = self.run(addr, I2cOp::ReadBlock { reg, len: buf.len() })?;
        let n = b.len().min(buf.len());
        buf[..n].copy_from_slice(&b[..n]);
        Ok(n)
    }

    fn write_block_no_reg(&mut self, addr: u8, data: &[u8]) -> HalResult<()> {
        self.run(addr, I2cOp::WriteNoReg { data: data.to_vec() })
            .map(drop)
    }

    fn read_block_no_reg(&mut self, addr: u8, buf: &mut [u8]) -> HalResult<usize> {
        let b = self.run(addr, I2cOp::ReadNoReg { len: buf.len() })?;
        let n = b.len().min(buf.len());
        buf[..n].copy_from_slice(&b[..n]);
        Ok(n)
    }

    fn read_byte_no_reg(&mut self, addr: u8) -> HalResult<u8> {
        self.byte(addr, I2cOp::ReadByteNoReg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::{FakeNvI2c, GpuInfo, NvapiHub, GPU_GONE};
    use huedeck_hal::smbus::{PciIds, SmbusBusKind};

    fn rtx3060() -> GpuInfo {
        GpuInfo {
            name: "NVIDIA GeForce RTX 3060".into(),
            pci: PciIds {
                vendor: 0x10DE,
                device: 0x2504,
                sub_vendor: 0x1462,
                sub_device: 0x3903,
            },
            bus: 1,
        }
    }

    fn hub(fake: FakeNvI2c) -> NvapiHub {
        NvapiHub::with_transport(fake).unwrap()
    }

    #[test]
    fn bus_descriptor_carries_pci_tuple_and_name() {
        let h = hub(FakeNvI2c::new(vec![rtx3060()]));
        let buses = h.buses();
        assert_eq!(buses.len(), 1);
        let d = buses[0].descriptor();
        assert_eq!(d.name, "NVIDIA GeForce RTX 3060");
        assert_eq!(d.kind, SmbusBusKind::Nvidia);
        assert_eq!(d.key(), "10de:2504:1462:3903@pci01");
        assert_eq!(d.max_block_len, 32);
    }

    #[test]
    fn read_byte_returns_register() {
        let h = hub(FakeNvI2c::new(vec![rtx3060()]).add_device(0, 0x68, [(0x22, 0x13)]));
        let mut bus = h.buses().remove(0);
        assert_eq!(bus.read_byte(0x68, 0x22).unwrap(), 0x13);
        assert_eq!(bus.read_byte(0x68, 0x30).unwrap(), 0xFF);
        assert!(matches!(bus.write_quick(0x68), Err(HalError::Unsupported(_))));
        assert!(bus.read_byte(0x02, 0).is_err(), "reserved address");
    }

    #[test]
    fn write_byte_sends_one_data_byte_at_register() {
        let fake = FakeNvI2c::new(vec![rtx3060()]).add_device(0, 0x68, []);
        let log = fake.log();
        let h = hub(fake);
        let mut bus = h.buses().remove(0);
        bus.write_byte(0x68, 0x30, 0x12).unwrap();
        bus.write_block(0x68, 0x03, &[1, 2]).unwrap();
        let seen = log.lock().unwrap().clone();
        assert_eq!(
            seen[0].plan,
            I2cPlan {
                write: true,
                reg: Some(0x30),
                data: vec![0x12]
            }
        );
        assert_eq!((seen[0].gpu, seen[0].addr), (0, 0x68));
        assert_eq!(seen[1].plan.data, vec![2, 1, 2], "block write count byte");
        assert_eq!(bus.read_byte(0x68, 0x30).unwrap(), 0x12);
    }

    #[test]
    fn nvapi_status_passes_through_as_negative() {
        let h = hub(FakeNvI2c::new(vec![rtx3060()]).fail_with(-105));
        let mut bus = h.buses().remove(0);
        let err = bus.write_byte(0x68, 0x30, 1).unwrap_err();
        assert_eq!(err.plugin_status(), -105);
        let h = hub(FakeNvI2c::new(vec![rtx3060()]));
        let mut bus = h.buses().remove(0);
        assert_eq!(
            bus.read_byte(0x68, 0).unwrap_err().plugin_status(),
            -1,
            "nobody at 0x68"
        );
    }

    fn on_bus(bus: u32) -> GpuInfo {
        GpuInfo { bus, ..rtx3060() }
    }

    #[test]
    fn transfers_follow_the_card_not_its_position() {
        // Enumerated as [bus 2, bus 1]: the bus of the card on PCI bus 1 still reaches it.
        let fake = FakeNvI2c::new(vec![on_bus(2), on_bus(1)])
            .add_device(0, 0x68, [(0x22, 0x02)])
            .add_device(1, 0x68, [(0x22, 0x01)]);
        let h = hub(fake);
        let mut first = h.bus_for(on_bus(1));
        assert_eq!(first.read_byte(0x68, 0x22).unwrap(), 0x01);
        let mut gone = h.bus_for(on_bus(7));
        assert_eq!(
            gone.write_byte(0x68, 0x30, 1).unwrap_err().plugin_status(),
            GPU_GONE,
            "a card no longer enumerated is never written"
        );
    }

    #[test]
    fn no_cross_process_lock_is_taken() {
        // TransactionLock asks nothing of the worker: the GPU never waits for
        // the chipset SMBus mutex (RAM traffic), as in OpenRGB and SignalRGB.
        let fake = FakeNvI2c::new(vec![rtx3060()]).add_device(0, 0x68, []);
        let log = fake.log();
        let h = hub(fake);
        let mut bus = h.buses().remove(0);
        bus.bus_lock(100).unwrap();
        bus.write_byte(0x68, 0x30, 3).unwrap();
        bus.bus_unlock().unwrap();
        assert_eq!(
            log.lock().unwrap().len(),
            1,
            "only the transfer reached the worker"
        );
    }

    #[test]
    fn a_request_whose_caller_gave_up_is_never_sent() {
        let fake = FakeNvI2c::new(vec![rtx3060()]).add_device(0, 0x68, []);
        let log = fake.log();
        let h = hub(fake);
        let bus = h.buses().remove(0);
        let (reply, answer) = mpsc::channel();
        bus.tx
            .send(Request {
                job: Job::Transfer {
                    gpu: rtx3060(),
                    addr: 0x68,
                    plan: I2cPlan::of(&I2cOp::WriteByte { reg: 0x30, val: 9 }).unwrap(),
                },
                deadline: Instant::now() - std::time::Duration::from_millis(1),
                reply,
            })
            .unwrap();
        assert!(matches!(answer.recv().unwrap(), Err(NvapiError::Timeout(_))));
        assert!(log.lock().unwrap().is_empty());
    }

    #[test]
    fn hub_serialises_calls_on_one_thread() {
        let fake = FakeNvI2c::new(vec![on_bus(1), on_bus(2)])
            .add_device(0, 0x68, [])
            .add_device(1, 0x68, []);
        let log = fake.log();
        let h = hub(fake);
        let buses = h.buses();
        assert_eq!(buses.len(), 2);
        std::thread::scope(|s| {
            for (i, mut bus) in buses.into_iter().enumerate() {
                s.spawn(move || {
                    for v in 0..20u8 {
                        bus.write_byte(0x68, 0x30, v).unwrap();
                        assert_eq!(bus.read_byte(0x68, 0x30).unwrap(), v, "bus {i}");
                    }
                });
            }
        });
        let seen = log.lock().unwrap();
        assert_eq!(seen.len(), 80);
        assert!(seen.iter().all(|t| t.thread == "huedeck-nvapi"));
    }
}
