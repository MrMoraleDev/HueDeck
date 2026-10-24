// SPDX-License-Identifier: GPL-3.0-or-later
//! Raw USB over WinUSB ([`WinUsbBackend`], Windows only), for `Type() ==
//! "rawusb"` / `"hybrid"` plugins.
//!
//! Built on `nusb`, which speaks WinUSB only. HueDeck ships no INF and never
//! installs, swaps or generates a driver (docs/research/drivers.md §2.4): a
//! device is reachable only when Windows bound WinUSB to it by itself (MS OS
//! descriptors) or its vendor's software did.
//!
//! Enumeration (`nusb::list_devices`: cached hub data and the configuration
//! descriptor, nothing is opened) records the driver of every interface: the
//! device's own service, or for a composite device (`usbccgp`) the service of
//! the child devnode `…&MI_xx` that owns the interface (an interface without
//! its own child belongs to the nearest lower one, as with interface
//! association descriptors). Only interfaces whose service is `WinUSB` are
//! [`UsbInterfaceInfo::accessible`]; the handle claims nothing else, so HID,
//! audio and mass-storage interfaces are never touched.
//!
//! [`interface_drivers`] and [`composite_interface`] are pure and tested on
//! every platform.

use huedeck_hal::usb::UsbInterfaceInfo;

/// The Windows service name of WinUSB.
pub const WINUSB_SERVICE: &str = "WinUSB";

/// The USB generic parent driver of composite devices.
const COMPOSITE_SERVICE: &str = "usbccgp";

/// Interface number of a composite-device child instance id
/// (`USB\VID_0CF2&PID_A103&MI_01\7&2f1d…` → 1).
pub fn composite_interface(instance_id: &str) -> Option<u8> {
    let upper = instance_id.to_ascii_uppercase();
    let (_, rest) = upper.split_once("&MI_")?;
    u8::from_str_radix(rest.get(..2)?, 16).ok()
}

/// Driver of each interface: `device_driver` for a non-composite device,
/// else the service of the child that owns the interface (the child with the
/// highest `MI_xx` not above it). `children` are `(instance id, service)`.
pub fn interface_drivers(device_driver: &str, children: &[(String, String)], numbers: &[u8]) -> Vec<(u8, String)> {
    let composite = device_driver.eq_ignore_ascii_case(COMPOSITE_SERVICE);
    let owned: Vec<(u8, &str)> = children
        .iter()
        .filter_map(|(id, service)| Some((composite_interface(id)?, service.as_str())))
        .collect();
    numbers
        .iter()
        .map(|&n| {
            let driver = if composite {
                owned
                    .iter()
                    .filter(|(mi, _)| *mi <= n)
                    .max_by_key(|(mi, _)| *mi)
                    .map_or("", |(_, s)| *s)
            } else {
                device_driver
            };
            (n, driver.to_string())
        })
        .collect()
}

/// An interface description with its driver; accessible only under WinUSB.
pub fn interface_info(number: u8, class: u8, subclass: u8, protocol: u8, driver: String) -> UsbInterfaceInfo {
    UsbInterfaceInfo {
        number,
        class,
        subclass,
        protocol,
        accessible: driver.eq_ignore_ascii_case(WINUSB_SERVICE),
        driver,
        endpoints: Vec::new(),
    }
}

#[cfg(windows)]
pub use imp::{WinUsbBackend, WinUsbHandle};

