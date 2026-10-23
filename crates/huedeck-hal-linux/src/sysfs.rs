// SPDX-License-Identifier: GPL-3.0-or-later
//! Reading hidraw nodes from sysfs without opening them.
//!
//! Pure file reads (no ioctls), so the parsers and the tree walk run on every
//! OS against a fake tree. Layout (docs/research/m7-linux-backend.md §1.3):
//!
//! ```text
//! /sys/class/hidraw/hidraw3/device -> …/usb1/1-4/1-4:1.2/0003:1532:007C.0005
//!   0003:1532:007C.0005/uevent             HID_ID, HID_NAME, HID_PHYS, HID_UNIQ
//!   0003:1532:007C.0005/report_descriptor  post-fixup descriptor (0444)
//!   1-4:1.2/bInterfaceNumber               "%02x" (hex)
//!   1-4/{idVendor,idProduct}               "%04x"; serial, manufacturer, product, busnum, devpath
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The file access the sysfs reader needs: the real filesystem, or an
/// in-memory tree in tests (sysfs names contain `:`, which Windows forbids).
pub trait Files {
    /// The whole file, `None` when unreadable.
    fn read(&self, path: &Path) -> Option<Vec<u8>>;
    /// `path` with every symbolic link resolved, `None` when it does not exist.
    fn resolve(&self, path: &Path) -> Option<PathBuf>;
}

/// The real filesystem.
#[derive(Clone, Copy, Debug, Default)]
pub struct RealFs;

impl Files for RealFs {
    fn read(&self, path: &Path) -> Option<Vec<u8>> {
        std::fs::read(path).ok()
    }

    fn resolve(&self, path: &Path) -> Option<PathBuf> {
        std::fs::canonicalize(path).ok()
    }
}

/// An in-memory tree: files plus symbolic links (link path -> target).
#[derive(Clone, Debug, Default)]
pub struct MemFs {
    /// File contents by path.
    pub files: BTreeMap<PathBuf, Vec<u8>>,
    /// Links by path.
    pub links: BTreeMap<PathBuf, PathBuf>,
}

impl MemFs {
    /// Add a file.
    pub fn file(&mut self, path: impl Into<PathBuf>, data: impl Into<Vec<u8>>) {
        self.files.insert(path.into(), data.into());
    }
}

impl Files for MemFs {
    fn read(&self, path: &Path) -> Option<Vec<u8>> {
        self.files.get(path).cloned()
    }

    fn resolve(&self, path: &Path) -> Option<PathBuf> {
        if let Some(t) = self.links.get(path) {
            return Some(t.clone());
        }
        self.files
            .keys()
            .any(|k| k.starts_with(path))
            .then(|| path.to_path_buf())
    }
}

/// `BUS_USB` from `linux/input.h`.
pub const BUS_USB: u16 = 0x03;
/// `BUS_BLUETOOTH`.
pub const BUS_BLUETOOTH: u16 = 0x05;

/// The parent `hid` device's `uevent`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HidUevent {
    /// Bus type (`BUS_USB`, `BUS_BLUETOOTH`, …).
    pub bus: u16,
    /// Vendor id (`HID_ID` carries 32 bits; USB ids fit in 16).
    pub vid: u16,
    /// Product id.
    pub pid: u16,
    /// `HID_NAME`.
    pub name: String,
    /// `HID_PHYS` (`usb-0000:00:14.0-4/input2` for USB).
    pub phys: String,
    /// `HID_UNIQ` (USB iSerialNumber, Bluetooth address; may be empty).
    pub uniq: String,
}

/// Parse `KEY=value` lines of a hid device `uevent`; `None` without a valid `HID_ID`.
pub fn parse_hid_uevent(text: &str) -> Option<HidUevent> {
    let mut out = HidUevent::default();
    let mut have_id = false;
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        match k {
            // HID_ID=%04X:%08X:%08X
            "HID_ID" => {
                let mut parts = v.split(':');
                let (Some(b), Some(vi), Some(pi)) = (parts.next(), parts.next(), parts.next()) else {
                    return None;
                };
                out.bus = u16::from_str_radix(b, 16).ok()?;
                out.vid = u32::from_str_radix(vi, 16).ok()? as u16;
                out.pid = u32::from_str_radix(pi, 16).ok()? as u16;
                have_id = true;
            }
            "HID_NAME" => out.name = v.to_string(),
            "HID_PHYS" => out.phys = v.to_string(),
            "HID_UNIQ" => out.uniq = v.to_string(),
            _ => {}
        }
    }
    have_id.then_some(out)
}

