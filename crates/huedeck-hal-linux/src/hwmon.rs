// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only hwmon sensors: temperatures and fan speeds from
//! `/sys/class/hwmon/hwmonN` ([`HwmonSensors`], a
//! [`SensorBackend`](huedeck_hal::sensors::SensorBackend)).
//!
//! Per chip (`name`: `k10temp`, `nct6687`, `amdgpu`, `nvme`, …) every
//! `tempN_input` (millidegrees Celsius) and `fanN_input` (RPM) is read, with
//! `tempN_label` / `fanN_label` for the name and `tempN_crit` / `tempN_max` /
//! `fanN_max` for the scale. Sensor names are `"<chip> <label>"` (`"k10temp
//! Tctl"`, `"nct6687 CPU Fan"`), made unique with ` #2`, ` #3` when two chips
//! of one driver answer. Nothing is ever written: `pwm*` and every other
//! attribute stay untouched (fan control is WP-SENSORS-FANS and opt-in; on
//! the mainline `nct6687` driver a `pwm` write reaches the EC unguarded,
//! docs/research/m7-linux-backend.md). Files that fail to read (a sensor
//! that is off, a chip asleep) are skipped, never an error.
//!
//! Pure file reads through [`Files`], so it builds and is tested on every OS.

use crate::sysfs::{Files, RealFs};
use huedeck_hal::sensors::{SensorBackend, SensorKind, SensorReading};
use huedeck_hal::HalResult;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The sysfs class directory.
pub const HWMON_CLASS: &str = "/sys/class/hwmon";

fn text(fs: &impl Files, path: &Path) -> Option<String> {
    Some(String::from_utf8_lossy(&fs.read(path)?).trim().to_string())
}

fn number(fs: &impl Files, path: &Path) -> Option<i64> {
    text(fs, path)?.parse().ok()
}

/// `temp3_input` → `("temp", 3)` for the kinds read here.
fn input_index(file: &str) -> Option<(&'static str, u32)> {
    let stem = file.strip_suffix("_input")?;
    for prefix in ["temp", "fan"] {
        if let Some(n) = stem.strip_prefix(prefix) {
            return n.parse().ok().map(|n| (prefix, n));
        }
    }
    None
}

/// The readings of one chip directory (`hwmonN`, or its `device` on old
/// drivers that keep the attributes there).
pub fn read_chip(fs: &impl Files, dir: &Path) -> Vec<SensorReading> {
    let dir = if fs.read(&dir.join("name")).is_none() && fs.read(&dir.join("device/name")).is_some() {
        dir.join("device")
    } else {
        dir.to_path_buf()
    };
    let chip = text(fs, &dir.join("name")).unwrap_or_else(|| "hwmon".into());
    let mut inputs: Vec<(&str, u32)> = fs.list(&dir).iter().filter_map(|f| input_index(f)).collect();
    inputs.sort();
    let mut out = Vec::new();
    for (prefix, n) in inputs {
        let attr = |a: &str| dir.join(format!("{prefix}{n}_{a}"));
        let Some(raw) = number(fs, &attr("input")) else {
            continue;
        };
        let label = text(fs, &attr("label"))
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| format!("{prefix}{n}"));
        let reading = if prefix == "temp" {
            let max = number(fs, &attr("crit"))
                .or_else(|| number(fs, &attr("max")))
                .map(|m| m as f64 / 1000.0)
                .filter(|m| *m > 0.0 && *m < 200.0)
                .unwrap_or(100.0);
            SensorReading {
                name: format!("{chip} {label}"),
                value: raw as f64 / 1000.0,
                units: "°C".into(),
                min: 0.0,
                max,
                kind: SensorKind::Temperature,
            }
        } else {
            let max = number(fs, &attr("max"))
                .filter(|m| *m > 0)
                .map(|m| m as f64)
                .unwrap_or(5000.0);
            SensorReading {
                name: format!("{chip} {label}"),
                value: raw as f64,
                units: "RPM".into(),
                min: 0.0,
                max,
                kind: SensorKind::Fan,
            }
        };
        out.push(reading);
    }
    out
}

/// Every reading under `class_dir`, chips in `hwmonN` order, names unique.
pub fn read_all(fs: &impl Files, class_dir: &Path) -> Vec<SensorReading> {
    let mut chips: Vec<(u32, PathBuf)> = fs
        .list(class_dir)
        .into_iter()
        .filter_map(|e| {
            let n = e.strip_prefix("hwmon")?.parse().ok()?;
            Some((n, class_dir.join(&e)))
        })
        .collect();
    chips.sort();
    let mut seen: BTreeMap<String, u32> = BTreeMap::new();
    let mut out = Vec::new();
    for (_, link) in chips {
        let dir = fs.resolve(&link).unwrap_or(link);
        for mut r in read_chip(fs, &dir) {
            let count = seen.entry(r.name.clone()).or_insert(0);
            *count += 1;
            if *count > 1 {
                r.name = format!("{} #{count}", r.name);
            }
            out.push(r);
        }
    }
    out
}

