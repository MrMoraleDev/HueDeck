// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Raw USB (libusb/WinUSB-class) backend traits, used by `Type() === "rawusb"`
//! and `"hybrid"` plugins through `device.bulk_transfer` / `device.control_transfer`.

use crate::error::HalResult;
use crate::key::DeviceKey;
use serde::{Deserialize, Serialize};

/// Direction bit of a USB endpoint address.
pub const ENDPOINT_IN: u8 = 0x80;

/// Default transfer timeout when a plugin passes none (open question in the
/// spec; 1000 ms chosen).
pub const DEFAULT_TIMEOUT_MS: u32 = 1000;

/// A USB device as seen by the raw-USB backend.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbDeviceInfo {
    /// USB vendor id.
    pub vid: u16,
    /// USB product id.
    pub pid: u16,
    /// Serial string (empty when none).
    pub serial: String,
    /// Manufacturer string.
    pub manufacturer: String,
    /// Product string.
    pub product: String,
    /// Backend-specific opaque path for [`UsbBackend::open`].
    pub path: String,
    /// Bus number.
    pub bus_number: u8,
    /// Device address on the bus.
    pub address: u8,
    /// Whether a user-mode-accessible driver (WinUSB/libusb) is bound.
    pub driver_bound: bool,
}

impl UsbDeviceInfo {
    /// `vid:pid:serial` identity.
    pub fn key(&self) -> DeviceKey {
        DeviceKey::new(self.vid, self.pid, self.serial.clone())
    }
}

/// An open raw USB device.
pub trait UsbHandle: Send {
    /// Static information.
    fn info(&self) -> &UsbDeviceInfo;

    /// Claim an interface before transfers on its endpoints.
    fn claim_interface(&mut self, interface: u8) -> HalResult<()>;

    /// Bulk transfer. Direction comes from bit 7 of `endpoint`: for IN
    /// endpoints (`>= 0x80`) `data` is filled, for OUT endpoints it is sent.
    /// Returns the number of bytes transferred.
    fn bulk_transfer(&mut self, endpoint: u8, data: &mut [u8], timeout_ms: u32) -> HalResult<usize>;

    /// Control transfer. Direction comes from bit 7 of `bm_request_type`.
    /// Returns the number of bytes transferred in the data stage.
    fn control_transfer(
        &mut self,
        bm_request_type: u8,
        b_request: u8,
        w_value: u16,
        w_index: u16,
        data: &mut [u8],
        timeout_ms: u32,
    ) -> HalResult<usize>;
}

/// Enumerates and opens raw USB devices.
pub trait UsbBackend: Send + Sync {
    /// Enumerate USB devices (all of them, regardless of driver binding).
    fn enumerate(&self) -> HalResult<Vec<UsbDeviceInfo>>;

    /// Open the device at `path`.
    fn open(&self, path: &str) -> HalResult<Box<dyn UsbHandle>>;
}