/// `(bus, vid, pid)` from a hid device directory name (`0003:1532:007C.0005`).
pub fn parse_hid_dev_name(name: &str) -> Option<(u16, u16, u16)> {
    let (ids, _seq) = name.split_once('.')?;
    let mut parts = ids.split(':');
    let bus = u16::from_str_radix(parts.next()?, 16).ok()?;
    let vid = u16::from_str_radix(parts.next()?, 16).ok()?;
    let pid = u16::from_str_radix(parts.next()?, 16).ok()?;
    (parts.next().is_none() && ids.len() == 14).then_some((bus, vid, pid))
}

/// The USB interface number at the end of a USB `HID_PHYS` (`…/input2` → 2, decimal).
pub fn interface_from_phys(phys: &str) -> Option<i32> {
    let (_, tail) = phys.rsplit_once("/input")?;
    tail.parse().ok()
}

/// Whether `name` is a USB interface directory (`1-4:1.2`, `3-1.4.2:1.0`).
pub fn is_usb_interface_name(name: &str) -> bool {
    let Some((dev, cfg_if)) = name.split_once(':') else {
        return false;
    };
    let Some((cfg, iface)) = cfg_if.split_once('.') else {
        return false;
    };
    is_usb_device_name(dev) && is_digits(cfg) && is_digits(iface)
}

/// Whether `name` is a USB device directory (`1-4`, `3-1.4.2`; not root hubs `usb1`).
pub fn is_usb_device_name(name: &str) -> bool {
    let Some((bus, path)) = name.split_once('-') else {
        return false;
    };
    is_digits(bus) && !path.is_empty() && path.split('.').all(is_digits)
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// The USB device a HID interface belongs to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsbParent {
    /// sysfs directory of the USB device (`/sys/devices/…/1-4`).
    pub sysfs: PathBuf,
    /// Its name, the port chain (`1-4`, `3-1.4.2`): stable per physical port.
    pub port: String,
    /// `bInterfaceNumber` of the HID interface (hex in sysfs).
    pub interface: i32,
    /// `idVendor`.
    pub vid: u16,
    /// `idProduct`.
    pub pid: u16,
    /// `serial` (empty when the device has none).
    pub serial: String,
    /// `manufacturer`.
    pub manufacturer: String,
    /// `product`.
    pub product: String,
}

/// One `/dev/hidrawN` node as sysfs describes it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HidrawNode {
    /// `hidraw3`.
    pub name: String,
    /// `/dev/hidraw3`.
    pub devnode: String,
    /// The parent hid device directory (resolved `device` link).
    pub hid_dir: PathBuf,
    /// Its `uevent`.
    pub hid: HidUevent,
    /// Its `report_descriptor` (empty when unreadable).
    pub descriptor: Vec<u8>,
    /// The USB device and interface above it, for `BUS_USB` nodes.
    pub usb: Option<UsbParent>,
}

impl HidrawNode {
    /// Whether the node sits on USB.
    pub fn is_usb(&self) -> bool {
        self.hid.bus == BUS_USB
    }
}

fn attr(fs: &impl Files, dir: &Path, name: &str) -> Option<String> {
    let bytes = fs.read(&dir.join(name))?;
    Some(String::from_utf8_lossy(&bytes).trim().to_string())
}

fn hex_attr(fs: &impl Files, dir: &Path, name: &str) -> Option<u16> {
    u16::from_str_radix(&attr(fs, dir, name)?, 16).ok()
}

