// SPDX-License-Identifier: GPL-3.0-or-later
//! Parsing of netlink `KOBJECT_UEVENT` messages (pure, every OS).
//!
//! Two shapes reach a `NETLINK_KOBJECT_UEVENT` socket:
//!
//! * **udev** (multicast group 2, sent by systemd-udevd after its rules ran):
//!   a `monitor_netlink_header` starting with `b"libudev\0"` and the magic
//!   `0xfeedcafe` in network order; `properties_off` at bytes 16..20 and
//!   `properties_len` at 20..24 (host order) locate the NUL-separated
//!   `KEY=value` list;
//! * **kernel** (group 1): `action@devpath\0` followed by `KEY=value\0` pairs.
//!
//! Lightloom listens to the udev group (devices are usable once the rules,
//! including `uaccess`, have run); the kernel shape is parsed too for tests
//! and for systems without udevd.

use crate::sysfs;
use crate::tlc::endpoint_path;
use lightloom_hal::hotplug::{DeviceClass, DeviceEvent};
use lightloom_hal::DeviceKey;
use std::collections::BTreeMap;
use std::path::Path;

/// `"libudev\0"`.
const UDEV_PREFIX: &[u8] = b"libudev\0";
/// `UDEV_MONITOR_MAGIC`, sent big-endian.
const UDEV_MAGIC: u32 = 0xfeed_cafe;

/// One device event.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Uevent {
    /// `ACTION` (`add`, `remove`, `change`, `bind`, …).
    pub action: String,
    /// `DEVPATH` (below `/sys`).
    pub devpath: String,
    /// `SUBSYSTEM`.
    pub subsystem: String,
    /// `DEVNAME` (`hidraw3` or `/dev/hidraw3`), when the device has a node.
    pub devname: Option<String>,
    /// Every property.
    pub props: BTreeMap<String, String>,
}

impl Uevent {
    /// `/dev/…` of the node, when it has one.
    pub fn devnode(&self) -> Option<String> {
        let name = self.devname.as_deref()?;
        Some(if name.starts_with('/') {
            name.to_string()
        } else {
            format!("/dev/{name}")
        })
    }
}

fn from_props<'a>(pairs: impl Iterator<Item = &'a [u8]>) -> Option<Uevent> {
    let mut ev = Uevent::default();
    for pair in pairs {
        let text = String::from_utf8_lossy(pair);
        let Some((k, v)) = text.split_once('=') else {
            continue;
        };
        ev.props.insert(k.to_string(), v.to_string());
    }
    ev.action = ev.props.get("ACTION")?.clone();
    ev.devpath = ev.props.get("DEVPATH")?.clone();
    ev.subsystem = ev.props.get("SUBSYSTEM").cloned().unwrap_or_default();
    ev.devname = ev.props.get("DEVNAME").cloned();
    Some(ev)
}

fn split_nul(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    bytes.split(|b| *b == 0).filter(|s| !s.is_empty())
}

/// Parse one datagram; `None` for anything that is not a well-formed event.
pub fn parse(msg: &[u8]) -> Option<Uevent> {
    if msg.starts_with(UDEV_PREFIX) {
        if msg.len() < 24 {
            return None;
        }
        let magic = u32::from_be_bytes(msg[8..12].try_into().ok()?);
        if magic != UDEV_MAGIC {
            return None;
        }
        let off = u32::from_ne_bytes(msg[16..20].try_into().ok()?) as usize;
        let len = u32::from_ne_bytes(msg[20..24].try_into().ok()?) as usize;
        let props = msg.get(off..off.checked_add(len)?)?;
        return from_props(split_nul(props));
    }
    // Kernel shape: "action@devpath\0KEY=value\0…".
    let mut parts = split_nul(msg);
    let head = parts.next()?;
    if !head.contains(&b'@') {
        return None;
    }
    from_props(parts)
}

