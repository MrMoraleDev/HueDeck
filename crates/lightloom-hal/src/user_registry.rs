// SPDX-License-Identifier: GPL-3.0-or-later
//! The per-user settings store of the OS (on Windows the registry under
//! `HKEY_CURRENT_USER`), for the few keys Lightloom writes on the user's
//! request, such as the opt-in `lightloom://` URL-scheme registration.
//!
//! Keys are paths relative to the per-user root with `\` separators
//! (`Software\Classes\lightloom\shell\open\command`); the value name `""` is a
//! key's default value. `lightloom-hal-windows` implements [`UserRegistry`]
//! over the real registry; `mock::MockRegistry` keeps keys in memory, so tests
//! never touch the user's registry.

use crate::error::HalResult;

/// String values under the per-user root.
pub trait UserRegistry: Send + Sync {
    /// The string value `name` of `key`; `None` when the key or value is missing.
    fn read_string(&self, key: &str, name: &str) -> HalResult<Option<String>>;
    /// Set the string value `name` of `key`, creating the key (and its parents).
    fn write_string(&self, key: &str, name: &str, value: &str) -> HalResult<()>;
    /// Delete `key` with all its subkeys and values; a missing key is fine.
    fn delete_tree(&self, key: &str) -> HalResult<()>;
}
