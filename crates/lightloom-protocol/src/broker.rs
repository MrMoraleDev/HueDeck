// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Daemon ⇄ io-broker protocol.
//!
//! The broker is a tiny LocalSystem service with a *closed* operation set:
//! every request is one [`BrokerOp`], every argument is range-checked by
//! [`BrokerOp::validate`] on both sides, and the response is a
//! [`BrokerResponse`] carrying the request id. Frames are length-prefixed
//! postcard messages (`u32 LE length` + payload) on the pipe.

use serde::{Deserialize, Serialize};

/// Protocol version; bumped on any incompatible change.
pub const BROKER_PROTOCOL_VERSION: u16 = 1;
/// Largest block payload the broker will accept on the wire (per-bus limits may be lower).
pub const MAX_BLOCK_LEN: usize = 64;
/// Largest encoded request/response frame.
pub const MAX_FRAME_BYTES: usize = 4096;
/// Lowest valid 7-bit SMBus address.
pub const SMBUS_ADDR_MIN: u8 = 0x03;
/// Highest valid 7-bit SMBus address.
pub const SMBUS_ADDR_MAX: u8 = 0x77;
/// SPD EEPROM address range: writes denied by default.
pub const SPD_EEPROM_RANGE: std::ops::RangeInclusive<u8> = 0x50..=0x57;
/// SPD write-protect / page-select range: writes denied by default.
pub const SPD_WRITE_PROTECT_RANGE: std::ops::RangeInclusive<u8> = 0x30..=0x37;

/// Broker-side bus index.
pub type BusId = u8;

/// Controller family of a broker bus (mirrors `lightloom_hal::SmbusBusKind`;
/// duplicated so the wire crate stays dependency-free).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BusKind {
    /// Chipset SMBus, vendor unknown.
    System,
    /// NVIDIA GPU I2C via NvAPI.
    Nvidia,
    /// AMD GPU I2C via ADLX.
    AmdGpu,
    /// Nuvoton SuperIO SMBus master.
    Nuvoton,
    /// Intel i801 / ICH8.
    Intel,
    /// AMD FCH / PIIX4.
    Amd,
}

/// Static bus description returned by [`BrokerOp::ListBuses`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BusInfo {
    /// Index to use in subsequent ops.
    pub id: BusId,
    /// Human-readable name (`"AMD PIIX4 Bus - 0x0b00"`).
    pub name: String,
    /// Controller family.
    pub kind: BusKind,
    /// PCI vendor id.
    pub vendor: u16,
    /// PCI device id.
    pub device: u16,
    /// PCI subsystem vendor id.
    pub sub_vendor: u16,
    /// PCI subsystem device id.
    pub sub_device: u16,
    /// Largest block transfer this bus accepts.
    pub max_block_len: u8,
}

/// Status of one pinned PawnIO module.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleStatus {
    /// Module name (`SmbusPIIX4`, `SmbusI801`, `LpcIO`).
    pub name: String,
    /// Hex SHA-256 of the loaded module blob.
    pub sha256: String,
    /// Whether it is currently loaded.
    pub loaded: bool,
}

/// Answer to [`BrokerOp::Health`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthReport {
    /// Broker build version.
    pub version: String,
    /// Protocol version the broker speaks.
    pub protocol: u16,
    /// Seconds since the service started.
    pub uptime_secs: u64,
    /// Whether every write is a no-op.
    pub dry_run: bool,
    /// Operations served since start.
    pub ops_served: u64,
    /// PawnIO modules and their pins.
    pub modules: Vec<ModuleStatus>,
}

