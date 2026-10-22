// SPDX-License-Identifier: GPL-3.0-or-later
//! Linux HAL backends (M7).
//!
//! Planned: hidapi `linux-native` (hidraw) with `huedeck-hid-desc` splitting
//! each interface into top-level collections so `Validate` sees the same
//! `{interface, usage, usage_page, collection}` tuples as on Windows; `nusb`
//! for raw USB; `serialport`; udev hotplug; `/dev/i2c-*` SMBus (no broker);
//! hwmon sensors and pwm fan control.

#![forbid(unsafe_code)]

#[cfg(unix)]
mod imp {
    use huedeck_hal::hid::{HidBackend, HidEndpointInfo, HidHandle};
    use huedeck_hal::{HalError, HalResult};

    /// Placeholder hidraw backend; every call reports `Unsupported` until M7.
    #[derive(Debug, Default, Clone, Copy)]
    pub struct LinuxHidBackend;

    impl HidBackend for LinuxHidBackend {
        fn enumerate(&self) -> HalResult<Vec<HidEndpointInfo>> {
            Err(HalError::Unsupported("hidraw backend lands in M7"))
        }

        fn open(&self, _path: &str) -> HalResult<Box<dyn HidHandle>> {
            Err(HalError::Unsupported("hidraw backend lands in M7"))
        }
    }
}

#[cfg(unix)]
pub use imp::LinuxHidBackend;
