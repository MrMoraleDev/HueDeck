// SPDX-License-Identifier: GPL-3.0-or-later
//! Raw USB (libusb/WinUSB-class) backend traits, used by `Type() === "rawusb"`
//! and `"hybrid"` plugins through `device.bulk_transfer` / `device.control_transfer`.
//!
//! A raw USB device is reachable only through the interfaces a user-mode
//! driver owns (WinUSB on Windows). An interface bound to any other driver
//! (HID, audio, mass storage, a vendor driver) is never claimed: the backend
//! reports it as not [`UsbInterfaceInfo::accessible`], and transfers that
//! would need it fail with [`HalError::NeedsDriver`]. HueDeck never installs
//! or swaps a driver (docs/research/drivers.md §2.4).

use crate::error::{HalError, HalResult};
use crate::key::DeviceKey;
use serde::{Deserialize, Serialize};

/// Direction bit of a USB endpoint address and of `bmRequestType`.
pub const ENDPOINT_IN: u8 = 0x80;

/// Default transfer timeout when a plugin passes none (open question in the
/// spec; 1000 ms chosen).
pub const DEFAULT_TIMEOUT_MS: u32 = 1000;

/// Longest transfer timeout honoured, so a stop request still leaves the
/// session time to run `Shutdown()` (the same cap as HID reads).
pub const MAX_TIMEOUT_MS: u32 = 5000;

/// Where the user learns why a device needs WinUSB and how to check it.
pub const WINUSB_HELP_URL: &str = "https://github.com/MrMoraleDev/HueDeck/blob/develop/docs/hardware/winusb.md";

/// `bmRequestType` recipient field (bits 0..=4).
const RECIPIENT_MASK: u8 = 0x1F;
/// `bmRequestType` recipient: interface (`wIndex` low byte = interface number).
const RECIPIENT_INTERFACE: u8 = 1;
/// `bmRequestType` recipient: endpoint (`wIndex` low byte = endpoint address).
const RECIPIENT_ENDPOINT: u8 = 2;

/// One interface of a USB device (first alternate setting).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsbInterfaceInfo {
    /// `bInterfaceNumber`.
    pub number: u8,
    /// `bInterfaceClass` (0x03 HID, 0x01 audio, 0x08 mass storage, 0xFF vendor).
    pub class: u8,
    /// `bInterfaceSubClass`.
    pub subclass: u8,
    /// `bInterfaceProtocol`.
    pub protocol: u8,
    /// The OS driver bound to the interface (Windows service name: `WinUSB`,
    /// `HidUsb`, `usbaudio`, ...); empty when none or unknown.
    pub driver: String,
    /// Whether this process may claim the interface: a user-mode driver
    /// (WinUSB) owns it. Interfaces bound to other drivers are never claimed.
    pub accessible: bool,
    /// Endpoint addresses of the interface; empty until the backend read the
    /// configuration descriptor (Windows: at open).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoints: Vec<u8>,
}

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
    /// Whether a user-mode-accessible driver (WinUSB/libusb) is bound to at
    /// least one interface.
    pub driver_bound: bool,
    /// The OS device instance id (`USB\VID_xxxx&PID_xxxx\<instance>` on
    /// Windows): HID collections whose USB parent has this id belong to the
    /// same physical device. Empty when the backend has none.
    #[serde(default)]
    pub instance_id: String,
    /// Interfaces of the active configuration.
    #[serde(default)]
    pub interfaces: Vec<UsbInterfaceInfo>,
}

impl UsbDeviceInfo {
    /// `vid:pid:serial` identity.
    pub fn key(&self) -> DeviceKey {
        DeviceKey::new(self.vid, self.pid, self.serial.clone())
    }

    /// What to tell the user when no interface of the device is reachable:
    /// which drivers own them, and where to read more. `None` when a
    /// user-mode driver is bound.
    pub fn driver_problem(&self) -> Option<String> {
        if self.driver_bound {
            return None;
        }
        let owners: Vec<String> = self
            .interfaces
            .iter()
            .map(|i| {
                let driver = if i.driver.is_empty() { "no driver" } else { &i.driver };
                format!("interface {} ({driver})", i.number)
            })
            .collect();
        let owners = if owners.is_empty() {
            String::new()
        } else {
            format!(" ({})", owners.join(", "))
        };
        Some(format!(
            "needs WinUSB driver: Windows did not bind WinUSB to {:04x}:{:04x}{owners}; HueDeck never installs \
             drivers, see {WINUSB_HELP_URL}",
            self.vid, self.pid
        ))
    }

