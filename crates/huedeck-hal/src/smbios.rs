// SPDX-License-Identifier: Apache-2.0 OR MIT
//! SMBIOS structure-table parsing (DMTF DSP0134): the motherboard, BIOS and
//! memory modules that `@SignalRGB/systeminfo` reports (SignalRGB reads them
//! through WMI, which reads SMBIOS). Pure; each platform supplies the raw
//! table: Windows `GetSystemFirmwareTable('RSMB')` ([`parse_raw_smbios_data`]),
//! Linux `/sys/firmware/dmi/tables/DMI` ([`parse_table`]).

use serde::{Deserialize, Serialize};

/// What the firmware says about the machine.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemInfo {
    /// Baseboard (SMBIOS type 2).
    pub motherboard: BoardInfo,
    /// BIOS (SMBIOS type 0).
    pub bios: BiosInfo,
    /// Populated memory slots (SMBIOS type 17 with a size).
    pub ram: Vec<RamModule>,
}

/// SMBIOS type 2.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardInfo {
    /// `Micro-Star International Co., Ltd.`
    pub manufacturer: String,
    /// `MAG B550 TOMAHAWK (MS-7C91)` (WMI `Win32_BaseBoard.Product`).
    pub model: String,
    /// Board revision.
    pub version: String,
}

/// SMBIOS type 0.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BiosInfo {
    /// Firmware vendor.
    pub vendor: String,
    /// Firmware version (`A.70`).
    pub version: String,
    /// Release date as `YYYY-MM-DD` (SMBIOS stores `MM/DD/YYYY`); the raw
    /// string when it does not parse.
    pub release_date: String,
}

/// SMBIOS type 17 (one populated slot).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RamModule {
    /// Module manufacturer (often `Unknown` on consumer kits).
    pub manufacturer: String,
    /// Part number (`BL8G32C16U4WL.8FE`).
    pub part_number: String,
    /// Serial number.
    pub serial_number: String,
    /// Slot name (`DIMM 1`).
    pub locator: String,
    /// Size in MiB.
    pub size_mb: u64,
    /// Rated speed in MT/s (0 = unknown).
    pub speed_mts: u32,
}

/// Parse the buffer of Windows `GetSystemFirmwareTable('RSMB', 0)`
/// (`RawSMBIOSData`: an 8-byte header, then the structure table).
pub fn parse_raw_smbios_data(raw: &[u8]) -> Option<SystemInfo> {
    let len = u32::from_le_bytes(raw.get(4..8)?.try_into().ok()?) as usize;
    let table = raw.get(8..8usize.checked_add(len)?)?;
    Some(parse_table(table))
}

/// Parse an SMBIOS structure table. Malformed tails are ignored.
pub fn parse_table(table: &[u8]) -> SystemInfo {
    let mut info = SystemInfo::default();
    let mut board_seen = false;
    let mut bios_seen = false;
    let mut i = 0usize;
    while i + 4 <= table.len() {
        let kind = table[i];
        let len = table[i + 1] as usize;
        if len < 4 || i + len > table.len() {
            break;
        }
        let formatted = &table[i..i + len];
        // The string set follows the formatted area and ends with two NULs.
        let mut end = i + len;
        while end + 1 < table.len() && !(table[end] == 0 && table[end + 1] == 0) {
            end += 1;
        }
        let strings: Vec<&[u8]> = table[i + len..end.min(table.len())].split(|&b| b == 0).collect();
        let s = |offset: usize| string_at(formatted, offset, &strings);
        match kind {
            0 if !bios_seen => {
                bios_seen = true;
                info.bios = BiosInfo {
                    vendor: s(0x04),
                    version: s(0x05),
                    release_date: iso_date(&s(0x08)),
                };
            }
            2 if !board_seen => {
                board_seen = true;
                info.motherboard = BoardInfo {
                    manufacturer: s(0x04),
                    model: s(0x05),
                    version: s(0x06),
                };
            }
            17 => {
                if let Some(size_mb) = memory_size_mb(formatted) {
                    info.ram.push(RamModule {
                        manufacturer: s(0x17),
                        part_number: s(0x1A),
                        serial_number: s(0x18),
                        locator: s(0x10),
                        size_mb,
                        speed_mts: word(formatted, 0x15).map_or(0, u32::from),
                    });
                }
            }
            127 => break,
            _ => {}
        }
        i = end + 2;
    }
    info
}

