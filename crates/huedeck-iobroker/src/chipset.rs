// SPDX-License-Identifier: GPL-3.0-or-later
//! What the two chipset SMBus modules share, and the bus the server drives.
//!
//! `SmbusPIIX4` (AMD FCH, [`crate::piix4`]) and `SmbusI801` (Intel PCH,
//! [`crate::i801`]) take the same `ioctl_smbus_xfer` cells for every
//! operation the broker sends (9 in / 5 out, `[addr, read, command,
//! protocol, data…]`, see [`crate::piix4::xfer_input`]); they differ in
//! port selection (PIIX4 only), identity, block-read output layout (refused
//! on both) and the I801's `ioctl_write_protection`. [`SmbusXfer`] carries
//! the shared part, including the DDR4 SPD read; [`ChipsetBus`] is one bus
//! of either kind as the [`crate::server::Server`] sees it.

use crate::i801::I801Bus;
use crate::pawnio::PawnExec;
use crate::piix4::{decode_output, xfer_input, Piix4Bus, PROTO_BYTE, WRITE, XFER_IN_CELLS, XFER_OUT_CELLS};
use huedeck_protocol::broker::{BrokerError, BrokerOp, BrokerPayload, SPD_PAGE_LEN, SPD_SET_PAGE};

/// A module handle that runs `ioctl_smbus_xfer`.
pub trait SmbusXfer {
    /// One raw `ioctl_smbus_xfer`. Caller holds the SMBus mutex.
    fn xfer(&mut self, input: &[u64; XFER_IN_CELLS]) -> Result<[u64; XFER_OUT_CELLS], BrokerError>;

    /// One SMBus transfer. Caller holds the SMBus mutex.
    fn transfer(&mut self, op: &BrokerOp) -> Result<BrokerPayload, BrokerError> {
        let input = xfer_input(op)?;
        let out = self.xfer(&input)?;
        Ok(decode_output(op, &out))
    }

    /// `BrokerOp::SpdRead`: `len` bytes of the DDR4 SPD at `addr` from
    /// `offset` (arguments already validated). Selects the page, reads byte
    /// by byte (Read Byte), and selects page 0 again even when a read failed.
    /// Caller holds the SMBus mutex for the whole sequence.
    fn spd_read(&mut self, addr: u8, offset: u16, len: u8) -> Result<BrokerPayload, BrokerError> {
        let page = (offset / SPD_PAGE_LEN) as u8;
        let start = (offset % SPD_PAGE_LEN) as u8;
        let mut bytes = Vec::with_capacity(usize::from(len));
        let mut result = spd_set_page(self, page);
        for i in 0..len {
            if result.is_err() {
                break;
            }
            let read = BrokerOp::SmbusReadByte {
                bus: 0,
                addr,
                reg: start.wrapping_add(i),
            };
            match self.transfer(&read) {
                Ok(BrokerPayload::Byte(b)) => bytes.push(b),
                Ok(other) => {
                    result = Err(BrokerError::BadRequest(format!("unexpected answer {other:?}")));
                    break;
                }
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }
        if page != 0 {
            if let Err(e) = spd_set_page(self, 0) {
                tracing::warn!(target: "huedeck::audit", "SPD page 0 not selected again: {e}");
                result = result.and(Err(e));
            }
        }
        result.map(|()| BrokerPayload::Block(bytes))
    }
}

/// EE1004 Set Page Address: a Send Byte to 0x36 (page 0) or 0x37 (page 1),
/// as Linux's `ee1004` driver sends it. Both modules put the Send Byte's
/// byte in SMBHSTCMD from the command cell (cell 2), so command and data
/// cells are both 0x00. Some modules switch without acknowledging (Linux's
/// `ee1004` allows for it): a NACKed select of page 0 counts when the Read
/// Page Address then answers page 0. A NACKed select of page 1 fails: a NACK
/// from Read Page Address means page 1 *or* no EE1004 at all, which is no
/// evidence (page 0 is still put back after it).
fn spd_set_page<T: SmbusXfer + ?Sized>(bus: &mut T, page: u8) -> Result<(), BrokerError> {
    let at = SPD_SET_PAGE[usize::from(page & 1)];
    let input = [u64::from(at), WRITE, 0x00, PROTO_BYTE, 0, 0, 0, 0, 0];
    match bus.xfer(&input) {
        Ok(_) => Ok(()),
        Err(BrokerError::Nack) if page & 1 == 0 && spd_page(bus)? == 0 => Ok(()),
        Err(e) => Err(e),
    }
}

/// EE1004 Read Page Address: a Receive Byte at 0x36 is acknowledged while
/// page 0 is selected and not acknowledged on page 1.
fn spd_page<T: SmbusXfer + ?Sized>(bus: &mut T) -> Result<u8, BrokerError> {
    match bus.transfer(&BrokerOp::SmbusReadByteNoReg {
        bus: 0,
        addr: SPD_SET_PAGE[0],
    }) {
        Ok(_) => Ok(0),
        Err(BrokerError::Nack) => Ok(1),
        Err(e) => Err(e),
    }
}

/// One chipset bus the broker serves.
#[derive(Debug)]
pub enum ChipsetBus<E> {
    /// An AMD FCH port (`SmbusPIIX4`).
    Piix4(Piix4Bus<E>),
    /// The Intel PCH SMBus (`SmbusI801`).
    I801(I801Bus<E>),
}

impl<E> From<Piix4Bus<E>> for ChipsetBus<E> {
    fn from(bus: Piix4Bus<E>) -> Self {
        ChipsetBus::Piix4(bus)
    }
}

impl<E> From<I801Bus<E>> for ChipsetBus<E> {
    fn from(bus: I801Bus<E>) -> Self {
        ChipsetBus::I801(bus)
    }
}

impl<E: PawnExec> ChipsetBus<E> {
    /// Whether the handle must select its port before each hold of the
    /// mutex: the PIIX4 primary controller (port 0), whose mux another
    /// program may move. The I801 has a single bus and no port select.
    pub fn selects_port(&self) -> bool {
        matches!(self, ChipsetBus::Piix4(b) if b.port() == 0)
    }

