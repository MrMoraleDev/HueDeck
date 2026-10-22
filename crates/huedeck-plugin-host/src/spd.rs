// SPDX-License-Identifier: GPL-3.0-or-later
//! SPD check of RAM plugins: which memory modules sit on a chipset bus, and
//! whether a RAM plugin may scan it.
//!
//! RGB controllers of different brands answer at overlapping addresses, and
//! a RAM plugin's `Scan()` writes to identify or remap its controller. So,
//! as OpenRGB does for its DRAM detectors (`REGISTER_I2C_DRAM_DETECTOR`, a
//! JEDEC id and a memory type per detector), the host first reads the SPD
//! EEPROM of every DIMM slot (0x50..=0x57) and offers `Scan(bus)` to a RAM
//! plugin only when a module from a manufacturer it declares
//! (`SpdManufacturers()`) is there. The plugin then sees the matching slots
//! through `bus.SpdSlots()` (HueDeck extension, read-only).
//!
//! What is read, on chipset buses only, and only reads reach the SPD:
//!
//! 1. bytes 0, 1 and 2 of each slot with plain Read Byte, no page select:
//!    no answer means an empty slot; byte 0 = 0x51 with byte 1 & 0xEF =
//!    0x08 is a DDR5 SPD5118 hub (and byte 2 = 0x12 is DDR5 too); byte 2 =
//!    0x0C is DDR4; anything else is reported as such and left alone;
//! 2. on DDR4 only, through [`SmbusBus::spd_read`](huedeck_hal::SmbusBus::spd_read):
//!    the module manufacturer id (bytes 320-321) and the part number (bytes
//!    329-348) on page 1. The backend does the EE1004 page select (0x36 /
//!    0x37) and selects page 0 again; no plugin can reach that call.
//!
//! DDR5 is not supported yet (reading its manufacturer id needs a write to
//! the SPD5118 hub's page register, MR11, at the SPD address itself): when a
//! DDR5 module is present, RAM plugins do not scan.

use crate::smbus::BusHandle;
use huedeck_hal::smbus::SPD_EEPROM;
use serde::{Deserialize, Serialize};

/// DDR4 SPD byte 320: module manufacturer, JEP106 continuation count (bit 7 parity).
pub const DDR4_MANUFACTURER: u16 = 320;
/// DDR4 SPD bytes 329..=348: module part number (ASCII, space padded).
pub const DDR4_PART_NUMBER: u16 = 329;
/// Length of the DDR4 part number.
pub const DDR4_PART_NUMBER_LEN: usize = 20;
/// SPD byte 2 of a DDR4 module.
pub const TYPE_DDR4: u8 = 0x0C;
/// SPD byte 2 of a DDR5 module.
pub const TYPE_DDR5: u8 = 0x12;

/// One manufacturer a RAM plugin drives (`SpdManufacturers()`), as JEP106
/// names it: the bank (1 = no continuation code) and the id byte within it
/// (with or without its parity bit, compared on the low 7 bits).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpdManufacturer {
    /// JEP106 bank, 1-based.
    pub bank: u8,
    /// JEP106 id byte in that bank (e.g. 0x2C Micron in bank 1).
    pub id: u8,
    /// Manufacturer name (informational).
    #[serde(default)]
    pub name: String,
}

impl SpdManufacturer {
    /// Whether a module of JEP106 `bank` / `id` is from this manufacturer.
    pub fn matches(&self, bank: u8, id: u8) -> bool {
        self.bank == bank && self.id & 0x7F == id & 0x7F
    }
}

/// One occupied DIMM slot, as `bus.SpdSlots()` hands it to a plugin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpdSlot {
    /// SPD address (0x50..=0x57).
    pub address: u8,
    /// `"DDR4"`, `"DDR5"`, or `"unknown"` (SPD byte 2 in [`SpdSlot::type_byte`]).
    pub memory_type: String,
    /// SPD byte 2 as read.
    pub type_byte: u8,
    /// Module manufacturer: JEP106 bank, 1-based (DDR4 only).
    pub jedec_bank: Option<u8>,
    /// Module manufacturer: JEP106 id byte as stored, parity bit included (DDR4 only).
    pub jedec_id: Option<u8>,
    /// Module part number, trailing spaces and NULs removed (DDR4 only).
    pub part_number: Option<String>,
}

impl SpdSlot {
    /// A DDR4 module whose manufacturer was read.
    fn jedec(&self) -> Option<(u8, u8)> {
        Some((self.jedec_bank?, self.jedec_id?))
    }

    /// One line for people: address, type, JEP106 bank and id, part number.
    pub fn describe(&self) -> String {
        let mut text = format!("0x{:02x} {}", self.address, self.memory_type);
        if let Some((bank, id)) = self.jedec() {
            text.push_str(&format!(" bank {bank} id 0x{id:02x}"));
        }
        if let Some(part) = &self.part_number {
            text.push_str(&format!(" {part}"));
        }
        text
    }
}

/// The SPD of every occupied slot of a bus.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SpdInventory {
    /// Occupied slots, by address.
    pub slots: Vec<SpdSlot>,
}

