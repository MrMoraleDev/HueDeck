// SPDX-License-Identifier: GPL-3.0-or-later
//! Raw USB on Linux (usbfs through `nusb`), for `Type() == "rawusb"` /
//! `"hybrid"` plugins: the Linux side of [`UsbBackend`].
//!
//! Enumeration reads sysfs only (`/sys/bus/usb/devices`, nothing opened):
//!
//! ```text
//! 1-4/{idVendor,idProduct,serial,manufacturer,product,busnum,devnum}
//! 1-4:1.2/{bInterfaceNumber,bInterfaceClass,bInterfaceSubClass,bInterfaceProtocol}
//! 1-4:1.2/driver -> …/drivers/usbhid      (absent: no kernel driver)
//! 1-4:1.2/ep_81/{bEndpointAddress,type}
//! ```
//!
//! Which interfaces may be claimed ([`Detach`]):
//!
//! * an interface without a kernel driver, always (a plain usbfs claim);
//! * an interface with a kernel driver, only through the backend that
//!   [`UsbBackend::for_rawusb`] hands to `rawusb` plugins, and only when it is
//!   not HID (class 0x03 or `usbhid`): the driver is detached on claim and
//!   re-attached when the handle closes. Hybrid devices keep every kernel
//!   driver; their HID side goes through hidraw.
//!
//! The device node is `/dev/bus/usb/BBB/DDD` (the `usb_device` node the udev
//! rules of `lightloom udev generate` tag `uaccess`). Its `instance_id` is
//! `USB\VID_xxxx&PID_xxxx\<serial or port>`, the `usb_parent` the hidraw
//! grouping gives the HID collections of the same device
//! ([`crate::tlc::group`]), so hybrid devices join their HID side.

use crate::sysfs::{is_usb_device_name, is_usb_interface_name, Files};
use lightloom_hal::usb::{UsbDeviceInfo, UsbInterfaceInfo};
use std::collections::BTreeMap;
use std::path::Path;

/// `/sys/bus/usb/devices`.
pub const USB_DEVICES: &str = "/sys/bus/usb/devices";

/// The USB HID interface class.
const CLASS_HID: u8 = 0x03;

/// Which kernel drivers a claim may detach.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Detach {
    /// None: only interfaces without a kernel driver (hybrid plugins).
    #[default]
    Never,
    /// Those of non-HID interfaces (rawusb plugins).
    NonHid,
}

/// Whether an interface with this class and kernel driver (empty: none) may
/// be claimed under `detach`.
pub fn claimable(class: u8, driver: &str, detach: Detach) -> bool {
    // `usbfs` is another usbfs claim (or ours): claiming says Busy if it holds.
    if driver.is_empty() || driver == "usbfs" {
        return true;
    }
    detach == Detach::NonHid && class != CLASS_HID && driver != "usbhid"
}

/// `info` with every interface re-judged under `detach`.
pub fn with_detach(mut info: UsbDeviceInfo, detach: Detach) -> UsbDeviceInfo {
    for i in &mut info.interfaces {
        i.accessible = claimable(i.class, &i.driver, detach);
    }
    info.driver_bound = info.interfaces.iter().any(|i| i.accessible);
    info
}

/// `USB\VID_xxxx&PID_xxxx\<id>`, the instance id shape the hidraw grouping
/// gives USB parents.
pub fn instance_id(vid: u16, pid: u16, id: &str) -> String {
    format!("USB\\VID_{vid:04X}&PID_{pid:04X}\\{id}")
}

/// `/dev/bus/usb/BBB/DDD`.
pub fn devnode(bus: u8, address: u8) -> String {
    format!("/dev/bus/usb/{bus:03}/{address:03}")
}

/// `(bus, address)` from a [`devnode`] path.
pub fn parse_devnode(path: &str) -> Option<(u8, u8)> {
    let rest = path.strip_prefix("/dev/bus/usb/")?;
    let (bus, addr) = rest.split_once('/')?;
    Some((bus.parse().ok()?, addr.parse().ok()?))
}

fn attr(fs: &impl Files, dir: &Path, name: &str) -> Option<String> {
    let bytes = fs.read(&dir.join(name))?;
    Some(String::from_utf8_lossy(&bytes).trim().to_string())
}

