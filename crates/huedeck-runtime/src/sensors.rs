// SPDX-License-Identifier: GPL-3.0-or-later
//! The hardware sensor sources of this machine, as
//! [`SensorBackend`]s (`engine.getSensorValue(name)` and, later, fan curves).
//!
//! Linux: hwmon temperatures and fan speeds, read-only
//! (`huedeck_hal_linux::hwmon`). Windows sources (NvAPI thermals, the broker
//! or LibreHardwareMonitor) and the plugin / effect facade come with
//! WP-SENSORS-FANS, which polls these backends; nothing polls them yet.

use huedeck_hal::sensors::SensorBackend;

/// Every sensor backend available here (none outside Linux yet).
pub fn backends() -> Vec<Box<dyn SensorBackend>> {
    #[cfg(target_os = "linux")]
    {
        vec![Box::new(huedeck_hal_linux::hwmon::HwmonSensors::new())]
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn backends_poll_without_error() {
        for mut b in super::backends() {
            assert!(b.poll().is_ok(), "{}", b.name());
        }
    }
}