/// The event for a hidraw uevent, if it is one Lightloom cares about.
pub fn device_event(ev: &Uevent, fs: &impl sysfs::Files, sys_root: &Path) -> Option<DeviceEvent> {
    if ev.subsystem != "hidraw" {
        return None;
    }
    let devnode = ev.devnode()?;
    let (vid, pid) = sysfs::vid_pid_from_devpath(&ev.devpath)?;
    let path = endpoint_path(&devnode, 0);
    let class = DeviceClass::Hid;
    match ev.action.as_str() {
        "add" | "change" => {
            let name = devnode.trim_start_matches("/dev/");
            let hid_dir = sys_root.join(ev.devpath.trim_start_matches('/')).join("device");
            let serial = fs
                .resolve(&hid_dir)
                .and_then(|d| sysfs::read_node(fs, name, &d))
                .map(|n| n.hid.uniq)
                .unwrap_or_default();
            Some(DeviceEvent::Arrived {
                key: DeviceKey::new(vid, pid, serial),
                class,
                path,
            })
        }
        "remove" => Some(DeviceEvent::Removed {
            key: DeviceKey::new(vid, pid, String::new()),
            class,
            path,
        }),
        _ => None,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A udev-shaped datagram with `props`.
    pub(crate) fn udev_msg(props: &[&str]) -> Vec<u8> {
        let mut body = Vec::new();
        for p in props {
            body.extend_from_slice(p.as_bytes());
            body.push(0);
        }
        let header_len = 40u32;
        let mut m = Vec::new();
        m.extend_from_slice(UDEV_PREFIX);
        m.extend_from_slice(&UDEV_MAGIC.to_be_bytes());
        m.extend_from_slice(&header_len.to_ne_bytes());
        m.extend_from_slice(&header_len.to_ne_bytes()); // properties_off
        m.extend_from_slice(&(body.len() as u32).to_ne_bytes()); // properties_len
        m.resize(header_len as usize, 0); // filter hashes
        m.extend_from_slice(&body);
        m
    }

    #[test]
    fn udev_message() {
        let m = udev_msg(&[
            "ACTION=add",
            "DEVPATH=/devices/pci0000:00/0000:00:14.0/usb1/1-4/1-4:1.2/0003:1532:007C.0005/hidraw/hidraw3",
            "SUBSYSTEM=hidraw",
            "DEVNAME=/dev/hidraw3",
            "MAJOR=241",
            "MINOR=3",
        ]);
        let ev = parse(&m).unwrap();
        assert_eq!(ev.action, "add");
        assert_eq!(ev.subsystem, "hidraw");
        assert_eq!(ev.devnode().as_deref(), Some("/dev/hidraw3"));
        assert_eq!(ev.props["MINOR"], "3");
    }

    #[test]
    fn kernel_message() {
        let m = b"remove@/devices/x/hidraw/hidraw3\0ACTION=remove\0DEVPATH=/devices/x/hidraw/hidraw3\0SUBSYSTEM=hidraw\0DEVNAME=hidraw3\0SEQNUM=5\0";
        let ev = parse(m).unwrap();
        assert_eq!(ev.action, "remove");
        assert_eq!(ev.devnode().as_deref(), Some("/dev/hidraw3"));
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(parse(b"").is_none());
        assert!(parse(b"libudev\0short").is_none());
        let mut bad = udev_msg(&["ACTION=add", "DEVPATH=/x"]);
        bad[8] ^= 0xff;
        assert!(parse(&bad).is_none());
        let mut oob = udev_msg(&["ACTION=add", "DEVPATH=/x"]);
        oob[20..24].copy_from_slice(&u32::MAX.to_ne_bytes());
        assert!(parse(&oob).is_none());
        assert!(parse(b"no-at-sign\0ACTION=add\0").is_none());
        assert!(parse(&udev_msg(&["DEVPATH=/x"])).is_none(), "no ACTION");
    }

    #[test]
    fn hidraw_events_become_device_events() {
        let mut fs = crate::sysfs::MemFs::default();
        let hid = crate::sysfs::tests::fake_usb_hid(&mut fs, "1-4", 2, (0x1532, 0x007c), Some("PM1"), 5, &[]);
        let hidraw = hid.join("hidraw").join("hidraw3");
        fs.links.insert(hidraw.join("device"), hid.clone());
        let root = Path::new("/sys");
        let devpath = hidraw
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\u{5c}', "/");
        let ev = |action: &str| {
            parse(&udev_msg(&[
                &format!("ACTION={action}"),
                &format!("DEVPATH=/{devpath}"),
                "SUBSYSTEM=hidraw",
                "DEVNAME=/dev/hidraw3",
            ]))
            .unwrap()
        };
        match device_event(&ev("add"), &fs, root).unwrap() {
            DeviceEvent::Arrived { key, class, path } => {
                assert_eq!(key.to_string(), "1532:007c:PM1");
                assert_eq!(class, DeviceClass::Hid);
                assert_eq!(path, "/dev/hidraw3#tlc0");
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            device_event(&ev("change"), &fs, root),
            Some(DeviceEvent::Arrived { .. })
        ));
        match device_event(&ev("remove"), &fs, root).unwrap() {
            DeviceEvent::Removed { key, .. } => assert_eq!(key.to_string(), "1532:007c:"),
            other => panic!("{other:?}"),
        }
        assert!(device_event(&ev("bind"), &fs, root).is_none());
        let mut input = ev("add");
        input.subsystem = "input".into();
        assert!(device_event(&input, &fs, root).is_none());
    }
}
