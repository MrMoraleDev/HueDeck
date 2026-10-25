// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only Windows probing of HID endpoints.
//!
//! * [`usb_parent`]: walks `CM_Get_Parent` from the HID collection's device
//!   node up to the first `USB\VID_xxxx&PID_xxxx\<instance>` node.
//! * [`present_hid`]: VID/PID of every present HID interface, from the
//!   interface paths alone (no device is opened), so callers can enumerate
//!   only the devices they have plugins for.
//! * [`read_caps`]: opens the collection with **zero access rights**
//!   (`dwDesiredAccess = 0`, the same metadata-only open hidapi uses while
//!   enumerating), reads `HIDP_CAPS` via `HidD_GetPreparsedData`/`HidP_GetCaps`
//!   and closes the handle immediately. A zero-access handle cannot read or
//!   write reports, so this is safe while another process (SignalRGB, the
//!   system keyboard/mouse class driver) holds the device.
//!
//! `HIDP_CAPS` byte lengths are what `hid_get_report_descriptor` +
//! `lightloom-hid-desc` would compute on Windows (hidapi's windows-native
//! backend reconstructs the descriptor *from the same preparsed data*), but
//! hidapi only exposes that after `open_path`, which requests
//! `GENERIC_READ | GENERIC_WRITE` first. We avoid that open on purpose.

use crate::path::{interface_path_to_instance_id, is_usb_device_instance, parse_hid_path};
use lightloom_hal::hid::ReportLengths;
use std::collections::BTreeSet;
use windows::core::PCWSTR;
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_DevNode_PropertyW, CM_Get_Device_IDW, CM_Get_Device_Interface_ListW,
    CM_Get_Device_Interface_List_SizeW, CM_Get_Device_Interface_PropertyW, CM_Get_Parent, CM_Locate_DevNodeW,
    CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CM_LOCATE_DEVNODE_NORMAL, CR_BUFFER_SMALL, CR_SUCCESS,
    MAX_DEVICE_ID_LEN,
};
use windows::Win32::Devices::HumanInterfaceDevice::{
    HidD_FreePreparsedData, HidD_GetHidGuid, HidD_GetPreparsedData, HidP_GetCaps, HIDP_CAPS,
    HIDP_STATUS_SUCCESS, PHIDP_PREPARSED_DATA,
};
use windows::Win32::Devices::Properties::{
    DEVPKEY_Device_CompatibleIds, DEVPKEY_Device_ContainerId, DEVPKEY_Device_InstanceId,
    DEVPKEY_Device_Manufacturer, DEVPROPTYPE, DEVPROP_TYPE_GUID, DEVPROP_TYPE_STRING,
    DEVPROP_TYPE_STRING_LIST,
};
use windows::Win32::Foundation::DEVPROPKEY;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};

