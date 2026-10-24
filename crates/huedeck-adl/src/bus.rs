// SPDX-License-Identifier: GPL-3.0-or-later
//! [`AdlBus`]: I2C line 1 of one AMD GPU as a [`SmbusBus`] of kind
//! [`huedeck_hal::SmbusBusKind::AmdGpu`].
//!
//! Every operation becomes one [`I2cPlan`] sent to the hub's worker thread,
//! addressed to the card by identity. Errors keep ADL's own status
//! (`HalError::Bus { status }`); the plugin host shows plugins `-1` on AMD GPU
//! buses, as on chipset buses. `TransactionLock` needs nothing here: the
//! plugin host's per-bus turn queue keeps other sessions off the bus.

use crate::hub::{GpuInfo, Job, Request, REPLY_GRACE, REPLY_TIMEOUT};
use crate::info::{I2cOp, I2cPlan};
use crate::AdlError;
use huedeck_hal::smbus::{validate_address, SmbusBus, SmbusBusDescriptor};
use huedeck_hal::{HalError, HalResult};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Instant;

/// One AMD GPU's RGB I2C line.
#[derive(Debug)]
pub struct AdlBus {
    descriptor: SmbusBusDescriptor,
    gpu: GpuInfo,
    tx: Sender<Request>,
}

impl From<AdlError> for HalError {
    fn from(e: AdlError) -> Self {
        match e {
            AdlError::Status(status) => HalError::Bus { status },
            AdlError::Unsupported(what) => HalError::Unsupported(what),
            AdlError::Length(n) => HalError::InvalidArgument(format!("transfer of {n} bytes")),
            AdlError::Timeout(ms) => HalError::Timeout(ms),
            other => HalError::Backend(other.to_string()),
        }
    }
}

