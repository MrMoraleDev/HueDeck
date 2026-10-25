// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only GPU sensors: temperature, load, fan speed and clocks.
//!
//! Four public NvAPI calls (NVIDIA's MIT `nvapi.h` / `nvapi_interface.h`),
//! each optional (a driver or card without one simply lacks that sensor):
//!
//! * `NvAPI_GPU_GetThermalSettings` (`NV_GPU_THERMAL_SETTINGS_V2`): °C;
//! * `NvAPI_GPU_GetDynamicPstatesInfoEx` (`NV_GPU_DYNAMIC_PSTATES_INFO_EX`):
//!   GPU and frame-buffer (memory controller) utilisation, percent;
//! * `NvAPI_GPU_GetTachReading`: fan RPM;
//! * `NvAPI_GPU_GetAllClockFrequencies` (`NV_GPU_CLOCK_FREQUENCIES_V2`,
//!   current clocks): graphics and memory clocks, kHz.
//!
//! Nothing is ever set: no cooler, clock or power call is resolved. The calls
//! run on the hub's worker thread like the I2C transfers ([`crate::hub`]).
//! The layouts and the reading names live here, pure and tested.

use crate::hub::{GpuInfo, Job, Reply, Request, REPLY_GRACE, REPLY_TIMEOUT};
use crate::{NvapiError, NvapiHub};
use lightloom_hal::sensors::{SensorBackend, SensorKind, SensorReading};
use lightloom_hal::{HalError, HalResult};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::Instant;

/// `NVAPI_MAX_THERMAL_SENSORS_PER_GPU`.
pub const MAX_THERMAL_SENSORS: usize = 3;
/// `NVAPI_THERMAL_TARGET_ALL`: every sensor in one call.
pub const THERMAL_TARGET_ALL: u32 = 15;
/// `NVAPI_THERMAL_TARGET_GPU`.
pub const THERMAL_TARGET_GPU: i32 = 1;
/// `NVAPI_MAX_GPU_UTILIZATIONS`.
pub const MAX_UTILIZATIONS: usize = 8;
/// `NVAPI_GPU_UTILIZATION_DOMAIN_GPU` / `_FB`.
pub const UTILIZATION_GPU: usize = 0;
/// Frame buffer (memory controller).
pub const UTILIZATION_FB: usize = 1;
/// `NVAPI_MAX_GPU_PUBLIC_CLOCKS`.
pub const MAX_PUBLIC_CLOCKS: usize = 32;
/// `NVAPI_GPU_PUBLIC_CLOCK_GRAPHICS`.
pub const CLOCK_GRAPHICS: usize = 0;
/// `NVAPI_GPU_PUBLIC_CLOCK_MEMORY`.
pub const CLOCK_MEMORY: usize = 4;

/// One entry of `NV_GPU_THERMAL_SETTINGS_V2.sensor`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct NvThermalSensor {
    /// `NV_THERMAL_CONTROLLER`.
    pub controller: i32,
    /// Lowest expected temperature, °C.
    pub default_min_temp: i32,
    /// Highest expected temperature, °C.
    pub default_max_temp: i32,
    /// Now, °C.
    pub current_temp: i32,
    /// `NV_THERMAL_TARGET` (1 = GPU core).
    pub target: i32,
}

/// `NV_GPU_THERMAL_SETTINGS_V2` (68 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NvThermalSettingsV2 {
    /// [`NV_GPU_THERMAL_SETTINGS_VER2`].
    pub version: u32,
    /// Sensors filled.
    pub count: u32,
    /// The sensors.
    pub sensor: [NvThermalSensor; MAX_THERMAL_SENSORS],
}

const _: () = assert!(core::mem::size_of::<NvThermalSettingsV2>() == 68);
/// `MAKE_NVAPI_VERSION(NV_GPU_THERMAL_SETTINGS_V2, 2)`.
pub const NV_GPU_THERMAL_SETTINGS_VER2: u32 = 0x0002_0044;

impl Default for NvThermalSettingsV2 {
    fn default() -> Self {
        Self {
            version: NV_GPU_THERMAL_SETTINGS_VER2,
            count: 0,
            sensor: [NvThermalSensor::default(); MAX_THERMAL_SENSORS],
        }
    }
}