/// Maximum parent hops (HID collection → USB interface → USB device is 2).
const MAX_HOPS: usize = 8;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn device_id(devinst: u32) -> Result<String, String> {
    let mut buf = vec![0u16; MAX_DEVICE_ID_LEN as usize + 1];
    // SAFETY: `buf` is a valid, writable UTF-16 buffer; cfgmgr32 writes at most `buf.len()` units.
    let cr = unsafe { CM_Get_Device_IDW(devinst, &mut buf, 0) };
    if cr != CR_SUCCESS {
        return Err(format!("CM_Get_Device_IDW failed (CONFIGRET {})", cr.0));
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Ok(String::from_utf16_lossy(&buf[..len]))
}

/// Device instance id behind a device-interface path: the interface's
/// `DEVPKEY_Device_InstanceId`, as hidapi reads it. Parsing the path is only the
/// fallback; it fails for paths that do not follow the `HID#VID_…` pattern
/// (e.g. VMBus devices: `HID#{guid}#…`).
fn interface_instance_id(path: &str) -> Option<String> {
    let wpath = wide(path);
    let mut kind = DEVPROPTYPE(0);
    let mut buf = [0u16; 512];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: NUL-terminated interface path; the buffer pointer and its size in bytes
    // describe `buf`; out pointers are valid.
    let cr = unsafe {
        CM_Get_Device_Interface_PropertyW(
            PCWSTR(wpath.as_ptr()),
            &DEVPKEY_Device_InstanceId,
            &mut kind,
            Some(buf.as_mut_ptr().cast()),
            &mut size,
            0,
        )
    };
    if cr == CR_SUCCESS && kind == DEVPROP_TYPE_STRING {
        let units = ((size as usize) / 2).min(buf.len());
        let id = String::from_utf16_lossy(&buf[..units]);
        let id = id.trim_end_matches('\0');
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }
    interface_path_to_instance_id(path)
}

/// The container id of the physical device behind a device-interface path
/// (`DEVPKEY_Device_ContainerId` of its devnode, normalised by
/// [`lightloom_hal::lamparray::normalize_container`]): every interface and
/// collection of one physical device shares it, across product ids and
/// transports. `None` when cfgmgr32 does not know it.
pub fn container_id(path: &str) -> Option<String> {
    let id = interface_instance_id(path)?;
    let wid = wide(&id);
    let mut inst = 0u32;
    // SAFETY: `wid` is NUL-terminated and outlives the call; `inst` is a valid out pointer.
    let cr = unsafe { CM_Locate_DevNodeW(&mut inst, PCWSTR(wid.as_ptr()), CM_LOCATE_DEVNODE_NORMAL) };
    if cr != CR_SUCCESS {
        return None;
    }
    let mut guid = windows::core::GUID::zeroed();
    let mut kind = DEVPROPTYPE(0);
    let mut size = std::mem::size_of::<windows::core::GUID>() as u32;
    // SAFETY: the buffer pointer and its size in bytes describe `guid`; out pointers are valid.
    let cr = unsafe {
        CM_Get_DevNode_PropertyW(
            inst,
            &DEVPKEY_Device_ContainerId,
            &mut kind,
            Some((&mut guid as *mut windows::core::GUID).cast()),
            &mut size,
            0,
        )
    };
    if cr != CR_SUCCESS || kind != DEVPROP_TYPE_GUID {
        return None;
    }
    lightloom_hal::lamparray::normalize_container(&format!("{guid:?}"))
}

/// USB parent device instance id of a HID collection path.
pub fn usb_parent(path: &str) -> Result<String, String> {
    let id = interface_instance_id(path).ok_or_else(|| "unparseable HID path".to_string())?;
    let wid = wide(&id);
    let mut inst = 0u32;
    // SAFETY: `wid` is NUL-terminated and outlives the call; `inst` is a valid out pointer.
    let cr = unsafe { CM_Locate_DevNodeW(&mut inst, PCWSTR(wid.as_ptr()), CM_LOCATE_DEVNODE_NORMAL) };
    if cr != CR_SUCCESS {
        return Err(format!("CM_Locate_DevNodeW({id}) failed (CONFIGRET {})", cr.0));
    }
    let mut chain = Vec::new();
    for _ in 0..MAX_HOPS {
        let mut parent = 0u32;
        // SAFETY: plain out-pointer call on a devnode handle cfgmgr32 gave us.
        let cr = unsafe { CM_Get_Parent(&mut parent, inst, 0) };
        if cr != CR_SUCCESS {
            break;
        }
        let pid = device_id(parent)?;
        if is_usb_device_instance(&pid) {
            return Ok(pid);
        }
        chain.push(pid);
        inst = parent;
    }
    Err(format!("no USB parent (chain: {})", chain.join(" <- ")))
}

/// `(vid, pid)` pairs of every present HID interface, read from the interface
/// paths alone (`CM_Get_Device_Interface_List`; nothing is opened).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PresentHid {
    /// Pairs found in the paths.
    pub keys: BTreeSet<(u16, u16)>,
    /// Paths whose VID/PID could not be parsed (non-USB transports with
    /// unusual path formats); callers fall back to a full enumeration.
    pub unparsed: usize,
}

/// VID/PID of a HID interface path: `VID_xxxx&PID_xxxx` (USB) or
/// `_VID&0002xxxx_PID&xxxx` (Bluetooth: vendor-id source + id).
pub fn path_vid_pid(path: &str) -> Option<(u16, u16)> {
    let p = parse_hid_path(path);
    if let (Some(v), Some(d)) = (p.vid, p.pid) {
        return Some((v, d));
    }
    let lower = path.to_ascii_lowercase();
    let hex = |marker: &str, digits: usize| {
        let start = lower.find(marker)? + marker.len();
        let s = lower.get(start..start + digits)?;
        s.chars().all(|c| c.is_ascii_hexdigit()).then_some(s)
    };
    let vid = hex("vid&", 8).and_then(|h| u16::from_str_radix(&h[4..], 16).ok())?;
    let pid = hex("pid&", 4).and_then(|h| u16::from_str_radix(h, 16).ok())?;
    Some((vid, pid))
}

