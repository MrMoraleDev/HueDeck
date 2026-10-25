// SPDX-License-Identifier: GPL-3.0-or-later
//! Serial ports (`/dev/ttyACM*`, `/dev/ttyUSB*`, …) from sysfs, without
//! opening them, and (Linux only) [`LinuxSerialBackend`].
//!
//! ```text
//! /sys/class/tty/ttyACM0/device -> …/usb1/1-4/1-4:1.0            (cdc_acm binds the interface)
//! /sys/class/tty/ttyUSB0/device -> …/usb1/1-4/1-4:1.0/ttyUSB0    (usb-serial port below it)
//!   1-4:1.0/bInterfaceNumber   "%02x" (hex)
//!   1-4/{idVendor,idProduct}   "%04x"; serial, manufacturer, product
//! ```
//!
//! Virtual terminals and ptys have no `device` link and are left out. Ports
//! that are not on USB (`ttyS*`) are listed without VID:PID, so no plugin
//! ever claims them. The pure part runs on every OS against a [`MemFs`](crate::sysfs::MemFs).

use crate::sysfs::{is_usb_device_name, is_usb_interface_name, Files};
use lightloom_hal::SerialPortInfo;
use std::path::Path;

fn attr(fs: &impl Files, dir: &Path, name: &str) -> Option<String> {
    let bytes = fs.read(&dir.join(name))?;
    Some(String::from_utf8_lossy(&bytes).trim().to_string())
}

fn hex(fs: &impl Files, dir: &Path, name: &str) -> Option<u16> {
    u16::from_str_radix(&attr(fs, dir, name)?, 16).ok()
}

/// The port `name` whose resolved `device` link is `device_dir`.
pub fn read_tty(fs: &impl Files, name: &str, device_dir: &Path) -> SerialPortInfo {
    let mut info = SerialPortInfo {
        port_name: format!("/dev/{name}"),
        ..SerialPortInfo::default()
    };
    // The USB interface is the device itself (cdc_acm) or a parent (usb-serial).
    let iface_dir = device_dir.ancestors().take(3).find(|d| {
        d.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(is_usb_interface_name)
    });
    let Some(iface_dir) = iface_dir else {
        return info;
    };
    let Some(dev_dir) = iface_dir.parent().filter(|d| {
        d.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(is_usb_device_name)
    }) else {
        return info;
    };
    let (Some(vid), Some(pid)) = (hex(fs, dev_dir, "idVendor"), hex(fs, dev_dir, "idProduct")) else {
        return info;
    };
    info.vid = Some(vid);
    info.pid = Some(pid);
    info.serial = attr(fs, dev_dir, "serial").filter(|s| !s.is_empty());
    info.interface = attr(fs, iface_dir, "bInterfaceNumber").and_then(|s| u8::from_str_radix(&s, 16).ok());
    info.manufacturer = attr(fs, dev_dir, "manufacturer");
    info.product = attr(fs, dev_dir, "product");
    info
}

/// Every tty under `class_dir` (normally `/sys/class/tty`) that has a
/// `device` link, sorted by name (`ttyACM2` before `ttyACM10`).
pub fn enumerate_ttys(fs: &impl Files, class_dir: &Path) -> Vec<SerialPortInfo> {
    let mut out: Vec<SerialPortInfo> = fs
        .list(class_dir)
        .into_iter()
        .filter(|name| name.starts_with("tty"))
        .filter_map(|name| {
            let device_dir = fs.resolve(&class_dir.join(&name).join("device"))?;
            Some(read_tty(fs, &name, &device_dir))
        })
        .collect();
    out.sort_by(|a, b| natural_order(&a.port_name, &b.port_name));
    out
}

/// `ttyACM2` before `ttyACM10`, then by text.
fn natural_order(a: &str, b: &str) -> std::cmp::Ordering {
    fn split(s: &str) -> (&str, Option<u64>) {
        let digits = s.len() - s.bytes().rev().take_while(u8::is_ascii_digit).count();
        (&s[..digits], s[digits..].parse().ok())
    }
    split(a).cmp(&split(b)).then_with(|| a.cmp(b))
}

/// Serial ports of this machine: sysfs enumeration, `serialport` I/O
/// (exclusive `TIOCEXCL` opens). Opening needs read/write access to the node
/// (the `dialout` / `uucp` group, or the udev rules `lightloom udev generate`
/// writes for claimed devices).
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, Default)]
pub struct LinuxSerialBackend;

#[cfg(target_os = "linux")]
impl LinuxSerialBackend {
    /// The backend (stateless).
    pub fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "linux")]