/// The closed operation set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrokerOp {
    /// SMBus Quick command (address probe).
    SmbusWriteQuick {
        /// Bus.
        bus: BusId,
        /// 7-bit address.
        addr: u8,
    },
    /// SMBus Read Byte.
    SmbusReadByte {
        /// Bus.
        bus: BusId,
        /// 7-bit address.
        addr: u8,
        /// Command/register byte.
        reg: u8,
    },
    /// SMBus Write Byte.
    SmbusWriteByte {
        /// Bus.
        bus: BusId,
        /// 7-bit address.
        addr: u8,
        /// Command/register byte.
        reg: u8,
        /// Value.
        value: u8,
    },
    /// SMBus Read Word.
    SmbusReadWord {
        /// Bus.
        bus: BusId,
        /// 7-bit address.
        addr: u8,
        /// Command/register byte.
        reg: u8,
    },
    /// SMBus Write Word.
    SmbusWriteWord {
        /// Bus.
        bus: BusId,
        /// 7-bit address.
        addr: u8,
        /// Command/register byte.
        reg: u8,
        /// Value (LSB first on the wire).
        value: u16,
    },
    /// Block write with command byte.
    SmbusWriteBlock {
        /// Bus.
        bus: BusId,
        /// 7-bit address.
        addr: u8,
        /// Command/register byte.
        reg: u8,
        /// Data bytes (≤ [`MAX_BLOCK_LEN`]).
        data: Vec<u8>,
    },
    /// Block read with command byte, fixed length.
    SmbusReadBlock {
        /// Bus.
        bus: BusId,
        /// 7-bit address.
        addr: u8,
        /// Command/register byte.
        reg: u8,
        /// Bytes to read (1..=[`MAX_BLOCK_LEN`]).
        len: u8,
    },
    /// Raw I2C write without command byte.
    SmbusWriteBlockNoReg {
        /// Bus.
        bus: BusId,
        /// 7-bit address.
        addr: u8,
        /// Data bytes.
        data: Vec<u8>,
    },
    /// Raw I2C read without command byte.
    SmbusReadBlockNoReg {
        /// Bus.
        bus: BusId,
        /// 7-bit address.
        addr: u8,
        /// Bytes to read.
        len: u8,
    },
    /// SMBus Receive Byte (NvAPI only).
    SmbusReadByteNoReg {
        /// Bus.
        bus: BusId,
        /// 7-bit address.
        addr: u8,
    },
    /// Enumerate buses.
    ListBuses,
    /// Liveness / version / module pins.
    Health,
}

impl BrokerOp {
    /// Bus the op targets, if any.
    pub fn bus(&self) -> Option<BusId> {
        match self {
            BrokerOp::SmbusWriteQuick { bus, .. }
            | BrokerOp::SmbusReadByte { bus, .. }
            | BrokerOp::SmbusWriteByte { bus, .. }
            | BrokerOp::SmbusReadWord { bus, .. }
            | BrokerOp::SmbusWriteWord { bus, .. }
            | BrokerOp::SmbusWriteBlock { bus, .. }
            | BrokerOp::SmbusReadBlock { bus, .. }
            | BrokerOp::SmbusWriteBlockNoReg { bus, .. }
            | BrokerOp::SmbusReadBlockNoReg { bus, .. }
            | BrokerOp::SmbusReadByteNoReg { bus, .. } => Some(*bus),
            BrokerOp::ListBuses | BrokerOp::Health => None,
        }
    }

    /// Target address, if any.
    pub fn addr(&self) -> Option<u8> {
        match self {
            BrokerOp::SmbusWriteQuick { addr, .. }
            | BrokerOp::SmbusReadByte { addr, .. }
            | BrokerOp::SmbusWriteByte { addr, .. }
            | BrokerOp::SmbusReadWord { addr, .. }
            | BrokerOp::SmbusWriteWord { addr, .. }
            | BrokerOp::SmbusWriteBlock { addr, .. }
            | BrokerOp::SmbusReadBlock { addr, .. }
            | BrokerOp::SmbusWriteBlockNoReg { addr, .. }
            | BrokerOp::SmbusReadBlockNoReg { addr, .. }
            | BrokerOp::SmbusReadByteNoReg { addr, .. } => Some(*addr),
            BrokerOp::ListBuses | BrokerOp::Health => None,
        }
    }

    /// Whether the op changes device state (subject to dry-run, SPD denial, allowlists).
    pub fn is_write(&self) -> bool {
        matches!(
            self,
            BrokerOp::SmbusWriteQuick { .. }
                | BrokerOp::SmbusWriteByte { .. }
                | BrokerOp::SmbusWriteWord { .. }
                | BrokerOp::SmbusWriteBlock { .. }
                | BrokerOp::SmbusWriteBlockNoReg { .. }
        )
    }

