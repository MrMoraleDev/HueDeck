// SPDX-License-Identifier: GPL-3.0-or-later
//! The ScreenCast portal's restore token, kept in a file in the data
//! directory so the portal dialog shows once, not on every effect start.
//!
//! The file holds two lines: the configured display (empty for the default)
//! and the token. A token is single-use: each session's `Start` returns the
//! one for the next session, which replaces it. A token saved for another
//! `[screen] display` is not used, so choosing another display asks again.
//! The file is created with mode 0600.

#![cfg_attr(not(feature = "screencast"), allow(dead_code))]

use std::path::PathBuf;

/// Where the token lives and which display setting it belongs to.
#[derive(Clone, Debug)]
pub struct TokenStore {
    path: Option<PathBuf>,
    key: String,
}

impl TokenStore {
    /// `path`: the file (`None`: tokens are not kept); `display`: the setting.
    pub fn new(path: Option<PathBuf>, display: Option<&str>) -> Self {
        Self {
            path,
            key: display.map(str::trim).unwrap_or_default().to_string(),
        }
    }

    /// The saved token for this display setting.
    pub fn load(&self) -> Option<String> {
        let text = std::fs::read_to_string(self.path.as_ref()?).ok()?;
        let mut lines = text.lines();
        let (key, token) = (lines.next()?, lines.next()?.trim());
        (key == self.key && !token.is_empty()).then(|| token.to_string())
    }

    /// Replace the saved token (`None`: forget it).
    pub fn save(&self, token: Option<&str>) {
        let Some(path) = &self.path else { return };
        let result = match token.filter(|t| !t.is_empty() && !t.contains('\n')) {
            Some(t) => write_private(path, &format!("{}\n{t}\n", self.key)),
            None => match std::fs::remove_file(path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                r => r,
            },
        };
        if let Err(e) = result {
            tracing::warn!(
                "screen: cannot keep the portal restore token in {}: {e}",
                path.display()
            );
        }
    }
}

fn write_private(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_kept_per_display_setting() {
        let dir = std::env::temp_dir().join(format!("lightloom-token-{}", std::process::id()));
        let file = dir.join("data").join("screencast-token");
        let default = TokenStore::new(Some(file.clone()), None);
        assert_eq!(default.load(), None);
        default.save(Some("abc-123"));
        assert_eq!(default.load().as_deref(), Some("abc-123"));
        // Another display setting does not reuse it.
        assert_eq!(TokenStore::new(Some(file.clone()), Some("DP-2")).load(), None);
        // The next session's token replaces it.
        let dp2 = TokenStore::new(Some(file.clone()), Some(" DP-2 "));
        dp2.save(Some("def-456"));
        assert_eq!(
            TokenStore::new(Some(file.clone()), Some("DP-2"))
                .load()
                .as_deref(),
            Some("def-456")
        );
        assert_eq!(default.load(), None);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        dp2.save(None);
        assert!(!file.exists());
        dp2.save(None);
        // Not kept at all.
        let none = TokenStore::new(None, None);
        none.save(Some("x"));
        assert_eq!(none.load(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