/// One `NV_GPU_DYNAMIC_PSTATES_INFO_EX.utilization` entry.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct NvUtilization {
    /// Bit 0: `bIsPresent`.
    pub is_present: u32,
    /// Percent.
    pub percentage: u32,
}

/// `NV_GPU_DYNAMIC_PSTATES_INFO_EX` (72 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NvDynamicPstatesInfoEx {
    /// [`NV_GPU_DYNAMIC_PSTATES_INFO_EX_VER`].
    pub version: u32,
    /// Bit 0: dynamic P-states enabled.
    pub flags: u32,
    /// GPU, FB, VID, BUS, …
    pub utilization: [NvUtilization; MAX_UTILIZATIONS],
}

const _: () = assert!(core::mem::size_of::<NvDynamicPstatesInfoEx>() == 72);
/// `MAKE_NVAPI_VERSION(NV_GPU_DYNAMIC_PSTATES_INFO_EX, 1)`.
pub const NV_GPU_DYNAMIC_PSTATES_INFO_EX_VER: u32 = 0x0001_0048;

impl Default for NvDynamicPstatesInfoEx {
    fn default() -> Self {
        Self {
            version: NV_GPU_DYNAMIC_PSTATES_INFO_EX_VER,
            flags: 0,
            utilization: [NvUtilization::default(); MAX_UTILIZATIONS],
        }
    }
}

/// One `NV_GPU_CLOCK_FREQUENCIES_V2.domain` entry.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct NvClockDomain {
    /// Bit 0: `bIsPresent`.
    pub is_present: u32,
    /// kHz.
    pub frequency: u32,
}

/// `NV_GPU_CLOCK_FREQUENCIES_V2` (264 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NvClockFrequenciesV2 {
    /// [`NV_GPU_CLOCK_FREQUENCIES_VER2`].
    pub version: u32,
    /// Bits 0-3: `ClockType` (0 = current); the rest reserved.
    pub clock_type: u32,
    /// Per public clock domain.
    pub domain: [NvClockDomain; MAX_PUBLIC_CLOCKS],
}

const _: () = assert!(core::mem::size_of::<NvClockFrequenciesV2>() == 264);
/// `MAKE_NVAPI_VERSION(NV_GPU_CLOCK_FREQUENCIES_V2, 2)`.
pub const NV_GPU_CLOCK_FREQUENCIES_VER2: u32 = 0x0002_0108;
/// `NV_GPU_CLOCK_FREQUENCIES_CURRENT_FREQ`.
pub const CLOCK_TYPE_CURRENT: u32 = 0;

impl Default for NvClockFrequenciesV2 {
    fn default() -> Self {
        Self {
            version: NV_GPU_CLOCK_FREQUENCIES_VER2,
            clock_type: CLOCK_TYPE_CURRENT,
            domain: [NvClockDomain::default(); MAX_PUBLIC_CLOCKS],
        }
    }
}

/// What one GPU reported; `None` where the call is missing or failed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GpuSensors {
    /// Core temperature, °C, with its expected range.
    pub temperature: Option<(f64, f64, f64)>,
    /// GPU utilisation, percent.
    pub load: Option<f64>,
    /// Memory controller utilisation, percent.
    pub memory_load: Option<f64>,
    /// Fan, RPM.
    pub fan_rpm: Option<f64>,
    /// Graphics clock, MHz.
    pub core_clock: Option<f64>,
    /// Memory clock, MHz.
    pub memory_clock: Option<f64>,
}

impl GpuSensors {
    /// The GPU core sensor of a thermal reply (the first one when none says GPU).
    pub fn from_thermal(t: &NvThermalSettingsV2) -> Option<(f64, f64, f64)> {
        let n = (t.count as usize).min(MAX_THERMAL_SENSORS);
        let s = t.sensor[..n]
            .iter()
            .find(|s| s.target == THERMAL_TARGET_GPU)
            .or_else(|| t.sensor[..n].first())?;
        let max = if s.default_max_temp > s.default_min_temp {
            f64::from(s.default_max_temp)
        } else {
            100.0
        };
        Some((f64::from(s.current_temp), f64::from(s.default_min_temp.max(0)), max))
    }

