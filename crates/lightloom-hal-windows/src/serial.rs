// SPDX-License-Identifier: GPL-3.0-or-later
//! Serial ports over the `serialport` crate.
//!
//! * [`WindowsSerialBackend`] (Windows only): COM ports from SetupAPI
//!   (`serialport::available_ports`, which reads the registry and the PnP
//!   tree and never opens a port), with the VID:PID, serial number and
//!   `&MI_xx` interface number of the USB device behind each one.
//! * [`open_port`] / [`PortHandle`]: opening a port and its I/O, shared with
//!   the Linux backend (`lightloom-hal-linux`), which enumerates ttys itself.
//!
//! Nothing here decides which port to open: the plugin host only opens ports
//! whose USB parent a serial plugin claims (see `lightloom_hal::serial`).

use lightloom_hal::serial::{Parity, SerialConfig, SerialHandle};
use lightloom_hal::{HalError, HalResult};
use std::io::{Read, Write};
use std::time::Duration;

/// Map a `serialport` error onto the HAL error.
fn hal_error(port: &str, e: serialport::Error) -> HalError {
    match e.kind() {
        serialport::ErrorKind::NoDevice => HalError::NotFound(format!("{port}: {}", e.description)),
        serialport::ErrorKind::InvalidInput => {
            HalError::InvalidArgument(format!("{port}: {}", e.description))
        }
        serialport::ErrorKind::Io(std::io::ErrorKind::PermissionDenied) => {
            HalError::PermissionDenied(format!("{port}: {}", e.description))
        }
        _ => HalError::Backend(format!("{port}: {}", e.description)),
    }
}

/// Map an I/O error of an open port onto the HAL error.
fn io_error(e: std::io::Error) -> HalError {
    use std::io::ErrorKind;
    match e.kind() {
        ErrorKind::BrokenPipe | ErrorKind::NotConnected | ErrorKind::ConnectionAborted => {
            HalError::Disconnected
        }
        _ => {
            // ERROR_DEVICE_NOT_CONNECTED (1167), ERROR_BAD_COMMAND (22) and
            // ERROR_GEN_FAILURE (31) are what an unplugged USB CDC port answers;
            // ENODEV (19) / EIO (5) on Linux.
            #[cfg(windows)]
            let gone = matches!(e.raw_os_error(), Some(1167 | 22 | 31));
            #[cfg(not(windows))]
            let gone = matches!(e.raw_os_error(), Some(19 | 5));
            if gone {
                HalError::Disconnected
            } else {
                HalError::Io(e)
            }
        }
    }
}

/// Open `port_name` with `config` (8N1-style settings, no flow control, DTR
/// asserted as most USB CDC firmwares expect before they talk).
pub fn open_port(port_name: &str, config: &SerialConfig) -> HalResult<Box<dyn SerialHandle>> {
    config.validate()?;
    let data_bits = match config.data_bits {
        5 => serialport::DataBits::Five,
        6 => serialport::DataBits::Six,
        7 => serialport::DataBits::Seven,
        _ => serialport::DataBits::Eight,
    };
    let stop_bits = if config.stop_bits == 2 {
        serialport::StopBits::Two
    } else {
        serialport::StopBits::One
    };
    let parity = match config.parity {
        Parity::None => serialport::Parity::None,
        Parity::Odd => serialport::Parity::Odd,
        Parity::Even => serialport::Parity::Even,
    };
    let timeout = Duration::from_millis(u64::from(config.timeout_ms.max(1)));
    let port = serialport::new(port_name, config.baud_rate)
        .data_bits(data_bits)
        .stop_bits(stop_bits)
        .parity(parity)
        .flow_control(serialport::FlowControl::None)
        .timeout(timeout)
        .open()
        .map_err(|e| hal_error(port_name, e))?;
    Ok(Box::new(PortHandle {
        port,
        port_name: port_name.to_string(),
        config: config.clone(),
        timeout,
    }))
}

/// An open port.
pub struct PortHandle {
    port: Box<dyn serialport::SerialPort>,
    port_name: String,
    config: SerialConfig,
    /// The read timeout currently set on the port.
    timeout: Duration,
}

