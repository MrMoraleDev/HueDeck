// SPDX-License-Identifier: GPL-3.0-or-later
//! The hardware sensor sources of this machine and the registry that merges
//! them (`engine.getSensorValue(name)`, `GET /v1/sensors`; docs/sensors.md).
//!
//! Every source is read-only:
//!
//! * [`system`]: `CPU Load`, `RAM Load`, `RAM Used` (every OS);
//! * NvAPI (Windows): `GPU Temperature`, `GPU Load`, `GPU Fan`, clocks
//!   ([`lightloom_nvapi::sensors`]) on the process-wide NvAPI worker;
//! * hwmon (Linux): temperatures and fans (`lightloom_hal_linux::hwmon`);
//! * [`lhm`]: LibreHardwareMonitor's JSON, when the user runs it with its web
//!   server and points `[sensors] lhm_url` at it (CPU temperature on Windows).
//!
//! [`SensorRegistry`] polls them, makes names unique and adds the alias
//! `CPU Temperature` (from LibreHardwareMonitor or hwmon). Polling is the
//! caller's job (the daemon polls at most once a second, only while an
//! effect or the dashboard reads sensors). Fan control is not here: Lightloom
//! never writes PWM, SuperIO or EC registers (a future, opt-in feature).

pub mod lhm;
pub mod system;

use lightloom_hal::sensors::{SensorBackend, SensorKind, SensorReading};
use std::collections::BTreeMap;

/// Which sources to open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SensorOptions {
    /// NVIDIA GPUs through NvAPI (Windows).
    pub nvapi: bool,
    /// LibreHardwareMonitor's `data.json` (loopback URLs only).
    pub lhm_url: Option<String>,
}

/// Every sensor backend available here.
pub fn backends(opts: &SensorOptions) -> Vec<Box<dyn SensorBackend>> {
    let mut out: Vec<Box<dyn SensorBackend>> = vec![Box::new(system::SystemLoad::new())];
    #[cfg(target_os = "linux")]
    out.push(Box::new(lightloom_hal_linux::hwmon::HwmonSensors::new()));
    if opts.nvapi && cfg!(windows) {
        match lightloom_nvapi::NvapiHub::shared() {
            Ok(hub) if !hub.gpus().is_empty() => out.push(Box::new(hub.sensors())),
            Ok(_) => tracing::debug!("sensors: NvAPI found no NVIDIA GPU"),
            Err(e) => tracing::debug!("sensors: no NvAPI ({e})"),
        }
    }
    if let Some(url) = opts.lhm_url.as_deref().filter(|u| !u.trim().is_empty()) {
        match lhm::LhmJson::new(url) {
            Ok(b) => out.push(Box::new(b)),
            Err(e) => tracing::warn!("sensors: [sensors] lhm_url {url:?}: {e}"),
        }
    }
    out
}

/// How one source did at the last poll.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceStatus {
    /// Backend name (`system`, `nvapi`, `hwmon`, `lhm`).
    pub name: String,
    /// Sensors it reported.
    pub sensors: usize,
    /// Why it failed, when it did.
    pub error: Option<String>,
}

/// The name effects use for the CPU temperature, whatever reports it.
pub const CPU_TEMPERATURE: &str = "CPU Temperature";

/// Labels that name the CPU package / die temperature, best first
/// (LibreHardwareMonitor: `"<CPU> Core (Tctl/Tdie)"`, `"<CPU> CPU Package"`;
/// hwmon: `"k10temp Tctl"`, `"coretemp Package id 0"`, `"zenpower Tdie"`).
const CPU_TEMPERATURE_LABELS: &[&str] = &[
    "tctl/tdie",
    "cpu package",
    "package id 0",
    "k10temp tctl",
    "zenpower tdie",
    "tdie",
    "tctl",
    "coretemp",
];

/// Polls a set of backends and merges their readings.
pub struct SensorRegistry {
    backends: Vec<Box<dyn SensorBackend>>,
    sources: Vec<SourceStatus>,
    /// Last error logged per backend (each new one is logged once).
    logged: BTreeMap<String, String>,
}

impl std::fmt::Debug for SensorRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SensorRegistry")
            .field(
                "backends",
                &self.backends.iter().map(|b| b.name()).collect::<Vec<_>>(),
            )
            .field("sources", &self.sources)
            .finish()
    }
}

impl SensorRegistry {
    /// Over `backends` (tests: fakes).
    pub fn new(backends: Vec<Box<dyn SensorBackend>>) -> Self {
        Self {
            backends,
            sources: Vec::new(),
            logged: BTreeMap::new(),
        }
    }

    /// Over the backends of this machine.
    pub fn open(opts: &SensorOptions) -> Self {
        Self::new(backends(opts))
    }

    /// How every source did at the last [`SensorRegistry::poll`].
    pub fn sources(&self) -> &[SourceStatus] {
        &self.sources
    }

