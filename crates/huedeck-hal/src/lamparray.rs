// SPDX-License-Identifier: GPL-3.0-or-later
//! LampArray backend (Windows Dynamic Lighting via WinRT `Windows.Devices.Lights`).
//!
//! HueDeck consumes LampArray devices as a generic backend so DL-certified
//! devices light without a plugin. A HID plugin (including the generic
//! `hid-lamparray` plugin) always wins over this backend on the same device;
//! the engine drops every LampArray a plugin already drives (see
//! [`plugin_owner`]).
//!
//! Only the ambient lighting controller (or the foreground app) chosen in
//! Settings > Personalization > Dynamic Lighting may set colours: a
//! [`LampArrayHandle`] reports that through [`LampArrayHandle::available`],
//! and the backend whether HueDeck has the package identity that makes it an
//! ambient controller at all ([`LampArrayBackend::has_identity`]).

use crate::error::HalResult;
use serde::{Deserialize, Serialize};

/// HID usage page of LampArray collections.
pub const LAMPARRAY_USAGE_PAGE: u16 = 0x59;
/// HID usage of the LampArray top-level collection.
pub const LAMPARRAY_USAGE: u16 = 0x01;

/// Kind of LampArray (HID LampArrayKind / WinRT `LampArrayKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LampArrayKind {
    /// Keyboard.
    Keyboard,
    /// Mouse.
    Mouse,
    /// Game controller.
    GameController,
    /// Peripheral.
    Peripheral,
    /// Scene lighting.
    Scene,
    /// Notification.
    Notification,
    /// Chassis.
    Chassis,
    /// Wearable.
    Wearable,
    /// Furniture.
    Furniture,
    /// Art.
    Art,
    /// Headset.
    Headset,
    /// Microphone.
    Microphone,
    /// Speaker.
    Speaker,
    /// Undefined (0) or a value this list does not know.
    Other(u32),
}

impl LampArrayKind {
    /// From the numeric value (HID usage `LampArrayKind`, WinRT enum value).
    pub fn from_raw(v: u32) -> Self {
        match v {
            1 => Self::Keyboard,
            2 => Self::Mouse,
            3 => Self::GameController,
            4 => Self::Peripheral,
            5 => Self::Scene,
            6 => Self::Notification,
            7 => Self::Chassis,
            8 => Self::Wearable,
            9 => Self::Furniture,
            10 => Self::Art,
            11 => Self::Headset,
            12 => Self::Microphone,
            13 => Self::Speaker,
            other => Self::Other(other),
        }
    }
}

/// One lamp of a LampArray.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LampInfo {
    /// Lamp index.
    pub index: u32,
    /// Position in micrometres from the front-top-left corner of the bounding
    /// box (+X right, +Y down, +Z away from the user).
    pub position_um: [i32; 3],
    /// Whether the lamp takes colours (a fixed-colour lamp does not).
    pub is_rgb: bool,
}

/// Static description of a LampArray device.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LampArrayInfo {
    /// Backend-specific id (Windows: the HID interface path, `DeviceInformation.Id`).
    pub id: String,
    /// Product name.
    pub name: String,
    /// Hardware vendor id.
    pub vid: u16,
    /// Hardware product id.
    pub pid: u16,
    /// Container id of the physical device (Windows `System.Devices.ContainerId`,
    /// see [`normalize_container`]), when known.
    pub container_id: Option<String>,
    /// Kind.
    pub kind: LampArrayKind,
    /// Bounding box in micrometres (width, height, depth).
    pub bounding_box_um: [u32; 3],
    /// Minimum update interval in microseconds.
    pub min_update_interval_us: u32,
    /// Lamps, by index.
    pub lamps: Vec<LampInfo>,
}

/// An open LampArray.
pub trait LampArrayHandle: Send {
    /// Static information.
    fn info(&self) -> &LampArrayInfo;
    /// Whether this process may set colours now (Windows: `LampArray.IsAvailable`,
    /// kept current by `AvailabilityChanged`). Writes while unavailable are
    /// dropped by the OS; callers skip them and resend a full frame when the
    /// device becomes available again.
    fn available(&self) -> bool {
        true
    }
    /// Set every lamp colour (RGBA, one entry per lamp of [`LampArrayInfo::lamps`],
    /// in that order) in one update. Fixed-colour lamps are skipped.
    fn set_colors(&mut self, rgba: &[[u8; 4]]) -> HalResult<()>;
    /// Return control to the OS/firmware (autonomous mode).
    fn release(&mut self) -> HalResult<()>;
}