impl AdlBus {
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
            .map_err(|_| HalError::from(AdlError::WorkerGone))?;
        match answer.recv_timeout(REPLY_TIMEOUT + REPLY_GRACE) {
            Ok(r) => Ok(r?),
            Err(RecvTimeoutError::Timeout) => Err(AdlError::Timeout(REPLY_TIMEOUT.as_millis() as u32).into()),
            Err(RecvTimeoutError::Disconnected) => Err(AdlError::WorkerGone.into()),
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

impl SmbusBus for AdlBus {
    fn descriptor(&self) -> &SmbusBusDescriptor {
        &self.descriptor
    }

    fn write_quick(&mut self, addr: u8) -> HalResult<()> {
        self.run(addr, I2cOp::Quick).map(drop)
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
    use crate::hub::{AdlHub, FakeAdlI2c, GpuInfo, GPU_GONE};
    use huedeck_hal::smbus::{PciIds, SmbusBusKind};

    fn rx6700() -> GpuInfo {
        GpuInfo {
            name: "AMD Radeon RX 6700 XT".into(),
            pci: PciIds {
                vendor: 0x1002,
                device: 0x73DF,
                sub_vendor: 0x1DA2,
                sub_device: 0xE445,
            },
            bus: 3,
        }
    }

    fn on_bus(bus: u32) -> GpuInfo {
        GpuInfo { bus, ..rx6700() }
    }

    fn hub(fake: FakeAdlI2c) -> AdlHub {
        AdlHub::with_transport(fake).unwrap()
    }

    #[test]
    fn bus_descriptor_is_an_amd_gpu_bus_with_the_pci_tuple() {
        let h = hub(FakeAdlI2c::new(vec![rx6700()]));
        let buses = h.buses();
        assert_eq!(buses.len(), 1);
        let d = buses[0].descriptor();
        assert_eq!(d.name, "AMD ADLX Bus - AMD Radeon RX 6700 XT");
        assert_eq!(d.kind, SmbusBusKind::AmdGpu);
        assert!(d.kind.is_amd_gpu_bus() && !d.kind.is_nvidia_bus() && !d.kind.is_system_bus());
        assert!(!d.kind.is_dimm_bus(), "GPU buses keep 0x30-0x37 and 0x48-0x4F");
        assert_eq!(d.key(), "1002:73df:1da2:e445@pci03");
        assert_eq!(d.max_block_len, 32);
    }

    #[test]
    fn read_and_write_byte_reach_the_register() {
        let fake = FakeAdlI2c::new(vec![rx6700()]).add_device(0, 0x28, [(0x10, 0x04)]);
        let log = fake.log();
        let h = hub(fake);
        let mut bus = h.buses().remove(0);
        assert_eq!(bus.read_byte(0x28, 0x10).unwrap(), 0x04);
        assert_eq!(bus.read_byte(0x28, 0x30).unwrap(), 0xFF);
        bus.write_byte(0x28, 0x1A, 0x12).unwrap();
        assert_eq!(bus.read_byte(0x28, 0x1A).unwrap(), 0x12);
        let seen = log.lock().unwrap().clone();
        assert_eq!(
            seen[0].plan,
            I2cPlan {
                write: false,
                offset: 0x10,
                data: vec![0]
            }
        );
        assert_eq!(
            seen[2].plan,
            I2cPlan {
                write: true,
                offset: 0,
                data: vec![0x1A, 0x12]
            },
            "register prepended, offset 0"
        );
        assert!(bus.read_byte(0x02, 0).is_err(), "reserved address");
    }

    #[test]
    fn words_and_blocks() {
        let fake = FakeAdlI2c::new(vec![rx6700()]).add_device(0, 0x36, []);
        let log = fake.log();
        let h = hub(fake);
        let mut bus = h.buses().remove(0);
        bus.write_word(0x36, 0x20, 0x1234).unwrap();
        assert_eq!(bus.read_word(0x36, 0x20).unwrap(), 0x1234);
        bus.write_block(0x36, 0x00, &[0x10, 0x00, 0x01]).unwrap();
        let mut buf = [0u8; 4];
        assert_eq!(bus.read_block(0x36, 0x00, &mut buf).unwrap(), 4);
        assert_eq!(buf, [3, 0x10, 0x00, 0x01], "count byte first on the wire");
        bus.write_block_no_reg(0x36, &[0x40, 7]).unwrap();
        let mut raw = [0u8; 1];
        bus.read_block_no_reg(0x36, &mut raw).unwrap();
        let seen = log.lock().unwrap();
        assert_eq!(seen[2].plan.data, vec![0x00, 3, 0x10, 0x00, 0x01]);
        assert_eq!(
            seen.last().unwrap().plan.offset,
            0,
            "register-less read at offset 0"
        );
    }

    #[test]
    fn quick_probes_and_receive_byte_is_unsupported() {
        let fake = FakeAdlI2c::new(vec![rx6700()]).add_device(0, 0x36, []);
        let log = fake.log();
        let h = hub(fake);
        let mut bus = h.buses().remove(0);
        bus.write_quick(0x36).unwrap();
        assert_eq!(
            bus.write_quick(0x37).unwrap_err().plugin_status(),
            -1,
            "nobody at 0x37"
        );
        assert!(log.lock().unwrap()[0].plan.data.is_empty());
        assert!(matches!(
            bus.read_byte_no_reg(0x36),
            Err(HalError::Unsupported(_))
        ));
        assert_eq!(log.lock().unwrap().len(), 2, "Receive Byte never reaches ADL");
    }

    #[test]
    fn adl_status_is_kept() {
        let h = hub(FakeAdlI2c::new(vec![rx6700()]).fail_with(-8));
        let mut bus = h.buses().remove(0);
        assert!(matches!(
            bus.write_byte(0x28, 0x1A, 1),
            Err(HalError::Bus { status: -8 })
        ));
        let h = hub(FakeAdlI2c::new(vec![rx6700()]));
        let mut bus = h.buses().remove(0);
        assert_eq!(
            bus.read_byte(0x28, 0).unwrap_err().plugin_status(),
            -1,
            "nobody at 0x28"
        );
    }

    #[test]
    fn transfers_follow_the_card_not_its_position() {
        let fake = FakeAdlI2c::new(vec![on_bus(7), on_bus(3)])
            .add_device(0, 0x28, [(0x1A, 0x07)])
            .add_device(1, 0x28, [(0x1A, 0x03)]);
        let h = hub(fake);
        let mut three = h.bus_for(on_bus(3));
        assert_eq!(three.read_byte(0x28, 0x1A).unwrap(), 0x03);
        let mut gone = h.bus_for(on_bus(9));
        assert_eq!(
            gone.write_byte(0x28, 0x1A, 1).unwrap_err().plugin_status(),
            GPU_GONE,
            "a card no longer enumerated is never written"
        );
    }

    #[test]
    fn a_request_whose_caller_gave_up_is_never_sent() {
        let fake = FakeAdlI2c::new(vec![rx6700()]).add_device(0, 0x28, []);
        let log = fake.log();
        let h = hub(fake);
        let bus = h.buses().remove(0);
        let (reply, answer) = mpsc::channel();
        bus.tx
            .send(Request {
                job: Job::Transfer {
                    gpu: rx6700(),
                    addr: 0x28,
                    plan: I2cPlan::of(&I2cOp::WriteByte { reg: 0x1A, val: 9 }).unwrap(),
                },
                deadline: Instant::now() - std::time::Duration::from_millis(1),
                reply,
            })
            .unwrap();
        assert!(matches!(answer.recv().unwrap(), Err(AdlError::Timeout(_))));
        assert!(log.lock().unwrap().is_empty());
    }

    #[test]
    fn no_cross_process_lock_is_taken() {
        let fake = FakeAdlI2c::new(vec![rx6700()]).add_device(0, 0x28, []);
        let log = fake.log();
        let h = hub(fake);
        let mut bus = h.buses().remove(0);
        bus.bus_lock(100).unwrap();
        bus.write_byte(0x28, 0x1A, 3).unwrap();
        bus.bus_unlock().unwrap();
        assert_eq!(
            log.lock().unwrap().len(),
            1,
            "only the transfer reached the worker"
        );
    }

    #[test]
    fn hub_serialises_calls_on_one_thread() {
        let fake = FakeAdlI2c::new(vec![on_bus(3), on_bus(7)])
            .add_device(0, 0x28, [])
            .add_device(1, 0x28, []);
        let log = fake.log();
        let h = hub(fake);
        let buses = h.buses();
        assert_eq!(buses.len(), 2);
        std::thread::scope(|s| {
            for (i, mut bus) in buses.into_iter().enumerate() {
                s.spawn(move || {
                    for v in 0..20u8 {
                        bus.write_byte(0x28, 0x1A, v).unwrap();
                        assert_eq!(bus.read_byte(0x28, 0x1A).unwrap(), v, "bus {i}");
                    }
                });
            }
        });
        let seen = log.lock().unwrap();
        assert_eq!(seen.len(), 80);
        assert!(seen.iter().all(|t| t.thread == "huedeck-adl"));
    }

    #[test]
    fn dropping_the_hub_keeps_buses_working_until_they_go() {
        let fake = FakeAdlI2c::new(vec![rx6700()]).add_device(0, 0x28, []);
        let h = hub(fake);
        let mut bus = h.buses().remove(0);
        drop(h);
        bus.write_byte(0x28, 0x1A, 1).unwrap();
    }
}
