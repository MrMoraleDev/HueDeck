// SPDX-License-Identifier: GPL-3.0-or-later
//! Daemon ⇄ io-broker protocol.
//!
//! The broker is a tiny LocalSystem service with a *closed* operation set:
//! every request is one [`BrokerOp`], every argument is range-checked by
//! [`BrokerOp::validate`] on both sides, and the response is a
//! [`BrokerResponse`] carrying the request id. Frames are length-prefixed
//! postcard messages (`u32 LE length` + payload) on the pipe.
//!
//! Version 2 adds the [`BrokerOp::BusLock`] / [`BrokerOp::BusUnlock`] lease
//! (a plugin's `TransactionLock`), [`BrokerError::BusContended`], the PawnIO
//! fields of [`HealthReport`], and bus-kind-aware rules: the DIMM write denial
//! and the 32-byte block limit depend on the bus kind
//! ([`BrokerOp::validate_for`], [`BrokerOp::touches_spd`]). Writes are only
//! accepted at the addresses of [`BROKER_WRITE_ALLOWLIST`]
//! ([`BrokerOp::write_allowed`]).
//!
//! Version 3 adds [`BrokerOp::SpdRead`]: a read of a DDR4 SPD EEPROM
//! (EE1004) across its two pages, for the host's SPD check of RAM plugins.
//! The page select (a Send Byte to 0x36 / 0x37, EE1004 "Set Page Address")
//! happens inside the broker as a step of that read, under the global SMBus
//! mutex, with page 0 selected again before the mutex is let go; no op sends
//! anything else to 0x30-0x37, which stay denied to every write op.
//! It also adds [`BrokerOp::Batch`]: up to [`MAX_BATCH_OPS`] transfers in
//! one request and one hold of the mutex, each checked as if sent alone
//! (one pipe round trip per frame of RAM colours instead of one per transfer).

use serde::{Deserialize, Serialize};

/// Protocol version; bumped on any incompatible change.
pub const BROKER_PROTOCOL_VERSION: u16 = 3;
/// Largest block payload the broker will accept on the wire (per-bus limits may be lower).
pub const MAX_BLOCK_LEN: usize = 64;
/// Largest block on a chipset bus (SMBus block transfers carry 1..=32 bytes).
pub const CHIPSET_BLOCK_MAX: usize = 32;
/// Longest [`BrokerOp::BusLock`] lease.
pub const BUS_LOCK_MAX_MS: u16 = 250;
/// Most ops in one [`BrokerOp::Batch`].
pub const MAX_BATCH_OPS: usize = 32;
/// A [`BrokerOp::Batch`] starts no op once it has run this long (it stops
/// with [`BrokerError::BatchTimeLimit`]; the client sends the rest again), so
/// one batch never keeps the global SMBus mutex much past the broker's hold cap.
pub const BATCH_HOLD_LIMIT_MS: u64 = 25;
/// The broker's named pipe (Windows).
pub const PIPE_NAME: &str = r"\\.\pipe\lightloom-iobroker";
/// Largest encoded request/response frame.
pub const MAX_FRAME_BYTES: usize = 4096;
/// Lowest valid 7-bit SMBus address.
pub const SMBUS_ADDR_MIN: u8 = 0x03;
/// Highest valid 7-bit SMBus address.
pub const SMBUS_ADDR_MAX: u8 = 0x77;
/// SPD EEPROM address range: writes denied on every bus.
pub const SPD_EEPROM_RANGE: std::ops::RangeInclusive<u8> = 0x50..=0x57;
/// SPD write-protect / page-select range: writes denied on DIMM buses.
pub const SPD_WRITE_PROTECT_RANGE: std::ops::RangeInclusive<u8> = 0x30..=0x37;
/// DDR5 PMIC range: writes denied on DIMM buses.
pub const DDR5_PMIC_RANGE: std::ops::RangeInclusive<u8> = 0x48..=0x4F;
/// [`BrokerOp::SpdRead`]: bytes of a DDR4 SPD (two 256-byte pages).
pub const SPD_DDR4_LEN: u16 = 512;
/// [`BrokerOp::SpdRead`]: EE1004 page size.
pub const SPD_PAGE_LEN: u16 = 256;
/// EE1004 Set Page Address commands (SPA0, SPA1): the only addresses of
/// 0x30-0x37 the broker ever sends to, from inside [`BrokerOp::SpdRead`].
pub const SPD_SET_PAGE: [u8; 2] = [0x36, 0x37];
/// EEPROM area (SPD, DDR5 RCD 0x58-0x5F, 24RF08-class EEPROMs): writes and
/// Quick commands denied on DIMM buses.
pub const EEPROM_AREA_RANGE: std::ops::RangeInclusive<u8> = 0x50..=0x5F;
/// The only addresses the broker writes to (Quick commands included): the
/// union of the `SmbusAddresses()` windows of the first-party chipset
/// plugins, today Crucial Ballistix (0x20-0x23, the 0x27 remap address,
/// 0x39-0x3C). The broker is the privilege boundary and does not know which
/// plugin asks, so this static list holds whatever a client sends; a new
/// chipset plugin adds its window here, after review. Reads are not limited.
pub const BROKER_WRITE_ALLOWLIST: &[u8] = &[0x20, 0x21, 0x22, 0x23, 0x27, 0x39, 0x3A, 0x3B, 0x3C];

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

