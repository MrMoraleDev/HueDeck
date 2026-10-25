// SPDX-License-Identifier: GPL-3.0-or-later
//! `CPU Load`, `RAM Load` and `RAM Used`: system-wide CPU time and physical
//! memory (Windows `GetSystemTimes` / `GlobalMemoryStatusEx`, Linux
//! `/proc/stat` / `/proc/meminfo`). Two small OS calls instead of a
//! system-information crate. CPU load is the busy share of the time between
//! two polls, so the first poll reports memory only.

use lightloom_hal::sensors::{SensorBackend, SensorKind, SensorReading};
use lightloom_hal::HalResult;

/// `(idle, total)` CPU time in any unit.
pub type CpuTimes = (u64, u64);
/// `(total, available)` physical memory in bytes.
pub type Memory = (u64, u64);

/// Where the samples come from (tests: fixed sequences).
pub trait LoadSource: Send {
    /// System-wide CPU time.
    fn cpu(&mut self) -> Option<CpuTimes>;
    /// Physical memory.
    fn memory(&mut self) -> Option<Memory>;
}

/// This machine.
#[derive(Clone, Copy, Debug, Default)]
pub struct Os;

impl LoadSource for Os {
    fn cpu(&mut self) -> Option<CpuTimes> {
        #[cfg(windows)]
        return lightloom_hal_windows::system::cpu_times();
        #[cfg(target_os = "linux")]
        return lightloom_hal_linux::procfs::cpu_times(&lightloom_hal_linux::sysfs::RealFs);
        #[cfg(not(any(windows, target_os = "linux")))]
        None
    }

    fn memory(&mut self) -> Option<Memory> {
        #[cfg(windows)]
        return lightloom_hal_windows::system::memory_status();
        #[cfg(target_os = "linux")]
        return lightloom_hal_linux::procfs::memory_status(&lightloom_hal_linux::sysfs::RealFs);
        #[cfg(not(any(windows, target_os = "linux")))]
        None
    }
}

/// The system load backend.
pub struct SystemLoad<S = Os> {
    source: S,
    last: Option<CpuTimes>,
}

impl std::fmt::Debug for SystemLoad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SystemLoad")
    }
}

impl SystemLoad<Os> {
    /// Over this machine; takes the first CPU sample now.
    pub fn new() -> Self {
        Self::with_source(Os)
    }
}

impl Default for SystemLoad<Os> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: LoadSource> SystemLoad<S> {
    /// Over `source`; takes the first CPU sample now.
    pub fn with_source(mut source: S) -> Self {
        let last = source.cpu();
        Self { source, last }
    }
}

fn reading(name: &str, value: f64, units: &str, max: f64) -> SensorReading {
    SensorReading {
        name: name.into(),
        value,
        units: units.into(),
        min: 0.0,
        max,
        kind: SensorKind::Load,
    }
}

/// Busy percent between two samples; `None` when no time passed.
pub fn cpu_load(before: CpuTimes, now: CpuTimes) -> Option<f64> {
    let total = now.1.checked_sub(before.1)?;
    let idle = now.0.checked_sub(before.0)?.min(total);
    (total > 0).then(|| ((total - idle) as f64 * 1000.0 / total as f64).round() / 10.0)
}

impl<S: LoadSource> SensorBackend for SystemLoad<S> {
    fn name(&self) -> &str {
        "system"
    }

    fn poll(&mut self) -> HalResult<Vec<SensorReading>> {
        let mut out = Vec::new();
        let now = self.source.cpu();
        if let (Some(before), Some(now)) = (self.last, now) {
            if let Some(load) = cpu_load(before, now) {
                out.push(reading("CPU Load", load, "%", 100.0));
            }
        }
        if now.is_some() {
            self.last = now;
        }
        if let Some((total, avail)) = self.source.memory().filter(|m| m.0 > 0) {
            let used = total.saturating_sub(avail);
            let gib = |b: u64| (b as f64 / f64::from(1u32 << 30) * 10.0).round() / 10.0;
            out.push(reading(
                "RAM Load",
                (used as f64 * 1000.0 / total as f64).round() / 10.0,
                "%",
                100.0,
            ));
            let mut used_gb = reading("RAM Used", gib(used), "GB", gib(total));
            used_gb.kind = SensorKind::Other;
            out.push(used_gb);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Seq(Vec<CpuTimes>, Memory);

    impl LoadSource for Seq {
        fn cpu(&mut self) -> Option<CpuTimes> {
            (!self.0.is_empty()).then(|| self.0.remove(0))
        }
        fn memory(&mut self) -> Option<Memory> {
            Some(self.1)
        }
    }

    #[test]
    fn load_from_two_samples() {
        assert_eq!(cpu_load((100, 1000), (150, 1100)), Some(50.0));
        assert_eq!(cpu_load((100, 1000), (100, 1000)), None);
        assert_eq!(cpu_load((100, 1000), (90, 900)), None, "counter went back");
        let gib = 1u64 << 30;
        let mut s = SystemLoad::with_source(Seq(vec![(0, 0), (75, 100)], (16 * gib, 4 * gib)));
        let r = s.poll().unwrap();
        let get = |n: &str| r.iter().find(|x| x.name == n).map(|x| (x.value, x.max));
        assert_eq!(get("CPU Load"), Some((25.0, 100.0)));
        assert_eq!(get("RAM Load"), Some((75.0, 100.0)));
        assert_eq!(get("RAM Used"), Some((12.0, 16.0)));
        // No new CPU sample: memory only.
        assert!(s.poll().unwrap().iter().all(|x| x.name != "CPU Load"));
    }

    #[test]
    fn this_machine_answers() {
        let mut s = SystemLoad::new();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let r = s.poll().unwrap();
        if cfg!(any(windows, target_os = "linux")) {
            assert!(r.iter().any(|x| x.name == "RAM Load"));
        }
    }
}
