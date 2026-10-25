// SPDX-License-Identifier: GPL-3.0-or-later
//! The connected monitors as the kernel sees them (`/sys/class/drm`), for
//! the display list when RandR is not available (Wayland). Read-only sysfs:
//! nothing is opened on the GPU.
//!
//! `card0-DP-1/status` is `connected`, `card0-DP-1/modes` lists the modes
//! with the preferred one first, and `card0/device/driver` names the GPU
//! driver. The name shown is the connector (`DP-1`), as compositors and
//! `xrandr` (on most drivers) name it.

use crate::screen::DisplayInfo;
use std::path::Path;

/// `WxH` from the first line of a `modes` file (`2560x1440`, `1920x1080i`).
fn first_mode(modes: &str) -> Option<(u32, u32)> {
    let line = modes.lines().next()?.trim();
    let (w, h) = line.split_once('x')?;
    let h: String = h.chars().take_while(char::is_ascii_digit).collect();
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// Connected connectors under `root` (normally `/sys/class/drm`), sorted by name.
pub fn displays(root: &Path) -> Vec<DisplayInfo> {
    let Ok(dir) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found: Vec<DisplayInfo> = dir
        .flatten()
        .filter_map(|entry| {
            let file = entry.file_name();
            let file = file.to_str()?;
            // `card0-DP-1`; `card0` itself and `renderD128` have no dash part.
            let (card, connector) = file.split_once('-')?;
            if !card.starts_with("card") || connector.is_empty() {
                return None;
            }
            let path = entry.path();
            let status = std::fs::read_to_string(path.join("status")).ok()?;
            if status.trim() != "connected" {
                return None;
            }
            let (width, height) = std::fs::read_to_string(path.join("modes"))
                .ok()
                .and_then(|m| first_mode(&m))
                .unwrap_or((0, 0));
            let adapter = std::fs::read_link(root.join(card).join("device").join("driver"))
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_else(|| card.to_string());
            Some(DisplayInfo {
                name: connector.to_string(),
                width,
                height,
                primary: false,
                adapter,
            })
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn modes_parse() {
        assert_eq!(first_mode("2560x1440\n1920x1080\n"), Some((2560, 1440)));
        assert_eq!(first_mode("1920x1080i\n"), Some((1920, 1080)));
        assert_eq!(first_mode(""), None);
    }

    #[test]
    fn lists_connected_connectors_from_a_fake_sysfs() {
        let root = std::env::temp_dir().join(format!("lightloom-drm-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let connector = |name: &str, status: &str, modes: &str| {
            let d = root.join(name);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("status"), status).unwrap();
            fs::write(d.join("modes"), modes).unwrap();
        };
        connector("card1-DP-1", "connected\n", "2560x1440\n1920x1080\n");
        connector("card1-HDMI-A-1", "disconnected\n", "");
        connector("card1-DP-2", "connected\n", "");
        fs::create_dir_all(root.join("card1").join("device")).unwrap();
        fs::create_dir_all(root.join("renderD128")).unwrap();
        std::os::unix::fs::symlink("/sys/bus/pci/drivers/amdgpu", root.join("card1/device/driver")).unwrap();
        let found = displays(&root);
        let _ = fs::remove_dir_all(&root);
        let names: Vec<_> = found.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["DP-1", "DP-2"]);
        assert_eq!((found[0].width, found[0].height), (2560, 1440));
        assert_eq!((found[1].width, found[1].height), (0, 0));
        assert_eq!(found[0].adapter, "amdgpu");
        assert!(displays(Path::new("/nonexistent/drm")).is_empty());
    }
}
