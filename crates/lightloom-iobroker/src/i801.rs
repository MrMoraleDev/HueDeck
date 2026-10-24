// SPDX-License-Identifier: GPL-3.0-or-later
//! The Intel PCH ("i801", ICH8 family) SMBus through PawnIO's signed
//! `SmbusI801` module (research m5-privileged-buses.md §1.6; the module
//! source `SmbusI801.p` of PawnIO.Modules 0.2.11, LGPL-2.1-or-later).
//!
//! What the module does on load (`main` → `i801_init`): it looks for an
//! Intel (0x8086) function of class 0x0C05 at 00:1F.4, 00:1F.3, 00:1F.1 or
//! 80:1F.4, in that order; requires SMBHSTCFG.HST_EN; records SMBHSTCFG.SPD_WD
//! (the SPD write protection); maps the MMIO BAR (SMBMBAR, 32- or 64-bit; an
//! I/O BAR, a below-1 MB or an unassigned one is `STATUS_NOT_SUPPORTED`); and
//! notes the I/O base (SMB_BASE) for identification only. Any controller it
//! does not drive answers `STATUS_NOT_SUPPORTED`, which the broker takes as
//! "not this chipset" ([`crate::boot`]).
//!
//! The ioctls the broker uses:
//!
//! * `ioctl_smbus_xfer`: the same 9 input / 5 output cells as `SmbusPIIX4`
//!   ([`crate::piix4::xfer_input`]) for Quick, Send/Receive Byte, Byte, Word
//!   and Block Write (`[len, data…]` packed little-endian from cell 4). A Send
//!   Byte sends the command cell (cell 2), as the PIIX4 does. Each transfer
//!   takes the controller's INUSE_STS hardware semaphore (waiting up to 80 ms
//!   for BIOS/ACPI to let it go: `STATUS_IO_TIMEOUT`), checks HOST_BUSY
//!   (`STATUS_DEVICE_BUSY`), runs with an 80 ms timeout, and maps DEV_ERR to
//!   `STATUS_NO_SUCH_DEVICE`, FAILED to `STATUS_IO_DEVICE_ERROR` and BUS_ERR
//!   (lost arbitration) to `STATUS_RETRY`. It enables PCI memory decoding for
//!   the call when it was off and clears it again. The caller still holds
//!   `Global\Access_SMBUS.HTP.Method` around it.
//! * The block-read layout differs from the PIIX4: the I801 answers the
//!   count alone in cell 0 and the data from byte 8 (cell 1) on; the PIIX4
//!   packs `[len, data…]` from byte 0. Block reads stay refused on both,
//!   like I2C block data (protocol 8, I801 only) and the process calls.
//! * `ioctl_identity`: `"i801"`, the I/O base (0 when only the MMIO BAR is
//!   assigned) and the PCI ids, decoded by [`crate::piix4::decode_identity`].
//! * `ioctl_write_protection`: SPD_WD (1 = the controller refuses writes to
//!   the SPD addresses 0x50-0x57). The broker never writes there anyway; a
//!   controller without it is logged, since on such a board only Lightloom's
//!   own policy stands between a buggy client and the SPD.
//! * `ioctl_set_sleep_mode`: as on the PIIX4 (`sleepmode.inc`).
//! * There is no `ioctl_piix4_port_sel`: one bus, one handle.
//!
//! The PCI device ids in [`KNOWN_DEVICES`] are the desktop and mobile PCH
//! SMBus functions the module drives, including those OpenRGB looks for RAM
//! on (`IF_DRAM_SMBUS`, mirrored by `plugins/crucial-ballistix`'s
//! `DRAM_SMBUS`). Another Intel class-0x0C05 function the module accepts is
//! still served, with a warning.

use crate::chipset::SmbusXfer;
use crate::pawnio::PawnExec;
use crate::piix4::{decode_identity, map_status, Identity, XFER_IN_CELLS, XFER_OUT_CELLS};
use lightloom_protocol::broker::BrokerError;