    /// Utilisation of `domain`, when present.
    pub fn utilization(p: &NvDynamicPstatesInfoEx, domain: usize) -> Option<f64> {
        let u = p.utilization.get(domain)?;
        (u.is_present & 1 == 1).then(|| f64::from(u.percentage.min(100)))
    }

    /// Clock of `domain` in MHz, when present.
    pub fn clock(c: &NvClockFrequenciesV2, domain: usize) -> Option<f64> {
        let d = c.domain.get(domain)?;
        (d.is_present & 1 == 1 && d.frequency > 0).then(|| (f64::from(d.frequency) / 1000.0).round())
    }
}

/// Sensor names: `"GPU Temperature"` for the only (or first) GPU, `"GPU 2
/// Temperature"` for the second, and so on.
pub fn readings(gpus: &[GpuSensors]) -> Vec<SensorReading> {
    let mut out = Vec::new();
    for (i, s) in gpus.iter().enumerate() {
        let prefix = if i == 0 {
            "GPU".to_string()
        } else {
            format!("GPU {}", i + 1)
        };
        let mut push = |what: &str, value: f64, units: &str, min: f64, max: f64, kind: SensorKind| {
            out.push(SensorReading {
                name: format!("{prefix} {what}"),
                value,
                units: units.into(),
                min,
                max,
                kind,
            });
        };
        if let Some((t, min, max)) = s.temperature {
            push("Temperature", t, "°C", min, max, SensorKind::Temperature);
        }
        if let Some(v) = s.load {
            push("Load", v, "%", 0.0, 100.0, SensorKind::Load);
        }
        if let Some(v) = s.memory_load {
            push("Memory Load", v, "%", 0.0, 100.0, SensorKind::Load);
        }
        if let Some(v) = s.fan_rpm {
            push("Fan", v, "RPM", 0.0, 4000.0, SensorKind::Fan);
        }
        if let Some(v) = s.core_clock {
            push("Core Clock", v, "MHz", 0.0, 3000.0, SensorKind::Clock);
        }
        if let Some(v) = s.memory_clock {
            push("Memory Clock", v, "MHz", 0.0, 12000.0, SensorKind::Clock);
        }
    }
    out
}

/// The GPUs' sensors as a [`SensorBackend`]; the calls run on the hub's
/// worker thread (the hub stays alive while this exists).
#[derive(Debug)]
pub struct NvapiSensors {
    hub: Arc<NvapiHub>,
}

impl NvapiSensors {
    /// Over `hub`.
    pub fn new(hub: Arc<NvapiHub>) -> Self {
        Self { hub }
    }

    fn ask(&self, gpu: &GpuInfo) -> HalResult<GpuSensors> {
        let (reply, answer) = mpsc::channel();
        self.hub
            .sender()
            .send(Request {
                job: Job::Sensors { gpu: gpu.clone() },
                deadline: Instant::now() + REPLY_TIMEOUT,
                reply,
            })
            .map_err(|_| HalError::from(NvapiError::WorkerGone))?;
        match answer.recv_timeout(REPLY_TIMEOUT + REPLY_GRACE) {
            Ok(Ok(Reply::Sensors(s))) => Ok(s),
            Ok(Ok(Reply::Bytes(_))) => Err(HalError::Backend("unexpected NvAPI reply".into())),
            Ok(Err(e)) => Err(e.into()),
            Err(RecvTimeoutError::Timeout) => Err(NvapiError::Timeout(REPLY_TIMEOUT.as_millis() as u32).into()),
            Err(RecvTimeoutError::Disconnected) => Err(NvapiError::WorkerGone.into()),
        }
    }
}

impl SensorBackend for NvapiSensors {
    fn name(&self) -> &str {
        "nvapi"
    }