fn hex_u8(fs: &impl Files, dir: &Path, name: &str) -> Option<u8> {
    u8::from_str_radix(&attr(fs, dir, name)?, 16).ok()
}

fn hex_u16(fs: &impl Files, dir: &Path, name: &str) -> Option<u16> {
    u16::from_str_radix(&attr(fs, dir, name)?, 16).ok()
}

fn dec_u8(fs: &impl Files, dir: &Path, name: &str) -> Option<u8> {
    attr(fs, dir, name)?.parse().ok()
}

/// One interface directory (`1-4:1.2`).
fn read_interface(fs: &impl Files, dir: &Path, detach: Detach) -> Option<UsbInterfaceInfo> {
    let number = hex_u8(fs, dir, "bInterfaceNumber")?;
    let class = hex_u8(fs, dir, "bInterfaceClass").unwrap_or(0);
    let driver = fs
        .resolve(&dir.join("driver"))
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();
    let endpoints = fs
        .list(dir)
        .iter()
        .filter(|n| n.starts_with("ep_"))
        .filter_map(|n| hex_u8(fs, &dir.join(n), "bEndpointAddress"))
        .filter(|a| a & 0x0F != 0) // not the default control pipe
        .collect();
    Some(UsbInterfaceInfo {
        number,
        class,
        subclass: hex_u8(fs, dir, "bInterfaceSubClass").unwrap_or(0),
        protocol: hex_u8(fs, dir, "bInterfaceProtocol").unwrap_or(0),
        accessible: claimable(class, &driver, detach),
        driver,
        endpoints,
    })
}

/// Every USB device under `devices_dir` (normally [`USB_DEVICES`]), root hubs
/// left out, sorted by port. Interfaces are those of the active configuration
/// (sysfs lists no other); accessibility follows `detach`.
pub fn enumerate(fs: &impl Files, devices_dir: &Path, detach: Detach) -> Vec<UsbDeviceInfo> {
    let names = fs.list(devices_dir);
    // Serial usage per VID:PID: a serial shared by two devices cannot be an id.
    let mut raw = Vec::new();
    for port in names.iter().filter(|n| is_usb_device_name(n)) {
        let dir = devices_dir.join(port);
        let (Some(vid), Some(pid)) = (hex_u16(fs, &dir, "idVendor"), hex_u16(fs, &dir, "idProduct")) else {
            continue;
        };
        let prefix = format!("{port}:");
        let mut interfaces: Vec<UsbInterfaceInfo> = names
            .iter()
            .filter(|n| n.starts_with(&prefix) && is_usb_interface_name(n))
            .filter_map(|n| read_interface(fs, &devices_dir.join(n), detach))
            .collect();
        interfaces.sort_by_key(|i| i.number);
        interfaces.dedup_by_key(|i| i.number);
        raw.push((port.clone(), dir, vid, pid, interfaces));
    }
    let mut serials: BTreeMap<(u16, u16, String), usize> = BTreeMap::new();
    for (_, dir, vid, pid, _) in &raw {
        let serial = attr(fs, dir, "serial").unwrap_or_default();
        *serials.entry((*vid, *pid, serial)).or_default() += 1;
    }
    raw.into_iter()
        .map(|(port, dir, vid, pid, interfaces)| {
            let serial = attr(fs, &dir, "serial").unwrap_or_default();
            let unique = !serial.is_empty()
                && !serial.contains('\\')
                && serials.get(&(vid, pid, serial.clone())) == Some(&1);
            let id = if unique { serial.as_str() } else { port.as_str() };
            let bus = dec_u8(fs, &dir, "busnum").unwrap_or(0);
            let address = dec_u8(fs, &dir, "devnum").unwrap_or(0);
            UsbDeviceInfo {
                vid,
                pid,
                manufacturer: attr(fs, &dir, "manufacturer").unwrap_or_default(),
                product: attr(fs, &dir, "product").unwrap_or_default(),
                path: devnode(bus, address),
                bus_number: bus,
                address,
                driver_bound: interfaces.iter().any(|i| i.accessible),
                instance_id: instance_id(vid, pid, id),
                interfaces,
                serial,
            }
        })
        .collect()
}

