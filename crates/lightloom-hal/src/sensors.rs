// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Hardware sensor sources (`engine.getSensorValue(name)` and fan curves).

use crate::error::HalResult;
use serde::{Deserialize, Serialize};

/// Physical quantity of a sensor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SensorKind {
    /// °C.
    Temperature,
    /// RPM.
    Fan,
    /// Volts.
    Voltage,
    /// Percent.
    Load,
    /// MHz.
    Clock,
    /// Watts.
    Power,
    /// Anything else; see `units`.
    Other,
}

/// One sensor sample.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SensorReading {
    /// Display name (what effects pass to `engine.getSensorValue`).
    pub name: String,
    /// Current value.
    pub value: f64,
    /// Unit string (`"°C"`, `"RPM"`, …).
    pub units: String,
    /// Expected minimum for UI scaling.
    pub min: f64,
    /// Expected maximum for UI scaling.
    pub max: f64,
    /// Kind.
    pub kind: SensorKind,
}

/// A source of sensor readings (broker MSR/NvAPI, hwmon, LibreHardwareMonitor JSON…).
pub trait SensorBackend: Send {
    /// Human-readable source name.
    fn name(&self) -> &str;
    /// Poll every sensor this backend knows about.
    fn poll(&mut self) -> HalResult<Vec<SensorReading>>;
}