    /// Range-check every argument. Run on both ends.
    pub fn validate(&self) -> Result<(), BrokerError> {
        if let Some(addr) = self.addr() {
            if !(SMBUS_ADDR_MIN..=SMBUS_ADDR_MAX).contains(&addr) {
                return Err(BrokerError::AddressOutOfRange { addr });
            }
        }
        let len = match self {
            BrokerOp::SmbusWriteBlock { data, .. } | BrokerOp::SmbusWriteBlockNoReg { data, .. } => data.len(),
            BrokerOp::SmbusReadBlock { len, .. } | BrokerOp::SmbusReadBlockNoReg { len, .. } => *len as usize,
            _ => 1,
        };
        if len == 0 || len > MAX_BLOCK_LEN {
            return Err(BrokerError::LengthOutOfRange { len });
        }
        Ok(())
    }

    /// Default-deny rule for SPD EEPROM and write-protect addresses.
    pub fn touches_spd(&self) -> bool {
        self.is_write()
            && self.addr().is_some_and(|a| SPD_EEPROM_RANGE.contains(&a) || SPD_WRITE_PROTECT_RANGE.contains(&a))
    }
}

/// A request frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerRequest {
    /// [`BROKER_PROTOCOL_VERSION`].
    pub version: u16,
    /// Caller-chosen id echoed in the response.
    pub id: u64,
    /// Operation.
    pub op: BrokerOp,
}

impl BrokerRequest {
    /// Request with the current protocol version.
    pub fn new(id: u64, op: BrokerOp) -> Self {
        Self { version: BROKER_PROTOCOL_VERSION, id, op }
    }
}

/// Successful payloads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrokerPayload {
    /// Write completed (or dry-run accepted).
    Ack,
    /// One byte.
    Byte(u8),
    /// One word.
    Word(u16),
    /// A block.
    Block(Vec<u8>),
    /// Bus list.
    Buses(Vec<BusInfo>),
    /// Health report.
    Health(HealthReport),
}

/// Failure codes. The plugin wrapper maps them to negative SignalRGB statuses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum BrokerError {
    /// Protocol version mismatch.
    #[error("protocol version {0} not supported")]
    UnsupportedVersion(u16),
    /// No such bus.
    #[error("unknown bus {0}")]
    UnknownBus(BusId),
    /// Address outside 0x03..=0x77.
    #[error("address {addr:#04x} out of range")]
    AddressOutOfRange {
        /// Offending address.
        addr: u8,
    },
    /// Address not in the plugin's Scan()-derived allowlist or in the SPD deny range.
    #[error("address {addr:#04x} denied by policy")]
    AddressDenied {
        /// Offending address.
        addr: u8,
    },
    /// Block length outside 1..=MAX_BLOCK_LEN (or the bus limit).
    #[error("block length {len} out of range")]
    LengthOutOfRange {
        /// Offending length.
        len: usize,
    },
    /// Slave did not ACK.
    #[error("no acknowledge from slave")]
    Nack,
    /// Transaction timed out.
    #[error("bus timeout")]
    Timeout,
    /// Operation not available on this bus kind.
    #[error("operation unsupported on this bus")]
    Unsupported,
    /// Too many ops per second.
    #[error("rate limited")]
    RateLimited,
    /// Write skipped because the broker runs in dry-run mode (reported, not silent).
    #[error("dry run: write not performed")]
    DryRun,
    /// Native status from the backend (e.g. NvAPI status code).
    #[error("backend failure (status {status}): {message}")]
    Backend {
        /// Native status code (negative).
        status: i32,
        /// Text.
        message: String,
    },
    /// Could not decode the request.
    #[error("bad request: {0}")]
    BadRequest(String),
}

