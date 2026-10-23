// SPDX-License-Identifier: GPL-3.0-or-later
//! Battery warnings: what a battery level means for the user. Plugins report
//! levels and charging states through the `battery` feature
//! ([`huedeck_plugin_host::battery`]); this module knows nothing about
//! devices, only the user's two thresholds.
//!
//! * A device on battery power at or below the low threshold is *low*; at or
//!   below the critical threshold it is *critical*. The dashboard shows both
//!   for as long as they last.
//! * Each discharge cycle raises at most one low and one critical alert
//!   (system notifications): an alert is armed again when the device is
//!   charging (or full on the charger) or climbs back to its threshold plus
//!   [`REARM_MARGIN`] points, so a level wobbling around the threshold does
//!   not raise it again. A critical alert disarms the low one too.

use huedeck_plugin_host::battery::{BatteryState, BatteryStatus};
use std::collections::BTreeMap;

/// Default low-battery threshold, percent.
pub const DEFAULT_LOW: u8 = 15;
/// Default critical threshold, percent.
pub const DEFAULT_CRITICAL: u8 = 5;
/// Points above a threshold that re-arm its alert.
pub const REARM_MARGIN: u8 = 5;
/// Highest low threshold the settings accept.
pub const MAX_LOW: u8 = 95;

/// The user's thresholds (percent; `critical < low`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Thresholds {
    /// Low battery.
    pub low: u8,
    /// Critical battery.
    pub critical: u8,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            low: DEFAULT_LOW,
            critical: DEFAULT_CRITICAL,
        }
    }
}

impl Thresholds {
    /// Checked thresholds: `1 <= low <= MAX_LOW`, `critical < low`.
    pub fn new(low: u8, critical: u8) -> Result<Self, String> {
        if !(1..=MAX_LOW).contains(&low) {
            return Err(format!("the low-battery threshold must be 1..={MAX_LOW} %"));
        }
        if critical >= low {
            return Err("the critical threshold must be below the low-battery threshold".into());
        }
        Ok(Self { low, critical })
    }
}

/// How a battery is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Unknown level, on power, or above the low threshold.
    Ok,
    /// On battery at or below the low threshold.
    Low,
    /// On battery at or below the critical threshold.
    Critical,
}

impl Severity {
    /// The name the API uses.
    pub fn name(self) -> &'static str {
        match self {
            Severity::Ok => "ok",
            Severity::Low => "low",
            Severity::Critical => "critical",
        }
    }
}

/// How `status` is doing under `t`.
pub fn severity(status: &BatteryStatus, t: Thresholds) -> Severity {
    match status.level {
        Some(level) if !status.state.on_power() => {
            if level <= t.critical {
                Severity::Critical
            } else if level <= t.low {
                Severity::Low
            } else {
                Severity::Ok
            }
        }
        _ => Severity::Ok,
    }
}

/// The API name of a charging state.
pub fn state_name(state: BatteryState) -> &'static str {
    match state {
        BatteryState::Unknown => "unknown",
        BatteryState::Draining => "discharging",
        BatteryState::Charging => "charging",
        BatteryState::FullCharging => "fullCharging",
        BatteryState::Full => "full",
        BatteryState::WirelessCharging => "wirelessCharging",
    }
}

#[derive(Clone, Copy, Debug)]
struct Armed {
    low: bool,
    critical: bool,
}

impl Default for Armed {
    fn default() -> Self {
        Self {
            low: true,
            critical: true,
        }
    }
}

/// Per-device alert state.
#[derive(Debug, Default)]
pub struct BatteryMonitor {
    armed: BTreeMap<String, Armed>,
}