/// Enumerates and opens LampArray devices.
pub trait LampArrayBackend: Send + Sync {
    /// LampArray devices present now.
    fn enumerate(&self) -> HalResult<Vec<LampArrayInfo>>;
    /// Open by id.
    fn open(&self, id: &str) -> HalResult<Box<dyn LampArrayHandle>>;
    /// A counter that changes whenever a LampArray arrives or leaves (the
    /// caller enumerates again); constant for backends without hotplug.
    fn generation(&self) -> u64 {
        0
    }
    /// Whether this process can be the ambient lighting controller (Windows:
    /// it runs with the HueDeck package identity). Without it the OS gives
    /// the devices to HueDeck only while it is the foreground app, which the
    /// daemon never is.
    fn has_identity(&self) -> bool {
        true
    }
    /// The container id (same form as [`LampArrayInfo::container_id`]) of the
    /// physical device a HID interface path belongs to, when the OS knows it.
    fn container_of(&self, _interface_path: &str) -> Option<String> {
        None
    }
}

/// A HID device a plugin drives, as far as LampArray ownership is concerned.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnedHid {
    /// Its device key.
    pub key: String,
    /// USB vendor id.
    pub vid: u16,
    /// USB product id.
    pub pid: u16,
    /// Interface paths of its collections.
    pub paths: Vec<String>,
    /// Container ids of its collections (see [`LampArrayBackend::container_of`]).
    pub containers: Vec<String>,
}

/// Why a LampArray belongs to a plugin's device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnedBy {
    /// The plugin's device has the LampArray's interface path among its
    /// collections (the `hid-lamparray` plugin, or a vendor plugin whose
    /// device includes the 0x59 collection).
    Path,
    /// Same physical device (container id): another interface, or another
    /// product id of the same device (wired / dongle).
    Container,
    /// Same VID:PID: plugins claim devices by VID:PID, so when neither path
    /// nor container id settles it HueDeck never drives one product through
    /// two routes.
    VidPid,
}

/// The plugin-driven HID device that owns `lamp`, if any, and why. A HID
/// plugin always wins over the LampArray backend on the same device; rules in
/// order: same interface path (case-insensitive), same container id, same
/// VID:PID.
pub fn plugin_owner<'a>(lamp: &LampArrayInfo, owned: &'a [OwnedHid]) -> Option<(&'a OwnedHid, OwnedBy)> {
    if let Some(o) = owned
        .iter()
        .find(|o| o.paths.iter().any(|p| p.eq_ignore_ascii_case(&lamp.id)))
    {
        return Some((o, OwnedBy::Path));
    }
    if let Some(c) = &lamp.container_id {
        if let Some(o) = owned
            .iter()
            .find(|o| o.containers.iter().any(|x| x.eq_ignore_ascii_case(c)))
        {
            return Some((o, OwnedBy::Container));
        }
    }
    owned
        .iter()
        .find(|o| (o.vid, o.pid) == (lamp.vid, lamp.pid))
        .map(|o| (o, OwnedBy::VidPid))
}

/// Normalise a container id: lower case, no braces (`{ABC-…}` → `abc-…`).
/// The null GUID and the all-ones GUID (Windows' "no container") give `None`.
pub fn normalize_container(id: &str) -> Option<String> {
    let s = id
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .to_ascii_lowercase();
    let digits: Vec<char> = s.chars().filter(char::is_ascii_hexdigit).collect();
    if digits.is_empty() || digits.iter().all(|&c| c == '0') || digits.iter().all(|&c| c == 'f') {
        return None;
    }
    Some(s)
}

