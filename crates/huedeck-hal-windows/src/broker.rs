// SPDX-License-Identifier: GPL-3.0-or-later
//! Client of the HueDeck I/O broker (`huedeck-iobroker`): the chipset SMBus
//! buses it serves, as `huedeck_hal::SmbusBus` objects.
//!
//! One connection to `\\.\pipe\huedeck-iobroker` ([`PipeTransport`]) is
//! shared by every bus of the broker; each operation is one request and one
//! response (`huedeck_protocol::broker`). Failures reach plugins as `-1`, as
//! on SignalRGB's chipset buses. Block reads and register-less block
//! transfers do not exist on these buses and are refused here already.
//! `TransactionLock` maps to the broker's `BusLock` lease, which holds the
//! global SMBus mutex across the locked operations.
//!
//! This process never opens PawnIO: only the broker (LocalSystem) does.

use huedeck_hal::smbus::{PciIds, SmbusBus, SmbusBusDescriptor, SmbusBusKind};
use huedeck_hal::{HalError, HalResult};
use huedeck_protocol::broker::{BrokerError, BrokerOp, BrokerPayload, BusId, BusInfo, BusKind};
use std::sync::{Arc, Mutex};

/// Something that carries broker requests: the pipe, or a test double.
pub trait BrokerTransport: Send {
    /// Send `op`, wait for its answer.
    fn call(&mut self, op: BrokerOp) -> Result<BrokerPayload, BrokerError>;
}

/// One connection shared by the buses of a broker.
pub type SharedTransport = Arc<Mutex<Box<dyn BrokerTransport>>>;

fn call(t: &SharedTransport, op: BrokerOp) -> Result<BrokerPayload, BrokerError> {
    t.lock().unwrap_or_else(|e| e.into_inner()).call(op)
}

fn kind(k: BusKind) -> SmbusBusKind {
    match k {
        BusKind::System => SmbusBusKind::System,
        BusKind::Nvidia => SmbusBusKind::Nvidia,
        BusKind::AmdGpu => SmbusBusKind::AmdGpu,
        BusKind::Nuvoton => SmbusBusKind::Nuvoton,
        BusKind::Intel => SmbusBusKind::Intel,
        BusKind::Amd => SmbusBusKind::Amd,
    }
}

/// `"AMD PIIX4 Bus - 0x0b00"` → `"0b00"`; otherwise `"bus<id>"`.
fn instance(info: &BusInfo) -> String {
    match info.name.rsplit_once("0x") {
        Some((_, hex)) if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) => {
            hex.to_ascii_lowercase()
        }
        _ => format!("bus{}", info.id),
    }
}

/// A broker error as the HAL sees it: `-1` for the plugin (chipset parity).
fn hal_error(e: BrokerError) -> HalError {
    match e {
        BrokerError::Unsupported => HalError::Unsupported("operation not served by the I/O broker"),
        BrokerError::BusContended => HalError::Timeout(u32::from(huedeck_protocol::broker::BUS_LOCK_MAX_MS)),
        _ => HalError::Bus { status: -1 },
    }
}

fn unexpected(p: BrokerPayload) -> HalError {
    HalError::InvalidArgument(format!("unexpected broker answer {p:?}"))
}

/// One chipset bus served by the broker.
pub struct BrokerSmbus {
    descriptor: SmbusBusDescriptor,
    bus: BusId,
    transport: SharedTransport,
}

impl std::fmt::Debug for BrokerSmbus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrokerSmbus")
            .field("descriptor", &self.descriptor)
            .field("bus", &self.bus)
            .finish_non_exhaustive()
    }
}

impl BrokerSmbus {
    /// The bus `info` of the broker behind `transport`.
    pub fn new(info: &BusInfo, transport: SharedTransport) -> Self {
        Self {
            descriptor: SmbusBusDescriptor {
                name: info.name.clone(),
                kind: kind(info.kind),
                pci: PciIds {
                    vendor: info.vendor,
                    device: info.device,
                    sub_vendor: info.sub_vendor,
                    sub_device: info.sub_device,
                },
                instance: instance(info),
                max_block_len: usize::from(info.max_block_len),
            },
            bus: info.id,
            transport,
        }
    }