    /// The interface that owns `endpoint`. When no interface lists its
    /// endpoints (the backend has not read the configuration descriptor),
    /// the first accessible interface.
    pub fn interface_for_endpoint(&self, endpoint: u8) -> Option<&UsbInterfaceInfo> {
        let known = self.interfaces.iter().any(|i| !i.endpoints.is_empty());
        if known {
            self.interfaces.iter().find(|i| i.endpoints.contains(&endpoint))
        } else {
            self.interfaces.iter().find(|i| i.accessible)
        }
    }

    /// The interface a control transfer goes through (WinUSB sends control
    /// requests on an interface handle): the addressed interface or the owner
    /// of the addressed endpoint, else (device / other recipients) the first
    /// accessible interface.
    pub fn interface_for_control(&self, bm_request_type: u8, w_index: u16) -> Option<&UsbInterfaceInfo> {
        let low = (w_index & 0xFF) as u8;
        match bm_request_type & RECIPIENT_MASK {
            RECIPIENT_INTERFACE => self.interfaces.iter().find(|i| i.number == low),
            RECIPIENT_ENDPOINT => self.interface_for_endpoint(low),
            _ => self.interfaces.iter().find(|i| i.accessible),
        }
    }

    /// The interface for a transfer on `endpoint`, only when it may be
    /// claimed: [`HalError::NotFound`] when no interface owns the endpoint,
    /// [`HalError::NeedsDriver`] when another driver owns it.
    pub fn claimable_for_endpoint(&self, endpoint: u8) -> HalResult<&UsbInterfaceInfo> {
        let iface = self
            .interface_for_endpoint(endpoint)
            .ok_or_else(|| self.missing(format!("no interface has endpoint 0x{endpoint:02x}")))?;
        self.claimable(iface)
    }

    /// Like [`UsbDeviceInfo::claimable_for_endpoint`] for a control transfer.
    pub fn claimable_for_control(&self, bm_request_type: u8, w_index: u16) -> HalResult<&UsbInterfaceInfo> {
        let iface = self
            .interface_for_control(bm_request_type, w_index)
            .ok_or_else(|| {
                self.missing(format!(
                    "no interface for control request type 0x{bm_request_type:02x}, index 0x{w_index:04x}"
                ))
            })?;
        self.claimable(iface)
    }

    /// Interface `number`, only when it may be claimed.
    pub fn claimable_interface(&self, number: u8) -> HalResult<&UsbInterfaceInfo> {
        let iface = self
            .interfaces
            .iter()
            .find(|i| i.number == number)
            .ok_or_else(|| HalError::NotFound(format!("no interface {number}")))?;
        self.claimable(iface)
    }

    fn missing(&self, what: String) -> HalError {
        match self.driver_problem() {
            Some(problem) => HalError::NeedsDriver(problem),
            None => HalError::NotFound(what),
        }
    }

    fn claimable<'a>(&self, iface: &'a UsbInterfaceInfo) -> HalResult<&'a UsbInterfaceInfo> {
        if iface.accessible {
            return Ok(iface);
        }
        let driver = if iface.driver.is_empty() { "no driver" } else { &iface.driver };
        Err(HalError::NeedsDriver(format!(
            "needs WinUSB driver: interface {} of {:04x}:{:04x} is bound to {driver}, not WinUSB; HueDeck never \
             claims it or swaps its driver, see {WINUSB_HELP_URL}",
            iface.number, self.vid, self.pid
        )))
    }
}

/// An open raw USB device.
pub trait UsbHandle: Send {
    /// Static information (with endpoint lists, once the backend read them).
    fn info(&self) -> &UsbDeviceInfo;

    /// Claim an interface before transfers on its endpoints. Refuses
    /// ([`HalError::NeedsDriver`]) an interface another driver owns.
    /// Transfers claim the interface they need on their own.
    fn claim_interface(&mut self, interface: u8) -> HalResult<()>;