impl SpdInventory {
    /// Read the SPD of slots 0x50..=0x57 of `bus` (module docs).
    pub fn read(bus: &BusHandle) -> Self {
        let slots = bus.with_bus(|b| {
            SPD_EEPROM
                .filter_map(|addr| {
                    let byte = |b: &mut dyn huedeck_hal::SmbusBus, reg| b.read_byte(addr, reg).ok();
                    let b0 = byte(b, 0)?;
                    let b1 = byte(b, 1)?;
                    let b2 = byte(b, 2)?;
                    let mut slot = SpdSlot {
                        address: addr,
                        memory_type: "unknown".into(),
                        type_byte: b2,
                        jedec_bank: None,
                        jedec_id: None,
                        part_number: None,
                    };
                    if (b0 == 0x51 && b1 & 0xEF == 0x08) || b2 == TYPE_DDR5 {
                        slot.memory_type = "DDR5".into();
                    } else if b2 == TYPE_DDR4 {
                        slot.memory_type = "DDR4".into();
                        let mut id = [0u8; 2];
                        if b.spd_read(addr, DDR4_MANUFACTURER, &mut id).is_ok() {
                            slot.jedec_bank = Some((id[0] & 0x7F) + 1);
                            slot.jedec_id = Some(id[1]);
                        }
                        let mut part = [0u8; DDR4_PART_NUMBER_LEN];
                        if b.spd_read(addr, DDR4_PART_NUMBER, &mut part).is_ok() {
                            slot.part_number = Some(part_text(&part)).filter(|p| !p.is_empty());
                        }
                    }
                    Some(slot)
                })
                .collect()
        });
        Self { slots }
    }

    /// Whether `Scan(bus)` may run for a RAM plugin declaring `declared`,
    /// and with which slots (the matching DDR4 modules); else why not.
    pub fn gate(&self, declared: &[SpdManufacturer]) -> Result<Vec<SpdSlot>, String> {
        if declared.is_empty() {
            return Err(
                "RAM plugin without SpdManufacturers(): not offered chipset buses \
                        (docs/host-api/smbus.md, SPD check)"
                    .into(),
            );
        }
        if self.slots.is_empty() {
            return Err(
                "no SPD answered on this bus (no DIMM here, or the SPD is unreadable): not scanned".into(),
            );
        }
        if self.slots.iter().any(|s| s.memory_type == "DDR5") {
            return Err("DDR5 SPD not supported yet: RAM plugins do not scan on DDR5 systems".into());
        }
        let matching: Vec<SpdSlot> = self
            .slots
            .iter()
            .filter(|s| {
                s.jedec()
                    .is_some_and(|(bank, id)| declared.iter().any(|m| m.matches(bank, id)))
            })
            .cloned()
            .collect();
        if matching.is_empty() {
            let wanted = declared
                .iter()
                .map(|m| format!("{} (bank {} id 0x{:02x})", m.name, m.bank, m.id))
                .collect::<Vec<_>>()
                .join(", ");
            let found = self
                .slots
                .iter()
                .map(SpdSlot::describe)
                .collect::<Vec<_>>()
                .join("; ");
            return Err(format!(
                "no module from {wanted} on this bus (SPD: {found}): not scanned"
            ));
        }
        Ok(matching)
    }
}