    fn poll(&mut self) -> HalResult<Vec<SensorReading>> {
        let mut all = Vec::new();
        let mut last_err = None;
        for gpu in self.hub.gpus() {
            match self.ask(gpu) {
                Ok(s) => all.push(s),
                Err(e) => {
                    // Keep the numbering stable: an empty entry for this card.
                    all.push(GpuSensors::default());
                    last_err = Some(e);
                }
            }
        }
        let out = readings(&all);
        match last_err {
            Some(e) if out.is_empty() => Err(e),
            _ => Ok(out),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hub::{decode_pci, FakeNvI2c};

    fn card(bus: u32) -> GpuInfo {
        GpuInfo {
            name: "NVIDIA GeForce RTX 3060".into(),
            pci: decode_pci(0x2504_10DE, 0x3903_1462),
            bus,
        }
    }

    #[test]
    fn layouts_and_versions_match_nvapi_h() {
        assert_eq!(NvThermalSettingsV2::default().version, 68 | 2 << 16);
        assert_eq!(NvDynamicPstatesInfoEx::default().version, 72 | 1 << 16);
        assert_eq!(NvClockFrequenciesV2::default().version, 264 | 2 << 16);
        assert_eq!(core::mem::offset_of!(NvThermalSettingsV2, sensor), 8);
        assert_eq!(core::mem::offset_of!(NvClockFrequenciesV2, domain), 8);
    }

    #[test]
    fn replies_decode() {
        let mut t = NvThermalSettingsV2::default();
        t.count = 2;
        t.sensor[0] = NvThermalSensor {
            target: 2,
            current_temp: 30,
            ..Default::default()
        };
        t.sensor[1] = NvThermalSensor {
            controller: 1,
            default_min_temp: -256,
            default_max_temp: 127,
            current_temp: 47,
            target: THERMAL_TARGET_GPU,
        };
        assert_eq!(GpuSensors::from_thermal(&t), Some((47.0, 0.0, 127.0)));
        t.count = 0;
        assert_eq!(GpuSensors::from_thermal(&t), None);

        let mut p = NvDynamicPstatesInfoEx::default();
        p.utilization[UTILIZATION_GPU] = NvUtilization {
            is_present: 1,
            percentage: 37,
        };
        p.utilization[UTILIZATION_FB].percentage = 12;
        assert_eq!(GpuSensors::utilization(&p, UTILIZATION_GPU), Some(37.0));
        assert_eq!(GpuSensors::utilization(&p, UTILIZATION_FB), None, "not present");

        let mut c = NvClockFrequenciesV2::default();
        c.domain[CLOCK_GRAPHICS] = NvClockDomain {
            is_present: 1,
            frequency: 1_777_500,
        };
        assert_eq!(GpuSensors::clock(&c, CLOCK_GRAPHICS), Some(1778.0));
        assert_eq!(GpuSensors::clock(&c, CLOCK_MEMORY), None);
    }

    #[test]
    fn names_number_the_gpus() {
        let one = GpuSensors {
            temperature: Some((50.0, 0.0, 90.0)),
            load: Some(20.0),
            fan_rpm: Some(1100.0),
            ..Default::default()
        };
        let r = readings(&[one.clone(), one]);
        let names: Vec<&str> = r.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "GPU Temperature",
                "GPU Load",
                "GPU Fan",
                "GPU 2 Temperature",
                "GPU 2 Load",
                "GPU 2 Fan"
            ]
        );
        assert_eq!(r[0].units, "°C");
        assert_eq!(r[0].kind, SensorKind::Temperature);
    }

    #[test]
    fn sensors_run_on_the_worker_thread() {
        let fake = FakeNvI2c::new(vec![card(1), card(2)]).with_sensors(
            0,
            GpuSensors {
                temperature: Some((55.0, 0.0, 95.0)),
                load: Some(80.0),
                ..Default::default()
            },
        );
        let hub = Arc::new(NvapiHub::with_transport(fake).unwrap());
        let mut s = NvapiSensors::new(Arc::clone(&hub));
        let r = s.poll().unwrap();
        assert_eq!(r.len(), 2, "the second card reports nothing");
        assert_eq!((r[0].name.as_str(), r[0].value), ("GPU Temperature", 55.0));
        assert_eq!(r[1].name, "GPU Load");
    }

    #[test]
    fn a_failing_card_is_an_error_only_when_nothing_answered() {
        let fake = FakeNvI2c::new(vec![card(1)]).fail_with(-1);
        let hub = Arc::new(NvapiHub::with_transport(fake).unwrap());
        assert!(NvapiSensors::new(hub).poll().is_err());
    }
}