    /// Bulk (or interrupt) transfer. Direction comes from bit 7 of `endpoint`:
    /// for IN endpoints (`>= 0x80`) `data` is filled, for OUT endpoints it is
    /// sent. Returns the number of bytes transferred; [`HalError::Timeout`]
    /// when nothing completed in `timeout_ms`.
    fn bulk_transfer(&mut self, endpoint: u8, data: &mut [u8], timeout_ms: u32) -> HalResult<usize>;

    /// Control transfer on the default pipe, through the interface
    /// [`UsbDeviceInfo::interface_for_control`] picks. Direction comes from
    /// bit 7 of `bm_request_type`. Returns the number of bytes transferred in
    /// the data stage.
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

    /// Open the device at `path`. Opening claims nothing.
    fn open(&self, path: &str) -> HalResult<Box<dyn UsbHandle>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface(number: u8, driver: &str, endpoints: &[u8]) -> UsbInterfaceInfo {
        UsbInterfaceInfo {
            number,
            class: if driver == "HidUsb" { 3 } else { 0xFF },
            driver: driver.into(),
            accessible: driver.eq_ignore_ascii_case("winusb"),
            endpoints: endpoints.to_vec(),
            ..Default::default()
        }
    }

    fn hybrid() -> UsbDeviceInfo {
        UsbDeviceInfo {
            vid: 0x0CF2,
            pid: 0xA103,
            driver_bound: true,
            interfaces: vec![iface(0, "HidUsb", &[0x81]), iface(1, "WinUSB", &[0x02, 0x82])],
            ..Default::default()
        }
    }

    #[test]
    fn endpoints_route_to_their_interface_and_other_drivers_are_refused() {
        let d = hybrid();
        assert_eq!(d.claimable_for_endpoint(0x02).unwrap().number, 1);
        assert_eq!(d.claimable_for_endpoint(0x82).unwrap().number, 1);
        let err = d.claimable_for_endpoint(0x81).unwrap_err();
        assert!(matches!(&err, HalError::NeedsDriver(m) if m.contains("HidUsb")), "{err}");
        assert!(matches!(d.claimable_for_endpoint(0x03), Err(HalError::NotFound(_))));
        assert!(matches!(d.claimable_interface(0), Err(HalError::NeedsDriver(_))));
        assert_eq!(d.claimable_interface(1).unwrap().number, 1);
    }

    #[test]
    fn control_requests_pick_the_addressed_or_first_reachable_interface() {
        let d = hybrid();
        // Vendor request to the device: any WinUSB interface carries it.
        assert_eq!(d.claimable_for_control(0xC0, 0xD800).unwrap().number, 1);
        // Class request to interface 0 (HID): refused, never claimed.
        assert!(matches!(d.claimable_for_control(0x21, 0x0000), Err(HalError::NeedsDriver(_))));
        assert_eq!(d.claimable_for_control(0x41, 0x0001).unwrap().number, 1);
        // Endpoint recipient: the endpoint's interface.
        assert_eq!(d.claimable_for_control(0x02, 0x0082).unwrap().number, 1);
    }

    #[test]
    fn a_device_without_winusb_explains_itself() {
        let d = UsbDeviceInfo {
            vid: 0x0416,
            pid: 0x8050,
            interfaces: vec![iface(0, "HidUsb", &[]), iface(1, "", &[])],
            ..Default::default()
        };
        let problem = d.driver_problem().unwrap();
        assert!(problem.starts_with("needs WinUSB driver"), "{problem}");
        assert!(problem.contains("interface 0 (HidUsb)") && problem.contains("interface 1 (no driver)"));
        assert!(problem.contains(WINUSB_HELP_URL));
        assert!(matches!(d.claimable_for_control(0x40, 0), Err(HalError::NeedsDriver(_))));
        assert!(hybrid().driver_problem().is_none());
    }

    #[test]
    fn unknown_endpoint_lists_fall_back_to_the_first_reachable_interface() {
        let d = UsbDeviceInfo {
            driver_bound: true,
            interfaces: vec![iface(0, "HidUsb", &[]), iface(2, "WinUSB", &[])],
            ..Default::default()
        };
        assert_eq!(d.claimable_for_endpoint(0x01).unwrap().number, 2);
    }
}