/// PCI vendor id of Intel.
pub const INTEL_VENDOR: u16 = 0x8086;

/// Intel PCH SMBus functions (PCI device ids) the `SmbusI801` module drives.
pub const KNOWN_DEVICES: &[(u16, &str)] = &[
    (0x2930, "ICH9"),
    (0x3A30, "ICH10"),
    (0x3A60, "ICH10 (corporate)"),
    (0x3B30, "5 Series / 3400 (Ibex Peak)"),
    (0x1C22, "6 Series (Cougar Point)"),
    (0x1D22, "X79 (Patsburg)"),
    (0x1E22, "7 Series (Panther Point)"),
    (0x8C22, "8 Series (Lynx Point)"),
    (0x9C22, "8 Series LP (Lynx Point-LP)"),
    (0x8D22, "X99 (Wellsburg)"),
    (0x8CA2, "9 Series (Wildcat Point)"),
    (0x9CA2, "Wildcat Point-LP"),
    (0xA123, "100 Series (Sunrise Point-H)"),
    (0x9D23, "Sunrise Point-LP"),
    (0xA2A3, "200 Series / X299 (Kaby Lake-H)"),
    (0xA323, "300 Series (Cannon Lake-H)"),
    (0x9DA3, "Cannon Lake-LP"),
    (0x06A3, "400 Series (Comet Lake-H)"),
    (0xA3A3, "Comet Lake-V"),
    (0x02A3, "Comet Lake-LP"),
    (0x43A3, "500 Series (Tiger Lake-H)"),
    (0xA0A3, "Tiger Lake-LP"),
    (0x7AA3, "600 Series (Alder Lake-S)"),
    (0x51A3, "Alder Lake-P"),
    (0x7A23, "700 Series (Raptor Lake-S)"),
    (0x7E22, "Meteor Lake-P"),
    (0x7F23, "800 Series (Arrow Lake-S)"),
];

/// Skylake-X PCU SMBus (`INTEL_SKYLAKEX_PCU_SMBUS_DEV`): in OpenRGB's DRAM
/// list, but an IMC SMBus behind the CPU's PCU, driven by
/// `SmbusIntelSkylakeIMC` (not pinned), never by `SmbusI801`.
pub const SKYLAKE_X_PCU_SMBUS: u16 = 0x2085;

/// The chipset name of a known Intel SMBus function.
pub fn known_device(vendor: u16, device: u16) -> Option<&'static str> {
    if vendor != INTEL_VENDOR {
        return None;
    }
    KNOWN_DEVICES.iter().find(|(d, _)| *d == device).map(|(_, n)| *n)
}

/// Whether the broker serves the controller `SmbusI801` reported: an Intel
/// function; the Skylake-X PCU never (the module does not drive it, so this
/// is an identity it should not report).
pub fn check_identity(identity: &Identity) -> Result<(), String> {
    if identity.vendor != INTEL_VENDOR {
        return Err(format!(
            "SmbusI801 reported PCI vendor {:04x}, not Intel",
            identity.vendor
        ));
    }
    if identity.device == SKYLAKE_X_PCU_SMBUS {
        return Err("SmbusI801 reported the Skylake-X PCU SMBus, which it does not drive".into());
    }
    Ok(())
}

/// `Intel ICH8 Bus - 0xefa0` (SignalRGB names its i801 bus class `I2CBus_ICH8`
/// with the prefix `Intel ICH8 Bus - `; the suffix follows its PIIX4 buses).
/// A controller with no I/O BAR is named after its PCI device id instead.
pub fn bus_name(identity: &Identity) -> String {
    if identity.base != 0 && identity.base <= u64::from(u16::MAX) {
        format!("Intel ICH8 Bus - 0x{:04x}", identity.base)
    } else {
        format!("Intel ICH8 Bus - 8086:{:04x}", identity.device)
    }
}

