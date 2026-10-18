// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `huedeck devices`: enumerate HID devices read-only and print the
//! SignalRGB-shaped endpoint tuples grouped by physical device.

use huedeck_hal_windows::DeviceGroup;
use serde_json::{json, Value};
use std::fmt::Write as _;

/// `--filter vid[:pid]` (hex, `0x` optional).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Filter {
    vid: u16,
    pid: Option<u16>,
}

fn hex16(s: &str) -> Result<u16, String> {
    let t = s.trim();
    let t = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")).unwrap_or(t);
    u16::from_str_radix(t, 16).map_err(|e| format!("bad hex id {s:?}: {e}"))
}

impl std::str::FromStr for Filter {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.splitn(2, ':');
        let vid = hex16(parts.next().unwrap_or(""))?;
        let pid = parts.next().map(hex16).transpose()?;
        Ok(Self { vid, pid })
    }
}

impl Filter {
    fn accepts(&self, g: &DeviceGroup) -> bool {
        g.key.vid == self.vid && self.pid.is_none_or(|p| p == g.key.pid)
    }
}

/// Keep only groups matching any filter (all groups when `filters` is empty).
pub fn apply_filters(groups: Vec<DeviceGroup>, filters: &[Filter]) -> Vec<DeviceGroup> {
    if filters.is_empty() {
        return groups;
    }
    groups
        .into_iter()
        .filter(|g| filters.iter().any(|f| f.accepts(g)))
        .collect()
}

fn len_or_q(
    r: &Result<huedeck_hal::hid::ReportLengths, String>,
    pick: fn(&huedeck_hal::hid::ReportLengths) -> usize,
) -> String {
    match r {
        Ok(l) => pick(l).to_string(),
        Err(_) => "?".into(),
    }
}

/// Human-readable table.
pub fn render_table(groups: &[DeviceGroup]) -> String {
    let mut out = String::new();
    for g in groups {
        let _ = writeln!(
            out,
            "{}  {} / {}  [{}{}]",
            g.key,
            if g.manufacturer.is_empty() {
                "?"
            } else {
                g.manufacturer.trim()
            },
            if g.product.is_empty() {
                "?"
            } else {
                g.product.trim()
            },
            g.source.as_str(),
            g.usb_parent
                .as_deref()
                .map(|p| format!(": {p}"))
                .unwrap_or_default(),
        );
        let _ = writeln!(
            out,
            "  {:>5} {:>10} {:>6} {:>12} {:>5} {:>5} {:>5}  path",
            "iface", "usage_page", "usage", "collection", "in", "out", "feat"
        );
        for e in &g.endpoints {
            let i = &e.info;
            // Windows writes ColNN as two hex digits (`Col0A` = 10th TLC).
            let raw = e
                .collection_raw
                .map(|r| format!("Col{r:02X}"))
                .unwrap_or_else(|| "-".into());
            let _ = writeln!(
                out,
                "  {:>5} {:>10} {:>6} {:>12} {:>5} {:>5} {:>5}  {}",
                i.interface_number,
                format!("0x{:04x}", i.usage_page),
                format!("0x{:04x}", i.usage),
                format!("{} ({raw})", i.collection),
                len_or_q(&e.caps, |l| l.input),
                len_or_q(&e.caps, |l| l.output),
                len_or_q(&e.caps, |l| l.feature),
                i.path,
            );
            if let Err(err) = &e.caps {
                let _ = writeln!(out, "        ! report lengths unavailable: {err}");
            }
        }
        let _ = writeln!(out);
    }
    let _ = write!(
        out,
        "{} device(s), {} endpoint(s)",
        groups.len(),
        groups.iter().map(|g| g.endpoints.len()).sum::<usize>()
    );
    out
}