#[cfg(target_os = "linux")]
pub use imp::{LinuxUsbBackend, LinuxUsbHandle};

#[cfg(target_os = "linux")]
mod imp {
    use super::{enumerate, parse_devnode, with_detach, Detach, USB_DEVICES};
    use crate::sysfs::RealFs;
    use lightloom_hal::usb::{UsbBackend, UsbDeviceInfo, UsbHandle, ENDPOINT_IN};
    use lightloom_hal::{HalError, HalResult};
    use nusb::descriptors::TransferType;
    use nusb::transfer::{
        Buffer, Bulk, ControlIn, ControlOut, ControlType, In, Interrupt, Out, Recipient, TransferError,
    };
    use nusb::{Device, Endpoint, Interface, MaybeFuture};
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;

    fn nusb_error(e: nusb::Error, what: &str) -> HalError {
        use nusb::ErrorKind as K;
        let text = format!("{what}: {e}");
        match e.kind() {
            K::Disconnected => HalError::Disconnected,
            K::Busy => HalError::Busy(text),
            K::PermissionDenied => HalError::PermissionDenied(format!(
                "{text}; install the udev rules (`lightloom udev generate`, docs/linux-porting.md)"
            )),
            K::NotFound => HalError::NotFound(text),
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

    /// The raw USB backend of Linux (usbfs through `nusb`). The default one
    /// detaches no kernel driver; [`UsbBackend::for_rawusb`] gives the one
    /// that detaches them from non-HID interfaces.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct LinuxUsbBackend {
        detach: Detach,
    }

    impl LinuxUsbBackend {
        /// The backend for hybrid devices (stateless: every call reads sysfs afresh).
        pub fn new() -> Self {
            Self::default()
        }

        /// The backend for rawusb plugins.
        pub fn detaching() -> Self {
            Self {
                detach: Detach::NonHid,
            }
        }
    }

    impl UsbBackend for LinuxUsbBackend {
        fn enumerate(&self) -> HalResult<Vec<UsbDeviceInfo>> {
            Ok(enumerate(&RealFs, Path::new(USB_DEVICES), self.detach))
        }

        fn open(&self, path: &str) -> HalResult<Box<dyn UsbHandle>> {
            let (bus, address) = parse_devnode(path)
                .ok_or_else(|| HalError::InvalidArgument(format!("not a usbfs node: {path}")))?;
            let info = self
                .enumerate()?
                .into_iter()
                .find(|d| d.path == path)
                .ok_or_else(|| HalError::NotFound(path.to_string()))?;
            if !info.driver_bound {
                return Err(HalError::NeedsDriver(info.driver_problem().unwrap_or_default()));
            }
            let found = nusb::list_devices()
                .wait()
                .map_err(|e| nusb_error(e, "listing USB devices"))?
                .find(|d| d.busnum() == bus && d.device_address() == address)
                .ok_or_else(|| HalError::NotFound(path.to_string()))?;
            // Opening reads the descriptors only: nothing is claimed yet.
            let device = found.open().wait().map_err(|e| nusb_error(e, "opening"))?;
            let mut kinds = BTreeMap::new();
            if let Ok(config) = device.active_configuration() {
                for group in config.interfaces() {
                    for e in group.first_alt_setting().endpoints() {
                        kinds.insert(e.address(), (e.transfer_type(), e.max_packet_size().max(1)));
                    }
                }
            }
            Ok(Box::new(LinuxUsbHandle {
                info,
                device,
                claimed: BTreeMap::new(),
                kinds,
                pipes: BTreeMap::new(),
            }))
        }

        fn for_rawusb(&self, info: &UsbDeviceInfo) -> Option<(Arc<dyn UsbBackend>, UsbDeviceInfo)> {
            (self.detach == Detach::Never).then(|| {
                let backend: Arc<dyn UsbBackend> = Arc::new(Self::detaching());
                (backend, with_detach(info.clone(), Detach::NonHid))
            })
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

    /// A device opened by [`LinuxUsbBackend`]. Interfaces are claimed on
    /// first use, only those [`UsbDeviceInfo::claimable_interface`] allows;
    /// a detached kernel driver is re-attached when the handle closes.
    pub struct LinuxUsbHandle {
        info: UsbDeviceInfo,
        device: Device,
        claimed: BTreeMap<u8, Interface>,
        /// Endpoint address → (transfer type, max packet size).
        kinds: BTreeMap<u8, (TransferType, usize)>,
        pipes: BTreeMap<u8, Pipe>,
    }

    impl std::fmt::Debug for LinuxUsbHandle {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("LinuxUsbHandle")
                .field("path", &self.info.path)
                .field("claimed", &self.claimed.keys().collect::<Vec<_>>())
                .finish_non_exhaustive()
        }
    }

    impl LinuxUsbHandle {
        fn interface(&mut self, number: u8) -> HalResult<Interface> {
            if let Some(i) = self.claimed.get(&number) {
                return Ok(i.clone());
            }
            let iface = self.info.claimable_interface(number)?;
            let what = |e: nusb::Error| nusb_error(e, &format!("claiming interface {number}"));
            let intf = if iface.driver.is_empty() || iface.driver == "usbfs" {
                self.device.claim_interface(number).wait().map_err(what)?
            } else {
                // Only non-HID interfaces of rawusb devices get here (`claimable`).
                tracing::info!(
                    "detaching kernel driver {} from interface {number} of {:04x}:{:04x}",
                    iface.driver,
                    self.info.vid,
                    self.info.pid
                );
                self.device
                    .detach_and_claim_interface(number)
                    .wait()
                    .map_err(what)?
            };
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

    impl UsbHandle for LinuxUsbHandle {
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
            // usbfs wants the addressed interface claimed; the same routing as Windows.
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
    use crate::sysfs::MemFs;
    use lightloom_hal::HalError;
    use std::path::PathBuf;

    const DEV: &str = "/sys/bus/usb/devices";

    fn file(fs: &mut MemFs, rel: &str, text: &str) {
        fs.file(format!("{DEV}/{rel}"), format!("{text}\n"));
    }

    fn device(fs: &mut MemFs, port: &str, vid: &str, pid: &str, serial: Option<&str>, bus: u8, dev: u8) {
        file(fs, &format!("{port}/idVendor"), vid);
        file(fs, &format!("{port}/idProduct"), pid);
        file(fs, &format!("{port}/busnum"), &bus.to_string());
        file(fs, &format!("{port}/devnum"), &dev.to_string());
        file(fs, &format!("{port}/product"), "Hub");
        if let Some(s) = serial {
            file(fs, &format!("{port}/serial"), s);
        }
    }

    fn interface(fs: &mut MemFs, name: &str, number: u8, class: u8, driver: Option<&str>, eps: &[u8]) {
        file(fs, &format!("{name}/bInterfaceNumber"), &format!("{number:02x}"));
        file(fs, &format!("{name}/bInterfaceClass"), &format!("{class:02x}"));
        file(fs, &format!("{name}/bInterfaceSubClass"), "00");
        file(fs, &format!("{name}/bInterfaceProtocol"), "00");
        for ep in eps {
            file(
                fs,
                &format!("{name}/ep_{ep:02x}/bEndpointAddress"),
                &format!("{ep:02x}"),
            );
        }
        if let Some(d) = driver {
            fs.links.insert(
                PathBuf::from(format!("{DEV}/{name}/driver")),
                PathBuf::from(format!("/sys/bus/usb/drivers/{d}")),
            );
        }
    }

    /// A hybrid hub (HID + vendor interface), a rawusb controller whose
    /// vendor interface has a kernel driver, two same-serial strips and a
    /// root hub.
    fn tree() -> MemFs {
        let mut fs = MemFs::default();
        device(&mut fs, "usb1", "1d6b", "0002", None, 1, 1);
        device(&mut fs, "1-4", "1234", "9999", Some("H1"), 1, 5);
        interface(&mut fs, "1-4:1.0", 0, 0x03, Some("usbhid"), &[0x81]);
        interface(&mut fs, "1-4:1.1", 1, 0xFF, None, &[0x02, 0x82]);
        device(&mut fs, "3-1.2", "1234", "7701", Some("SAME"), 3, 7);
        interface(&mut fs, "3-1.2:1.0", 0, 0xFF, Some("vendor_rgb"), &[0x01]);
        interface(&mut fs, "3-1.2:1.1", 1, 0x03, Some("usbhid"), &[0x83]);
        device(&mut fs, "3-1.3", "1234", "7701", Some("SAME"), 3, 8);
        interface(&mut fs, "3-1.3:1.0", 0, 0xFF, None, &[0x01]);
        fs
    }

    #[test]
    fn sysfs_describes_devices_interfaces_endpoints_and_drivers() {
        let list = enumerate(&tree(), Path::new(DEV), Detach::Never);
        let ids: Vec<&str> = list.iter().map(|d| d.instance_id.as_str()).collect();
        // Root hubs are left out; a serial two devices share gives way to the port.
        assert_eq!(
            ids,
            [
                r"USB\VID_1234&PID_9999\H1",
                r"USB\VID_1234&PID_7701\3-1.2",
                r"USB\VID_1234&PID_7701\3-1.3"
            ]
        );
        let hub = &list[0];
        assert_eq!((hub.vid, hub.pid, hub.serial.as_str()), (0x1234, 0x9999, "H1"));
        assert_eq!((hub.bus_number, hub.address), (1, 5));
        assert_eq!(hub.path, "/dev/bus/usb/001/005");
        assert_eq!(parse_devnode(&hub.path), Some((1, 5)));
        assert_eq!(hub.product, "Hub");
        assert_eq!(hub.interfaces.len(), 2);
        assert_eq!(hub.interfaces[0].driver, "usbhid");
        assert!(!hub.interfaces[0].accessible);
        assert_eq!(hub.interfaces[1].endpoints, [0x02, 0x82]);
        assert!(hub.interfaces[1].accessible && hub.driver_bound);
        // The vendor interface's endpoints route to it; the HID one is refused.
        assert_eq!(hub.claimable_for_endpoint(0x02).unwrap().number, 1);
        assert!(matches!(
            hub.claimable_for_endpoint(0x81),
            Err(HalError::NeedsDriver(_))
        ));
    }

    #[test]
    fn hybrid_instance_ids_match_the_hidraw_grouping() {
        // tlc::group names the USB parent the same way for a unique serial.
        let list = enumerate(&tree(), Path::new(DEV), Detach::Never);
        assert_eq!(list[0].instance_id, instance_id(0x1234, 0x9999, "H1"));
        assert_eq!(
            lightloom_hal_windows::path::signalrgb_serial_from_instance(&list[1].instance_id).as_deref(),
            Some("3-1.2")
        );
    }

    #[test]
    fn only_rawusb_backends_take_non_hid_interfaces_from_their_driver() {
        let never = enumerate(&tree(), Path::new(DEV), Detach::Never);
        let ctrl = &never[1];
        assert!(!ctrl.driver_bound, "vendor_rgb holds the only non-HID interface");
        let problem = ctrl.driver_problem().unwrap();
        assert!(problem.starts_with(lightloom_hal::usb::NEEDS_DRIVER), "{problem}");
        assert!(problem.contains("vendor_rgb"), "{problem}");

        let raw = with_detach(ctrl.clone(), Detach::NonHid);
        assert!(raw.driver_bound);
        assert!(raw.interfaces[0].accessible, "vendor driver: detached on claim");
        assert!(!raw.interfaces[1].accessible, "usbhid is never detached");
        assert_eq!(raw, enumerate(&tree(), Path::new(DEV), Detach::NonHid)[1]);

        assert!(claimable(0xFF, "", Detach::Never));
        assert!(claimable(0x03, "", Detach::Never), "HID without usbhid: free");
        assert!(!claimable(0x03, "usbhid", Detach::NonHid));
        assert!(!claimable(0xFF, "usbhid", Detach::NonHid));
        assert!(!claimable(0x03, "hid-generic", Detach::NonHid));
        assert!(claimable(0xFF, "usbfs", Detach::Never));
    }

    #[test]
    fn devnodes_round_trip() {
        assert_eq!(devnode(3, 12), "/dev/bus/usb/003/012");
        assert_eq!(parse_devnode("/dev/bus/usb/003/012"), Some((3, 12)));
        assert_eq!(parse_devnode("/dev/hidraw3"), None);
    }
}