    /// Select this bus's port; returns the port the mux was on before (0
    /// for a bus without ports). Caller holds the SMBus mutex.
    pub fn select_port(&mut self) -> Result<u8, BrokerError> {
        match self {
            ChipsetBus::Piix4(b) => b.select_port(),
            ChipsetBus::I801(_) => Ok(0),
        }
    }

    /// Put the mux back to `previous` (see [`Piix4Bus::restore_port`]).
    pub fn restore_port(&mut self, previous: u8) -> Result<(), BrokerError> {
        match self {
            ChipsetBus::Piix4(b) => b.restore_port(previous),
            ChipsetBus::I801(_) => Ok(()),
        }
    }

    /// Refuse, before the mutex, what neither module can do (the same set:
    /// block reads and register-less block transfers).
    pub fn check(&self, op: &BrokerOp) -> Result<(), BrokerError> {
        xfer_input(op).map(drop)
    }
}

impl<E: PawnExec> SmbusXfer for ChipsetBus<E> {
    fn xfer(&mut self, input: &[u64; XFER_IN_CELLS]) -> Result<[u64; XFER_OUT_CELLS], BrokerError> {
        match self {
            ChipsetBus::Piix4(b) => b.xfer(input),
            ChipsetBus::I801(b) => b.xfer(input),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakePiix4;

    #[test]
    fn only_the_piix4_primary_selects_a_port() {
        let piix4 = |port| ChipsetBus::from(Piix4Bus::new(FakePiix4::new(), port).unwrap());
        assert!(piix4(0).selects_port());
        assert!(!piix4(1).selects_port());
        let fake = FakePiix4::new_i801();
        let mut i801 = ChipsetBus::from(I801Bus::new(fake.clone()));
        assert!(!i801.selects_port());
        assert_eq!(i801.select_port(), Ok(0));
        assert_eq!(i801.restore_port(3), Ok(()));
        assert!(fake.calls().is_empty(), "no ioctl for a bus without ports");
    }

    #[test]
    fn both_refuse_the_same_ops() {
        let i801 = ChipsetBus::from(I801Bus::new(FakePiix4::new_i801()));
        let piix4 = ChipsetBus::from(Piix4Bus::new(FakePiix4::new(), 0).unwrap());
        for op in [
            BrokerOp::SmbusReadBlock {
                bus: 0,
                addr: 0x39,
                reg: 0,
                len: 4,
            },
            BrokerOp::SmbusReadBlockNoReg {
                bus: 0,
                addr: 0x39,
                len: 1,
            },
            BrokerOp::SmbusWriteBlockNoReg {
                bus: 0,
                addr: 0x39,
                data: vec![1],
            },
        ] {
            assert_eq!(i801.check(&op), Err(BrokerError::Unsupported));
            assert_eq!(piix4.check(&op), Err(BrokerError::Unsupported));
        }
        let ok = BrokerOp::SmbusWriteByte {
            bus: 0,
            addr: 0x39,
            reg: 1,
            value: 2,
        };
        assert!(i801.check(&ok).is_ok() && piix4.check(&ok).is_ok());
    }
}
