// SPDX-License-Identifier: GPL-3.0-or-later
//! The AMD FCH ("PIIX4") SMBus through PawnIO's signed `SmbusPIIX4` module
//! (research m5-privileged-buses.md §1.6, §1.7, §6.2).
//!
//! Every transfer is one `ioctl_smbus_xfer` with 9 input cells
//! `[addr, read, command, protocol, data…]` and 5 output cells, zero-filled,
//! as OpenRGB calls it; results are masked to 8 or 16 bits. Protocols are the
//! Linux `I2C_SMBUS_*` sizes. A Block Write packs `[len, data…]` byte-wise
//! little-endian from cell 4. Block reads and register-less block transfers
//! are refused on chipset buses (the PIIX4 and I801 block-read layouts
//! differ, and SignalRGB has none there either); Receive Byte is served
//! (OpenRGB's RAM detection probes with it).
//!
//! One module handle per port: handle A on port 0 (base 0x0B00, the DIMMs),
//! handle B on port 1 (the AUX controller at 0x0B20). Ports 2-4 sit behind a
//! global hardware mux of the primary controller and are never selected for
//! HueDeck's own use.

use crate::pawnio::{NtStatus, PawnExec};
use huedeck_protocol::broker::{BrokerError, BrokerOp, BrokerPayload};

/// `I2C_SMBUS_QUICK`.
pub const PROTO_QUICK: u64 = 0;
/// `I2C_SMBUS_BYTE` (Receive / Send Byte).
pub const PROTO_BYTE: u64 = 1;
/// `I2C_SMBUS_BYTE_DATA`.
pub const PROTO_BYTE_DATA: u64 = 2;
/// `I2C_SMBUS_WORD_DATA`.
pub const PROTO_WORD_DATA: u64 = 3;
/// `I2C_SMBUS_BLOCK_DATA` (count byte first).
pub const PROTO_BLOCK_DATA: u64 = 5;
/// Direction cell: read.
pub const READ: u64 = 1;
/// Direction cell: write.
pub const WRITE: u64 = 0;
/// Input cells of `ioctl_smbus_xfer`.
pub const XFER_IN_CELLS: usize = 9;
/// Output cells of `ioctl_smbus_xfer`.
pub const XFER_OUT_CELLS: usize = 5;
/// Largest SMBus block.
pub const BLOCK_MAX: usize = 32;
/// I/O base of each port HueDeck uses (`AMD PIIX4 Bus - 0x0b00` / `0x0b20`).
pub const PORT_BASES: [u16; 2] = [0x0B00, 0x0B20];

/// `ioctl_smbus_xfer` input cells for `op`.
pub fn xfer_input(op: &BrokerOp) -> Result<[u64; XFER_IN_CELLS], BrokerError> {
    let mut cells = [0u64; XFER_IN_CELLS];
    let mut set = |values: &[u64]| cells[..values.len()].copy_from_slice(values);
    match op {
        BrokerOp::SmbusWriteQuick { addr, .. } => set(&[u64::from(*addr), WRITE, 0, PROTO_QUICK]),
        BrokerOp::SmbusReadByteNoReg { addr, .. } => set(&[u64::from(*addr), READ, 0, PROTO_BYTE]),
        BrokerOp::SmbusReadByte { addr, reg, .. } => {
            set(&[u64::from(*addr), READ, u64::from(*reg), PROTO_BYTE_DATA])
        }
        BrokerOp::SmbusWriteByte { addr, reg, value, .. } => set(&[
            u64::from(*addr),
            WRITE,
            u64::from(*reg),
            PROTO_BYTE_DATA,
            u64::from(*value),
        ]),
        BrokerOp::SmbusReadWord { addr, reg, .. } => {
            set(&[u64::from(*addr), READ, u64::from(*reg), PROTO_WORD_DATA])
        }
        BrokerOp::SmbusWriteWord { addr, reg, value, .. } => set(&[
            u64::from(*addr),
            WRITE,
            u64::from(*reg),
            PROTO_WORD_DATA,
            u64::from(*value),
        ]),
        BrokerOp::SmbusWriteBlock { addr, reg, data, .. } => {
            if data.is_empty() || data.len() > BLOCK_MAX {
                return Err(BrokerError::LengthOutOfRange { len: data.len() });
            }
            set(&[u64::from(*addr), WRITE, u64::from(*reg), PROTO_BLOCK_DATA]);
            // Byte 0 = count, then the data, packed little-endian into cells 4..9.
            let mut packed = [0u8; 40];
            packed[0] = data.len() as u8;
            packed[1..=data.len()].copy_from_slice(data);
            let (chunks, _) = packed.as_chunks::<8>();
            for (i, chunk) in chunks.iter().enumerate() {
                cells[4 + i] = u64::from_le_bytes(*chunk);
            }
        }
        BrokerOp::SmbusReadBlock { .. }
        | BrokerOp::SmbusWriteBlockNoReg { .. }
        | BrokerOp::SmbusReadBlockNoReg { .. } => return Err(BrokerError::Unsupported),
        BrokerOp::BusLock { .. } | BrokerOp::BusUnlock { .. } | BrokerOp::ListBuses | BrokerOp::Health => {
            return Err(BrokerError::BadRequest("not a bus transfer".into()))
        }
    }
    Ok(cells)
}