/// The HueDeck device key of a LampArray: `lamparray:<vid>:<pid>:<instance>`,
/// where the instance is the device-instance segment of a Windows HID
/// interface path (`\\?\HID#VID_1532&PID_007C&MI_03#8&5cc7230&0&0000#{…}` →
/// `8-5cc7230-0-0000`, `&` → `-` as in HID device keys) or, for any other id,
/// a hash of it.
pub fn device_key(info: &LampArrayInfo) -> String {
    let segs: Vec<&str> = info.id.split('#').collect();
    let instance = if segs.len() >= 3 && !segs[2].is_empty() {
        segs[2].replace('&', "-").to_ascii_lowercase()
    } else {
        // FNV-1a: stable across runs and platforms.
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in info.id.to_ascii_lowercase().bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        format!("{h:016x}")
    };
    format!("lamparray:{:04x}:{:04x}:{instance}", info.vid, info.pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lamp(id: &str, vid: u16, pid: u16, container: Option<&str>) -> LampArrayInfo {
        LampArrayInfo {
            id: id.into(),
            name: "x".into(),
            vid,
            pid,
            container_id: container.map(Into::into),
            kind: LampArrayKind::Mouse,
            bounding_box_um: [1, 1, 1],
            min_update_interval_us: 0,
            lamps: Vec::new(),
        }
    }

    fn hid(key: &str, vid: u16, pid: u16, paths: &[&str], containers: &[&str]) -> OwnedHid {
        OwnedHid {
            key: key.into(),
            vid,
            pid,
            paths: paths.iter().map(|s| s.to_string()).collect(),
            containers: containers.iter().map(|s| s.to_string()).collect(),
        }
    }

    const PATH: &str =
        r"\\?\HID#VID_1532&PID_007C&MI_03#8&5cc7230&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}";

    #[test]
    fn device_key_uses_the_instance_segment() {
        assert_eq!(
            device_key(&lamp(PATH, 0x1532, 0x007c, None)),
            "lamparray:1532:007c:8-5cc7230-0-0000"
        );
        let k = device_key(&lamp("fake-lamp-1", 1, 2, None));
        assert!(k.starts_with("lamparray:0001:0002:") && k.len() == "lamparray:0001:0002:".len() + 16);
        assert_eq!(k, device_key(&lamp("FAKE-LAMP-1", 1, 2, None)));
    }

    #[test]
    fn plugin_owner_rules_in_order() {
        let lower = PATH.to_ascii_lowercase();
        let owned = vec![
            hid("1532:007c:a", 0x1532, 0x007c, &[], &["c1"]),
            hid("1532:007c:b", 0x1532, 0x007c, &[&lower], &[]),
        ];
        // The exact interface path wins over an earlier VID:PID match.
        let (o, why) = plugin_owner(&lamp(PATH, 0x1532, 0x007c, None), &owned).unwrap();
        assert_eq!((o.key.as_str(), why), ("1532:007c:b", OwnedBy::Path));
        // Container id: another product id of the same physical device (dongle / cable).
        let (o, why) = plugin_owner(&lamp("other", 0x1532, 0x007d, Some("C1")), &owned).unwrap();
        assert_eq!((o.key.as_str(), why), ("1532:007c:a", OwnedBy::Container));
        // VID:PID fallback.
        let (_, why) = plugin_owner(&lamp("other", 0x1532, 0x007c, Some("c9")), &owned).unwrap();
        assert_eq!(why, OwnedBy::VidPid);
        // Nobody.
        assert!(plugin_owner(&lamp("other", 0x1532, 0x0099, Some("c9")), &owned).is_none());
        assert!(plugin_owner(&lamp("other", 0x1532, 0x0099, None), &[]).is_none());
    }

    #[test]
    fn container_ids_are_normalised() {
        assert_eq!(normalize_container("{0A1B-CC}").as_deref(), Some("0a1b-cc"));
        assert_eq!(
            normalize_container("{00000000-0000-0000-0000-000000000000}"),
            None
        );
        assert_eq!(
            normalize_container("{ffffffff-ffff-ffff-ffff-ffffffffffff}"),
            None
        );
        assert_eq!(normalize_container(""), None);
    }

    #[test]
    fn kinds_from_raw() {
        assert_eq!(LampArrayKind::from_raw(1), LampArrayKind::Keyboard);
        assert_eq!(LampArrayKind::from_raw(13), LampArrayKind::Speaker);
        assert_eq!(LampArrayKind::from_raw(0), LampArrayKind::Other(0));
    }
}