/// Device-interface paths of every present HID collection (`CM_Get_Device_Interface_List`;
/// nothing is opened).
pub fn hid_interface_paths() -> Result<Vec<String>, String> {
    // SAFETY: plain out-value call.
    let guid = unsafe { HidD_GetHidGuid() };
    for _ in 0..4 {
        let mut len = 0u32;
        // SAFETY: valid out pointer and GUID; no device-id filter.
        let cr = unsafe {
            CM_Get_Device_Interface_List_SizeW(
                &mut len,
                &guid,
                PCWSTR::null(),
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        if cr != CR_SUCCESS {
            return Err(format!(
                "CM_Get_Device_Interface_List_SizeW failed (CONFIGRET {})",
                cr.0
            ));
        }
        let mut buf = vec![0u16; len as usize];
        // SAFETY: `buf` holds `len` UTF-16 units as the size call requested.
        let cr = unsafe {
            CM_Get_Device_Interface_ListW(
                &guid,
                PCWSTR::null(),
                &mut buf,
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        if cr == CR_BUFFER_SMALL {
            continue; // an interface arrived between the two calls
        }
        if cr != CR_SUCCESS {
            return Err(format!(
                "CM_Get_Device_Interface_ListW failed (CONFIGRET {})",
                cr.0
            ));
        }
        return Ok(buf
            .split(|&c| c == 0)
            .filter(|s| !s.is_empty())
            .map(String::from_utf16_lossy)
            .collect());
    }
    Err("the HID interface list kept changing".into())
}

/// List the present HID interfaces and their VID/PID without opening any device.
pub fn present_hid() -> Result<PresentHid, String> {
    let mut present = PresentHid::default();
    for path in hid_interface_paths()? {
        match path_vid_pid(&path) {
            Some(key) => {
                present.keys.insert(key);
            }
            None => present.unparsed += 1,
        }
    }
    Ok(present)
}

/// Transport of a HID collection, decided as hidapi does: from the compatible
/// ids of the device node directly above the HID collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HidBus {
    /// `USB\...` compatible ids.
    Usb,
    /// `BTHENUM\...` (Bluetooth classic).
    Bluetooth,
    /// `BTHLEDEVICE\...` (Bluetooth LE).
    BluetoothLe,
    /// `PNP0C50` (HID over I2C).
    I2c,
    /// `PNP0C51` (HID over SPI).
    Spi,
    /// Anything else (virtual devices, VMBus, unknown).
    Unknown,
}

/// The device node directly above the HID collection at `path`.
fn parent_devnode(path: &str) -> Option<u32> {
    let id = interface_instance_id(path)?;
    let wid = wide(&id);
    let mut inst = 0u32;
    // SAFETY: NUL-terminated id that outlives the call; valid out pointer.
    if unsafe { CM_Locate_DevNodeW(&mut inst, PCWSTR(wid.as_ptr()), CM_LOCATE_DEVNODE_NORMAL) } != CR_SUCCESS
    {
        return None;
    }
    let mut parent = 0u32;
    // SAFETY: plain out-pointer call on a devnode handle cfgmgr32 gave us.
    (unsafe { CM_Get_Parent(&mut parent, inst, 0) } == CR_SUCCESS).then_some(parent)
}

/// A string or string-list property of a devnode, as UTF-16 strings.
fn devnode_strings(inst: u32, key: &DEVPROPKEY) -> Vec<String> {
    let mut buf = vec![0u16; 512];
    for _ in 0..2 {
        let mut kind = DEVPROPTYPE(0);
        let mut size = (buf.len() * 2) as u32;
        // SAFETY: the buffer pointer and its size in bytes describe `buf`; out pointers are valid.
        let cr = unsafe {
            CM_Get_DevNode_PropertyW(inst, key, &mut kind, Some(buf.as_mut_ptr().cast()), &mut size, 0)
        };
        if cr == CR_BUFFER_SMALL {
            buf = vec![0u16; (size as usize).div_ceil(2) + 1];
            continue;
        }
        if cr != CR_SUCCESS || (kind != DEVPROP_TYPE_STRING && kind != DEVPROP_TYPE_STRING_LIST) {
            return Vec::new();
        }
        let units = ((size as usize) / 2).min(buf.len());
        return buf[..units]
            .split(|&c| c == 0)
            .filter(|s| !s.is_empty())
            .map(String::from_utf16_lossy)
            .collect();
    }
    Vec::new()
}

/// Transport of the HID collection at `path` (see [`HidBus`]).
///
/// Decided as the hidapi C library does (SignalRGB ships a hidapi C build): the
/// first compatible id of the parent devnode that *contains* a known token.
/// hidapi-rs 2.6.7 (`windows-native`) differs in two ways: it matches prefixes
/// only (missing `ACPI\PNP0C50`), and it also tests the empty string that ends
/// the id list, which "starts with" anything, so every unknown transport
/// (Hyper-V VMBus input, ACPI I2C) comes out as USB with interface 0.
pub fn hid_bus(path: &str) -> HidBus {
    let Some(parent) = parent_devnode(path) else {
        return HidBus::Unknown;
    };
    bus_from_compatible_ids(&devnode_strings(parent, &DEVPKEY_Device_CompatibleIds))
}

fn bus_from_compatible_ids(ids: &[String]) -> HidBus {
    const TOKENS: [(&str, HidBus); 5] = [
        ("USB", HidBus::Usb),
        ("BTHENUM", HidBus::Bluetooth),
        ("BTHLEDEVICE", HidBus::BluetoothLe),
        ("PNP0C50", HidBus::I2c),
        ("PNP0C51", HidBus::Spi),
    ];
    ids.iter()
        .filter(|id| !id.is_empty())
        .find_map(|id| {
            let id = id.to_ascii_uppercase();
            TOKENS
                .iter()
                .find_map(|(token, bus)| id.contains(token).then_some(*bus))
        })
        .unwrap_or(HidBus::Unknown)
}

/// `DEVPKEY_Device_Manufacturer` of the device node directly above a HID
/// collection (the USB interface, whose INF names the vendor): the fallback
/// hidapi uses for USB devices when `HidD_GetManufacturerString` returns nothing.
pub fn parent_manufacturer(path: &str) -> Option<String> {
    let parent = parent_devnode(path)?;
    devnode_strings(parent, &DEVPKEY_Device_Manufacturer)
        .into_iter()
        .next()
}

/// Usage page / usage / report lengths from `HIDP_CAPS`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caps {
    /// Top-level collection usage page.
    pub usage_page: u16,
    /// Top-level collection usage.
    pub usage: u16,
    /// Report byte lengths (including the report-id byte).
    pub lengths: ReportLengths,
}

struct OwnedHandle(HANDLE);
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: handle came from a successful CreateFileW and is closed exactly once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Read `HIDP_CAPS` through a zero-access handle. Never sends or receives reports.
pub fn read_caps(path: &str) -> Result<Caps, String> {
    let wpath = wide(path);
    // SAFETY: NUL-terminated path; access 0 = query-only handle; no template/security attrs.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wpath.as_ptr()),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .map_err(|e| format!("CreateFileW(access=0): {}", e.message()))?;
    let handle = OwnedHandle(handle);

    let mut pp = PHIDP_PREPARSED_DATA(0);
    // SAFETY: valid handle and out pointer.
    if !unsafe { HidD_GetPreparsedData(handle.0, &mut pp) } {
        return Err(format!(
            "HidD_GetPreparsedData: {}",
            windows::core::Error::from_thread().message()
        ));
    }
    let mut caps = HIDP_CAPS::default();
    // SAFETY: `pp` was returned by HidD_GetPreparsedData and is freed right after.
    let status = unsafe { HidP_GetCaps(pp, &mut caps) };
    // SAFETY: frees the preparsed data allocated above, exactly once.
    unsafe {
        let _ = HidD_FreePreparsedData(pp);
    }
    drop(handle);
    if status != HIDP_STATUS_SUCCESS {
        return Err(format!("HidP_GetCaps failed (NTSTATUS {:#x})", status.0));
    }
    Ok(Caps {
        usage_page: caps.UsagePage,
        usage: caps.Usage,
        lengths: ReportLengths {
            input: caps.InputReportByteLength as usize,
            output: caps.OutputReportByteLength as usize,
            feature: caps.FeatureReportByteLength as usize,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_from_compatible_ids_follows_hidapi_c() {
        let bus =
            |ids: &[&str]| bus_from_compatible_ids(&ids.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(
            bus(&["USB\\Class_03&SubClass_00&Prot_00", "USB\\Class_03"]),
            HidBus::Usb
        );
        assert_eq!(
            bus(&["BTHENUM\\{00001124-0000-1000-8000-00805f9b34fb}"]),
            HidBus::Bluetooth
        );
        assert_eq!(bus(&["ACPI\\PNP0C50", "*PNP0C50"]), HidBus::I2c);
        // Hyper-V synthetic mouse: no known token, and the list terminator is not a match.
        assert_eq!(
            bus(&["VMBUS\\{cfa8b69e-5b4a-4cc0-b98b-8ba1a1f3f95a}", ""]),
            HidBus::Unknown
        );
        assert_eq!(bus(&[]), HidBus::Unknown);
    }

    #[test]
    fn vid_pid_from_usb_and_bluetooth_paths() {
        assert_eq!(
            path_vid_pid(
                r"\\?\HID#VID_1462&PID_7C91#7&1402d2e5&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}"
            ),
            Some((0x1462, 0x7C91))
        );
        assert_eq!(
            path_vid_pid(
                r"\\?\HID#{00001124-0000-1000-8000-00805f9b34fb}_VID&0002054c_PID&0ce6#9&1&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}"
            ),
            Some((0x054C, 0x0CE6))
        );
        assert_eq!(
            path_vid_pid(r"\\?\HID#ConvertedDevice&Col01#5&1&0&0000#{g}"),
            None
        );
    }

    #[test]
    fn present_interfaces_are_listed_without_opening_them() {
        let present = present_hid().expect("interface list");
        // Every machine running the tests has at least one HID interface (keyboard/mouse);
        // CI VMs may have none, so only the call itself is checked there.
        let _ = present.keys.len() + present.unparsed;
    }
}