fn word(formatted: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        formatted.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn string_at(formatted: &[u8], offset: usize, strings: &[&[u8]]) -> String {
    match formatted.get(offset) {
        Some(&n) if n > 0 => strings
            .get(n as usize - 1)
            .map(|s| String::from_utf8_lossy(s).trim().to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// Size of a type-17 structure in MiB; `None` for an empty slot or an unknown size.
fn memory_size_mb(formatted: &[u8]) -> Option<u64> {
    let size = word(formatted, 0x0C)?;
    match size {
        0 | 0xFFFF => None,
        0x7FFF => {
            let ext = u32::from_le_bytes(formatted.get(0x1C..0x20)?.try_into().ok()?);
            Some(u64::from(ext & 0x7FFF_FFFF))
        }
        s if s & 0x8000 != 0 => Some(u64::from(s & 0x7FFF) / 1024),
        s => Some(u64::from(s)),
    }
}

/// `MM/DD/YYYY` (or `MM/DD/YY`) → `YYYY-MM-DD`; anything else unchanged.
fn iso_date(raw: &str) -> String {
    let parts: Vec<&str> = raw.split('/').collect();
    if let [m, d, y] = parts[..] {
        if let (Ok(m), Ok(d), Ok(y)) = (m.parse::<u32>(), d.parse::<u32>(), y.parse::<u32>()) {
            let y = match y {
                0..=69 => 2000 + y,
                70..=99 => 1900 + y,
                y => y,
            };
            if (1..=12).contains(&m) && (1..=31).contains(&d) {
                return format!("{y:04}-{m:02}-{d:02}");
            }
        }
    }
    raw.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One structure: header + formatted area (after the 4-byte header) + strings.
    fn structure(kind: u8, body: &[u8], strings: &[&str]) -> Vec<u8> {
        let mut v = vec![kind, (4 + body.len()) as u8, 0, 0];
        v.extend_from_slice(body);
        for s in strings {
            v.extend_from_slice(s.as_bytes());
            v.push(0);
        }
        if strings.is_empty() {
            v.push(0);
        }
        v.push(0);
        v
    }

    fn table() -> Vec<u8> {
        let mut t = Vec::new();
        // Type 0: vendor=1, version=2, start segment (2 bytes), release date=3.
        t.extend(structure(
            0,
            &[1, 2, 0, 0, 3],
            &["American Megatrends International, LLC.", "A.70", "06/29/2021"],
        ));
        // Type 2: manufacturer=1, product=2, version=3.
        t.extend(structure(
            2,
            &[1, 2, 3],
            &[
                "Micro-Star International Co., Ltd.",
                "MAG B550 TOMAHAWK (MS-7C91)",
                "2.0",
            ],
        ));
        // Type 17 (populated): size 8192 MiB at 0x0C, locator=1 at 0x10, speed 3200 at 0x15,
        // manufacturer=2 at 0x17, serial=3 at 0x18, part=4 at 0x1A; formatted length 0x22.
        let mut body = vec![0u8; 0x22 - 4];
        body[0x0C - 4..0x0E - 4].copy_from_slice(&8192u16.to_le_bytes());
        body[0x10 - 4] = 1;
        body[0x15 - 4..0x17 - 4].copy_from_slice(&3200u16.to_le_bytes());
        body[0x17 - 4] = 2;
        body[0x18 - 4] = 3;
        body[0x1A - 4] = 4;
        t.extend(structure(
            17,
            &body,
            &["DIMM 1", "Unknown", "E4F1", "BL8G32C16U4WL.8FE"],
        ));
        // Type 17 (empty slot): size 0.
        t.extend(structure(17, &[0u8; 0x22 - 4], &[]));
        t.extend(structure(127, &[], &[]));
        t
    }

    #[test]
    fn board_bios_and_memory_are_read() {
        let info = parse_table(&table());
        assert_eq!(info.motherboard.model, "MAG B550 TOMAHAWK (MS-7C91)");
        assert_eq!(
            info.motherboard.manufacturer,
            "Micro-Star International Co., Ltd."
        );
        assert_eq!(info.motherboard.version, "2.0");
        assert_eq!(info.bios.version, "A.70");
        assert_eq!(info.bios.release_date, "2021-06-29");
        assert_eq!(info.ram.len(), 1, "the empty slot is skipped");
        let m = &info.ram[0];
        assert_eq!(
            (
                m.manufacturer.as_str(),
                m.part_number.as_str(),
                m.locator.as_str()
            ),
            ("Unknown", "BL8G32C16U4WL.8FE", "DIMM 1")
        );
        assert_eq!(
            (m.size_mb, m.speed_mts, m.serial_number.as_str()),
            (8192, 3200, "E4F1")
        );
    }

    #[test]
    fn raw_windows_buffer_and_garbage() {
        let t = table();
        let mut raw = vec![0u8, 3, 3, 0];
        raw.extend_from_slice(&(t.len() as u32).to_le_bytes());
        raw.extend_from_slice(&t);
        assert_eq!(parse_raw_smbios_data(&raw).unwrap().bios.version, "A.70");
        assert!(parse_raw_smbios_data(&raw[..6]).is_none());
        // Truncated or nonsense tables never panic.
        for cut in 0..t.len() {
            let _ = parse_table(&t[..cut]);
        }
        let _ = parse_table(&[2, 2, 0, 0, 0xFF, 0xFF]);
    }

    #[test]
    fn dates_and_sizes() {
        assert_eq!(iso_date("06/29/2021"), "2021-06-29");
        assert_eq!(iso_date("12/31/99"), "1999-12-31");
        assert_eq!(iso_date("01/02/05"), "2005-01-02");
        assert_eq!(iso_date("2021-06-29"), "2021-06-29");
        assert_eq!(iso_date("13/01/2020"), "13/01/2020");
        let mut f = vec![0u8; 0x22];
        f[0x0C..0x0E].copy_from_slice(&0x8000u16.to_le_bytes());
        assert_eq!(memory_size_mb(&f), Some(0), "KiB granularity");
        f[0x0C..0x0E].copy_from_slice(&0x7FFFu16.to_le_bytes());
        f[0x1C..0x20].copy_from_slice(&65536u32.to_le_bytes());
        assert_eq!(memory_size_mb(&f), Some(65536), "extended size");
        f[0x0C..0x0E].copy_from_slice(&0xFFFFu16.to_le_bytes());
        assert_eq!(memory_size_mb(&f), None);
    }
}
