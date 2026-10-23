// SPDX-License-Identifier: GPL-3.0-or-later
//! The `battery` feature global (`device.addFeature("battery")`,
//! docs/host-api/hid-usb.md §addFeature): what a plugin reports about its
//! device's battery. The host only stores it; the daemon decides what a level
//! means (thresholds, warnings, notifications, lighting override), so nothing
//! here knows a device.
//!
//! Numeric states follow the plugin corpus (hid-usb.md, battery gap): 0
//! unknown, 1 draining, 2 charging, 3 full and still charging, 4 full, 5
//! wireless charging. Other values are kept as unknown.

use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

/// The default `battery.lowBatteryWarningLevel` (percent).
pub const DEFAULT_WARNING_LEVEL: u8 = 15;

/// `battery.setBatteryState(n)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BatteryState {
    /// 0 (and anything outside 0..=5).
    #[default]
    Unknown,
    /// 1: running on the battery.
    Draining,
    /// 2: charging.
    Charging,
    /// 3: full, still on the charger.
    FullCharging,
    /// 4: full.
    Full,
    /// 5: on a wireless charger.
    WirelessCharging,
}

impl BatteryState {
    /// The state of a plugin's raw number.
    pub fn from_raw(n: i32) -> Self {
        match n {
            1 => Self::Draining,
            2 => Self::Charging,
            3 => Self::FullCharging,
            4 => Self::Full,
            5 => Self::WirelessCharging,
            _ => Self::Unknown,
        }
    }

    /// The number plugins read back (`battery.batteryState`).
    pub fn raw(self) -> i32 {
        match self {
            Self::Unknown => 0,
            Self::Draining => 1,
            Self::Charging => 2,
            Self::FullCharging => 3,
            Self::Full => 4,
            Self::WirelessCharging => 5,
        }
    }

    /// On external power (charging, or full on the charger).
    pub fn on_power(self) -> bool {
        matches!(
            self,
            Self::Charging | Self::FullCharging | Self::Full | Self::WirelessCharging
        )
    }

    /// `battery.stateString`.
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::Draining => "Draining",
            Self::Charging => "Charging",
            Self::FullCharging => "Full (charging)",
            Self::Full => "Full",
            Self::WirelessCharging => "Wireless charging",
        }
    }
}

/// What a plugin last reported.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatteryStatus {
    /// `0..=100`; `None` until `setBatteryLevel` was called.
    pub level: Option<u8>,
    /// Charging state.
    pub state: BatteryState,
    /// When the plugin last called a setter (Unix time, milliseconds).
    pub updated_ms: u64,
}

impl BatteryStatus {
    /// Store a level (`setBatteryLevel`): clamped to `0..=100`, rounded;
    /// ignored when it is not a finite number.
    pub fn set_level(&mut self, level: f64) {
        if level.is_finite() {
            self.level = Some(level.round().clamp(0.0, 100.0) as u8);
            self.touch();
        }
    }

    /// Store a state (`setBatteryState`).
    pub fn set_state(&mut self, raw: i32) {
        self.state = BatteryState::from_raw(raw);
        self.touch();
    }

    fn touch(&mut self) {
        self.updated_ms = now_ms();
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_clamped_and_rounded() {
        let mut b = BatteryStatus::default();
        assert_eq!(b.level, None);
        b.set_level(42.6);
        assert_eq!(b.level, Some(43));
        assert!(b.updated_ms > 0);
        b.set_level(250.0);
        assert_eq!(b.level, Some(100));
        b.set_level(-3.0);
        assert_eq!(b.level, Some(0));
        b.set_level(f64::NAN);
        assert_eq!(b.level, Some(0));
    }

    #[test]
    fn states_round_trip_and_unknown_numbers_are_unknown() {
        for n in 0..=5 {
            assert_eq!(BatteryState::from_raw(n).raw(), n);
        }
        assert_eq!(BatteryState::from_raw(9), BatteryState::Unknown);
        assert_eq!(BatteryState::from_raw(-1), BatteryState::Unknown);
        assert!(BatteryState::Charging.on_power());
        assert!(BatteryState::Full.on_power());
        assert!(!BatteryState::Draining.on_power());
        assert!(!BatteryState::Unknown.on_power());
    }
}
