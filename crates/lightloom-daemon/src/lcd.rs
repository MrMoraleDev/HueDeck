// SPDX-License-Identifier: GPL-3.0-or-later
//! LCD faces: one face renderer per LCD device (docs/host-api/lcd-lightloom.md).
//!
//! A face is an HTML page with `onReady()` from the face library (Lightloom's starter
//! faces, then the user's `faces` directory; ids `lightloom:clock`, `user:<stem>`). When a
//! device's plugin calls `LCD.initialize({width, height})`, its session attaches here
//! ([`LcdProvider::attach`]): the device gets its own `lightloom-renderer --face` process (a
//! second CEF browser next to the canvas one) at exactly that size and at the device's own
//! frame rate (`lcdFramerate`), writing into a frame store the plugin's `getFrame` reads.
//! Without the renderer executable, or with no face at all, a rainbow test pattern of the
//! right size stands in (the mock path; tests use it).
//!
//! The face and frame rate are chosen per device ([`LcdFaces::select`], `PUT
//! /v1/devices/{key}/lcd`) and kept in the database by the engine. Faces that read
//! sensors get the poller's values like a lightscript does (`engine.getSensorValue`).

use lightloom_api::model::{LcdFaceView, LcdView};
use lightloom_engine::FrameStore;
use lightloom_lcd::provider::next_attachment_id;
use lightloom_lcd::{LcdAttachment, LcdProvider, LcdSpec};
use lightloom_render::{CefRenderer, CefRendererConfig, NullRenderer, Pattern, RendererClient};
use lightloom_runtime::effects::{EffectFile, EffectLibrary};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

/// The face a device shows until one is chosen.
pub const DEFAULT_FACE: &str = "lightloom:clock";
/// Face frames per second until a rate is chosen.
pub const DEFAULT_FRAMERATE: u32 = 30;
/// Highest face frame rate.
pub const MAX_FRAMERATE: u32 = 60;

/// A device's choice (stored by the engine).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LcdChoice {
    /// Face id; `None`: [`DEFAULT_FACE`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<String>,
    /// Frames per second; `None`: [`DEFAULT_FRAMERATE`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framerate: Option<u32>,
}

/// What renders one device's face.
enum FaceRenderer {
    Cef(CefRenderer),
    Mock(NullRenderer),
}

impl FaceRenderer {
    fn client(&mut self) -> &mut dyn RendererClient {
        match self {
            FaceRenderer::Cef(c) => c,
            FaceRenderer::Mock(n) => n,
        }
    }
}

/// A device whose plugin initialized its LCD.
struct Attached {
    id: u64,
    spec: LcdSpec,
    frames: Arc<FrameStore>,
    face: Option<String>,
    framerate: u32,
    renderer: FaceRenderer,
}

#[derive(Default)]
struct Inner {
    library: EffectLibrary,
    choices: BTreeMap<String, LcdChoice>,
    devices: BTreeMap<String, Attached>,
    software_compositing: bool,
    /// The last sensor values, handed to face renderers that start later.
    sensors: Option<Map<String, Value>>,
}

/// The face renderers of every LCD device (shared by the engine and the sessions).
pub struct LcdFaces {
    renderer: Option<PathBuf>,
    work_dir: PathBuf,
    inner: Mutex<Inner>,
}