    /// Poll every backend: readings in source order, names unique (` #2`, …),
    /// non-finite values dropped, plus [`CPU_TEMPERATURE`] when a source
    /// reports the CPU's temperature under another name.
    pub fn poll(&mut self) -> Vec<SensorReading> {
        let mut out: Vec<SensorReading> = Vec::new();
        let mut seen: BTreeMap<String, u32> = BTreeMap::new();
        self.sources.clear();
        for b in &mut self.backends {
            let name = b.name().to_string();
            match b.poll() {
                Ok(readings) => {
                    self.logged.remove(&name);
                    let readings: Vec<SensorReading> =
                        readings.into_iter().filter(|r| r.value.is_finite()).collect();
                    self.sources.push(SourceStatus {
                        name,
                        sensors: readings.len(),
                        error: None,
                    });
                    for mut r in readings {
                        let n = seen.entry(r.name.clone()).or_insert(0);
                        *n += 1;
                        if *n > 1 {
                            r.name = format!("{} #{n}", r.name);
                        }
                        out.push(r);
                    }
                }
                Err(e) => {
                    let e = e.to_string();
                    if self.logged.get(&name) != Some(&e) {
                        tracing::info!("sensors: {name}: {e}");
                        self.logged.insert(name.clone(), e.clone());
                    }
                    self.sources.push(SourceStatus {
                        name,
                        sensors: 0,
                        error: Some(e),
                    });
                }
            }
        }
        if let Some(alias) = cpu_temperature(&out) {
            out.push(alias);
        }
        out
    }
}

/// A [`CPU_TEMPERATURE`] copy of the best CPU temperature reading, when no
/// reading has that name yet.
pub fn cpu_temperature(readings: &[SensorReading]) -> Option<SensorReading> {
    if readings.iter().any(|r| r.name == CPU_TEMPERATURE) {
        return None;
    }
    let temps: Vec<&SensorReading> = readings
        .iter()
        .filter(|r| r.kind == SensorKind::Temperature && !r.name.starts_with("GPU"))
        .collect();
    CPU_TEMPERATURE_LABELS.iter().find_map(|label| {
        temps
            .iter()
            .find(|r| r.name.to_ascii_lowercase().contains(label))
            .map(|r| SensorReading {
                name: CPU_TEMPERATURE.into(),
                ..(*r).clone()
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_hal::{HalError, HalResult};

    /// A backend that answers what it is told.
    struct Fake(&'static str, Result<Vec<SensorReading>, &'static str>);

    impl SensorBackend for Fake {
        fn name(&self) -> &str {
            self.0
        }
        fn poll(&mut self) -> HalResult<Vec<SensorReading>> {
            self.1.clone().map_err(|e| HalError::Backend(e.into()))
        }
    }

    fn temp(name: &str, value: f64) -> SensorReading {
        SensorReading {
            name: name.into(),
            value,
            units: "°C".into(),
            min: 0.0,
            max: 100.0,
            kind: SensorKind::Temperature,
        }
    }

    #[test]
    fn backends_poll_without_error() {
        // No NvAPI here: tests never touch a driver.
        for mut b in backends(&SensorOptions::default()) {
            assert!(b.poll().is_ok(), "{}", b.name());
        }
    }

    #[test]
    fn names_are_unique_and_failures_reported() {
        let mut reg = SensorRegistry::new(vec![
            Box::new(Fake("a", Ok(vec![temp("X", 1.0), temp("Y", f64::NAN)]))),
            Box::new(Fake("b", Err("gone"))),
            Box::new(Fake("c", Ok(vec![temp("X", 2.0)]))),
        ]);
        let r = reg.poll();
        let names: Vec<&str> = r.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["X", "X #2"]);
        assert_eq!(reg.sources().len(), 3);
        assert_eq!(reg.sources()[0].sensors, 1, "NaN dropped");
        assert!(reg.sources()[1].error.as_deref().unwrap().contains("gone"));
    }

    #[test]
    fn cpu_temperature_alias() {
        let lhm = [
            temp("GPU Temperature", 50.0),
            temp("AMD Ryzen 7 5800X Core (Tctl/Tdie)", 61.5),
            temp("Nuvoton NCT6798D CPU", 40.0),
        ];
        let a = cpu_temperature(&lhm).unwrap();
        assert_eq!((a.name.as_str(), a.value), (CPU_TEMPERATURE, 61.5));
        let hwmon = [temp("nvme Composite", 35.0), temp("coretemp Package id 0", 48.0)];
        assert_eq!(cpu_temperature(&hwmon).unwrap().value, 48.0);
        assert!(cpu_temperature(&[temp(CPU_TEMPERATURE, 1.0), temp("k10temp Tctl", 2.0)]).is_none());
        assert!(cpu_temperature(&[temp("nvme Composite", 35.0)]).is_none());
        let mut reg = SensorRegistry::new(vec![Box::new(Fake("hwmon", Ok(hwmon.to_vec())))]);
        assert_eq!(reg.poll().last().unwrap().name, CPU_TEMPERATURE);
    }
}
