// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Module-specifier resolution for the compat profile.
//!
//! Rules (synthesis §4 "Loading"):
//! * `@SignalRGB/<name>` and the alias `@huedeck/<name>` resolve to the host
//!   shim `<name>` (a trailing `.js` is dropped: `@SignalRGB/Errors.js` → `Errors`).
//!   Unknown shim names are an error so the corpus dry-run reports them as API gaps.
//! * `./x.js`, `../lib/y.js` resolve lexically against the importing file and
//!   must stay inside the plugin root (no symlink following, no `..` escapes).
//! * Any other bare specifier (`fs`, `lodash`, absolute paths, URLs) is refused.
//!
//! Resolution is purely lexical: it never touches the file system, so it can
//! run in tests and in the index-cache pass without I/O.

use crate::meta::{HOST_MODULES, HUEDECK_ONLY_MODULES};
use std::path::{Component, Path, PathBuf};

/// Accepted bare-specifier prefixes, in priority order.
pub const HOST_PREFIXES: [&str; 2] = ["@SignalRGB/", "@huedeck/"];

/// What a specifier resolved to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedModule {
    /// A host-provided module (JS prelude or native), by canonical name.
    HostShim(String),
    /// A file inside the plugin root.
    File(PathBuf),
}

/// Why a specifier was refused.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ResolveError {
    /// `@SignalRGB/<name>` with a name the host does not provide.
    #[error("unknown host module \"{0}\" (API gap)")]
    UnknownHostModule(String),
    /// Relative import that leaves the plugin root.
    #[error("import \"{specifier}\" escapes the plugin root")]
    EscapesRoot {
        /// The offending specifier.
        specifier: String,
    },
    /// Anything that is neither a host module nor a relative path.
    #[error("unsupported module specifier \"{0}\"")]
    Unsupported(String),
}

/// Lexical resolver rooted at a plugin directory.
#[derive(Clone, Debug)]
pub struct ModuleResolver {
    root: PathBuf,
}

impl ModuleResolver {
    /// Resolver for plugins under `root` (should be absolute).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: normalize(&root.into()),
        }
    }

    /// The plugin root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve `specifier` as imported from the file `importer`.
    pub fn resolve(&self, importer: &Path, specifier: &str) -> Result<ResolvedModule, ResolveError> {
        for prefix in HOST_PREFIXES {
            if let Some(rest) = specifier.strip_prefix(prefix) {
                let name = rest.strip_suffix(".js").unwrap_or(rest);
                let huedeck_only = prefix == "@huedeck/" && HUEDECK_ONLY_MODULES.contains(&name);
                return if HOST_MODULES.contains(&name) || huedeck_only {
                    Ok(ResolvedModule::HostShim(name.to_string()))
                } else {
                    Err(ResolveError::UnknownHostModule(name.to_string()))
                };
            }
        }

        if specifier.starts_with("./") || specifier.starts_with("../") {
            let base = importer.parent().unwrap_or(&self.root);
            let joined = normalize(&base.join(specifier));
            return if joined.starts_with(&self.root) {
                Ok(ResolvedModule::File(joined))
            } else {
                Err(ResolveError::EscapesRoot {
                    specifier: specifier.to_string(),
                })
            };
        }

        Err(ResolveError::Unsupported(specifier.to_string()))
    }
}

/// Lexically collapse `.` and `..` components without touching the file system.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    // `..` above the start of a relative path: keep it so the
                    // escape check still sees a path outside the root.
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver() -> ModuleResolver {
        ModuleResolver::new(PathBuf::from("/plugins"))
    }

    #[test]
    fn host_modules_resolve_under_both_prefixes() {
        let r = resolver();
        let importer = Path::new("/plugins/Razer/Razer_Mouse.js");
        assert_eq!(
            r.resolve(importer, "@SignalRGB/udp").unwrap(),
            ResolvedModule::HostShim("udp".into())
        );
        assert_eq!(
            r.resolve(importer, "@huedeck/udp").unwrap(),
            ResolvedModule::HostShim("udp".into())
        );
        assert_eq!(
            r.resolve(importer, "@SignalRGB/Errors.js").unwrap(),
            ResolvedModule::HostShim("Errors".into())
        );
        assert_eq!(
            r.resolve(importer, "@SignalRGB/DeviceDiscovery").unwrap(),
            ResolvedModule::HostShim("DeviceDiscovery".into())
        );
        assert_eq!(
            r.resolve(importer, "@huedeck/BinaryUtils").unwrap(),
            ResolvedModule::HostShim("BinaryUtils".into())
        );
        // HueDeck-only shims must not resolve under the SignalRGB prefix.
        assert_eq!(
            r.resolve(importer, "@SignalRGB/BinaryUtils"),
            Err(ResolveError::UnknownHostModule("BinaryUtils".into()))
        );
        assert_eq!(
            r.resolve(importer, "@SignalRGB/nope"),
            Err(ResolveError::UnknownHostModule("nope".into()))
        );
    }

    #[test]
    fn relative_imports_stay_inside_root() {
        let r = resolver();
        let importer = Path::new("/plugins/Corsair/Modern_Corsair/Corsair_Bragi_Device.js");
        assert_eq!(
            r.resolve(importer, "./CorsairLib.js").unwrap(),
            ResolvedModule::File(PathBuf::from("/plugins/Corsair/Modern_Corsair/CorsairLib.js"))
        );
        assert_eq!(
            r.resolve(importer, "../Shared/Protocol.js").unwrap(),
            ResolvedModule::File(PathBuf::from("/plugins/Corsair/Shared/Protocol.js"))
        );
        assert_eq!(
            r.resolve(importer, "../../../etc/passwd"),
            Err(ResolveError::EscapesRoot {
                specifier: "../../../etc/passwd".into()
            })
        );
        assert_eq!(
            r.resolve(importer, "./a/../../../../x.js"),
            Err(ResolveError::EscapesRoot {
                specifier: "./a/../../../../x.js".into()
            })
        );
    }

    #[test]
    fn other_specifiers_are_refused() {
        let r = resolver();
        let importer = Path::new("/plugins/x.js");
        assert_eq!(
            r.resolve(importer, "fs"),
            Err(ResolveError::Unsupported("fs".into()))
        );
        assert_eq!(
            r.resolve(importer, "https://example.com/x.js"),
            Err(ResolveError::Unsupported("https://example.com/x.js".into()))
        );
        assert_eq!(
            r.resolve(importer, "/abs/x.js"),
            Err(ResolveError::Unsupported("/abs/x.js".into()))
        );
    }

    #[test]
    fn normalize_collapses_dots() {
        assert_eq!(normalize(Path::new("/a/./b/../c")), PathBuf::from("/a/c"));
        assert_eq!(normalize(Path::new("a/../../b")), PathBuf::from("../b"));
    }
}
