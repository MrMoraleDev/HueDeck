// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `config.toml`: static settings the user edits by hand (ports, frame rate,
//! extra plugin directories, where the renderer and the dashboard are). The
//! daemon never writes it; choices made in the dashboard live in the database.
//!
//! ```toml
//! [api]
//! port = 16138                  # native API and dashboard
//! compat = true                 # SignalRGB-compatible listeners on 16038 / 16034
//! compat_open_mutations = false # let 16038 change state without the token
//! canvas_events_need_token = false
//!
//! [lighting]
//! fps = 30
//!
//! [plugins]
//! dirs = []                     # extra plugin directories
//!
//! [renderer]
//! # path = "C:/.../huedeck-renderer.exe"
//!
//! [ui]
//! # dir = "C:/.../ui/dist"
//! ```

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Everything in `config.toml`; missing keys take their defaults.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Listeners.
    pub api: ApiConfig,
    /// Canvas.
    pub lighting: LightingConfig,
    /// Plugin directories.
    pub plugins: PluginsConfig,
    /// Effect renderer.
    pub renderer: RendererConfig,
    /// Dashboard.
    pub ui: UiConfig,
}

/// `[api]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ApiConfig {
    /// Native API and dashboard port.
    pub port: u16,
    /// Start the SignalRGB-compatible listeners (16038, 16034).
    pub compat: bool,
    /// Let the compatible REST API change state without the token.
    pub compat_open_mutations: bool,
    /// Require the token for `POST /canvas/event` on 16034.
    pub canvas_events_need_token: bool,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            port: huedeck_api::NATIVE_PORT,
            compat: true,
            compat_open_mutations: false,
            canvas_events_need_token: false,
        }
    }
}

/// `[lighting]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LightingConfig {
    /// Effect frame rate, 1-120.
    pub fps: u32,
}

impl Default for LightingConfig {
    fn default() -> Self {
        Self { fps: 30 }
    }
}

/// `[plugins]`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PluginsConfig {
    /// Extra plugin directories (scanned after the built-in ones; later wins).
    pub dirs: Vec<PathBuf>,
}

/// `[renderer]`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RendererConfig {
    /// The `huedeck-renderer` executable (default: next to huedeckd).
    pub path: Option<PathBuf>,
}

/// `[ui]`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiConfig {
    /// Dashboard files (default: `ui` next to huedeckd, else the source tree's `ui/dist`).
    pub dir: Option<PathBuf>,
}

impl Config {
    /// Read `path`; a missing file is the defaults.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let mut c: Config =
                    toml::from_str(&text).with_context(|| format!("{} is not valid", path.display()))?;
                c.lighting.fps = c.lighting.fps.clamp(1, 120);
                Ok(c)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_partial_files() {
        let c: Config = toml::from_str("").unwrap();
        assert_eq!(c, Config::default());
        assert_eq!((c.api.port, c.api.compat, c.lighting.fps), (16138, true, 30));
        let c: Config = toml::from_str("[api]\nport = 17000\n[lighting]\nfps = 60\n").unwrap();
        assert_eq!((c.api.port, c.api.compat, c.lighting.fps), (17000, true, 60));
        assert!(
            toml::from_str::<Config>("[api]\nprot = 1\n").is_err(),
            "typos are reported"
        );
        assert_eq!(
            Config::load(Path::new("definitely/missing.toml")).unwrap(),
            Config::default()
        );
    }
}
