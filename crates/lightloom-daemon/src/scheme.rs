// SPDX-License-Identifier: GPL-3.0-or-later
//! The per-user `lightloom://` URL-scheme registration (opt-in, Settings).
//!
//! On Windows a per-user URL scheme is a key under
//! `HKEY_CURRENT_USER\Software\Classes` (no administrator rights):
//!
//! ```text
//! HKCU\Software\Classes\lightloom
//!     (Default)    = "URL:Lightloom action"
//!     URL Protocol = ""
//!     DefaultIcon\(Default)        = "<dir>\lightloom.exe",0
//!     shell\open\command\(Default) = "<dir>\lightloom.exe" action "%1"
//! ```
//!
//! Opening a `lightloom://...` link then runs the command-line client, which
//! hands the URL to the running daemon (`POST /v1/actions`). The key is
//! written only when the user turns the setting on, rewritten at startup
//! while the setting is on and the client moved (a portable copy), and
//! deleted when the setting is turned off. Every access goes through a
//! [`UserRegistry`], so tests use an in-memory one.

use lightloom_hal::UserRegistry;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The scheme's key, relative to `HKEY_CURRENT_USER`.
pub const SCHEME_KEY: &str = r"Software\Classes\lightloom";
/// Its description (the key's default value).
pub const SCHEME_DESCRIPTION: &str = "URL:Lightloom action";

/// The registration: a per-user registry and the client the links run.
#[derive(Clone, Default)]
pub struct UrlScheme {
    registry: Option<Arc<dyn UserRegistry>>,
    client: Option<PathBuf>,
}

impl std::fmt::Debug for UrlScheme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UrlScheme")
            .field("registry", &self.registry.is_some())
            .field("client", &self.client)
            .finish()
    }
}

/// The `lightloom` client next to this executable, if it is there.
pub fn client_next_to_exe() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "lightloom.exe"
    } else {
        "lightloom"
    };
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join(name)))
        .filter(|p| p.is_file())
}

impl UrlScheme {
    /// Registration through `registry`, running `client`.
    pub fn new(registry: Option<Arc<dyn UserRegistry>>, client: Option<PathBuf>) -> Self {
        Self { registry, client }
    }

    /// No registration possible (the default of [`crate::engine::start`], so
    /// tests never reach the real registry).
    pub fn unavailable() -> Self {
        Self::default()
    }

    /// This OS's per-user registry and the client next to `lightloomd`.
    pub fn system() -> Self {
        Self::new(
            lightloom_runtime::user_registry::system_registry(),
            client_next_to_exe(),
        )
    }

    /// The scheme can be registered (a registry and a client).
    pub fn available(&self) -> bool {
        self.registry.is_some() && self.client.is_some()
    }

    /// Why it cannot be registered; `None` when it can.
    pub fn unavailable_reason(&self) -> Option<String> {
        if self.registry.is_none() {
            return Some("this system has no per-user URL-scheme registry Lightloom can use".into());
        }
        if self.client.is_none() {
            return Some("the lightloom command-line client is not next to lightloomd".into());
        }
        None
    }

    /// The command the links run.
    pub fn command(&self) -> Option<String> {
        self.client.as_deref().map(command_for)
    }

    /// Write the registration.
    pub fn register(&self) -> Result<(), String> {
        let (Some(reg), Some(client)) = (&self.registry, &self.client) else {
            return Err(self.unavailable_reason().unwrap_or_default());
        };
        let exe = client.display().to_string();
        let write = |key: &str, name: &str, value: &str| {
            reg.write_string(key, name, value)
                .map_err(|e| format!("cannot write HKCU\\{key}: {e}"))
        };
        write(SCHEME_KEY, "", SCHEME_DESCRIPTION)?;
        write(SCHEME_KEY, "URL Protocol", "")?;
        write(&format!(r"{SCHEME_KEY}\DefaultIcon"), "", &format!("\"{exe}\",0"))?;
        write(
            &format!(r"{SCHEME_KEY}\shell\open\command"),
            "",
            &command_for(client),
        )?;
        Ok(())
    }