impl BusKind {
    /// Chipset SMBus: System, Intel, Amd.
    pub fn is_chipset(self) -> bool {
        matches!(self, BusKind::System | BusKind::Intel | BusKind::Amd)
    }

    /// A bus that may reach the DIMMs: the chipset kinds and Nuvoton.
    pub fn is_dimm_bus(self) -> bool {
        self.is_chipset() || self == BusKind::Nuvoton
    }
}

/// Which PawnIO build the broker talks to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PawnIoEdition {
    /// The signed official driver: only namazso-signed modules load.
    Official,
    /// The unsigned / test-signed build that loads any module (refused).
    Unrestricted,
    /// Not determined (no device, or the probes did not run).
    Unknown,
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
    /// PawnIO driver version (`IOCTL_PIO_VERSION`, `maj<<16 | min<<8 | patch`), 0 if unknown.
    pub pawnio_version: u32,
    /// PawnIO edition.
    pub pawnio_edition: PawnIoEdition,
    /// `SYSTEM_CODEINTEGRITY_INFORMATION.CodeIntegrityOptions`.
    pub ci_options: u32,
    /// PIIX4 `ioctl_set_sleep_mode` setting (0 AlwaysBusy, 1 ShortBusy, 2 AlwaysSleep).
    pub sleep_mode: u8,
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
    /// SMBus Receive Byte.
    SmbusReadByteNoReg {
        /// Bus.
        bus: BusId,
        /// 7-bit address.
        addr: u8,
    },
    /// Read `len` bytes (1..=32, within one 256-byte page) of the DDR4 SPD
    /// EEPROM (EE1004) at `addr` (0x50..=0x57) from byte `offset` (0..512).
    /// The broker selects the page with an EE1004 Set Page Address (Send
    /// Byte, data 0x00, to 0x36 for page 0 or 0x37 for page 1), reads the
    /// bytes, and selects page 0 again, all under one hold of the global
    /// SMBus mutex. Not a write op ([`BrokerOp::is_write`]): the page select
    /// is volatile and put back. Served in dry-run too.
    SpdRead {
        /// Bus.
        bus: BusId,
        /// SPD address, 0x50..=0x57.
        addr: u8,
        /// First byte, 0..512.
        offset: u16,
        /// Bytes, 1..=32, not crossing a page.
        len: u8,
    },
    /// Up to [`MAX_BATCH_OPS`] plain transfers on `bus`, run in order under
    /// one hold of the global SMBus mutex (or inside the client's lease).
    /// Every op is checked exactly as if it came alone (validation, policy,
    /// rate limit, dry-run, audit). The batch stops at the first op that is
    /// refused or fails, and once [`BATCH_HOLD_LIMIT_MS`] of bus time has
    /// passed; the answer ([`BrokerPayload::Batch`]) holds the results of
    /// the ops done and the error of the op where it stopped. Only the
    /// `Smbus*` transfer ops may be batched (no `SpdRead`, lock or nested batch).
    Batch {
        /// Bus; every op must name it too.
        bus: BusId,
        /// 1..=[`MAX_BATCH_OPS`] transfer ops.
        ops: Vec<BrokerOp>,
    },
    /// Hold the bus (on chipset buses the global SMBus mutex) across the
    /// following ops until [`BrokerOp::BusUnlock`] or until the lease ends.
    BusLock {
        /// Bus.
        bus: BusId,
        /// Lease, 1..=[`BUS_LOCK_MAX_MS`] ms; the broker releases the bus when it runs out.
        lease_ms: u16,
    },
    /// Release a [`BrokerOp::BusLock`].
    BusUnlock {
        /// Bus.
        bus: BusId,
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
            | BrokerOp::SmbusReadByteNoReg { bus, .. }
            | BrokerOp::SpdRead { bus, .. }
            | BrokerOp::Batch { bus, .. }
            | BrokerOp::BusLock { bus, .. }
            | BrokerOp::BusUnlock { bus } => Some(*bus),
            BrokerOp::ListBuses | BrokerOp::Health => None,
        }
    }

    /// A single bus transfer: the ops a [`BrokerOp::Batch`] may carry.
    pub fn is_transfer(&self) -> bool {
        matches!(
            self,
            BrokerOp::SmbusWriteQuick { .. }
                | BrokerOp::SmbusReadByte { .. }
                | BrokerOp::SmbusWriteByte { .. }
                | BrokerOp::SmbusReadWord { .. }
                | BrokerOp::SmbusWriteWord { .. }
                | BrokerOp::SmbusWriteBlock { .. }
                | BrokerOp::SmbusReadBlock { .. }
                | BrokerOp::SmbusWriteBlockNoReg { .. }
                | BrokerOp::SmbusReadBlockNoReg { .. }
                | BrokerOp::SmbusReadByteNoReg { .. }
        )
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
            | BrokerOp::SmbusReadByteNoReg { addr, .. }
            | BrokerOp::SpdRead { addr, .. } => Some(*addr),
            BrokerOp::Batch { .. }
            | BrokerOp::BusLock { .. }
            | BrokerOp::BusUnlock { .. }
            | BrokerOp::ListBuses
            | BrokerOp::Health => None,
        }
    }

    /// Whether the op changes device state (subject to dry-run, SPD denial,
    /// allowlists). A Quick command counts as a write.
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
        if let BrokerOp::BusLock { lease_ms, .. } = self {
            if *lease_ms == 0 || *lease_ms > BUS_LOCK_MAX_MS {
                return Err(BrokerError::LengthOutOfRange {
                    len: usize::from(*lease_ms),
                });
            }
        }
        if let BrokerOp::SpdRead {
            addr, offset, len, ..
        } = self
        {
            if !SPD_EEPROM_RANGE.contains(addr) {
                return Err(BrokerError::AddressDenied { addr: *addr });
            }
            let in_page = usize::from(*offset % SPD_PAGE_LEN);
            if *offset >= SPD_DDR4_LEN || in_page + usize::from(*len) > usize::from(SPD_PAGE_LEN) {
                return Err(BrokerError::LengthOutOfRange {
                    len: usize::from(*offset) + usize::from(*len),
                });
            }
        }
        if let BrokerOp::Batch { bus, ops } = self {
            if ops.is_empty() || ops.len() > MAX_BATCH_OPS {
                return Err(BrokerError::LengthOutOfRange { len: ops.len() });
            }
            for op in ops {
                if !op.is_transfer() {
                    return Err(BrokerError::BadRequest(format!("{op:?} cannot be batched")));
                }
                if op.bus() != Some(*bus) {
                    return Err(BrokerError::BadRequest("a batch names one bus".into()));
                }
                op.validate()?;
            }
            return Ok(());
        }
        let len = self.block_len();
        if len == 0 || len > MAX_BLOCK_LEN {
            return Err(BrokerError::LengthOutOfRange { len });
        }
        Ok(())
    }

    /// Bus transfers the op makes (rate limit): an SPD read is its bytes
    /// plus the two page selects; a batch is charged op by op; anything else one.
    pub fn transfers(&self) -> u32 {
        match self {
            BrokerOp::SpdRead { len, .. } => u32::from(*len) + 2,
            _ => 1,
        }
    }

    /// [`BrokerOp::validate`] plus the limits of a bus of `kind`: blocks of at
    /// most [`CHIPSET_BLOCK_MAX`] bytes on chipset buses (in a batch, for
    /// every op).
    pub fn validate_for(&self, kind: BusKind) -> Result<(), BrokerError> {
        self.validate()?;
        if let BrokerOp::Batch { ops, .. } = self {
            return ops.iter().try_for_each(|op| op.validate_for(kind));
        }
        let len = self.block_len();
        if kind.is_chipset() && len > CHIPSET_BLOCK_MAX {
            return Err(BrokerError::LengthOutOfRange { len });
        }
        Ok(())
    }

    /// Bytes moved by a block op (1 for every other op).
    fn block_len(&self) -> usize {
        match self {
            BrokerOp::SmbusWriteBlock { data, .. } | BrokerOp::SmbusWriteBlockNoReg { data, .. } => {
                data.len()
            }
            BrokerOp::SmbusReadBlock { len, .. }
            | BrokerOp::SmbusReadBlockNoReg { len, .. }
            | BrokerOp::SpdRead { len, .. } => *len as usize,
            _ => 1,
        }
    }

    /// Deny rule for the DIMMs' own parts: a write (or Quick command) to the
    /// SPD EEPROM on any bus, and on a DIMM bus also to the write-protect
    /// addresses, the DDR5 PMIC and the EEPROM area 0x50-0x5F. GPU buses keep
    /// the others (the ASRock GPU plugin quick-writes 0x36 on its ADLX bus).
    /// Mirrors `lightloom_hal::smbus::write_denied`.
    pub fn touches_spd(&self, kind: BusKind) -> bool {
        self.is_write()
            && self.addr().is_some_and(|a| {
                SPD_EEPROM_RANGE.contains(&a)
                    || (kind.is_dimm_bus()
                        && (SPD_WRITE_PROTECT_RANGE.contains(&a)
                            || DDR5_PMIC_RANGE.contains(&a)
                            || EEPROM_AREA_RANGE.contains(&a)))
            })
    }

    /// Whether the broker may send this op: anything but a write, or a write
    /// to an address of [`BROKER_WRITE_ALLOWLIST`].
    pub fn write_allowed(&self) -> bool {
        !self.is_write() || self.addr().is_some_and(|a| BROKER_WRITE_ALLOWLIST.contains(&a))
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
        Self {
            version: BROKER_PROTOCOL_VERSION,
            id,
            op,
        }
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
    /// Answer to [`BrokerOp::Batch`]: one result per op done, in order, and
    /// when the batch stopped early the error of op `results.len()` (the ops
    /// after it were not sent).
    Batch {
        /// Results of the ops done.
        results: Vec<BrokerPayload>,
        /// Why op `results.len()` was not done, if the batch stopped there.
        error: Option<BrokerError>,
    },
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
    /// Address outside the write allowlist (the plugin's Scan()-derived one in
    /// the client, [`BROKER_WRITE_ALLOWLIST`] in the broker) or in a deny range.
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
    /// The bus (its global SMBus mutex, or another client's lease) stayed busy.
    #[error("bus busy: another program holds it")]
    BusContended,
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
    /// A [`BrokerOp::Batch`] reached [`BATCH_HOLD_LIMIT_MS`]: the op it
    /// stopped at was not sent (send it and the rest again).
    #[error("batch time limit reached")]
    BatchTimeLimit,
}