/// The hwmon sensors of this machine.
#[derive(Clone, Debug)]
pub struct HwmonSensors<F = RealFs> {
    fs: F,
    class_dir: PathBuf,
}

impl HwmonSensors<RealFs> {
    /// `/sys/class/hwmon` on the real filesystem.
    pub fn new() -> Self {
        Self {
            fs: RealFs,
            class_dir: PathBuf::from(HWMON_CLASS),
        }
    }
}

impl Default for HwmonSensors<RealFs> {
    fn default() -> Self {
        Self::new()
    }
}

impl<F: Files> HwmonSensors<F> {
    /// Over `fs` at `class_dir` (tests).
    pub fn with_fs(fs: F, class_dir: impl Into<PathBuf>) -> Self {
        Self {
            fs,
            class_dir: class_dir.into(),
        }
    }
}

impl<F: Files + Send> SensorBackend for HwmonSensors<F> {
    fn name(&self) -> &str {
        "hwmon"
    }

    fn poll(&mut self) -> HalResult<Vec<SensorReading>> {
        Ok(read_all(&self.fs, &self.class_dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sysfs::MemFs;

    fn chip(fs: &mut MemFs, n: u32, name: &str, attrs: &[(&str, &str)]) {
        let dir = PathBuf::from(format!("/sys/devices/platform/chip{n}/hwmon/hwmon{n}"));
        fs.links
            .insert(PathBuf::from(format!("{HWMON_CLASS}/hwmon{n}")), dir.clone());
        fs.file(dir.join("name"), format!("{name}\n"));
        for (a, v) in attrs {
            fs.file(dir.join(a), format!("{v}\n"));
        }
    }

    #[test]
    fn temperatures_and_fans() {
        let mut fs = MemFs::default();
        chip(
            &mut fs,
            0,
            "k10temp",
            &[
                ("temp1_input", "45250"),
                ("temp1_label", "Tctl"),
                ("temp3_input", "38000"),
                ("temp3_label", "Tccd1"),
            ],
        );
        chip(
            &mut fs,
            1,
            "nct6687",
            &[
                ("fan1_input", "1180"),
                ("fan1_label", "CPU Fan"),
                ("fan2_input", "0"),
                ("pwm1", "128"),
                ("temp1_input", "41000"),
                ("temp1_crit", "105000"),
            ],
        );
        let mut s = HwmonSensors::with_fs(fs, HWMON_CLASS);
        assert_eq!(s.name(), "hwmon");
        let r = s.poll().unwrap();
        let names: Vec<&str> = r.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "k10temp Tctl",
                "k10temp Tccd1",
                "nct6687 CPU Fan",
                "nct6687 fan2",
                "nct6687 temp1"
            ]
        );
        assert_eq!(r[0].value, 45.25);
        assert_eq!(
            (r[0].kind, r[0].units.as_str(), r[0].max),
            (SensorKind::Temperature, "°C", 100.0)
        );
        assert_eq!(
            (r[2].value, r[2].kind, r[2].units.as_str()),
            (1180.0, SensorKind::Fan, "RPM")
        );
        assert_eq!(r[4].max, 105.0);
    }

    #[test]
    fn duplicate_chips_get_numbered_and_unreadable_inputs_are_skipped() {
        let mut fs = MemFs::default();
        chip(
            &mut fs,
            2,
            "nvme",
            &[("temp1_input", "33850"), ("temp1_label", "Composite")],
        );
        chip(
            &mut fs,
            3,
            "nvme",
            &[("temp1_input", "36850"), ("temp1_label", "Composite")],
        );
        chip(&mut fs, 4, "acpitz", &[("temp1_input", "")]);
        let r = read_all(&fs, Path::new(HWMON_CLASS));
        let names: Vec<&str> = r.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["nvme Composite", "nvme Composite #2"]);
    }

    #[test]
    fn input_names() {
        assert_eq!(input_index("temp12_input"), Some(("temp", 12)));
        assert_eq!(input_index("fan1_input"), Some(("fan", 1)));
        assert_eq!(input_index("in0_input"), None);
        assert_eq!(input_index("temp1_label"), None);
        assert_eq!(input_index("pwm1"), None);
    }
}