    fn ack(&self, op: BrokerOp) -> HalResult<()> {
        match call(&self.transport, op).map_err(hal_error)? {
            BrokerPayload::Ack => Ok(()),
            other => Err(unexpected(other)),
        }
    }

    fn byte(&self, op: BrokerOp) -> HalResult<u8> {
        match call(&self.transport, op).map_err(hal_error)? {
            BrokerPayload::Byte(b) => Ok(b),
            other => Err(unexpected(other)),
        }
    }
}

/// Every bus the broker serves, checking it speaks this protocol version.
pub fn broker_buses(transport: SharedTransport) -> Result<Vec<BrokerSmbus>, BrokerError> {
    match call(&transport, BrokerOp::Health)? {
        BrokerPayload::Health(h) if h.protocol == huedeck_protocol::broker::BROKER_PROTOCOL_VERSION => {}
        BrokerPayload::Health(h) => return Err(BrokerError::UnsupportedVersion(h.protocol)),
        other => return Err(BrokerError::BadRequest(format!("unexpected answer {other:?}"))),
    }
    match call(&transport, BrokerOp::ListBuses)? {
        BrokerPayload::Buses(list) => Ok(list
            .iter()
            .map(|info| BrokerSmbus::new(info, Arc::clone(&transport)))
            .collect()),
        other => Err(BrokerError::BadRequest(format!("unexpected answer {other:?}"))),
    }
}

impl SmbusBus for BrokerSmbus {
    fn descriptor(&self) -> &SmbusBusDescriptor {
        &self.descriptor
    }

    fn write_quick(&mut self, addr: u8) -> HalResult<()> {
        self.ack(BrokerOp::SmbusWriteQuick { bus: self.bus, addr })
    }

    fn read_byte(&mut self, addr: u8, reg: u8) -> HalResult<u8> {
        self.byte(BrokerOp::SmbusReadByte {
            bus: self.bus,
            addr,
            reg,
        })
    }

    fn write_byte(&mut self, addr: u8, reg: u8, value: u8) -> HalResult<()> {
        self.ack(BrokerOp::SmbusWriteByte {
            bus: self.bus,
            addr,
            reg,
            value,
        })
    }

    fn read_word(&mut self, addr: u8, reg: u8) -> HalResult<u16> {
        match call(
            &self.transport,
            BrokerOp::SmbusReadWord {
                bus: self.bus,
                addr,
                reg,
            },
        )
        .map_err(hal_error)?
        {
            BrokerPayload::Word(w) => Ok(w),
            other => Err(unexpected(other)),
        }
    }

    fn write_word(&mut self, addr: u8, reg: u8, value: u16) -> HalResult<()> {
        self.ack(BrokerOp::SmbusWriteWord {
            bus: self.bus,
            addr,
            reg,
            value,
        })
    }

    fn write_block(&mut self, addr: u8, reg: u8, data: &[u8]) -> HalResult<()> {
        self.ack(BrokerOp::SmbusWriteBlock {
            bus: self.bus,
            addr,
            reg,
            data: data.to_vec(),
        })
    }

    fn read_block(&mut self, _addr: u8, _reg: u8, _buf: &mut [u8]) -> HalResult<usize> {
        Err(HalError::Unsupported("ReadBlockBytes on a chipset bus"))
    }

    fn write_block_no_reg(&mut self, _addr: u8, _data: &[u8]) -> HalResult<()> {
        Err(HalError::Unsupported(
            "WriteBlockWithoutRegister on a chipset bus",
        ))
    }

    fn read_block_no_reg(&mut self, _addr: u8, _buf: &mut [u8]) -> HalResult<usize> {
        Err(HalError::Unsupported("ReadBlockWithoutRegister on a chipset bus"))
    }

