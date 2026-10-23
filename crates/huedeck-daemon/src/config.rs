// SPDX-License-Identifier: GPL-3.0-or-later
//! `config.toml`: static settings the user edits by hand (ports, frame rate,
//! extra plugin directories, where the renderer and the dashboard are). The
//! daemon never writes it; choices made in the dashboard live in the database.
//!
//! ```toml
//! [api]
//! port = 53000                  # native API and dashboard (1024-65535; the Port setting saved from the dashboard wins)
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
//!
//! [devices]
//! ignore = []                   # "vid:pid" or "vid:pid:serial" prefixes HueDeck never opens
//!
//! [smbus]
//! enabled = true                # drive SMBus devices (GPU RGB; RAM RGB when the I/O broker is installed); false turns it off (the dashboard setting wins)
//!
//! [network]
//! enabled = false               # network lights: discovery and LAN devices (WLED, Hue, ...); the dashboard setting wins
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
    /// Devices.
    pub devices: DevicesConfig,
    /// SMBus devices.
    pub smbus: SmbusConfig,
    /// Network lights.
    pub network: NetworkConfig,
}

/// `[network]`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NetworkConfig {
    /// Run the network plugins' discovery and drive the LAN devices they
    /// announce, unless the dashboard setting says otherwise. Off by default
    /// (opt-in): it sends discovery traffic (mDNS, broadcasts) on the LAN.
    pub enabled: bool,
}

/// `[smbus]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SmbusConfig {
    /// Drive SMBus devices (NVIDIA GPU RGB through NvAPI; RAM RGB through
    /// the HueDeck I/O broker, used only when it is installed and healthy)
    /// unless the dashboard setting says otherwise. On by default (opt-out);
    /// the safety rules hold either way: never alongside SignalRGB, OpenRGB,
    /// a vendor lighting service or another HueDeck process on the buses.
    pub enabled: bool,
}

impl Default for SmbusConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// `[api]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ApiConfig {
    /// Native API and dashboard port (`None`: [`huedeck_api::NATIVE_PORT`]).
    /// A port saved from the dashboard (Settings → Port) wins over this one.
    pub port: Option<u16>,
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
            port: None,
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

/// `[devices]`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DevicesConfig {
    /// Devices never driven, by key prefix (`vid:pid` or `vid:pid:serial`, hex):
    /// HueDeck does not even open them.
    pub ignore: Vec<String>,
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
        assert_eq!((c.api.port, c.api.compat, c.lighting.fps), (None, true, 30));
        let c: Config = toml::from_str("[api]\nport = 17000\n[lighting]\nfps = 60\n").unwrap();
        assert_eq!(
            (c.api.port, c.api.compat, c.lighting.fps),
            (Some(17000), true, 60)
        );
        assert!(c.smbus.enabled, "SMBus is on unless turned off");
        let c: Config = toml::from_str("[smbus]\nenabled = false\n").unwrap();
        assert!(!c.smbus.enabled);
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