impl std::fmt::Debug for LcdFaces {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LcdFaces")
            .field("renderer", &self.renderer)
            .field(
                "devices",
                &self.lock().devices.keys().cloned().collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

/// A directory name for a device key (`1e71:3008:ABC` → `1e71_3008_ABC`).
fn dir_name(key: &str) -> String {
    let name: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    if name.is_empty() {
        "device".into()
    } else {
        name
    }
}

impl LcdFaces {
    /// Face renderers run `renderer` (`None`: the mock pattern); their rings, profiles and
    /// logs go under `work_dir/<device>`.
    pub fn new(renderer: Option<PathBuf>, work_dir: PathBuf) -> Self {
        Self {
            renderer,
            work_dir,
            inner: Mutex::new(Inner::default()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The face library (faces already showing keep running).
    pub fn set_library(&self, library: EffectLibrary) {
        self.lock().library = library;
    }

    /// Stored choices (at start).
    pub fn set_choices(&self, choices: BTreeMap<String, LcdChoice>) {
        self.lock().choices = choices;
    }

    /// Every device's choice.
    pub fn choices(&self) -> BTreeMap<String, LcdChoice> {
        self.lock().choices.clone()
    }

    /// Face renderers started from now on composite in software (the GPU setting).
    pub fn set_software_compositing(&self, on: bool) {
        self.lock().software_compositing = on;
    }

    /// The face library for the API.
    pub fn faces_view(&self) -> Vec<LcdFaceView> {
        self.lock()
            .library
            .iter()
            .map(|f| LcdFaceView {
                id: f.id.clone(),
                title: f.meta.title.clone(),
                description: f.meta.description.clone(),
                publisher: f.meta.publisher.clone(),
                uses_sensors: f.meta.uses.sensors,
            })
            .collect()
    }

    /// Device `key`'s LCD, while its plugin drives one.
    pub fn view(&self, key: &str) -> Option<LcdView> {
        let mut inner = self.lock();
        let a = inner.devices.get_mut(key)?;
        let (renderer, running, status) = match &mut a.renderer {
            FaceRenderer::Cef(c) => ("cef", c.status().running, c.status_line()),
            FaceRenderer::Mock(n) => ("mock", n.is_running(), None),
        };
        Some(LcdView {
            width: a.spec.width,
            height: a.spec.height,
            circular: a.spec.circular,
            face: a.face.clone(),
            framerate: a.framerate,
            renderer: renderer.into(),
            running,
            status,
        })
    }

    /// Devices with a face renderer now.
    pub fn attached(&self) -> Vec<String> {
        self.lock().devices.keys().cloned().collect()
    }

    /// Whether a face showing now reads sensors.
    pub fn uses_sensors(&self) -> bool {
        let inner = self.lock();
        inner.devices.values().any(|a| {
            a.face
                .as_deref()
                .and_then(|id| inner.library.get(id))
                .is_some_and(|f| f.meta.uses.sensors)
        })
    }

    /// Sensor values for the faces that read them (`{"sensors": {...}}`).
    pub fn push_sensors(&self, data: Map<String, Value>) {
        let mut inner = self.lock();
        let Inner {
            library,
            devices,
            sensors,
            ..
        } = &mut *inner;
        for a in devices.values() {
            let reads = a
                .face
                .as_deref()
                .and_then(|id| library.get(id))
                .is_some_and(|f| f.meta.uses.sensors);
            if let (true, FaceRenderer::Cef(c)) = (reads, &a.renderer) {
                c.push_engine_data(data.clone());
            }
        }
        *sensors = Some(data);
    }

    /// Choose device `key`'s face (`Some("")`: the default) and/or frame rate. A face
    /// showing now restarts on the new choice. Returns the device's choice.
    pub fn select(
        &self,
        key: &str,
        face: Option<String>,
        framerate: Option<u32>,
    ) -> Result<LcdChoice, String> {
        let mut inner = self.lock();
        if let Some(id) = face.as_deref().filter(|id| !id.is_empty()) {
            if inner.library.get(id).is_none() {
                return Err(format!("no LCD face {id}"));
            }
        }
        if let Some(fps) = framerate {
            if !(1..=MAX_FRAMERATE).contains(&fps) {
                return Err(format!(
                    "framerate {fps} is out of range: choose 1-{MAX_FRAMERATE}"
                ));
            }
        }
        let choice = {
            let c = inner.choices.entry(key.to_string()).or_default();
            if let Some(id) = face {
                c.face = (!id.is_empty()).then_some(id);
            }
            if framerate.is_some() {
                c.framerate = framerate;
            }
            c.clone()
        };
        if choice == LcdChoice::default() {
            inner.choices.remove(key);
        }
        // Restart the face on the new choice (the old renderer stops outside the lock).
        let Inner {
            library,
            devices,
            sensors,
            software_compositing,
            ..
        } = &mut *inner;
        let ctx = StartContext {
            library,
            sensors,
            software_compositing: *software_compositing,
        };
        let old = devices.get_mut(key).map(|a| {
            let (face, framerate, renderer) = self.start(&ctx, key, a.spec, &a.frames, &choice);
            a.face = face;
            a.framerate = framerate;
            std::mem::replace(&mut a.renderer, renderer)
        });
        drop(inner);
        if let Some(mut r) = old {
            r.client().stop();
        }
        Ok(choice)
    }

    /// Stop every face renderer (daemon shutdown, system sleep).
    pub fn stop_all(&self) {
        let devices = std::mem::take(&mut self.lock().devices);
        for (_, mut a) in devices {
            a.renderer.client().stop();
        }
    }

    /// Start the renderer of `key`'s face per `choice` into `frames`; returns the face
    /// shown, the frame rate and the renderer.
    fn start(
        &self,
        ctx: &StartContext<'_>,
        key: &str,
        spec: LcdSpec,
        frames: &Arc<FrameStore>,
        choice: &LcdChoice,
    ) -> (Option<String>, u32, FaceRenderer) {
        let framerate = choice
            .framerate
            .unwrap_or(DEFAULT_FRAMERATE)
            .clamp(1, MAX_FRAMERATE);
        let face: Option<&EffectFile> = choice
            .face
            .as_deref()
            .and_then(|id| ctx.library.get(id))
            .or_else(|| ctx.library.get(DEFAULT_FACE))
            .or_else(|| ctx.library.iter().next());
        let mock = |why: &str| {
            tracing::info!(target: "lightloom::lcd", device = key, "{why}: LCD shows a test pattern");
            let mut n =
                NullRenderer::with_size(Pattern::Rainbow, f64::from(framerate), spec.width, spec.height);
            if let Err(e) = n.start(Arc::clone(frames)) {
                tracing::warn!(target: "lightloom::lcd", device = key, "mock face did not start: {e}");
            }
            FaceRenderer::Mock(n)
        };
        let renderer = match (&self.renderer, face) {
            (Some(exe), Some(f)) => {
                let mut c = CefRenderer::new(CefRendererConfig {
                    renderer: exe.clone(),
                    effect: f.path.clone(),
                    fps: framerate,
                    properties: Map::new(),
                    software_compositing: ctx.software_compositing,
                    work_dir: self.work_dir.join(dir_name(key)),
                    face: Some((spec.width, spec.height)),
                });
                match c.start(Arc::clone(frames)) {
                    Ok(()) => {
                        if let (true, Some(data)) = (f.meta.uses.sensors, ctx.sensors) {
                            c.push_engine_data(data.clone());
                        }
                        FaceRenderer::Cef(c)
                    }
                    Err(e) => mock(&format!("the face renderer did not start ({e})")),
                }
            }
            (None, _) => mock("lightloom-renderer is not available"),
            (_, None) => mock("no LCD face is installed"),
        };
        (face.map(|f| f.id.clone()), framerate, renderer)
    }
}

/// What [`LcdFaces::start`] reads from the shared state.
struct StartContext<'a> {
    library: &'a EffectLibrary,
    sensors: &'a Option<Map<String, Value>>,
    software_compositing: bool,
}

impl LcdProvider for LcdFaces {
    fn attach(&self, key: &str, name: &str, spec: LcdSpec) -> LcdAttachment {
        let frames = Arc::new(FrameStore::new(spec.width, spec.height));
        let id = next_attachment_id();
        let mut inner = self.lock();
        let choice = inner.choices.get(key).cloned().unwrap_or_default();
        let ctx = StartContext {
            library: &inner.library,
            sensors: &inner.sensors,
            software_compositing: inner.software_compositing,
        };
        let (face, framerate, renderer) = self.start(&ctx, key, spec, &frames, &choice);
        tracing::info!(
            target: "lightloom::lcd",
            device = key,
            name,
            width = spec.width,
            height = spec.height,
            face = face.as_deref().unwrap_or("-"),
            framerate,
            "LCD face attached"
        );
        let old = inner.devices.insert(
            key.to_string(),
            Attached {
                id,
                spec,
                frames: Arc::clone(&frames),
                face,
                framerate,
                renderer,
            },
        );
        drop(inner);
        if let Some(mut a) = old {
            a.renderer.client().stop();
        }
        LcdAttachment { id, frames }
    }

    fn detach(&self, key: &str, id: u64) {
        let mut inner = self.lock();
        let gone = match inner.devices.get(key) {
            Some(a) if a.id == id => inner.devices.remove(key),
            _ => None,
        };
        drop(inner);
        if let Some(mut a) = gone {
            tracing::info!(target: "lightloom::lcd", device = key, "LCD face detached");
            a.renderer.client().stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_runtime::effects::EffectSource;
    use std::time::Duration;

    fn library() -> EffectLibrary {
        let mut lib = EffectLibrary::new();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../faces");
        assert!(
            lib.add_dir(&dir, EffectSource::Lightloom, false) >= 2,
            "starter faces"
        );
        lib
    }

    #[test]
    fn mock_faces_follow_attach_select_and_detach() {
        let faces = LcdFaces::new(None, std::env::temp_dir().join("lightloom-lcd-unused"));
        faces.set_library(library());
        let ids: Vec<String> = faces.faces_view().into_iter().map(|f| f.id).collect();
        assert!(ids.contains(&"lightloom:clock".to_string()), "{ids:?}");
        assert!(ids.contains(&"lightloom:sensor".to_string()), "{ids:?}");
        assert!(faces.view("k").is_none());

        let spec = LcdSpec::new(24.0, 16.0, true).unwrap();
        let a = faces.attach("k", "Kraken", spec);
        assert_eq!((a.frames.width(), a.frames.height()), (24, 16));
        assert!(
            a.frames.wait_for_newer(0, Duration::from_secs(5)).is_some(),
            "the mock paints"
        );
        let v = faces.view("k").unwrap();
        assert_eq!((v.width, v.height, v.circular), (24, 16, true));
        assert_eq!(v.face.as_deref(), Some(DEFAULT_FACE));
        assert_eq!(
            (v.framerate, v.renderer.as_str(), v.running),
            (DEFAULT_FRAMERATE, "mock", true)
        );
        assert!(!faces.uses_sensors());

        assert!(faces.select("k", Some("lightloom:nope".into()), None).is_err());
        assert!(faces.select("k", None, Some(0)).is_err());
        assert!(faces.select("k", None, Some(61)).is_err());
        let c = faces
            .select("k", Some("lightloom:sensor".into()), Some(10))
            .unwrap();
        assert_eq!(c.face.as_deref(), Some("lightloom:sensor"));
        let v = faces.view("k").unwrap();
        assert_eq!((v.face.as_deref(), v.framerate), (Some("lightloom:sensor"), 10));
        assert!(faces.uses_sensors(), "the sensor face reads sensors");
        faces.push_sensors(Map::new());

        // Back to the default face; the rate stays.
        let c = faces.select("k", Some(String::new()), None).unwrap();
        assert_eq!(
            c,
            LcdChoice {
                face: None,
                framerate: Some(10)
            }
        );
        assert_eq!(faces.view("k").unwrap().face.as_deref(), Some(DEFAULT_FACE));

        // A stale detach (an older attachment) leaves the current one alone.
        let b = faces.attach("k", "Kraken", spec);
        faces.detach("k", a.id);
        assert!(faces.view("k").is_some());
        faces.detach("k", b.id);
        assert!(faces.view("k").is_none());
        assert_eq!(
            faces.choices()["k"].framerate,
            Some(10),
            "choices outlive the session"
        );
    }

    #[test]
    fn without_faces_the_mock_still_paints() {
        let faces = LcdFaces::new(Some(PathBuf::from("no-renderer.exe")), std::env::temp_dir());
        let a = faces.attach("k", "K", LcdSpec::new(8.0, 8.0, false).unwrap());
        assert!(a.frames.wait_for_newer(0, Duration::from_secs(5)).is_some());
        let v = faces.view("k").unwrap();
        assert_eq!((v.face, v.renderer.as_str()), (None, "mock"));
        faces.stop_all();
        assert!(faces.attached().is_empty());
    }

    #[test]
    fn device_keys_make_safe_dir_names() {
        assert_eq!(dir_name("1e71:3008:AB/C"), "1e71_3008_AB_C");
        assert_eq!(dir_name(""), "device");
        assert_eq!(dir_name("..\\x"), "___x");
    }
}