/// Machine-readable form (`--json`).
pub fn to_json(groups: &[DeviceGroup]) -> Value {
    Value::Array(
        groups
            .iter()
            .map(|g| {
                json!({
                    "key": g.key.to_string(),
                    "vid": g.key.vid,
                    "pid": g.key.pid,
                    "serial": g.key.serial,
                    "grouping": g.source.as_str(),
                    "usb_parent": g.usb_parent,
                    "manufacturer": g.manufacturer,
                    "product": g.product,
                    "endpoints": g.endpoints.iter().map(|e| {
                        let i = &e.info;
                        json!({
                            "interface": i.interface_number,
                            "usage": i.usage,
                            "usage_page": i.usage_page,
                            "collection": i.collection,
                            "collection_raw": e.collection_raw,
                            "path": i.path,
                            "serial": i.serial,
                            "report_lengths": match &e.caps {
                                Ok(l) => json!({"input": l.input, "output": l.output, "feature": l.feature}),
                                Err(_) => Value::Null,
                            },
                            "report_lengths_error": e.caps.as_ref().err(),
                            "usb_parent_error": e.usb_parent.as_ref().err(),
                        })
                    }).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

/// Enumerate on this OS.
#[cfg(windows)]
pub fn enumerate() -> anyhow::Result<Vec<DeviceGroup>> {
    let backend = huedeck_hal_windows::WindowsHidBackend::new()?;
    Ok(backend.enumerate_grouped()?)
}

/// Enumerate on this OS.
#[cfg(not(windows))]
pub fn enumerate() -> anyhow::Result<Vec<DeviceGroup>> {
    anyhow::bail!("`huedeck devices` is only implemented on Windows so far (Linux backend: later milestone)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use huedeck_hal::hid::{HidEndpointInfo, ReportLengths};
    use huedeck_hal_windows::{group_endpoints, ProbedEndpoint};

    fn sample() -> Vec<DeviceGroup> {
        let mut a = ProbedEndpoint::from_info(HidEndpointInfo {
            vid: 0x1462,
            pid: 0x7c91,
            serial: "A00000000001".into(),
            manufacturer: "MSI".into(),
            product: "MYSTIC LIGHT ".into(),
            path: r"\\?\hid#vid_1462&pid_7c91&col01#7&1&0&0000#{g}".into(),
            interface_number: 0,
            usage_page: 0x0001,
            usage: 0x0000,
            collection: 1,
            report_lengths: ReportLengths::default(),
            feature_reports: Vec::new(),
        });
        a.usb_parent = Ok(r"USB\VID_1462&PID_7C91\A00000000001".into());
        a.caps = Ok(ReportLengths {
            input: 64,
            output: 64,
            feature: 185,
        });
        let mut b = ProbedEndpoint::from_info(HidEndpointInfo {
            vid: 0x1532,
            pid: 0x007c,
            path: r"\\?\hid#vid_1532&pid_007c&mi_00#8&2&0&0000#{g}".into(),
            interface_number: 0,
            usage_page: 1,
            usage: 2,
            ..Default::default()
        });
        b.caps = Err("access denied".into());
        group_endpoints(vec![a, b])
    }

    #[test]
    fn filter_parsing() {
        assert_eq!(
            "1462:7c91".parse::<Filter>().unwrap(),
            Filter {
                vid: 0x1462,
                pid: Some(0x7c91)
            }
        );
        assert_eq!(
            "0x1532".parse::<Filter>().unwrap(),
            Filter {
                vid: 0x1532,
                pid: None
            }
        );
        assert!("zz".parse::<Filter>().is_err());
        let groups = apply_filters(sample(), &["1532".parse().unwrap()]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].key.vid, 0x1532);
    }

    #[test]
    fn table_shows_tuples_raw_collection_and_question_marks() {
        let t = render_table(&sample());
        assert!(t.contains("1462:7c91:A00000000001  MSI / MYSTIC LIGHT  [usb-parent: USB\\VID_1462&PID_7C91\\A00000000001]"), "{t}");
        assert!(t.contains("1 (Col01)"), "{t}");
        assert!(t.contains("185"), "{t}");
        assert!(t.contains("report lengths unavailable: access denied"), "{t}");
        assert!(t.ends_with("2 device(s), 2 endpoint(s)"), "{t}");
    }

    #[test]
    fn hex_collection_is_rendered_as_windows_writes_it() {
        let e = ProbedEndpoint::from_info(HidEndpointInfo {
            vid: 0x046d,
            pid: 0xc539,
            serial: "S".into(),
            path: r"\\?\hid#vid_046d&pid_c539&mi_02&col0a#9&1&0&0009#{g}".into(),
            interface_number: 2,
            collection: 10,
            ..Default::default()
        });
        assert_eq!(e.collection_raw, Some(10));
        let t = render_table(&group_endpoints(vec![e]));
        assert!(t.contains("10 (Col0A)"), "{t}");
    }

    #[test]
    fn json_shape() {
        let v = to_json(&sample());
        let msi = &v[0];
        assert_eq!(msi["key"], "1462:7c91:A00000000001");
        assert_eq!(msi["endpoints"][0]["interface"], 0);
        assert_eq!(msi["endpoints"][0]["collection"], 1);
        assert_eq!(msi["endpoints"][0]["collection_raw"], 1);
        assert_eq!(msi["endpoints"][0]["report_lengths"]["feature"], 185);
        assert!(v[1]["endpoints"][0]["report_lengths"].is_null());
    }
}