/// Printable ASCII of an SPD text field, trailing spaces and NULs removed.
fn part_text(bytes: &[u8]) -> String {
    let text: String = bytes
        .iter()
        .map(|&b| {
            if (0x20..0x7F).contains(&b) {
                char::from(b)
            } else {
                ' '
            }
        })
        .collect();
    text.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use huedeck_hal::mock::{MockSmbus, TransactionKind, TransactionRecorder};
    use huedeck_hal::SmbusBusKind;

    /// A DDR4 SPD of a module from JEP106 `bank` / `id` (as stored, parity included).
    pub(crate) fn ddr4(b320: u8, b321: u8, part: &str) -> Vec<u8> {
        let mut spd = vec![0u8; 512];
        spd[0] = 0x23;
        spd[1] = 0x11;
        spd[2] = TYPE_DDR4;
        spd[320] = b320;
        spd[321] = b321;
        let mut p = part.as_bytes().to_vec();
        p.resize(DDR4_PART_NUMBER_LEN, b' ');
        spd[329..349].copy_from_slice(&p);
        spd
    }

    fn crucial() -> Vec<SpdManufacturer> {
        vec![
            SpdManufacturer {
                bank: 1,
                id: 0x2C,
                name: "Micron Technology".into(),
            },
            SpdManufacturer {
                bank: 6,
                id: 0x9B,
                name: "Crucial Technology".into(),
            },
        ]
    }

    fn bus_with(f: impl FnOnce(&mut MockSmbus)) -> (BusHandle, TransactionRecorder) {
        let rec = TransactionRecorder::new();
        let mut bus = MockSmbus::new("AMD PIIX4 Bus - 0x0b00", SmbusBusKind::Amd, rec.clone());
        f(&mut bus);
        (BusHandle::new(Box::new(bus)), rec)
    }

    #[test]
    fn ddr4_modules_are_identified_through_page_1() {
        let (h, rec) = bus_with(|b| {
            b.add_ddr4_spd(0x50, &ddr4(0x85, 0x9B, "BL8G32C16U4WL.M8FE1"));
            b.add_ddr4_spd(0x52, &ddr4(0x01, 0x98, "KHX3200C16D4/8GX"));
        });
        let inv = SpdInventory::read(&h);
        assert_eq!(inv.slots.len(), 2);
        let s = &inv.slots[0];
        assert_eq!(
            (s.address, s.memory_type.as_str(), s.jedec_bank, s.jedec_id),
            (0x50, "DDR4", Some(6), Some(0x9B))
        );
        assert_eq!(s.part_number.as_deref(), Some("BL8G32C16U4WL.M8FE1"));
        assert_eq!(inv.slots[1].jedec_bank, Some(2), "Kingston: bank 2");
        let ok = inv.gate(&crucial()).unwrap();
        assert_eq!(ok.iter().map(|s| s.address).collect::<Vec<_>>(), vec![0x50]);
        // Writes: only the page selects, page 1 then page 0, two per read.
        let writes = rec.payloads(TransactionKind::SmbusWrite);
        assert_eq!(writes.len(), 8);
        assert!(
            writes.chunks(2).all(|w| w == [vec![0x37, 0], vec![0x36, 0]]),
            "{writes:?}"
        );
    }

    #[test]
    fn no_matching_module_no_scan() {
        let (h, _) = bus_with(|b| b.add_ddr4_spd(0x51, &ddr4(0x02, 0x9E, "CMW16GX4M2C3200C16")));
        let e = SpdInventory::read(&h).gate(&crucial()).unwrap_err();
        assert!(
            e.contains("no module from Micron Technology") && e.contains("bank 3 id 0x9e"),
            "{e}"
        );
    }

    #[test]
    fn ddr5_systems_are_not_scanned() {
        let (h, rec) = bus_with(|b| {
            b.add_device(0x50, [(0, 0x51), (1, 0x18), (2, 0x00)]);
            b.add_ddr4_spd(0x52, &ddr4(0x80, 0x2C, "x"));
        });
        let inv = SpdInventory::read(&h);
        assert_eq!(inv.slots[0].memory_type, "DDR5");
        assert_eq!(inv.slots[0].jedec_id, None, "the DDR5 hub is only read");
        let e = inv.gate(&crucial()).unwrap_err();
        assert!(e.contains("DDR5 SPD not supported yet"), "{e}");
        assert!(
            rec.payloads(TransactionKind::SmbusWrite)
                .iter()
                .all(|w| w[0] == 0x36 || w[0] == 0x37),
            "nothing written to the DDR5 hub"
        );
    }

    #[test]
    fn unreadable_spd_no_scan() {
        let (h, rec) = bus_with(|_| {});
        let inv = SpdInventory::read(&h);
        assert!(inv.slots.is_empty());
        assert!(inv.gate(&crucial()).unwrap_err().contains("no SPD answered"));
        assert!(
            rec.payloads(TransactionKind::SmbusWrite).is_empty(),
            "no page select without DDR4"
        );
        // A DDR4 type byte but a manufacturer id that cannot be read.
        let (h, _) = bus_with(|b| b.add_device(0x50, [(2, TYPE_DDR4)]));
        let inv = SpdInventory::read(&h);
        assert_eq!(
            (inv.slots[0].memory_type.as_str(), inv.slots[0].jedec_id),
            ("DDR4", None)
        );
        assert!(inv.gate(&crucial()).is_err());
    }

    #[test]
    fn other_memory_types_are_left_alone() {
        let (h, rec) = bus_with(|b| b.add_device(0x50, [(0, 0x92), (1, 0x13), (2, 0x0B)]));
        let inv = SpdInventory::read(&h);
        assert_eq!(
            (inv.slots[0].memory_type.as_str(), inv.slots[0].type_byte),
            ("unknown", 0x0B)
        );
        assert!(
            rec.payloads(TransactionKind::SmbusWrite).is_empty(),
            "DDR3: no page select"
        );
        assert!(inv.gate(&crucial()).is_err());
    }

    #[test]
    fn undeclared_ram_plugins_are_refused() {
        let (h, _) = bus_with(|b| b.add_ddr4_spd(0x50, &ddr4(0x80, 0x2C, "x")));
        let e = SpdInventory::read(&h).gate(&[]).unwrap_err();
        assert!(e.contains("without SpdManufacturers()"), "{e}");
    }

    #[test]
    fn manufacturer_match_ignores_the_parity_bit() {
        let m = SpdManufacturer {
            bank: 6,
            id: 0x1B,
            name: String::new(),
        };
        assert!(m.matches(6, 0x9B) && m.matches(6, 0x1B));
        assert!(!m.matches(1, 0x9B));
        assert_eq!(part_text(b"AB\0\0  "), "AB");
    }
}
