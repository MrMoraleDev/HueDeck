// SPDX-License-Identifier: GPL-3.0-or-later
//! The effect library: lightscripts found in effect directories, with their
//! metadata and a stable id.
//!
//! Ids are `<source>:<name>`: `lightloom:plasma` for Lightloom's starter effects,
//! `user:<file stem>` for the user's own directory, `signalrgb:<file stem>` for
//! effects bundled with SignalRGB, `signalrgb:<catalog id>` for its catalog
//! cache (one directory per effect holding `effect.html`) and `developer:<uid>`
//! for pages loaded through the compatible API. The id is what
//! persistence and the APIs refer to; paths can move between versions.

use lightloom_effects::EffectMeta;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where an effect comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EffectSource {
    /// Lightloom's starter effects (CC0).
    Lightloom,
    /// The user's own effect directory.
    User,
    /// A local SignalRGB installation, read in place.
    SignalRgb,
    /// Pages a developer loaded through the compatible API (`lighting/loadEffect`).
    Developer,
}

impl EffectSource {
    /// Id prefix.
    pub fn prefix(self) -> &'static str {
        match self {
            EffectSource::Lightloom => "lightloom",
            EffectSource::User => "user",
            EffectSource::SignalRgb => "signalrgb",
            EffectSource::Developer => "developer",
        }
    }
}

/// One effect file.
#[derive(Clone, Debug, Serialize)]
pub struct EffectFile {
    /// Stable id (`<source>:<name>`).
    pub id: String,
    /// The HTML file.
    pub path: PathBuf,
    /// Where it comes from.
    pub source: EffectSource,
    /// Parsed `<head>` metadata.
    pub meta: EffectMeta,
}

/// Image file types an effect's picture may have, with their media types.
const IMAGE_TYPES: [(&str, &str); 4] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("webp", "image/webp"),
];

impl EffectFile {
    /// The effect's picture and its media type, when one sits next to the page:
    /// `<stem>.png` (or `.jpg`, `.jpeg`, `.webp`) beside `<stem>.html`, and for
    /// a catalog-cache `effect.html` also `image.*` or `thumbnail.*` in its directory.
    pub fn image(&self) -> Option<(PathBuf, &'static str)> {
        let dir = self.path.parent()?;
        let stem = self.path.file_stem()?.to_string_lossy().into_owned();
        let mut stems = vec![stem.clone()];
        if stem.eq_ignore_ascii_case("effect") {
            stems.extend(["image".to_string(), "thumbnail".to_string()]);
        }
        stems.iter().find_map(|s| {
            IMAGE_TYPES.iter().find_map(|(ext, mime)| {
                let p = dir.join(format!("{s}.{ext}"));
                p.is_file().then_some((p, *mime))
            })
        })
    }
}

/// Effects from several directories, by id. A later directory's effect with the
/// same id replaces an earlier one.
#[derive(Clone, Debug, Default)]
pub struct EffectLibrary {
    effects: BTreeMap<String, EffectFile>,
}

/// Effect files larger than this are skipped (lightscripts are a few KB).
const MAX_EFFECT_BYTES: u64 = 4 << 20;

impl EffectLibrary {
    /// An empty library.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add every `*.html` in `dir` (and, with `recursive`, below it) as effects
    /// from `source`. Returns how many were added; unreadable files are skipped.
    pub fn add_dir(&mut self, dir: &Path, source: EffectSource, recursive: bool) -> usize {
        let mut files = Vec::new();
        collect_html(dir, recursive, 0, &mut files);
        files.sort();
        let mut added = 0;
        for path in files {
            let Some(name) = effect_name(&path) else {
                continue;
            };
            if std::fs::metadata(&path).map_or(true, |m| m.len() > MAX_EFFECT_BYTES) {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let id = format!("{}:{name}", source.prefix());
            let meta =
                lightloom_effects::parse_effect_meta(&String::from_utf8_lossy(&bytes)).with_id(id.clone());
            self.effects.insert(
                id.clone(),
                EffectFile {
                    id,
                    path,
                    source,
                    meta,
                },
            );
            added += 1;
        }
        added
    }

    /// The effect with `id`.
    pub fn get(&self, id: &str) -> Option<&EffectFile> {
        self.effects.get(id)
    }

    /// Every effect, by id.
    pub fn iter(&self) -> impl Iterator<Item = &EffectFile> {
        self.effects.values()
    }

    /// Number of effects.
    pub fn len(&self) -> usize {
        self.effects.len()
    }

    /// Whether there is none.
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }
}

/// The name part of an effect's id: the file stem, or the directory name for a
/// catalog-cache `effect.html`.
fn effect_name(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    if stem.eq_ignore_ascii_case("effect") {
        if let Some(dir) = path.parent().and_then(|d| d.file_name()) {
            return Some(dir.to_string_lossy().into_owned());
        }
    }
    Some(stem)
}

fn collect_html(dir: &Path, recursive: bool, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if recursive && depth < 4 {
                collect_html(&path, recursive, depth + 1, out);
            }
        } else if kind.is_file() && path.extension().is_some_and(|x| x.eq_ignore_ascii_case("html")) {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_come_from_file_stems_and_cache_directories() {
        let dir = std::env::temp_dir().join(format!("lightloom-effects-lib-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Static")).unwrap();
        std::fs::create_dir_all(dir.join("-Mabc123")).unwrap();
        let page = |title: &str| {
            format!(
                "<head><title>{title}</title><meta property=\"speed\" type=\"number\" default=\"3\"></head>"
            )
        };
        std::fs::write(dir.join("Static").join("Neon Shift.html"), page("Neon")).unwrap();
        std::fs::write(dir.join("-Mabc123").join("effect.html"), page("Cobalt")).unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        let mut lib = EffectLibrary::new();
        assert_eq!(lib.add_dir(&dir, EffectSource::SignalRgb, false), 0);
        assert_eq!(lib.add_dir(&dir, EffectSource::SignalRgb, true), 2);
        let neon = lib.get("signalrgb:Neon Shift").expect("bundled effect");
        assert_eq!(
            (neon.meta.title.as_str(), neon.meta.id.as_str()),
            ("Neon", "signalrgb:Neon Shift")
        );
        assert_eq!(lib.get("signalrgb:-Mabc123").unwrap().meta.title, "Cobalt");
        assert_eq!(lib.len(), 2);
        // Pictures next to the page.
        assert!(neon.image().is_none());
        std::fs::write(dir.join("Static").join("Neon Shift.png"), b"png").unwrap();
        std::fs::write(dir.join("-Mabc123").join("thumbnail.jpg"), b"jpg").unwrap();
        let (path, mime) = neon.image().unwrap();
        assert_eq!(
            (path.file_name().unwrap().to_str(), mime),
            (Some("Neon Shift.png"), "image/png")
        );
        let (_, mime) = lib.get("signalrgb:-Mabc123").unwrap().image().unwrap();
        assert_eq!(mime, "image/jpeg");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