impl BatteryMonitor {
    /// A new report of device `key`: the alert it raises, if any
    /// ([`Severity::Low`] or [`Severity::Critical`]).
    pub fn observe(&mut self, key: &str, status: &BatteryStatus, t: Thresholds) -> Option<Severity> {
        let level = status.level?;
        let armed = self.armed.entry(key.to_string()).or_default();
        if status.state.on_power() {
            *armed = Armed::default();
            return None;
        }
        if level >= t.low.saturating_add(REARM_MARGIN) {
            armed.low = true;
        }
        if level >= t.critical.saturating_add(REARM_MARGIN) {
            armed.critical = true;
        }
        match severity(status, t) {
            Severity::Critical if armed.critical => {
                armed.critical = false;
                armed.low = false;
                Some(Severity::Critical)
            }
            Severity::Low if armed.low => {
                armed.low = false;
                Some(Severity::Low)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(level: u8, state: BatteryState) -> BatteryStatus {
        BatteryStatus {
            level: Some(level),
            state,
            updated_ms: 1,
        }
    }

    fn drain(level: u8) -> BatteryStatus {
        on(level, BatteryState::Draining)
    }

    /// The alerts a sequence of reports raises.
    fn run(reports: &[BatteryStatus]) -> Vec<(u8, Severity)> {
        let mut m = BatteryMonitor::default();
        reports
            .iter()
            .filter_map(|r| {
                m.observe("k", r, Thresholds::default())
                    .map(|s| (r.level.unwrap(), s))
            })
            .collect()
    }

    #[test]
    fn severities_follow_the_thresholds_and_power() {
        let t = Thresholds::default();
        assert_eq!(severity(&drain(16), t), Severity::Ok);
        assert_eq!(severity(&drain(15), t), Severity::Low);
        assert_eq!(severity(&drain(6), t), Severity::Low);
        assert_eq!(severity(&drain(5), t), Severity::Critical);
        assert_eq!(severity(&on(3, BatteryState::Charging), t), Severity::Ok);
        assert_eq!(severity(&on(3, BatteryState::Unknown), t), Severity::Critical);
        assert_eq!(severity(&BatteryStatus::default(), t), Severity::Ok);
    }

    #[test]
    fn one_low_and_one_critical_alert_per_discharge_cycle() {
        let levels = [40, 20, 15, 14, 16, 15, 12, 5, 4, 6, 5, 2];
        let reports: Vec<_> = levels.iter().map(|&l| drain(l)).collect();
        assert_eq!(run(&reports), vec![(15, Severity::Low), (5, Severity::Critical)]);
    }

    #[test]
    fn a_device_found_critical_raises_only_the_critical_alert() {
        assert_eq!(
            run(&[drain(3), drain(12), drain(14)]),
            vec![(3, Severity::Critical)]
        );
    }

    #[test]
    fn charging_rearms_both_alerts() {
        let reports = [
            drain(14),
            drain(4),
            on(4, BatteryState::Charging),
            drain(4),
            on(30, BatteryState::Full),
            drain(13),
        ];
        assert_eq!(
            run(&reports),
            vec![
                (14, Severity::Low),
                (4, Severity::Critical),
                (4, Severity::Critical),
                (13, Severity::Low),
            ]
        );
    }

    #[test]
    fn climbing_past_the_margin_rearms_without_charging() {
        // Low 15: back to 19 is not enough, 20 re-arms.
        let reports = [drain(15), drain(19), drain(15), drain(20), drain(15)];
        assert_eq!(run(&reports), vec![(15, Severity::Low), (15, Severity::Low)]);
        // Critical 5: 9 is not enough, 10 re-arms it (and low stays disarmed under 20).
        let reports = [drain(5), drain(9), drain(5), drain(10), drain(5)];
        assert_eq!(
            run(&reports),
            vec![(5, Severity::Critical), (5, Severity::Critical)]
        );
    }

    #[test]
    fn unknown_levels_raise_nothing() {
        let mut m = BatteryMonitor::default();
        let unknown = BatteryStatus {
            level: None,
            state: BatteryState::Draining,
            updated_ms: 1,
        };
        assert_eq!(m.observe("k", &unknown, Thresholds::default()), None);
    }

    #[test]
    fn thresholds_are_checked() {
        assert!(Thresholds::new(15, 5).is_ok());
        assert!(Thresholds::new(0, 0).is_err());
        assert!(Thresholds::new(96, 5).is_err());
        assert!(Thresholds::new(10, 10).is_err());
        assert!(Thresholds::new(10, 0).is_ok());
    }
}
