// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Device and endpoint identifiers.
//!
//! The grammar follows SignalRGB exactly so that persisted layouts, channel
//! arrays and log lines can be imported 1:1:
//!
//! | kind | uid |
//! |---|---|
//! | USB/HID device | `vid:pid:serial` (lowercase hex, 4 digits each) |
//! | subdevice | `<device uid>:<subdevice id>` |
//! | component channel | `<device uid>:<channel name>` |
//! | component instance | bare GUID |
//! | SMBus device | `I2CBUS:<decimal address>` |
//! | GPU (NvAPI/ADLX bus) | the GPU name |

use serde::{Deserialize, Serialize};
use std::fmt;

/// Identity of a physical USB/HID/serial device: `vid:pid:serial`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DeviceKey {
    /// USB vendor id.
    pub vid: u16,
    /// USB product id.
    pub pid: u16,
    /// USB serial string as reported by the device (may be empty; case preserved).
    pub serial: String,
}

impl DeviceKey {
    /// Build a key.
    pub fn new(vid: u16, pid: u16, serial: impl Into<String>) -> Self {
        Self { vid, pid, serial: serial.into() }
    }

    /// Parse `vid:pid:serial`. The serial may itself contain colons; only the
    /// first two separators are significant. Returns `None` on malformed hex.
    pub fn parse(s: &str) -> Option<Self> {
        let mut parts = s.splitn(3, ':');
        let vid = u16::from_str_radix(parts.next()?, 16).ok()?;
        let pid = u16::from_str_radix(parts.next()?, 16).ok()?;
        let serial = parts.next().unwrap_or("").to_string();
        Some(Self { vid, pid, serial })
    }

    /// `(vid, pid)` pair (plugin registry key).
    pub fn vid_pid(&self) -> (u16, u16) {
        (self.vid, self.pid)
    }
}

impl fmt::Display for DeviceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04x}:{:04x}:{}", self.vid, self.pid, self.serial)
    }
}

/// Identity of a canvas endpoint (anything that owns a placement on the layout).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EndpointUid {
    /// A USB/HID/serial device.
    Device(DeviceKey),
    /// A plugin-created subdevice.
    Subdevice {
        /// Owning device.
        device: DeviceKey,
        /// Subdevice id as passed to `device.createSubdevice(id)`.
        id: String,
    },
    /// A component channel (ARGB header) on a device.
    Channel {
        /// Owning device.
        device: DeviceKey,
        /// Channel name as passed to `device.addChannel(name, …)` (case-sensitive).
        name: String,
    },
    /// A component instance on a channel, keyed by its bare GUID.
    Component(String),
    /// An SMBus device at a 7-bit address.
    Smbus {
        /// 7-bit address.
        address: u8,
    },
    /// A GPU bus device, keyed by the GPU name.
    Gpu(String),
}

impl fmt::Display for EndpointUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EndpointUid::Device(k) => write!(f, "{k}"),
            EndpointUid::Subdevice { device, id } => write!(f, "{device}:{id}"),
            EndpointUid::Channel { device, name } => write!(f, "{device}:{name}"),
            EndpointUid::Component(guid) => f.write_str(guid),
            EndpointUid::Smbus { address } => write!(f, "I2CBUS:{address}"),
            EndpointUid::Gpu(name) => f.write_str(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_key_display_matches_signalrgb() {
        let k = DeviceKey::new(0x1E71, 0x2007, "00000000001A");
        assert_eq!(k.to_string(), "1e71:2007:00000000001A");
        let k = DeviceKey::new(0x1462, 0x7C91, "");
        assert_eq!(k.to_string(), "1462:7c91:");
    }

    #[test]
    fn device_key_parse_round_trips() {
        let k = DeviceKey::parse("1532:007c:000000000000").unwrap();
        assert_eq!(k, DeviceKey::new(0x1532, 0x007C, "000000000000"));
        assert_eq!(DeviceKey::parse(&k.to_string()).unwrap(), k);
        assert!(DeviceKey::parse("zz:0001:x").is_none());
        assert!(DeviceKey::parse("1532").is_none());
    }

    #[test]
    fn endpoint_uid_grammar() {
        let d = DeviceKey::new(0x1E71, 0x2007, "AB");
        assert_eq!(
            EndpointUid::Subdevice { device: d.clone(), id: "Fan1".into() }.to_string(),
            "1e71:2007:AB:Fan1"
        );
        assert_eq!(
            EndpointUid::Channel { device: d, name: "JRAINBOW1".into() }.to_string(),
            "1e71:2007:AB:JRAINBOW1"
        );
        assert_eq!(EndpointUid::Smbus { address: 0x39 }.to_string(), "I2CBUS:57");
        assert_eq!(
            EndpointUid::Gpu("NVIDIA GeForce RTX 3060".into()).to_string(),
            "NVIDIA GeForce RTX 3060"
        );
    }
}