    fn read_byte_no_reg(&mut self, addr: u8) -> HalResult<u8> {
        self.byte(BrokerOp::SmbusReadByteNoReg { bus: self.bus, addr })
    }

    fn bus_lock(&mut self, lease_ms: u16) -> HalResult<()> {
        self.ack(BrokerOp::BusLock {
            bus: self.bus,
            lease_ms,
        })
    }

    fn bus_unlock(&mut self) -> HalResult<()> {
        self.ack(BrokerOp::BusUnlock { bus: self.bus })
    }
}

#[cfg(windows)]
pub use pipe::PipeTransport;

#[cfg(windows)]
mod pipe {
    use super::*;
    use huedeck_protocol::broker::{read_frame, write_frame, BrokerRequest, BrokerResponse, PIPE_NAME};
    use std::fs::File;
    use std::io;

    /// A connection to the broker's named pipe.
    #[derive(Debug)]
    pub struct PipeTransport {
        pipe: File,
        next_id: u64,
    }

    impl PipeTransport {
        /// Connect; `NotFound` when the broker service is not running.
        pub fn connect() -> io::Result<Self> {
            let pipe = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(PIPE_NAME)?;
            Ok(Self { pipe, next_id: 1 })
        }

        /// [`PipeTransport::connect`] as a [`SharedTransport`].
        pub fn shared() -> io::Result<SharedTransport> {
            Ok(Arc::new(Mutex::new(Box::new(Self::connect()?))))
        }