/// A response frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrokerResponse {
    /// Echo of the request id.
    pub id: u64,
    /// Result.
    pub result: Result<BrokerPayload, BrokerError>,
}

/// Read one frame (`u32 LE length` + payload, at most [`MAX_FRAME_BYTES`]);
/// `None` when the peer closed the stream before a new frame.
pub fn read_frame<R: std::io::Read>(r: &mut R) -> std::io::Result<Option<Vec<u8>>> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len) as usize;
    if len == 0 || len > MAX_FRAME_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("frame of {len} bytes"),
        ));
    }
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)?;
    Ok(Some(payload))
}

/// Write one frame.
pub fn write_frame<W: std::io::Write>(w: &mut W, payload: &[u8]) -> std::io::Result<()> {
    if payload.is_empty() || payload.len() > MAX_FRAME_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("frame of {} bytes", payload.len()),
        ));
    }
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(payload);
    w.write_all(&frame)?;
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{decode, encode};

    fn every_op() -> Vec<BrokerOp> {
        vec![
            BrokerOp::SmbusWriteQuick { bus: 0, addr: 0x39 },
            BrokerOp::SmbusReadByte {
                bus: 0,
                addr: 0x39,
                reg: 0xA0,
            },
            BrokerOp::SmbusWriteByte {
                bus: 0,
                addr: 0x39,
                reg: 0x01,
                value: 0xFF,
            },
            BrokerOp::SmbusReadWord {
                bus: 1,
                addr: 0x5B,
                reg: 0x02,
            },
            BrokerOp::SmbusWriteWord {
                bus: 1,
                addr: 0x5B,
                reg: 0x00,
                value: 0x80F8,
            },
            BrokerOp::SmbusWriteBlock {
                bus: 0,
                addr: 0x39,
                reg: 0x03,
                data: vec![1, 2, 3, 4],
            },
            BrokerOp::SmbusReadBlock {
                bus: 0,
                addr: 0x39,
                reg: 0x03,
                len: 12,
            },
            BrokerOp::SmbusWriteBlockNoReg {
                bus: 2,
                addr: 0x47,
                data: vec![0xAB],
            },
            BrokerOp::SmbusReadBlockNoReg {
                bus: 2,
                addr: 0x47,
                len: 4,
            },
            BrokerOp::SmbusReadByteNoReg { bus: 2, addr: 0x47 },
            BrokerOp::SpdRead {
                bus: 0,
                addr: 0x50,
                offset: 320,
                len: 2,
            },
            BrokerOp::Batch {
                bus: 0,
                ops: vec![
                    BrokerOp::SmbusWriteWord {
                        bus: 0,
                        addr: 0x39,
                        reg: 0x00,
                        value: 0x0083,
                    },
                    BrokerOp::SmbusWriteBlock {
                        bus: 0,
                        addr: 0x39,
                        reg: 0x03,
                        data: vec![0xFF; 8],
                    },
                ],
            },
            BrokerOp::BusLock {
                bus: 0,
                lease_ms: 100,
            },
            BrokerOp::BusUnlock { bus: 0 },
            BrokerOp::ListBuses,
            BrokerOp::Health,
        ]
    }

    #[test]
    fn batches_are_bounded_and_carry_transfers_of_their_bus_only() {
        let write = |bus, addr| BrokerOp::SmbusWriteByte {
            bus,
            addr,
            reg: 1,
            value: 2,
        };
        let batch = |ops: Vec<BrokerOp>| BrokerOp::Batch { bus: 0, ops };
        assert!(batch(vec![write(0, 0x39)]).validate().is_ok());
        assert!(batch(Vec::new()).validate().is_err(), "empty");
        assert!(batch(vec![write(0, 0x39); MAX_BATCH_OPS]).validate().is_ok());
        assert!(batch(vec![write(0, 0x39); MAX_BATCH_OPS + 1]).validate().is_err());
        assert!(batch(vec![write(1, 0x39)]).validate().is_err(), "another bus");
        assert!(
            batch(vec![write(0, 0x02)]).validate().is_err(),
            "each op validated"
        );
        for not_a_transfer in [
            BrokerOp::SpdRead {
                bus: 0,
                addr: 0x50,
                offset: 0,
                len: 1,
            },
            BrokerOp::BusLock { bus: 0, lease_ms: 10 },
            BrokerOp::BusUnlock { bus: 0 },
            batch(vec![write(0, 0x39)]),
        ] {
            assert!(
                batch(vec![not_a_transfer.clone()]).validate().is_err(),
                "{not_a_transfer:?}"
            );
        }
        let big = BrokerOp::SmbusWriteBlock {
            bus: 0,
            addr: 0x39,
            reg: 3,
            data: vec![0; 33],
        };
        assert!(batch(vec![big.clone()]).validate().is_ok());
        assert!(
            batch(vec![big]).validate_for(BusKind::Amd).is_err(),
            "chipset block limit per op"
        );
        assert!(
            !batch(vec![write(0, 0x50)]).is_write(),
            "each op is checked on its own"
        );
        // The largest batch still fits a frame.
        let widest = BrokerOp::SmbusWriteBlock {
            bus: 0,
            addr: 0x39,
            reg: 3,
            data: vec![0xFF; MAX_BLOCK_LEN],
        };
        let req = BrokerRequest::new(u64::MAX, batch(vec![widest; MAX_BATCH_OPS]));
        assert!(encode(&req).unwrap().len() <= MAX_FRAME_BYTES);
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
                modules: vec![ModuleStatus {
                    name: "SmbusPIIX4".into(),
                    sha256: "00".repeat(32),
                    loaded: true,
                }],
                pawnio_version: 0x0002_0200,
                pawnio_edition: PawnIoEdition::Official,
                ci_options: 0xF401,
                sleep_mode: 1,
            })),
            Ok(BrokerPayload::Batch {
                results: vec![BrokerPayload::Ack, BrokerPayload::Byte(7)],
                error: Some(BrokerError::Nack),
            }),
            Err(BrokerError::AddressDenied { addr: 0x50 }),
            Err(BrokerError::BusContended),
            Err(BrokerError::BatchTimeLimit),
            Err(BrokerError::Backend {
                status: -5,
                message: "NVAPI_ERROR".into(),
            }),
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
        assert!(BrokerOp::SmbusReadByte {
            bus: 0,
            addr: 0x02,
            reg: 0
        }
        .validate()
        .is_err());
        assert!(BrokerOp::SmbusReadByte {
            bus: 0,
            addr: 0x78,
            reg: 0
        }
        .validate()
        .is_err());
        assert!(BrokerOp::SmbusWriteBlock {
            bus: 0,
            addr: 0x39,
            reg: 0,
            data: vec![]
        }
        .validate()
        .is_err());
        assert!(BrokerOp::SmbusWriteBlock {
            bus: 0,
            addr: 0x39,
            reg: 0,
            data: vec![0; 65]
        }
        .validate()
        .is_err());
        assert!(BrokerOp::SmbusReadBlock {
            bus: 0,
            addr: 0x39,
            reg: 0,
            len: 0
        }
        .validate()
        .is_err());
        assert!(BrokerOp::Health.validate().is_ok());
        for op in every_op() {
            assert!(op.validate().is_ok(), "{op:?}");
        }
    }

    #[test]
    fn spd_denial_only_applies_to_writes() {
        assert!(BrokerOp::SmbusWriteByte {
            bus: 0,
            addr: 0x50,
            reg: 0,
            value: 0
        }
        .touches_spd(BusKind::Amd));
        assert!(BrokerOp::SmbusWriteQuick { bus: 0, addr: 0x37 }.touches_spd(BusKind::Amd));
        assert!(!BrokerOp::SmbusReadByte {
            bus: 0,
            addr: 0x50,
            reg: 0
        }
        .touches_spd(BusKind::Amd));
        assert!(!BrokerOp::SmbusWriteByte {
            bus: 0,
            addr: 0x39,
            reg: 0,
            value: 0
        }
        .touches_spd(BusKind::Amd));
        assert!(!BrokerOp::BusLock { bus: 0, lease_ms: 1 }.touches_spd(BusKind::Amd));
    }

    #[test]
    fn spd_denial_depends_on_bus_kind() {
        let quick = |addr| BrokerOp::SmbusWriteQuick { bus: 0, addr };
        for kind in [BusKind::System, BusKind::Intel, BusKind::Amd, BusKind::Nuvoton] {
            for addr in [0x30, 0x36, 0x48, 0x4F, 0x50, 0x57, 0x58, 0x5F] {
                assert!(quick(addr).touches_spd(kind), "{kind:?} 0x{addr:02x}");
            }
            for addr in [0x2F, 0x39, 0x47, 0x60] {
                assert!(!quick(addr).touches_spd(kind), "{kind:?} 0x{addr:02x}");
            }
        }
        for kind in [BusKind::Nvidia, BusKind::AmdGpu] {
            assert!(quick(0x50).touches_spd(kind) && quick(0x57).touches_spd(kind));
            for addr in [0x36, 0x48, 0x58] {
                assert!(!quick(addr).touches_spd(kind), "{kind:?} 0x{addr:02x}");
            }
        }
    }

    #[test]
    fn write_allowlist_holds_writes_only() {
        let write = |addr| BrokerOp::SmbusWriteByte {
            bus: 0,
            addr,
            reg: 0,
            value: 0,
        };
        for &addr in BROKER_WRITE_ALLOWLIST {
            assert!(write(addr).write_allowed(), "0x{addr:02x}");
            assert!(
                !write(addr).touches_spd(BusKind::Amd),
                "allowlist and deny ranges overlap"
            );
        }
        for addr in [0x08, 0x24, 0x2D, 0x3D, 0x48, 0x50, 0x68, 0x77] {
            assert!(!write(addr).write_allowed(), "0x{addr:02x}");
            assert!(!BrokerOp::SmbusWriteQuick { bus: 0, addr }.write_allowed());
        }
        assert!(BrokerOp::SmbusReadByte {
            bus: 0,
            addr: 0x68,
            reg: 0
        }
        .write_allowed());
        assert!(BrokerOp::SmbusReadByteNoReg { bus: 0, addr: 0x50 }.write_allowed());
    }

    #[test]
    fn spd_read_is_bounded_and_not_a_write() {
        let spd = |addr, offset, len| BrokerOp::SpdRead {
            bus: 0,
            addr,
            offset,
            len,
        };
        assert!(spd(0x50, 2, 1).validate().is_ok());
        assert!(spd(0x57, 329, 20).validate().is_ok());
        assert!(spd(0x50, 0x1E0, 32).validate_for(BusKind::Amd).is_ok());
        for addr in [0x36, 0x37, 0x30, 0x4F, 0x58, 0x39] {
            assert_eq!(
                spd(addr, 0, 1).validate(),
                Err(BrokerError::AddressDenied { addr }),
                "SPD addresses only: 0x{addr:02x}"
            );
        }
        assert!(spd(0x50, 0, 0).validate().is_err());
        assert!(spd(0x50, 0, 33).validate_for(BusKind::Amd).is_err());
        assert!(spd(0x50, 250, 8).validate().is_err(), "crosses a page");
        assert!(spd(0x50, 512, 1).validate().is_err());
        let op = spd(0x51, 320, 2);
        assert!(!op.is_write() && !op.touches_spd(BusKind::Amd) && op.write_allowed());
        assert_eq!(op.transfers(), 4);
    }

    /// Every op a client can send that writes, aimed at 0x30-0x37 (the
    /// EE1004 page select and write protection among them), is denied on
    /// DIMM buses; [`BrokerOp::SpdRead`] takes no address in that range.
    #[test]
    fn no_client_op_writes_the_page_select_or_write_protect_addresses() {
        for addr in 0x30..=0x37u8 {
            let writes = [
                BrokerOp::SmbusWriteQuick { bus: 0, addr },
                BrokerOp::SmbusWriteByte {
                    bus: 0,
                    addr,
                    reg: 0,
                    value: 0xFF,
                },
                BrokerOp::SmbusWriteWord {
                    bus: 0,
                    addr,
                    reg: 0,
                    value: 0,
                },
                BrokerOp::SmbusWriteBlock {
                    bus: 0,
                    addr,
                    reg: 0,
                    data: vec![0],
                },
                BrokerOp::SmbusWriteBlockNoReg {
                    bus: 0,
                    addr,
                    data: vec![0],
                },
            ];
            for op in writes {
                for kind in [BusKind::System, BusKind::Amd, BusKind::Intel, BusKind::Nuvoton] {
                    assert!(op.touches_spd(kind), "{op:?} on {kind:?}");
                }
                assert!(!op.write_allowed(), "{op:?} outside the allowlist too");
            }
            assert!(BrokerOp::SpdRead {
                bus: 0,
                addr,
                offset: 0,
                len: 1
            }
            .validate()
            .is_err());
        }
        for a in SPD_SET_PAGE {
            assert!(!BROKER_WRITE_ALLOWLIST.contains(&a));
        }
    }

    #[test]
    fn bus_lock_lease_bounds() {
        let lease = |lease_ms| BrokerOp::BusLock { bus: 0, lease_ms }.validate();
        assert!(lease(0).is_err());
        assert!(lease(1).is_ok());
        assert!(lease(BUS_LOCK_MAX_MS).is_ok());
        assert!(lease(BUS_LOCK_MAX_MS + 1).is_err());
        assert!(!BrokerOp::BusLock { bus: 0, lease_ms: 50 }.is_write());
        assert_eq!(BrokerOp::BusUnlock { bus: 3 }.bus(), Some(3));
        assert_eq!(BrokerOp::BusUnlock { bus: 3 }.addr(), None);
    }

    #[test]
    fn chipset_block_limit_is_32() {
        let block = |n: usize| BrokerOp::SmbusWriteBlock {
            bus: 0,
            addr: 0x39,
            reg: 3,
            data: vec![0; n],
        };
        assert!(block(32).validate_for(BusKind::Amd).is_ok());
        assert_eq!(
            block(33).validate_for(BusKind::Amd),
            Err(BrokerError::LengthOutOfRange { len: 33 })
        );
        assert!(block(33).validate_for(BusKind::Nvidia).is_ok());
        assert!(block(65).validate_for(BusKind::Nvidia).is_err());
    }
}