    /// Delete the registration (nothing to do when it is not there).
    pub fn unregister(&self) -> Result<(), String> {
        let Some(reg) = &self.registry else {
            return Ok(());
        };
        reg.delete_tree(SCHEME_KEY)
            .map_err(|e| format!("cannot delete HKCU\\{SCHEME_KEY}: {e}"))
    }

    /// The command registered now, if any.
    pub fn registered_command(&self) -> Option<String> {
        let reg = self.registry.as_ref()?;
        reg.read_string(&format!(r"{SCHEME_KEY}\shell\open\command"), "")
            .ok()
            .flatten()
    }

    /// The registration is there and runs this client.
    pub fn is_current(&self) -> bool {
        self.command().is_some() && self.registered_command() == self.command()
    }
}

fn command_for(client: &Path) -> String {
    format!("\"{}\" action \"%1\"", client.display())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_hal::mock::MockRegistry;

    fn scheme(reg: &Arc<MockRegistry>, client: &str) -> UrlScheme {
        let r: Arc<dyn UserRegistry> = reg.clone();
        UrlScheme::new(Some(r), Some(PathBuf::from(client)))
    }

    #[test]
    fn registers_and_removes_the_scheme_in_a_fake_registry() {
        let reg = Arc::new(MockRegistry::default());
        let s = scheme(&reg, r"C:\Lightloom\lightloom.exe");
        assert!(s.available());
        assert!(!s.is_current());
        s.register().unwrap();
        assert!(s.is_current());
        let dump = reg.dump();
        let get = |k: &str, n: &str| {
            dump.iter()
                .find(|(key, name, _)| key == k && name == n)
                .map(|(_, _, v)| v.clone())
        };
        assert_eq!(
            get(r"software\classes\lightloom", "").as_deref(),
            Some(SCHEME_DESCRIPTION)
        );
        assert_eq!(
            get(r"software\classes\lightloom", "url protocol").as_deref(),
            Some("")
        );
        assert_eq!(
            get(r"software\classes\lightloom\defaulticon", "").as_deref(),
            Some(r#""C:\Lightloom\lightloom.exe",0"#)
        );
        assert_eq!(
            get(r"software\classes\lightloom\shell\open\command", "").as_deref(),
            Some(r#""C:\Lightloom\lightloom.exe" action "%1""#)
        );
        assert_eq!(dump.len(), 4);

        // A moved client is not current until registered again.
        let moved = scheme(&reg, r"D:\Portable\lightloom.exe");
        assert!(!moved.is_current());
        moved.register().unwrap();
        assert!(moved.is_current());
        assert_eq!(reg.dump().len(), 4);

        moved.unregister().unwrap();
        assert!(reg.dump().is_empty());
        assert!(!moved.is_current());
        // Removing twice is fine.
        moved.unregister().unwrap();
    }

    #[test]
    fn unavailable_without_a_registry_or_a_client() {
        let none = UrlScheme::unavailable();
        assert!(!none.available());
        assert!(none.register().is_err());
        assert!(none.unregister().is_ok());
        let reg = Arc::new(MockRegistry::default());
        let r: Arc<dyn UserRegistry> = reg.clone();
        let no_client = UrlScheme::new(Some(r), None);
        assert!(no_client.unavailable_reason().unwrap().contains("client"));
        assert!(no_client.register().is_err());
        assert!(reg.dump().is_empty());
    }

    #[test]
    fn a_failing_registry_reports_the_key() {
        let reg = Arc::new(MockRegistry::default());
        reg.fail_writes.store(true, std::sync::atomic::Ordering::SeqCst);
        let e = scheme(&reg, r"C:\h\lightloom.exe").register().unwrap_err();
        assert!(e.contains(r"HKCU\Software\Classes\lightloom"), "{e}");
    }
}