/// The USB parent of the hid device at `hid_dir`: its directory's parent must
/// be a USB interface, and that one's parent the USB device.
pub fn read_usb_parent(fs: &impl Files, hid_dir: &Path) -> Option<UsbParent> {
    let iface_dir = hid_dir.parent()?;
    if !is_usb_interface_name(iface_dir.file_name()?.to_str()?) {
        return None;
    }
    let dev_dir = iface_dir.parent()?;
    let port = dev_dir.file_name()?.to_str()?.to_string();
    if !is_usb_device_name(&port) {
        return None;
    }
    let interface = attr(fs, iface_dir, "bInterfaceNumber")
        .and_then(|s| i32::from_str_radix(&s, 16).ok())
        .unwrap_or(0);
    Some(UsbParent {
        sysfs: dev_dir.to_path_buf(),
        port,
        interface,
        vid: hex_attr(fs, dev_dir, "idVendor")?,
        pid: hex_attr(fs, dev_dir, "idProduct")?,
        serial: attr(fs, dev_dir, "serial").unwrap_or_default(),
        manufacturer: attr(fs, dev_dir, "manufacturer").unwrap_or_default(),
        product: attr(fs, dev_dir, "product").unwrap_or_default(),
    })
}

/// Read one hidraw node given its name and its resolved hid device directory.
/// `None` when the hid `uevent` is missing or has no `HID_ID`.
pub fn read_node(fs: &impl Files, name: &str, hid_dir: &Path) -> Option<HidrawNode> {
    let hid = parse_hid_uevent(&String::from_utf8_lossy(&fs.read(&hid_dir.join("uevent"))?))?;
    let descriptor = fs.read(&hid_dir.join("report_descriptor")).unwrap_or_default();
    let usb = if hid.bus == BUS_USB {
        read_usb_parent(fs, hid_dir)
    } else {
        None
    };
    Some(HidrawNode {
        name: name.to_string(),
        devnode: format!("/dev/{name}"),
        hid_dir: hid_dir.to_path_buf(),
        hid,
        descriptor,
        usb,
    })
}

/// Every hidraw node under `class_dir` (normally `/sys/class/hidraw`), sorted
/// by minor number. Nodes whose `device` link cannot be resolved are skipped.
pub fn enumerate(class_dir: &Path) -> std::io::Result<Vec<HidrawNode>> {
    let mut nodes = Vec::new();
    for entry in std::fs::read_dir(class_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("hidraw") {
            continue;
        }
        let Some(hid_dir) = RealFs.resolve(&entry.path().join("device")) else {
            continue;
        };
        if let Some(node) = read_node(&RealFs, &name, &hid_dir) {
            nodes.push(node);
        }
    }
    nodes.sort_by_key(|n| minor_of(&n.name));
    Ok(nodes)
}

/// `hidraw12` → 12 (sorting only).
fn minor_of(name: &str) -> u32 {
    name.trim_start_matches("hidraw").parse().unwrap_or(u32::MAX)
}