#[cfg(windows)]
mod imp {
    use super::{interface_drivers, interface_info, WINUSB_SERVICE};
    use crate::probe::{devnode_strings, device_id, wide};
    use huedeck_hal::usb::{UsbBackend, UsbDeviceInfo, UsbHandle, ENDPOINT_IN};
    use huedeck_hal::{HalError, HalResult};
    use nusb::descriptors::TransferType;
    use nusb::transfer::{Buffer, Bulk, ControlIn, ControlOut, ControlType, In, Interrupt, Out, Recipient, TransferError};
    use nusb::{Device, Endpoint, Interface, MaybeFuture};
    use std::collections::BTreeMap;
    use std::time::Duration;
    use windows::core::PCWSTR;
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_Child, CM_Get_Sibling, CM_Locate_DevNodeW, CM_LOCATE_DEVNODE_NORMAL, CR_SUCCESS,
    };
    use windows::Win32::Devices::Properties::DEVPKEY_Device_Service;

    /// Children of a devnode at most (composite devices have a handful).
    const MAX_CHILDREN: usize = 64;

    /// `(instance id, service)` of every child devnode of the device `instance_id`.
    fn children(instance_id: &str) -> Vec<(String, String)> {
        let wid = wide(instance_id);
        let mut inst = 0u32;
        // SAFETY: `wid` is NUL-terminated and outlives the call; `inst` is a valid out pointer.
        if unsafe { CM_Locate_DevNodeW(&mut inst, PCWSTR(wid.as_ptr()), CM_LOCATE_DEVNODE_NORMAL) } != CR_SUCCESS {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut child = 0u32;
        // SAFETY: plain out-pointer call on a devnode handle cfgmgr32 gave us.
        if unsafe { CM_Get_Child(&mut child, inst, 0) } != CR_SUCCESS {
            return out;
        }
        for _ in 0..MAX_CHILDREN {
            if let Ok(id) = device_id(child) {
                let service = devnode_strings(child, &DEVPKEY_Device_Service)
                    .into_iter()
                    .next()
                    .unwrap_or_default();
                out.push((id, service));
            }
            let mut next = 0u32;
            // SAFETY: as above.
            if unsafe { CM_Get_Sibling(&mut next, child, 0) } != CR_SUCCESS {
                break;
            }
            child = next;
        }
        out
    }

    fn describe(d: &nusb::DeviceInfo) -> UsbDeviceInfo {
        let instance_id = d.instance_id().to_string_lossy().into_owned();
        let device_driver = d.driver().unwrap_or_default().to_string();
        let numbers: Vec<u8> = d.interfaces().map(|i| i.interface_number()).collect();
        let kids = if device_driver.eq_ignore_ascii_case("usbccgp") {
            children(&instance_id)
        } else {
            Vec::new()
        };
        let drivers: BTreeMap<u8, String> = interface_drivers(&device_driver, &kids, &numbers)
            .into_iter()
            .collect();
        let mut interfaces: Vec<_> = d
            .interfaces()
            .map(|i| {
                let n = i.interface_number();
                interface_info(
                    n,
                    i.class(),
                    i.subclass(),
                    i.protocol(),
                    drivers.get(&n).cloned().unwrap_or_default(),
                )
            })
            .collect();
        if interfaces.is_empty() && device_driver.eq_ignore_ascii_case(WINUSB_SERVICE) {
            // The configuration descriptor was not readable; WinUSB owns the device.
            interfaces.push(interface_info(0, d.class(), d.subclass(), d.protocol(), device_driver.clone()));
        }
        UsbDeviceInfo {
            vid: d.vendor_id(),
            pid: d.product_id(),
            serial: d.serial_number().unwrap_or_default().to_string(),
            manufacturer: d.manufacturer_string().unwrap_or_default().to_string(),
            product: d.product_string().unwrap_or_default().to_string(),
            path: instance_id.clone(),
            bus_number: 0,
            address: d.device_address(),
            driver_bound: interfaces.iter().any(|i| i.accessible),
            instance_id,
            interfaces,
        }
    }

    fn nusb_error(e: nusb::Error, what: &str) -> HalError {
        use nusb::ErrorKind as K;
        let text = format!("{what}: {e}");
        match e.kind() {
            K::Disconnected => HalError::Disconnected,
            K::Busy => HalError::Busy(text),
            K::PermissionDenied => HalError::PermissionDenied(text),
            K::NotFound => HalError::NotFound(text),
            K::Unsupported => HalError::NeedsDriver(format!(
                "needs WinUSB driver: {text}; HueDeck never installs drivers, see {}",
                huedeck_hal::usb::WINUSB_HELP_URL
            )),
            _ => HalError::Backend(text),
        }
    }

    fn transfer_error(e: TransferError, timeout_ms: u32) -> HalError {
        match e {
            TransferError::Cancelled => HalError::Timeout(timeout_ms),
            TransferError::Disconnected => HalError::Disconnected,
            TransferError::InvalidArgument => HalError::InvalidArgument(e.to_string()),
            other => HalError::Backend(format!("USB transfer failed: {other}")),
        }
    }

    /// The raw USB backend of Windows (WinUSB through `nusb`).
    #[derive(Clone, Copy, Debug, Default)]
    pub struct WinUsbBackend;

    impl WinUsbBackend {
        /// The backend (stateless: every call enumerates afresh).
        pub fn new() -> Self {
            Self
        }
    }

    impl UsbBackend for WinUsbBackend {
        fn enumerate(&self) -> HalResult<Vec<UsbDeviceInfo>> {
            let devices = nusb::list_devices()
                .wait()
                .map_err(|e| nusb_error(e, "listing USB devices"))?;
            Ok(devices.map(|d| describe(&d)).collect())
        }

        fn open(&self, path: &str) -> HalResult<Box<dyn UsbHandle>> {
            let found = nusb::list_devices()
                .wait()
                .map_err(|e| nusb_error(e, "listing USB devices"))?
                .find(|d| d.instance_id().to_string_lossy().eq_ignore_ascii_case(path))
                .ok_or_else(|| HalError::NotFound(path.to_string()))?;
            let mut info = describe(&found);
            if !info.driver_bound {
                return Err(HalError::NeedsDriver(info.driver_problem().unwrap_or_default()));
            }
            // Cached descriptors only: nothing is claimed yet.
            let device = found.open().wait().map_err(|e| nusb_error(e, "opening"))?;
            let mut pipes = BTreeMap::new();
            if let Ok(config) = device.active_configuration() {
                for group in config.interfaces() {
                    let alt = group.first_alt_setting();
                    let endpoints: Vec<u8> = alt.endpoints().map(|e| e.address()).collect();
                    for e in alt.endpoints() {
                        pipes.insert(e.address(), (e.transfer_type(), e.max_packet_size().max(1)));
                    }
                    if let Some(i) = info.interfaces.iter_mut().find(|i| i.number == group.interface_number()) {
                        i.endpoints = endpoints;
                    }
                }
            }
            Ok(Box::new(WinUsbHandle {
                info,
                device,
                claimed: BTreeMap::new(),
                kinds: pipes,
                pipes: BTreeMap::new(),
            }))
        }
    }

    /// An open endpoint (bulk or interrupt, either direction).
    enum Pipe {
        BulkOut(Endpoint<Bulk, Out>),
        BulkIn(Endpoint<Bulk, In>),
        IntOut(Endpoint<Interrupt, Out>),
        IntIn(Endpoint<Interrupt, In>),
    }

    impl Pipe {
        fn transfer(&mut self, buf: Buffer, timeout: Duration) -> nusb::transfer::Completion {
            match self {
                Pipe::BulkOut(e) => e.transfer_blocking(buf, timeout),
                Pipe::BulkIn(e) => e.transfer_blocking(buf, timeout),
                Pipe::IntOut(e) => e.transfer_blocking(buf, timeout),
                Pipe::IntIn(e) => e.transfer_blocking(buf, timeout),
            }
        }
    }

    /// A device opened by [`WinUsbBackend`]. Interfaces are claimed on first
    /// use, only when WinUSB owns them.
    pub struct WinUsbHandle {
        info: UsbDeviceInfo,
        device: Device,
        claimed: BTreeMap<u8, Interface>,
        /// Endpoint address → (transfer type, max packet size).
        kinds: BTreeMap<u8, (TransferType, usize)>,
        pipes: BTreeMap<u8, Pipe>,
    }

    impl std::fmt::Debug for WinUsbHandle {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("WinUsbHandle")
                .field("path", &self.info.path)
                .field("claimed", &self.claimed.keys().collect::<Vec<_>>())
                .finish_non_exhaustive()
        }
    }

    impl WinUsbHandle {
        fn interface(&mut self, number: u8) -> HalResult<Interface> {
            if let Some(i) = self.claimed.get(&number) {
                return Ok(i.clone());
            }
            self.info.claimable_interface(number)?;
            let intf = self
                .device
                .claim_interface(number)
                .wait()
                .map_err(|e| nusb_error(e, &format!("claiming interface {number}")))?;
            self.claimed.insert(number, intf.clone());
            Ok(intf)
        }

        fn pipe(&mut self, endpoint: u8) -> HalResult<&mut Pipe> {
            if !self.pipes.contains_key(&endpoint) {
                let number = self.info.claimable_for_endpoint(endpoint)?.number;
                let intf = self.interface(number)?;
                let (kind, _) = self
                    .kinds
                    .get(&endpoint)
                    .copied()
                    .ok_or_else(|| HalError::NotFound(format!("endpoint 0x{endpoint:02x}")))?;
                let open = |e: nusb::Error| nusb_error(e, &format!("opening endpoint 0x{endpoint:02x}"));
                let input = endpoint & ENDPOINT_IN != 0;
                let pipe = match (kind, input) {
                    (TransferType::Bulk, false) => Pipe::BulkOut(intf.endpoint(endpoint).map_err(open)?),
                    (TransferType::Bulk, true) => Pipe::BulkIn(intf.endpoint(endpoint).map_err(open)?),
                    (TransferType::Interrupt, false) => Pipe::IntOut(intf.endpoint(endpoint).map_err(open)?),
                    (TransferType::Interrupt, true) => Pipe::IntIn(intf.endpoint(endpoint).map_err(open)?),
                    _ => return Err(HalError::Unsupported("isochronous and control endpoints")),
                };
                self.pipes.insert(endpoint, pipe);
            }
            Ok(self.pipes.get_mut(&endpoint).expect("inserted above"))
        }
    }

    fn control_kind(bm_request_type: u8) -> HalResult<(ControlType, Recipient)> {
        let kind = match (bm_request_type >> 5) & 0x03 {
            0 => ControlType::Standard,
            1 => ControlType::Class,
            2 => ControlType::Vendor,
            _ => return Err(HalError::InvalidArgument("reserved request type".into())),
        };
        let recipient = match bm_request_type & 0x1F {
            0 => Recipient::Device,
            1 => Recipient::Interface,
            2 => Recipient::Endpoint,
            3 => Recipient::Other,
            _ => return Err(HalError::InvalidArgument("reserved recipient".into())),
        };
        Ok((kind, recipient))
    }

    impl UsbHandle for WinUsbHandle {
        fn info(&self) -> &UsbDeviceInfo {
            &self.info
        }

        fn claim_interface(&mut self, interface: u8) -> HalResult<()> {
            self.interface(interface).map(drop)
        }

        fn bulk_transfer(&mut self, endpoint: u8, data: &mut [u8], timeout_ms: u32) -> HalResult<usize> {
            let max_packet = self.kinds.get(&endpoint).map_or(64, |(_, m)| *m);
            let timeout = Duration::from_millis(u64::from(timeout_ms));
            let pipe = self.pipe(endpoint)?;
            if endpoint & ENDPOINT_IN == 0 {
                let c = pipe.transfer(Buffer::from(data.to_vec()), timeout);
                c.status.map_err(|e| transfer_error(e, timeout_ms))?;
                return Ok(c.actual_len);
            }
            // IN transfers are requested in whole packets; extra bytes are dropped.
            let requested = data.len().max(1).div_ceil(max_packet) * max_packet;
            let c = pipe.transfer(Buffer::new(requested), timeout);
            c.status.map_err(|e| transfer_error(e, timeout_ms))?;
            let got = c.buffer.into_vec();
            let n = got.len().min(data.len());
            data[..n].copy_from_slice(&got[..n]);
            Ok(n)
        }

        fn control_transfer(
            &mut self,
            bm_request_type: u8,
            b_request: u8,
            w_value: u16,
            w_index: u16,
            data: &mut [u8],
            timeout_ms: u32,
        ) -> HalResult<usize> {
            let (control_type, recipient) = control_kind(bm_request_type)?;
            let number = self.info.claimable_for_control(bm_request_type, w_index)?.number;
            let intf = self.interface(number)?;
            let timeout = Duration::from_millis(u64::from(timeout_ms));
            if bm_request_type & ENDPOINT_IN == 0 {
                intf.control_out(
                    ControlOut {
                        control_type,
                        recipient,
                        request: b_request,
                        value: w_value,
                        index: w_index,
                        data,
                    },
                    timeout,
                )
                .wait()
                .map_err(|e| transfer_error(e, timeout_ms))?;
                return Ok(data.len());
            }
            let length = u16::try_from(data.len())
                .map_err(|_| HalError::InvalidArgument("control transfer longer than 65535 bytes".into()))?;
            let got = intf
                .control_in(
                    ControlIn {
                        control_type,
                        recipient,
                        request: b_request,
                        value: w_value,
                        index: w_index,
                        length,
                    },
                    timeout,
                )
                .wait()
                .map_err(|e| transfer_error(e, timeout_ms))?;
            let n = got.len().min(data.len());
            data[..n].copy_from_slice(&got[..n]);
            Ok(n)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kids(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn composite_children_own_their_interfaces() {
        assert_eq!(composite_interface(r"USB\VID_0CF2&PID_A103&MI_01\7&2f1d&0&0001"), Some(1));
        assert_eq!(composite_interface(r"usb\vid_0b05&pid_1aa2&mi_0a\x"), Some(10));
        assert_eq!(composite_interface(r"USB\VID_0CF2&PID_A103\SERIAL"), None);
        let children = kids(&[
            (r"USB\VID_0CF2&PID_A103&MI_00\7&1", "HidUsb"),
            (r"USB\VID_0CF2&PID_A103&MI_01\7&2", "WinUSB"),
            (r"USB\VID_0CF2&PID_A103&MI_03\7&3", "usbaudio"),
        ]);
        // Interface 2 has no child of its own (IAD): it belongs to MI_01.
        assert_eq!(
            interface_drivers("usbccgp", &children, &[0, 1, 2, 3]),
            vec![
                (0, "HidUsb".to_string()),
                (1, "WinUSB".to_string()),
                (2, "WinUSB".to_string()),
                (3, "usbaudio".to_string()),
            ]
        );
    }

    #[test]
    fn a_single_function_device_shares_its_driver() {
        assert_eq!(
            interface_drivers("WinUSB", &[], &[0]),
            vec![(0, "WinUSB".to_string())]
        );
        let i = interface_info(0, 0xFF, 0, 0, "winusb".into());
        assert!(i.accessible);
        assert!(!interface_info(0, 3, 0, 0, "HidUsb".into()).accessible);
        assert!(!interface_info(0, 0xFF, 0, 0, String::new()).accessible);
    }
}