impl std::fmt::Debug for PortHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PortHandle")
            .field("port_name", &self.port_name)
            .field("config", &self.config)
            .finish()
    }
}

impl SerialHandle for PortHandle {
    fn port_name(&self) -> &str {
        &self.port_name
    }

    fn config(&self) -> &SerialConfig {
        &self.config
    }

    fn write(&mut self, data: &[u8]) -> HalResult<usize> {
        self.port.write_all(data).map_err(io_error)?;
        self.port.flush().map_err(io_error)?;
        Ok(data.len())
    }

    fn read(&mut self, buf: &mut [u8], timeout_ms: u32) -> HalResult<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let timeout = Duration::from_millis(u64::from(timeout_ms.max(1)));
        if timeout != self.timeout {
            self.port
                .set_timeout(timeout)
                .map_err(|e| hal_error(&self.port_name, e))?;
            self.timeout = timeout;
        }
        match self.port.read(buf) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(0),
            Err(e) => Err(io_error(e)),
        }
    }

    fn bytes_to_read(&mut self) -> HalResult<usize> {
        self.port
            .bytes_to_read()
            .map(|n| n as usize)
            .map_err(|e| hal_error(&self.port_name, e))
    }
}

/// COM ports of this machine (SetupAPI; nothing is opened to enumerate).
#[cfg(windows)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowsSerialBackend;

#[cfg(windows)]
impl WindowsSerialBackend {
    /// The backend (stateless).
    pub fn new() -> Self {
        Self
    }
}

#[cfg(windows)]
fn port_info(p: serialport::SerialPortInfo) -> lightloom_hal::SerialPortInfo {
    match p.port_type {
        serialport::SerialPortType::UsbPort(usb) => lightloom_hal::SerialPortInfo {
            port_name: p.port_name,
            vid: Some(usb.vid),
            pid: Some(usb.pid),
            serial: usb.serial_number,
            interface: usb.interface,
            manufacturer: usb.manufacturer,
            product: usb.product,
        },
        _ => lightloom_hal::SerialPortInfo {
            port_name: p.port_name,
            ..lightloom_hal::SerialPortInfo::default()
        },
    }
}

#[cfg(windows)]
impl lightloom_hal::SerialBackend for WindowsSerialBackend {
    fn available_ports(&self) -> HalResult<Vec<lightloom_hal::SerialPortInfo>> {
        let mut ports: Vec<_> = serialport::available_ports()
            .map_err(|e| hal_error("enumeration", e))?
            .into_iter()
            .map(port_info)
            .collect();
        ports.sort_by(|a, b| natural_port_order(&a.port_name, &b.port_name));
        ports.dedup_by(|a, b| a.port_name == b.port_name);
        Ok(ports)
    }

    fn open(&self, port_name: &str, config: &SerialConfig) -> HalResult<Box<dyn SerialHandle>> {
        open_port(port_name, config)
    }
}

/// `COM2` before `COM10`; then by text.
pub fn natural_port_order(a: &str, b: &str) -> std::cmp::Ordering {
    fn split(s: &str) -> (&str, Option<u64>) {
        let digits = s.len() - s.bytes().rev().take_while(u8::is_ascii_digit).count();
        (&s[..digits], s[digits..].parse().ok())
    }
    split(a).cmp(&split(b)).then_with(|| a.cmp(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports_sort_by_number() {
        let mut v = vec![
            "COM10",
            "COM2",
            "/dev/ttyACM0",
            "COM1",
            "/dev/ttyACM10",
            "/dev/ttyACM9",
        ];
        v.sort_by(|a, b| natural_port_order(a, b));
        assert_eq!(
            v,
            [
                "/dev/ttyACM0",
                "/dev/ttyACM9",
                "/dev/ttyACM10",
                "COM1",
                "COM2",
                "COM10"
            ]
        );
    }

    #[test]
    fn invalid_configs_never_reach_the_os() {
        let bad = SerialConfig {
            baud_rate: 0,
            ..SerialConfig::default()
        };
        // A made-up name: validation fails first, nothing is opened.
        assert!(matches!(
            open_port("LIGHTLOOM-NO-SUCH-PORT", &bad),
            Err(HalError::InvalidArgument(_))
        ));
    }
}
