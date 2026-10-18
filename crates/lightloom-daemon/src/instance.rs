// SPDX-License-Identifier: Apache-2.0 OR MIT
//! One daemon per user (a lock file), and the API token.

use anyhow::{bail, Context};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

/// Held while the daemon runs; the OS releases the lock when the process ends.
#[derive(Debug)]
pub struct InstanceLock {
    _file: File,
}

/// Take the single-instance lock at `path`.
pub fn acquire(path: &Path) -> anyhow::Result<InstanceLock> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    match file.try_lock() {
        Ok(()) => Ok(InstanceLock { _file: file }),
        Err(std::fs::TryLockError::WouldBlock) => {
            bail!(
                "another lightloomd is already running for this user ({} is locked)",
                path.display()
            )
        }
        Err(std::fs::TryLockError::Error(e)) => Err(e).with_context(|| format!("locking {}", path.display())),
    }
}

/// The API token at `path`, created (64 hex characters, from the OS random
/// source) when missing or malformed. Only the user can read the data
/// directory; on Unix the file is also `0600`.
pub fn load_or_create_token(path: &Path) -> anyhow::Result<String> {
    if let Ok(text) = std::fs::read_to_string(path) {
        let token = text.trim();
        if token.len() >= 32 && token.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(token.to_string());
        }
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let token = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let mut opts = OpenOptions::new();
    opts.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts
        .open(path)
        .with_context(|| format!("writing {}", path.display()))?;
    file.write_all(token.as_bytes())?;
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_instance_is_refused_and_the_token_is_stable() {
        let dir = std::env::temp_dir().join(format!("lightloom-instance-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let first = acquire(&dir.join("lightloomd.lock")).unwrap();
        let second = acquire(&dir.join("lightloomd.lock"));
        assert!(second.unwrap_err().to_string().contains("already running"));
        drop(first);
        assert!(acquire(&dir.join("lightloomd.lock")).is_ok());

        let token = load_or_create_token(&dir.join("api-token")).unwrap();
        assert_eq!(token.len(), 64);
        assert_eq!(load_or_create_token(&dir.join("api-token")).unwrap(), token);
        std::fs::write(dir.join("api-token"), "short").unwrap();
        assert_ne!(load_or_create_token(&dir.join("api-token")).unwrap(), token);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