/// A response frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerResponse {
    /// Echo of the request id.
    pub id: u64,
    /// Result.
    pub result: Result<BrokerPayload, BrokerError>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{decode, encode};

    fn every_op() -> Vec<BrokerOp> {
        vec![
            BrokerOp::SmbusWriteQuick { bus: 0, addr: 0x39 },
            BrokerOp::SmbusReadByte { bus: 0, addr: 0x39, reg: 0xA0 },
            BrokerOp::SmbusWriteByte { bus: 0, addr: 0x39, reg: 0x01, value: 0xFF },
            BrokerOp::SmbusReadWord { bus: 1, addr: 0x5B, reg: 0x02 },
            BrokerOp::SmbusWriteWord { bus: 1, addr: 0x5B, reg: 0x00, value: 0x80F8 },
            BrokerOp::SmbusWriteBlock { bus: 0, addr: 0x39, reg: 0x03, data: vec![1, 2, 3, 4] },
            BrokerOp::SmbusReadBlock { bus: 0, addr: 0x39, reg: 0x03, len: 12 },
            BrokerOp::SmbusWriteBlockNoReg { bus: 2, addr: 0x47, data: vec![0xAB] },
            BrokerOp::SmbusReadBlockNoReg { bus: 2, addr: 0x47, len: 4 },
            BrokerOp::SmbusReadByteNoReg { bus: 2, addr: 0x47 },
            BrokerOp::ListBuses,
            BrokerOp::Health,
        ]
    }

    #[test]
    fn requests_round_trip_through_postcard() {
        for (i, op) in every_op().into_iter().enumerate() {
            let req = BrokerRequest::new(i as u64, op);
            let bytes = encode(&req).unwrap();
            assert!(bytes.len() <= MAX_FRAME_BYTES);
            let back: BrokerRequest = decode(&bytes).unwrap();
            assert_eq!(back, req);
        }
    }

    #[test]
    fn responses_round_trip_through_postcard() {
        let payloads = vec![
            Ok(BrokerPayload::Ack),
            Ok(BrokerPayload::Byte(0x1B)),
            Ok(BrokerPayload::Word(0xBEEF)),
            Ok(BrokerPayload::Block(vec![0xAB, 0x10])),
            Ok(BrokerPayload::Buses(vec![BusInfo {
                id: 0,
                name: "AMD PIIX4 Bus - 0x0b00".into(),
                kind: BusKind::Amd,
                vendor: 0x1022,
                device: 0x790B,
                sub_vendor: 0,
                sub_device: 0,
                max_block_len: 32,
            }])),
            Ok(BrokerPayload::Health(HealthReport {
                version: "0.0.1".into(),
                protocol: BROKER_PROTOCOL_VERSION,
                uptime_secs: 42,
                dry_run: true,
                ops_served: 7,
                modules: vec![ModuleStatus { name: "SmbusPIIX4".into(), sha256: "00".repeat(32), loaded: true }],
            })),
            Err(BrokerError::AddressDenied { addr: 0x50 }),
            Err(BrokerError::Backend { status: -5, message: "NVAPI_ERROR".into() }),
        ];
        for (i, result) in payloads.into_iter().enumerate() {
            let resp = BrokerResponse { id: i as u64, result };
            let bytes = encode(&resp).unwrap();
            let back: BrokerResponse = decode(&bytes).unwrap();
            assert_eq!(back, resp);
        }
    }

    #[test]
    fn validation_rules() {
        assert!(BrokerOp::SmbusReadByte { bus: 0, addr: 0x02, reg: 0 }.validate().is_err());
        assert!(BrokerOp::SmbusReadByte { bus: 0, addr: 0x78, reg: 0 }.validate().is_err());
        assert!(BrokerOp::SmbusWriteBlock { bus: 0, addr: 0x39, reg: 0, data: vec![] }.validate().is_err());
        assert!(BrokerOp::SmbusWriteBlock { bus: 0, addr: 0x39, reg: 0, data: vec![0; 65] }.validate().is_err());
        assert!(BrokerOp::SmbusReadBlock { bus: 0, addr: 0x39, reg: 0, len: 0 }.validate().is_err());
        assert!(BrokerOp::Health.validate().is_ok());
        for op in every_op() {
            assert!(op.validate().is_ok(), "{op:?}");
        }
    }

    #[test]
    fn spd_denial_only_applies_to_writes() {
        assert!(BrokerOp::SmbusWriteByte { bus: 0, addr: 0x50, reg: 0, value: 0 }.touches_spd());
        assert!(BrokerOp::SmbusWriteQuick { bus: 0, addr: 0x37 }.touches_spd());
        assert!(!BrokerOp::SmbusReadByte { bus: 0, addr: 0x50, reg: 0 }.touches_spd());
        assert!(!BrokerOp::SmbusWriteByte { bus: 0, addr: 0x39, reg: 0, value: 0 }.touches_spd());
    }
}