impl lightloom_hal::SerialBackend for LinuxSerialBackend {
    fn available_ports(&self) -> lightloom_hal::HalResult<Vec<SerialPortInfo>> {
        Ok(enumerate_ttys(&crate::sysfs::RealFs, Path::new("/sys/class/tty")))
    }

    fn open(
        &self,
        port_name: &str,
        config: &lightloom_hal::SerialConfig,
    ) -> lightloom_hal::HalResult<Box<dyn lightloom_hal::SerialHandle>> {
        if !port_name.starts_with("/dev/tty") || port_name.contains("..") {
            return Err(lightloom_hal::HalError::InvalidArgument(format!(
                "{port_name} is not a tty device node"
            )));
        }
        lightloom_hal_windows::serial::open_port(port_name, config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sysfs::MemFs;
    use std::path::PathBuf;

    fn usb_dev(fs: &mut MemFs, port: &str, ids: (u16, u16), serial: Option<&str>) -> PathBuf {
        let dev = Path::new("/sys/devices/pci0000:00/0000:00:14.0/usb1").join(port);
        fs.file(dev.join("idVendor"), format!("{:04x}\n", ids.0));
        fs.file(dev.join("idProduct"), format!("{:04x}\n", ids.1));
        fs.file(dev.join("manufacturer"), "Maker\n");
        fs.file(dev.join("product"), "Strip\n");
        if let Some(s) = serial {
            fs.file(dev.join("serial"), format!("{s}\n"));
        }
        dev
    }

    #[test]
    fn acm_and_usb_serial_ports_carry_their_usb_parent() {
        let mut fs = MemFs::default();
        let class = Path::new("/sys/class/tty");
        // cdc_acm: device -> the interface.
        let acm = usb_dev(&mut fs, "1-4", (0x16D5, 0x2A08), Some("N8-0001"));
        let acm_if = acm.join("1-4:1.0");
        fs.file(acm_if.join("bInterfaceNumber"), "00\n");
        fs.links.insert(class.join("ttyACM10/device"), acm_if.clone());
        fs.file(class.join("ttyACM10/dev"), "166:10\n");
        // usb-serial (CH340): device -> the port below the interface.
        let ch = usb_dev(&mut fs, "1-2.3", (0x1A86, 0x7523), None);
        let ch_if = ch.join("1-2.3:1.0");
        fs.file(ch_if.join("bInterfaceNumber"), "00\n");
        fs.file(ch_if.join("ttyUSB0/uevent"), "DRIVER=ch341-uart\n");
        fs.links
            .insert(class.join("ttyUSB0/device"), ch_if.join("ttyUSB0"));
        fs.file(class.join("ttyUSB0/dev"), "188:0\n");
        // A platform UART and a virtual console (no device link).
        fs.file(Path::new("/sys/devices/pnp0/00:05/id"), "PNP0501\n");
        fs.links.insert(
            class.join("ttyS0/device"),
            PathBuf::from("/sys/devices/pnp0/00:05"),
        );
        fs.file(class.join("ttyS0/dev"), "4:64\n");
        fs.file(class.join("tty1/dev"), "4:1\n");
        // A second ACM port on another interface of the same device.
        let acm_if2 = acm.join("1-4:1.2");
        fs.file(acm_if2.join("bInterfaceNumber"), "02\n");
        fs.links.insert(class.join("ttyACM2/device"), acm_if2);
        fs.file(class.join("ttyACM2/dev"), "166:2\n");

        let ports = enumerate_ttys(&fs, class);
        let names: Vec<&str> = ports.iter().map(|p| p.port_name.as_str()).collect();
        assert_eq!(
            names,
            ["/dev/ttyACM2", "/dev/ttyACM10", "/dev/ttyS0", "/dev/ttyUSB0"]
        );
        let acm10 = &ports[1];
        assert_eq!(acm10.usb_ids(), Some((0x16D5, 0x2A08)));
        assert_eq!(acm10.serial.as_deref(), Some("N8-0001"));
        assert_eq!(acm10.interface, Some(0));
        assert_eq!(acm10.product.as_deref(), Some("Strip"));
        assert_eq!(ports[0].interface, Some(2));
        assert_eq!(ports[2].usb_ids(), None, "platform UART");
        let ch340 = &ports[3];
        assert_eq!(ch340.usb_ids(), Some((0x1A86, 0x7523)));
        assert_eq!(ch340.serial, None);
        assert_eq!(ch340.interface, Some(0));
    }
}