        fn round_trip(&mut self, op: BrokerOp) -> io::Result<BrokerResponse> {
            let id = self.next_id;
            self.next_id += 1;
            let req = huedeck_protocol::encode(&BrokerRequest::new(id, op)).map_err(io::Error::other)?;
            write_frame(&mut self.pipe, &req)?;
            let bytes = read_frame(&mut self.pipe)?
                .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "broker closed the pipe"))?;
            let resp: BrokerResponse = huedeck_protocol::decode(&bytes).map_err(io::Error::other)?;
            if resp.id != id {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("answer {} to request {id}", resp.id),
                ));
            }
            Ok(resp)
        }
    }

    impl BrokerTransport for PipeTransport {
        fn call(&mut self, op: BrokerOp) -> Result<BrokerPayload, BrokerError> {
            op.validate()?;
            match self.round_trip(op) {
                Ok(resp) => resp.result,
                Err(e) => Err(BrokerError::Backend {
                    status: -1,
                    message: format!("I/O broker pipe: {e}"),
                }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use huedeck_protocol::broker::{HealthReport, PawnIoEdition, BROKER_PROTOCOL_VERSION};

    /// Records ops; answers from a script.
    struct FakeTransport {
        log: Arc<Mutex<Vec<BrokerOp>>>,
        answer: fn(&BrokerOp) -> Result<BrokerPayload, BrokerError>,
    }

    impl BrokerTransport for FakeTransport {
        fn call(&mut self, op: BrokerOp) -> Result<BrokerPayload, BrokerError> {
            let r = (self.answer)(&op);
            self.log.lock().unwrap().push(op);
            r
        }
    }

    fn info(id: u8, name: &str) -> BusInfo {
        BusInfo {
            id,
            name: name.into(),
            kind: BusKind::Amd,
            vendor: 0x1022,
            device: 0x790B,
            sub_vendor: 0x1022,
            sub_device: 0x790B,
            max_block_len: 32,
        }
    }

    fn broker(op: &BrokerOp) -> Result<BrokerPayload, BrokerError> {
        match op {
            BrokerOp::Health => Ok(BrokerPayload::Health(HealthReport {
                version: "0.0.1".into(),
                protocol: BROKER_PROTOCOL_VERSION,
                uptime_secs: 1,
                dry_run: false,
                ops_served: 0,
                modules: Vec::new(),
                pawnio_version: 0x0002_0200,
                pawnio_edition: PawnIoEdition::Official,
                ci_options: 0,
                sleep_mode: 1,
            })),
            BrokerOp::ListBuses => Ok(BrokerPayload::Buses(vec![
                info(0, "AMD PIIX4 Bus - 0x0b00"),
                info(1, "AMD PIIX4 Bus - 0x0b20"),
            ])),
            BrokerOp::SmbusReadByte { addr: 0x39, .. } => Ok(BrokerPayload::Byte(0x07)),
            BrokerOp::SmbusReadByte { .. } => Err(BrokerError::Nack),
            BrokerOp::SmbusWriteByte { addr: 0x50, .. } => Err(BrokerError::AddressDenied { addr: 0x50 }),
            _ => Ok(BrokerPayload::Ack),
        }
    }

    fn rig() -> (Vec<BrokerSmbus>, Arc<Mutex<Vec<BrokerOp>>>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let t: SharedTransport = Arc::new(Mutex::new(Box::new(FakeTransport {
            log: Arc::clone(&log),
            answer: broker,
        })));
        (broker_buses(t).unwrap(), log)
    }

    #[test]
    fn list_buses_become_descriptors() {
        let (buses, log) = rig();
        let keys: Vec<String> = buses.iter().map(|b| b.descriptor().key()).collect();
        assert_eq!(keys, vec!["1022:790b:1022:790b@0b00", "1022:790b:1022:790b@0b20"]);
        assert_eq!(buses[1].descriptor().name, "AMD PIIX4 Bus - 0x0b20");
        assert_eq!(buses[0].descriptor().kind, SmbusBusKind::Amd);
        assert_eq!(buses[0].descriptor().max_block_len, 32);
        assert_eq!(*log.lock().unwrap(), vec![BrokerOp::Health, BrokerOp::ListBuses]);
        assert_eq!(instance(&info(3, "Some bus")), "bus3");
    }

    #[test]
    fn block_write_goes_as_one_op() {
        let (mut buses, log) = rig();
        log.lock().unwrap().clear();
        buses[0]
            .write_block(0x39, 0x03, &[1, 2, 3, 4, 5, 6, 7, 8])
            .unwrap();
        assert_eq!(
            *log.lock().unwrap(),
            vec![BrokerOp::SmbusWriteBlock {
                bus: 0,
                addr: 0x39,
                reg: 0x03,
                data: vec![1, 2, 3, 4, 5, 6, 7, 8]
            }]
        );
        let mut buf = [0u8; 4];
        assert!(matches!(
            buses[0].read_block(0x39, 0, &mut buf),
            Err(HalError::Unsupported(_))
        ));
        assert_eq!(log.lock().unwrap().len(), 1, "refused without asking the broker");
    }

    #[test]
    fn lock_maps_to_bus_lock() {
        let (mut buses, log) = rig();
        log.lock().unwrap().clear();
        buses[1].bus_lock(100).unwrap();
        buses[1].bus_unlock().unwrap();
        assert_eq!(
            *log.lock().unwrap(),
            vec![
                BrokerOp::BusLock {
                    bus: 1,
                    lease_ms: 100
                },
                BrokerOp::BusUnlock { bus: 1 }
            ]
        );
    }

    #[test]
    fn broker_errors_map_to_negative_status() {
        let (mut buses, _) = rig();
        assert_eq!(buses[0].read_byte(0x39, 0xA0).unwrap(), 0x07);
        let nack = buses[0].read_byte(0x3A, 0xA0).unwrap_err();
        assert_eq!(nack.plugin_status(), -1);
        let denied = buses[0].write_byte(0x50, 0, 0).unwrap_err();
        assert!(matches!(denied, HalError::Bus { status: -1 }));
        assert!(matches!(
            hal_error(BrokerError::BusContended),
            HalError::Timeout(_)
        ));
        assert!(matches!(
            hal_error(BrokerError::Unsupported),
            HalError::Unsupported(_)
        ));
    }
}