/// The Intel SMBus on its module handle.
#[derive(Debug)]
pub struct I801Bus<E> {
    exec: E,
}

impl<E: PawnExec> I801Bus<E> {
    /// The bus on a handle that has `SmbusI801` loaded.
    pub fn new(exec: E) -> Self {
        Self { exec }
    }

    /// `ioctl_identity` of the controller.
    pub fn identity(&mut self) -> Result<Identity, BrokerError> {
        let mut out = [0u64; 3];
        self.exec
            .execute("ioctl_identity", &[0], &mut out)
            .map_err(map_status)?;
        Ok(decode_identity(&out))
    }

    /// `ioctl_write_protection`: whether SMBHSTCFG.SPD_WD is set (the
    /// controller refuses writes to 0x50-0x57).
    pub fn write_protection(&mut self) -> Result<bool, BrokerError> {
        let mut out = [0u64; 1];
        self.exec
            .execute("ioctl_write_protection", &[0], &mut out)
            .map_err(map_status)?;
        Ok(out[0] & 1 == 1)
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

impl<E: PawnExec> SmbusXfer for I801Bus<E> {
    fn xfer(&mut self, input: &[u64; XFER_IN_CELLS]) -> Result<[u64; XFER_OUT_CELLS], BrokerError> {
        let mut out = [0u64; XFER_OUT_CELLS];
        self.exec
            .execute("ioctl_smbus_xfer", input, &mut out)
            .map_err(map_status)?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakePiix4;
    use crate::pawnio::NtStatus;
    use crate::piix4::{xfer_input, PROTO_BYTE, PROTO_BYTE_DATA, READ, WRITE};
    use lightloom_protocol::broker::{BrokerOp, BrokerPayload};

    fn i801() -> FakePiix4 {
        FakePiix4::new_i801()
    }

    #[test]
    fn transfers_use_the_piix4_cells() {
        let fake = i801().with_device(0x39, [(0xA1, 0x01)]);
        let mut b = I801Bus::new(fake.clone());
        let read = BrokerOp::SmbusReadByte {
            bus: 0,
            addr: 0x39,
            reg: 0xA1,
        };
        assert_eq!(b.transfer(&read), Ok(BrokerPayload::Byte(0x01)));
        let block = BrokerOp::SmbusWriteBlock {
            bus: 0,
            addr: 0x39,
            reg: 0x03,
            data: vec![0x10, 0x11, 0x12],
        };
        assert_eq!(b.transfer(&block), Ok(BrokerPayload::Ack));
        let word = BrokerOp::SmbusWriteWord {
            bus: 0,
            addr: 0x39,
            reg: 0x20,
            value: 0xBEEF,
        };
        assert_eq!(b.transfer(&word), Ok(BrokerPayload::Ack));
        let calls = fake.calls();
        assert_eq!(
            calls[0],
            ("ioctl_smbus_xfer".into(), xfer_input(&read).unwrap().to_vec())
        );
        assert_eq!(calls[1].1, xfer_input(&block).unwrap().to_vec());
        assert_eq!(
            fake.register(0x39, 0x05),
            Some(0x12),
            "block bytes from the register on"
        );
        assert_eq!(fake.register(0x39, 0x20), Some(0xEF), "low byte first");
        assert!(
            calls.iter().all(|(f, _)| f == "ioctl_smbus_xfer"),
            "no port select"
        );
    }

    #[test]
    fn statuses_map_like_the_piix4() {
        let mut b = I801Bus::new(i801());
        let op = BrokerOp::SmbusReadByteNoReg { bus: 0, addr: 0x3A };
        assert_eq!(b.transfer(&op), Err(BrokerError::Nack), "DEV_ERR");
        for (status, want) in [
            (NtStatus::IO_TIMEOUT, BrokerError::Timeout),
            (NtStatus::NOT_SUPPORTED, BrokerError::Unsupported),
        ] {
            let mut b = I801Bus::new(i801().with_device(0x3A, []).failing(status));
            assert_eq!(b.transfer(&op), Err(want));
        }
        // HOST_BUSY, lost arbitration: a backend failure (-1 for the plugin).
        for status in [NtStatus::DEVICE_BUSY, NtStatus::RETRY] {
            let mut b = I801Bus::new(i801().with_device(0x3A, []).failing(status));
            assert!(matches!(b.transfer(&op), Err(BrokerError::Backend { .. })));
        }
    }

    #[test]
    fn spd_read_runs_the_ee1004_page_dance() {
        let mut spd = vec![0u8; 512];
        spd[2] = 0x0C;
        spd[320] = 0x80;
        spd[321] = 0x2C;
        let fake = i801().with_spd(0x50, &spd);
        let mut b = I801Bus::new(fake.clone());
        assert_eq!(
            b.spd_read(0x50, 320, 2),
            Ok(BrokerPayload::Block(vec![0x80, 0x2C]))
        );
        let cells: Vec<Vec<u64>> = fake.calls().into_iter().map(|(_, c)| c[..5].to_vec()).collect();
        assert_eq!(
            cells,
            vec![
                vec![0x37, WRITE, 0x00, PROTO_BYTE, 0],
                vec![0x50, READ, 0x40, PROTO_BYTE_DATA, 0],
                vec![0x50, READ, 0x41, PROTO_BYTE_DATA, 0],
                vec![0x36, WRITE, 0x00, PROTO_BYTE, 0],
            ]
        );
        assert_eq!(fake.spd_page(), 0);
    }

    #[test]
    fn identity_write_protection_and_sleep_mode() {
        let fake = i801();
        let mut b = I801Bus::new(fake.clone());
        let id = b.identity().unwrap();
        assert_eq!(id.name, "i801");
        assert_eq!(id.base, 0xEFA0);
        assert_eq!((id.vendor, id.device), (0x8086, 0x7A23));
        assert_eq!((id.sub_vendor, id.sub_device), (0x1462, 0x7D25));
        assert_eq!(bus_name(&id), "Intel ICH8 Bus - 0xefa0");
        assert!(check_identity(&id).is_ok());
        assert_eq!(
            known_device(id.vendor, id.device),
            Some("700 Series (Raptor Lake-S)")
        );
        assert_eq!(b.write_protection(), Ok(true));
        assert!(b.set_sleep_mode(0).is_ok());
        assert!(b.set_sleep_mode(3).is_err());
        let unprotected = i801().spd_write_protection(false);
        assert_eq!(I801Bus::new(unprotected).write_protection(), Ok(false));
    }

    #[test]
    fn identities_the_broker_refuses() {
        let mut id = I801Bus::new(i801()).identity().unwrap();
        id.base = 0;
        assert_eq!(bus_name(&id), "Intel ICH8 Bus - 8086:7a23");
        id.device = SKYLAKE_X_PCU_SMBUS;
        assert!(check_identity(&id).is_err());
        id.vendor = 0x1022;
        assert!(check_identity(&id).is_err());
        assert_eq!(known_device(0x1022, 0x7A23), None);
    }

    #[test]
    fn known_devices_cover_openrgb_dram_list() {
        // IF_DRAM_SMBUS (pci_ids.h) / plugins/crucial-ballistix DRAM_SMBUS,
        // the Intel entries the i801 drives.
        for dev in [
            0x3A30, 0xA123, 0xA2A3, 0xA323, 0x06A3, 0xA3A3, 0x43A3, 0x7AA3, 0x7A23, 0x7F23,
        ] {
            assert!(known_device(INTEL_VENDOR, dev).is_some(), "{dev:04x}");
        }
        assert_eq!(known_device(INTEL_VENDOR, SKYLAKE_X_PCU_SMBUS), None);
        let mut ids: Vec<u16> = KNOWN_DEVICES.iter().map(|(d, _)| *d).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), KNOWN_DEVICES.len(), "no duplicate ids");
    }
}