/// The payload of a completed transfer, masked to the documented width.
pub fn decode_output(op: &BrokerOp, out: &[u64; XFER_OUT_CELLS]) -> BrokerPayload {
    match op {
        BrokerOp::SmbusReadByte { .. } | BrokerOp::SmbusReadByteNoReg { .. } => {
            BrokerPayload::Byte((out[0] & 0xFF) as u8)
        }
        BrokerOp::SmbusReadWord { .. } => BrokerPayload::Word((out[0] & 0xFFFF) as u16),
        _ => BrokerPayload::Ack,
    }
}

/// NTSTATUS of the module → broker error (research §6.3). The plugin sees
/// `-1` for any of them on a chipset bus.
pub fn map_status(status: NtStatus) -> BrokerError {
    match status {
        NtStatus::NO_SUCH_DEVICE => BrokerError::Nack,
        NtStatus::IO_TIMEOUT => BrokerError::Timeout,
        NtStatus::NOT_SUPPORTED => BrokerError::Unsupported,
        other => BrokerError::Backend {
            status: other.0,
            message: other.to_string(),
        },
    }
}

/// `ioctl_identity`: controller name, the handle's I/O base, PCI ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    /// `"PIIX4"`.
    pub name: String,
    /// The handle's current I/O base (0x0B00 / 0x0B20).
    pub base: u64,
    /// PCI vendor of 00:14.0.
    pub vendor: u16,
    /// PCI device.
    pub device: u16,
    /// PCI subsystem vendor.
    pub sub_vendor: u16,
    /// PCI subsystem device.
    pub sub_device: u16,
}

/// Decode `ioctl_identity`'s three cells: packed name, base,
/// `vid | did << 16 | subvid << 32 | subdid << 48`.
pub fn decode_identity(out: &[u64; 3]) -> Identity {
    let name: String = out[0]
        .to_le_bytes()
        .iter()
        .take_while(|b| **b != 0)
        .map(|b| char::from(*b))
        .collect();
    let ids = out[2];
    Identity {
        name,
        base: out[1],
        vendor: ids as u16,
        device: (ids >> 16) as u16,
        sub_vendor: (ids >> 32) as u16,
        sub_device: (ids >> 48) as u16,
    }
}

/// One PIIX4 port on its own module handle.
#[derive(Debug)]
pub struct Piix4Bus<E> {
    exec: E,
    port: u8,
}

impl<E: PawnExec> Piix4Bus<E> {
    /// The bus of `port` (0 or 1) on a handle that has `SmbusPIIX4` loaded.
    /// [`Piix4Bus::select_port`] must run once, under the SMBus mutex, before
    /// the first transfer.
    pub fn new(exec: E, port: u8) -> Result<Self, BrokerError> {
        if usize::from(port) >= PORT_BASES.len() {
            return Err(BrokerError::Unsupported);
        }
        Ok(Self { exec, port })
    }

    /// The port this bus drives.
    pub fn port(&self) -> u8 {
        self.port
    }

    /// One SMBus transfer. Caller holds the SMBus mutex.
    pub fn transfer(&mut self, op: &BrokerOp) -> Result<BrokerPayload, BrokerError> {
        let input = xfer_input(op)?;
        let mut out = [0u64; XFER_OUT_CELLS];
        self.exec
            .execute("ioctl_smbus_xfer", &input, &mut out)
            .map_err(map_status)?;
        Ok(decode_output(op, &out))
    }

    /// Select this bus's port; returns the port the handle (for port 0: the
    /// primary controller's mux) was on before. Caller holds the SMBus mutex.
    pub fn select_port(&mut self) -> Result<u8, BrokerError> {
        self.port_sel(self.port)
    }