/// `(vid, pid)` from a `DEVPATH` that runs through a hid device
/// (`…/0003:1532:007C.0005/hidraw/hidraw3`), for hotplug events of nodes that
/// are already gone.
pub fn vid_pid_from_devpath(devpath: &str) -> Option<(u16, u16)> {
    devpath
        .split('/')
        .rev()
        .find_map(parse_hid_dev_name)
        .map(|(_, v, p)| (v, p))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn uevent_parses() {
        let u = parse_hid_uevent(
            "DRIVER=hid-generic\nHID_ID=0003:00001532:0000007C\nHID_NAME=Razer Razer DeathAdder V2 Pro\n\
             HID_PHYS=usb-0000:00:14.0-4/input2\nHID_UNIQ=\nMODALIAS=hid:b0003g0001v00001532p0000007C\n",
        )
        .unwrap();
        assert_eq!((u.bus, u.vid, u.pid), (BUS_USB, 0x1532, 0x007c));
        assert_eq!(u.name, "Razer Razer DeathAdder V2 Pro");
        assert_eq!(interface_from_phys(&u.phys), Some(2));
        assert_eq!(u.uniq, "");
        assert!(parse_hid_uevent("HID_NAME=x\n").is_none());
        assert!(parse_hid_uevent("HID_ID=zz:1:2\n").is_none());
    }

    #[test]
    fn names() {
        assert_eq!(parse_hid_dev_name("0003:1532:007C.0005"), Some((3, 0x1532, 0x7c)));
        assert_eq!(parse_hid_dev_name("0005:054C:0CE6.000A"), Some((5, 0x054c, 0x0ce6)));
        assert_eq!(parse_hid_dev_name("1-4:1.2"), None);
        assert!(is_usb_interface_name("1-4:1.2"));
        assert!(is_usb_interface_name("3-1.4.2:1.10"));
        assert!(!is_usb_interface_name("1-4"));
        assert!(!is_usb_interface_name("0003:1532:007C.0005"));
        assert!(is_usb_device_name("3-1.4.2"));
        assert!(!is_usb_device_name("usb1"));
        assert!(!is_usb_device_name("1-"));
        assert_eq!(
            vid_pid_from_devpath("/devices/pci0000:00/0000:00:14.0/usb1/1-4/1-4:1.2/0003:1532:007C.0005/hidraw/hidraw3"),
            Some((0x1532, 0x7c))
        );
        assert_eq!(vid_pid_from_devpath("/devices/virtual/misc/uhid"), None);
        assert_eq!(interface_from_phys("usb-0000:00:14.0-4/input10"), Some(10));
        assert_eq!(interface_from_phys("a0:b1:c2:d3:e4:f5"), None);
    }

    /// Add a fake USB HID interface to `fs`; returns the hid device dir.
    pub(crate) fn fake_usb_hid(
        fs: &mut MemFs,
        port: &str,
        iface: u8,
        ids: (u16, u16),
        serial: Option<&str>,
        seq: u16,
        descriptor: &[u8],
    ) -> PathBuf {
        let dev = Path::new("/sys/devices/pci0000:00/0000:00:14.0/usb1").join(port);
        let iface_dir = dev.join(format!("{port}:1.{iface}"));
        let hid = iface_dir.join(format!("0003:{:04X}:{:04X}.{seq:04X}", ids.0, ids.1));
        fs.file(dev.join("idVendor"), format!("{:04x}\n", ids.0));
        fs.file(dev.join("idProduct"), format!("{:04x}\n", ids.1));
        fs.file(dev.join("manufacturer"), "Maker\n");
        fs.file(dev.join("product"), "Gadget\n");
        if let Some(s) = serial {
            fs.file(dev.join("serial"), format!("{s}\n"));
        }
        fs.file(iface_dir.join("bInterfaceNumber"), format!("{iface:02x}\n"));
        fs.file(
            hid.join("uevent"),
            format!(
                "DRIVER=hid-generic\nHID_ID=0003:{:08X}:{:08X}\nHID_NAME=Maker Gadget\n\
                 HID_PHYS=usb-0000:00:14.0-{}/input{iface}\nHID_UNIQ={}\n",
                ids.0,
                ids.1,
                port.trim_start_matches("1-"),
                serial.unwrap_or("")
            ),
        );
        fs.file(hid.join("report_descriptor"), descriptor);
        hid
    }

    #[test]
    fn reads_a_fake_tree() {
        let mut fs = MemFs::default();
        let hid = fake_usb_hid(&mut fs, "1-4", 0x0a, (0x1532, 0x007c), Some("PM1234"), 5, &[0x06, 0x00, 0xff]);
        let node = read_node(&fs, "hidraw3", &hid).unwrap();
        assert_eq!(node.devnode, "/dev/hidraw3");
        assert!(node.is_usb());
        let usb = node.usb.as_ref().unwrap();
        assert_eq!(usb.interface, 10, "bInterfaceNumber is hex");
        assert_eq!((usb.vid, usb.pid), (0x1532, 0x007c));
        assert_eq!(usb.port, "1-4");
        assert_eq!(usb.serial, "PM1234");
        assert_eq!(usb.product, "Gadget");
        assert_eq!(node.descriptor, vec![0x06, 0x00, 0xff]);
    }

    #[test]
    fn non_usb_node_has_no_usb_parent() {
        let mut fs = MemFs::default();
        let hid = Path::new("/sys/devices/virtual/bt/0005:054C:0CE6.000A").to_path_buf();
        fs.file(
            hid.join("uevent"),
            "HID_ID=0005:0000054C:00000CE6\nHID_NAME=DualSense Wireless Controller\nHID_UNIQ=a0:b1:c2:d3:e4:f5\n",
        );
        let node = read_node(&fs, "hidraw7", &hid).unwrap();
        assert_eq!(node.hid.bus, BUS_BLUETOOTH);
        assert!(node.usb.is_none());
        assert!(node.descriptor.is_empty());
    }
}
