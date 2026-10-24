// SPDX-License-Identifier: GPL-3.0-or-later
//! ADL status codes (AMD `adl_defines.h`, MIT).
//!
//! Zero and the positive `ADL_OK_*` codes are success. ADL has no dedicated
//! I2C NACK status: a device that does not answer shows up as `ADL_ERR` (-1).

/// `ADL_OK`.
pub const ADL_OK: i32 = 0;
/// `ADL_ERR`: generic error (also what an unanswered I2C transfer gives).
pub const ADL_ERR: i32 = -1;
/// `ADL_ERR_NOT_INIT`: the context is not initialised.
pub const ADL_ERR_NOT_INIT: i32 = -2;
/// `ADL_ERR_INVALID_ADL_IDX`: no adapter with that index.
pub const ADL_ERR_INVALID_ADL_IDX: i32 = -5;
/// `ADL_ERR_DISABLED_ADAPTER`: the adapter is disabled.
pub const ADL_ERR_DISABLED_ADAPTER: i32 = -10;

/// Every named status, for lookups.
const NAMED: &[(i32, &str)] = &[
    (4, "ADL_OK_WAIT"),
    (3, "ADL_OK_RESTART"),
    (2, "ADL_OK_MODE_CHANGE"),
    (1, "ADL_OK_WARNING"),
    (0, "ADL_OK"),
    (-1, "ADL_ERR"),
    (-2, "ADL_ERR_NOT_INIT"),
    (-3, "ADL_ERR_INVALID_PARAM"),
    (-4, "ADL_ERR_INVALID_PARAM_SIZE"),
    (-5, "ADL_ERR_INVALID_ADL_IDX"),
    (-6, "ADL_ERR_INVALID_CONTROLLER_IDX"),
    (-7, "ADL_ERR_INVALID_DIPLAY_IDX"),
    (-8, "ADL_ERR_NOT_SUPPORTED"),
    (-9, "ADL_ERR_NULL_POINTER"),
    (-10, "ADL_ERR_DISABLED_ADAPTER"),
    (-11, "ADL_ERR_INVALID_CALLBACK"),
    (-12, "ADL_ERR_RESOURCE_CONFLICT"),
    (-20, "ADL_ERR_SET_INCOMPLETE"),
    (-21, "ADL_ERR_NO_XDISPLAY"),
    (-22, "ADL_ERR_CALL_TO_INCOMPATIABLE_DRIVER"),
    (-23, "ADL_ERR_NO_ADMINISTRATOR_PRIVILEGES"),
    (-24, "ADL_ERR_FEATURESYNC_NOT_STARTED"),
    (-25, "ADL_ERR_INVALID_POWER_STATE"),
];

/// The AMD name of `status` (`"ADL_ERR"`), or `"unknown"`.
pub fn name(status: i32) -> &'static str {
    NAMED
        .iter()
        .find(|(code, _)| *code == status)
        .map_or("unknown", |(_, n)| n)
}

/// Whether ADL reported success (`ADL_OK` or one of the `ADL_OK_*` warnings).
pub fn is_ok(status: i32) -> bool {
    status >= ADL_OK
}

/// Whether `status` asks for a new context and a re-enumeration before one
/// retry: the context was lost, or the adapter index no longer names the
/// card (driver reset or update, adapter disabled and enabled again).
pub fn needs_reinit(status: i32) -> bool {
    matches!(
        status,
        ADL_ERR_NOT_INIT | ADL_ERR_INVALID_ADL_IDX | ADL_ERR_DISABLED_ADAPTER
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_names_and_classes() {
        for code in [
            4, 3, 2, 1, 0, -1, -2, -3, -4, -5, -6, -7, -8, -9, -10, -11, -12, -20, -21, -22, -23, -24, -25,
        ] {
            assert_ne!(name(code), "unknown", "{code}");
        }
        assert_eq!(name(-5), "ADL_ERR_INVALID_ADL_IDX");
        assert_eq!(name(-13), "unknown");
        assert!(is_ok(0) && is_ok(1) && !is_ok(-1));
        assert!(needs_reinit(-2) && needs_reinit(-5) && needs_reinit(-10));
        assert!(!needs_reinit(-1) && !needs_reinit(-8));
    }
}