    /// Put the primary controller's mux back to `previous` when another
    /// program had left it on port 2, 3 or 4 ([`Piix4Bus::select_port`]
    /// reported it). HueDeck itself only ever selects ports 0 and 1.
    pub fn restore_port(&mut self, previous: u8) -> Result<(), BrokerError> {
        if self.port != 0 || !(2..=4).contains(&previous) {
            return Ok(());
        }
        self.port_sel(previous).map(|_| ())
    }

    fn port_sel(&mut self, port: u8) -> Result<u8, BrokerError> {
        let mut out = [0u64; 1];
        self.exec
            .execute("ioctl_piix4_port_sel", &[u64::from(port)], &mut out)
            .map_err(map_status)?;
        Ok((out[0] & 0xFF) as u8)
    }

    /// `ioctl_identity` of this handle.
    pub fn identity(&mut self) -> Result<Identity, BrokerError> {
        let mut out = [0u64; 3];
        self.exec
            .execute("ioctl_identity", &[0], &mut out)
            .map_err(map_status)?;
        Ok(decode_identity(&out))
    }

    /// `ioctl_set_sleep_mode`: 0 AlwaysBusy, 1 ShortBusy, 2 AlwaysSleep.
    pub fn set_sleep_mode(&mut self, mode: u8) -> Result<(), BrokerError> {
        if mode > 2 {
            return Err(BrokerError::Unsupported);
        }
        self.exec
            .execute("ioctl_set_sleep_mode", &[u64::from(mode)], &mut [])
            .map_err(map_status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakePiix4;

    fn bus(fake: &FakePiix4) -> Piix4Bus<FakePiix4> {
        Piix4Bus::new(fake.clone(), 0).unwrap()
    }

    #[test]
    fn read_byte_cells() {
        let fake = FakePiix4::new().with_device(0x39, [(0xA0, 0x00), (0xA1, 0x01)]);
        let mut b = bus(&fake);
        let op = BrokerOp::SmbusReadByte {
            bus: 0,
            addr: 0x39,
            reg: 0xA1,
        };
        assert_eq!(b.transfer(&op), Ok(BrokerPayload::Byte(0x01)));
        assert_eq!(
            fake.calls(),
            vec![("ioctl_smbus_xfer".into(), vec![0x39, 1, 0xA1, 2, 0, 0, 0, 0, 0])]
        );
    }

    #[test]
    fn write_word_cells_lsb_first() {
        let fake = FakePiix4::new().with_device(0x39, []);
        let mut b = bus(&fake);
        let op = BrokerOp::SmbusWriteWord {
            bus: 0,
            addr: 0x39,
            reg: 0x00,
            value: 0x0083,
        };
        assert_eq!(b.transfer(&op), Ok(BrokerPayload::Ack));
        assert_eq!(fake.calls()[0].1, vec![0x39, 0, 0x00, 3, 0x0083, 0, 0, 0, 0]);
        assert_eq!(fake.register(0x39, 0x00), Some(0x83), "low byte first");
        assert_eq!(fake.register(0x39, 0x01), Some(0x00));
        let read = BrokerOp::SmbusReadWord {
            bus: 0,
            addr: 0x39,
            reg: 0x00,
        };
        assert_eq!(b.transfer(&read), Ok(BrokerPayload::Word(0x0083)));
    }

    #[test]
    fn block_write_packs_count_and_data_le() {
        let fake = FakePiix4::new().with_device(0x39, []);
        let mut b = bus(&fake);
        let op = BrokerOp::SmbusWriteBlock {
            bus: 0,
            addr: 0x39,
            reg: 0x03,
            data: vec![0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17],
        };
        assert_eq!(b.transfer(&op), Ok(BrokerPayload::Ack));
        let cells = &fake.calls()[0].1;
        assert_eq!(&cells[..4], &[0x39, 0, 0x03, 5]);
        assert_eq!(cells[4], 0x1615_1413_1211_1008, "count byte 8, then data");
        assert_eq!(cells[5], 0x17);
        assert_eq!(&cells[6..], &[0, 0, 0]);
        let full = BrokerOp::SmbusWriteBlock {
            bus: 0,
            addr: 0x39,
            reg: 0x03,
            data: (1..=32).collect(),
        };
        let cells = xfer_input(&full).unwrap();
        assert_eq!(cells[4] & 0xFF, 32);
        assert_eq!(cells[7] >> 56, 31, "packed byte 31 is data byte 31");
        assert_eq!(cells[8], 32, "packed byte 32, the last data byte, opens cell 8");
    }

    #[test]
    fn block_over_32_refused() {
        let op = BrokerOp::SmbusWriteBlock {
            bus: 0,
            addr: 0x39,
            reg: 0x03,
            data: vec![0; 33],
        };
        assert_eq!(xfer_input(&op), Err(BrokerError::LengthOutOfRange { len: 33 }));
        for op in [
            BrokerOp::SmbusReadBlock {
                bus: 0,
                addr: 0x39,
                reg: 0,
                len: 4,
            },
            BrokerOp::SmbusWriteBlockNoReg {
                bus: 0,
                addr: 0x39,
                data: vec![1],
            },
            BrokerOp::SmbusReadBlockNoReg {
                bus: 0,
                addr: 0x39,
                len: 1,
            },
        ] {
            assert_eq!(xfer_input(&op), Err(BrokerError::Unsupported), "{op:?}");
        }
    }

    #[test]
    fn receive_byte_cells() {
        let fake = FakePiix4::new().with_device(0x27, [(0x00, 0x5A)]);
        let mut b = bus(&fake);
        let op = BrokerOp::SmbusReadByteNoReg { bus: 0, addr: 0x27 };
        assert_eq!(b.transfer(&op), Ok(BrokerPayload::Byte(0x5A)));
        assert_eq!(fake.calls()[0].1, vec![0x27, 1, 0, 1, 0, 0, 0, 0, 0]);
        let quick = BrokerOp::SmbusWriteQuick { bus: 0, addr: 0x27 };
        assert_eq!(xfer_input(&quick).unwrap(), [0x27, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn no_such_device_is_nack() {
        let fake = FakePiix4::new();
        let mut b = bus(&fake);
        let op = BrokerOp::SmbusReadByteNoReg { bus: 0, addr: 0x3A };
        assert_eq!(b.transfer(&op), Err(BrokerError::Nack));
    }

    #[test]
    fn timeout_maps_to_timeout() {
        assert_eq!(map_status(NtStatus::IO_TIMEOUT), BrokerError::Timeout);
        assert_eq!(map_status(NtStatus::NOT_SUPPORTED), BrokerError::Unsupported);
        assert!(matches!(
            map_status(NtStatus::IO_DEVICE_ERROR),
            BrokerError::Backend { status, .. } if status as u32 == 0xC000_0185
        ));
        let fake = FakePiix4::new()
            .with_device(0x39, [])
            .failing(NtStatus::IO_TIMEOUT);
        let mut b = bus(&fake);
        let op = BrokerOp::SmbusWriteByte {
            bus: 0,
            addr: 0x39,
            reg: 1,
            value: 2,
        };
        assert_eq!(b.transfer(&op), Err(BrokerError::Timeout));
    }

    #[test]
    fn port_2_to_4_refused() {
        let fake = FakePiix4::new();
        assert!(Piix4Bus::new(fake.clone(), 2).is_err());
        assert!(Piix4Bus::new(fake.clone(), 4).is_err());
        let mut aux = Piix4Bus::new(fake.clone(), 1).unwrap();
        assert_eq!(aux.select_port(), Ok(0), "the handle was on port 0");
        assert_eq!(fake.calls()[0], ("ioctl_piix4_port_sel".into(), vec![1]));
        // Port 0 found the mux on 3 (another program): restored as found, never chosen.
        let fake = FakePiix4::new().on_port(3);
        let mut primary = Piix4Bus::new(fake.clone(), 0).unwrap();
        assert_eq!(primary.select_port(), Ok(3));
        primary.restore_port(3).unwrap();
        primary.restore_port(0).unwrap();
        assert_eq!(
            fake.calls(),
            vec![
                ("ioctl_piix4_port_sel".into(), vec![0]),
                ("ioctl_piix4_port_sel".into(), vec![3])
            ]
        );
    }

    #[test]
    fn identity_decodes_pci_ids() {
        let fake = FakePiix4::new();
        let mut b = bus(&fake);
        let id = b.identity().unwrap();
        assert_eq!(id.name, "PIIX4");
        assert_eq!(id.base, 0x0B00);
        assert_eq!(
            (id.vendor, id.device, id.sub_vendor, id.sub_device),
            (0x1022, 0x790B, 0x1022, 0x790B)
        );
        assert!(b.set_sleep_mode(1).is_ok());
        assert!(b.set_sleep_mode(3).is_err());
    }
}
