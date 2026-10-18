// SPDX-License-Identifier: GPL-3.0-or-later
//! `lightloom-iobroker` — privileged I/O broker (M5).
//!
//! Will run as a Windows service (LocalSystem), installed once by
//! `lightloom-setup` only when the user opts into SMBus / GPU-I2C / SuperIO.
//! Single-threaded request loop over a named pipe, closed op enum
//! (`lightloom_protocol::broker`), every SMBus transaction under
//! `Global\Access_SMBUS.HTP.Method`, Scan()-derived address allowlists, SPD
//! write denial, rate limit, dry-run, audit log. Linux needs no broker.
//!
//! M0 stub: decodes nothing, answers `Health` only, exits.

#![forbid(unsafe_code)]

use lightloom_protocol::broker::{
    BrokerError, BrokerOp, BrokerPayload, BrokerRequest, BrokerResponse, HealthReport,
};

/// Dispatch one request. Only `Health` is answered until the PawnIO client exists.
fn handle(req: &BrokerRequest) -> BrokerResponse {
    let result = if req.version != lightloom_protocol::broker::BROKER_PROTOCOL_VERSION {
        Err(BrokerError::UnsupportedVersion(req.version))
    } else if let Err(e) = req.op.validate() {
        Err(e)
    } else {
        match req.op {
            BrokerOp::Health => Ok(BrokerPayload::Health(HealthReport {
                version: env!("CARGO_PKG_VERSION").to_string(),
                protocol: lightloom_protocol::broker::BROKER_PROTOCOL_VERSION,
                uptime_secs: 0,
                dry_run: true,
                ops_served: 0,
                modules: Vec::new(),
            })),
            BrokerOp::ListBuses => Ok(BrokerPayload::Buses(Vec::new())),
            _ => Err(BrokerError::Unsupported),
        }
    };
    BrokerResponse { id: req.id, result }
}

fn main() {
    println!(
        "lightloom-iobroker {} (stub; the service lands in M5)",
        env!("CARGO_PKG_VERSION")
    );
    let resp = handle(&BrokerRequest::new(0, BrokerOp::Health));
    if let Ok(BrokerPayload::Health(h)) = resp.result {
        println!("protocol v{}, dry_run={}", h.protocol, h.dry_run);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_round_trips_and_writes_are_refused() {
        let req = BrokerRequest::new(7, BrokerOp::Health);
        let bytes = lightloom_protocol::encode(&req).unwrap();
        let decoded: BrokerRequest = lightloom_protocol::decode(&bytes).unwrap();
        let resp = handle(&decoded);
        assert_eq!(resp.id, 7);
        assert!(matches!(resp.result, Ok(BrokerPayload::Health(_))));

        let resp = handle(&BrokerRequest::new(
            8,
            BrokerOp::SmbusWriteByte {
                bus: 0,
                addr: 0x39,
                reg: 0,
                value: 1,
            },
        ));
        assert_eq!(resp.result, Err(BrokerError::Unsupported));

        let resp = handle(&BrokerRequest::new(
            9,
            BrokerOp::SmbusReadByte {
                bus: 0,
                addr: 0x00,
                reg: 0,
            },
        ));
        assert_eq!(resp.result, Err(BrokerError::AddressOutOfRange { addr: 0 }));

        let mut bad = BrokerRequest::new(10, BrokerOp::Health);
        bad.version = 99;
        assert_eq!(handle(&bad).result, Err(BrokerError::UnsupportedVersion(99)));
    }
}
