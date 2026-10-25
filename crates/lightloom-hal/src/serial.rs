// SPDX-License-Identifier: GPL-3.0-or-later
//! Serial (COM/tty) backend traits for `Type() === "serial"` plugins and the
//! `@SignalRGB/serial` module.
//!
//! Serial ports are shared resources: a modem, a console cable or another
//! program's device can sit behind any of them, and writing a probe to one is
//! never harmless. Lightloom therefore only opens ports whose USB parent's
//! VID:PID a serial plugin claims, and never writes to a port to find out what
//! it is: [`SerialBackend::available_ports`] must not open anything.

use crate::error::{HalError, HalResult};
use serde::{Deserialize, Serialize};

/// Parity setting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Parity {
    /// No parity bit.
    #[default]
    None,
    /// Odd parity.
    Odd,
    /// Even parity.
    Even,
}

/// Port configuration passed to `serial.connect({...})`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SerialConfig {
    /// Baud rate (default 9600).
    pub baud_rate: u32,
    /// Data bits 5..=8 (default 8).
    pub data_bits: u8,
    /// Stop bits 1 or 2 (default 1).
    pub stop_bits: u8,
    /// Parity (default none).
    pub parity: Parity,
    /// Read timeout in ms (default 1000, the documented `serial.read` default).
    pub timeout_ms: u32,
}

/// Highest baud rate a plugin may ask for (the fastest corpus plugin uses 512000).
pub const MAX_BAUD_RATE: u32 = 4_000_000;

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            baud_rate: 9600,
            data_bits: 8,
            stop_bits: 1,
            parity: Parity::None,
            timeout_ms: 1000,
        }
    }
}

impl SerialConfig {
    /// Refuse settings no backend supports.
    pub fn validate(&self) -> HalResult<()> {
        if self.baud_rate == 0 || self.baud_rate > MAX_BAUD_RATE {
            return Err(HalError::InvalidArgument(format!("baud rate {}", self.baud_rate)));
        }
        if !(5..=8).contains(&self.data_bits) {
            return Err(HalError::InvalidArgument(format!("{} data bits", self.data_bits)));
        }
        if !(1..=2).contains(&self.stop_bits) {
            return Err(HalError::InvalidArgument(format!("{} stop bits", self.stop_bits)));
        }
        Ok(())
    }
}

/// A serial port and, when the OS knows it, the USB device it belongs to.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SerialPortInfo {
    /// `COM7` / `/dev/ttyUSB0`.
    pub port_name: String,
    /// USB vendor id of the parent, when the port is USB-backed.
    pub vid: Option<u16>,
    /// USB product id of the parent.
    pub pid: Option<u16>,
    /// USB serial / instance id of the parent.
    pub serial: Option<String>,
    /// USB interface number of the port's communication interface (composite
    /// CDC devices), when the OS reports it.
    pub interface: Option<u8>,
    /// USB manufacturer string of the parent.
    pub manufacturer: Option<String>,
    /// USB product string of the parent.
    pub product: Option<String>,
}

impl SerialPortInfo {
    /// `(vid, pid)` of the USB parent, when the port is USB-backed.
    pub fn usb_ids(&self) -> Option<(u16, u16)> {
        Some((self.vid?, self.pid?))
    }
}

/// An open serial port.
pub trait SerialHandle: Send {
    /// Port name used to open this handle.
    fn port_name(&self) -> &str;
    /// Active configuration.
    fn config(&self) -> &SerialConfig;
    /// Write bytes; returns bytes written.
    fn write(&mut self, data: &[u8]) -> HalResult<usize>;
    /// Read up to `buf.len()` bytes waiting at most `timeout_ms`. `Ok(0)` on timeout.
    fn read(&mut self, buf: &mut [u8], timeout_ms: u32) -> HalResult<usize>;
    /// Number of bytes waiting in the input queue.
    fn bytes_to_read(&mut self) -> HalResult<usize>;
}

/// Enumerates and opens serial ports.
pub trait SerialBackend: Send + Sync {
    /// All ports with their USB parent (when known). Never opens a port.
    fn available_ports(&self) -> HalResult<Vec<SerialPortInfo>>;
    /// Open `port_name` with `config`.
    fn open(&self, port_name: &str, config: &SerialConfig) -> HalResult<Box<dyn SerialHandle>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configs_out_of_range_are_refused() {
        assert!(SerialConfig::default().validate().is_ok());
        let bad = [
            SerialConfig {
                baud_rate: 0,
                ..SerialConfig::default()
            },
            SerialConfig {
                data_bits: 9,
                ..SerialConfig::default()
            },
            SerialConfig {
                stop_bits: 3,
                ..SerialConfig::default()
            },
        ];
        assert!(bad.iter().all(|c| c.validate().is_err()));
    }

    #[test]
    fn usb_ids_need_both_halves() {
        let mut p = SerialPortInfo {
            vid: Some(0x1A86),
            ..SerialPortInfo::default()
        };
        assert_eq!(p.usb_ids(), None);
        p.pid = Some(0x7523);
        assert_eq!(p.usb_ids(), Some((0x1A86, 0x7523)));
    }
}
