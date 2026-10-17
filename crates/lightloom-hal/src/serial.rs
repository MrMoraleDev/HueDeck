// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Serial (COM/tty) backend traits for `Type() === "serial"` plugins and the
//! `@SignalRGB/serial` module.

use crate::error::HalResult;
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

impl Default for SerialConfig {
    fn default() -> Self {
        Self { baud_rate: 9600, data_bits: 8, stop_bits: 1, parity: Parity::None, timeout_ms: 1000 }
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
    /// All ports with their USB parent (when known).
    fn available_ports(&self) -> HalResult<Vec<SerialPortInfo>>;
    /// Open `port_name` with `config`.
    fn open(&self, port_name: &str, config: &SerialConfig) -> HalResult<Box<dyn SerialHandle>>;
}
