// SPDX-License-Identifier: GPL-3.0-or-later
//! `@SignalRGB/lcd` in device sessions: a plugin's LCD frames, encoded as it asks.
//!
//! `LCD.initialize({width, height, circular})` attaches the session to its face renderer
//! through the session's [`LcdProvider`] (the daemon renders the device's chosen face at
//! that size; tests use [`lightloom_lcd::MockLcdProvider`]). Without a provider the LCD
//! shows black frames of the right size. `getFrame`, `getSlicedFrame` and `getImageBuffer`
//! read the latest face frame and encode it with [`lightloom_lcd`]; errors reach the plugin
//! as an `LcdError` with a `code` (`js/host.js`, `lcdModule`).
//!
//! [`install`] adds the `lcd*` natives. Encoding is host work: like blocking I/O it
//! suspends the plugin's JavaScript budget.

use crate::device::{lock_device, panic_text, SharedDevice};
use crate::runtime::Deadline;
use lightloom_engine::{Frame, FrameStore};
use lightloom_lcd::{
    encode, geometry, ImageRequest, LcdAttachment, LcdError, LcdFormat, LcdProvider, LcdSpec,
};
use rquickjs::{Array, Ctx, Function, IntoJs, Object, TypedArray, Value};
use std::sync::{Arc, Mutex, MutexGuard};

/// What a session's LCD did, for its report.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LcdStats {
    /// The current `initialize` settings.
    pub spec: Option<LcdSpec>,
    /// `initialize` calls that (re)attached the face.
    pub attaches: u64,
    /// Frames handed out (`getFrame`, each `getSlicedFrame` cell, `getImageBuffer`).
    pub frames: u64,
    /// Bytes handed out.
    pub bytes: u64,
    /// The last format asked for.
    pub format: Option<String>,
    /// Failed calls.
    pub errors: u64,
    /// The most recent failure (`CODE: message`).
    pub last_error: Option<String>,
}

/// One session's LCD.
pub struct LcdContext {
    key: String,
    name: String,
    provider: Option<Arc<dyn LcdProvider>>,
    attachment: Option<LcdAttachment>,
    stats: LcdStats,
}

/// Shared between the natives and the session.
pub type SharedLcd = Arc<Mutex<LcdContext>>;

/// Lock, recovering from a poisoned mutex (a panicking native already faulted the session).
pub fn lock_lcd(lcd: &SharedLcd) -> MutexGuard<'_, LcdContext> {
    lcd.lock().unwrap_or_else(|e| e.into_inner())
}

impl std::fmt::Debug for LcdContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LcdContext")
            .field("key", &self.key)
            .field("provider", &self.provider.is_some())
            .field("stats", &self.stats)
            .finish()
    }
}

impl LcdContext {
    /// The LCD of device `key` (named `name`); frames from `provider`, black without one.
    pub fn new(key: &str, name: &str, provider: Option<Arc<dyn LcdProvider>>) -> Self {
        Self {
            key: key.to_string(),
            name: name.to_string(),
            provider,
            attachment: None,
            stats: LcdStats::default(),
        }
    }

    /// Wrap for the natives.
    pub fn shared(self) -> SharedLcd {
        Arc::new(Mutex::new(self))
    }

    /// What happened so far.
    pub fn stats(&self) -> &LcdStats {
        &self.stats
    }

    /// Whether `initialize` succeeded (`LCD.initialized`).
    pub fn initialized(&self) -> bool {
        self.attachment.is_some()
    }

    fn fail<T>(&mut self, r: Result<T, LcdError>) -> Result<T, LcdError> {
        if let Err(e) = &r {
            self.stats.errors += 1;
            self.stats.last_error = Some(format!("{}: {e}", e.code()));
        }
        r
    }

    /// Detach from the face renderer (the session ended); later calls are "not initialized".
    pub fn release(&mut self) {
        self.detach();
    }

    fn detach(&mut self) {
        if let (Some(a), Some(p)) = (self.attachment.take(), &self.provider) {
            p.detach(&self.key, a.id);
        }
    }

    /// `LCD.initialize`: attach at the new size (the same settings again keep the face).
    pub fn initialize(&mut self, width: f64, height: f64, circular: bool) -> Result<(), LcdError> {
        let spec = match LcdSpec::new(width, height, circular) {
            Ok(s) => s,
            Err(e) => return self.fail(Err(e)),
        };
        if self.attachment.is_some() && self.stats.spec == Some(spec) {
            return Ok(());
        }
        self.detach();
        let attachment = match &self.provider {
            Some(p) => p.attach(&self.key, &self.name, spec),
            None => LcdAttachment {
                id: lightloom_lcd::provider::next_attachment_id(),
                frames: Arc::new(FrameStore::new(spec.width, spec.height)),
            },
        };
        self.attachment = Some(attachment);
        self.stats.spec = Some(spec);
        self.stats.attaches += 1;
        Ok(())
    }

    /// The latest face frame (`call` names the member for the not-initialized error).
    fn frame(&mut self, call: &'static str) -> Result<Arc<Frame>, LcdError> {
        let r = match &self.attachment {
            Some(a) => Ok(a.frames.latest().1),
            None => Err(LcdError::NotInitialized { call }),
        };
        self.fail(r)
    }

    fn format(&mut self, name: &str) -> Result<LcdFormat, LcdError> {
        let r = LcdFormat::parse(name);
        if let Ok(f) = &r {
            self.stats.format = Some(f.name().to_string());
        }
        self.fail(r)
    }

    fn encoded(
        &mut self,
        frame: &Frame,
        format: LcdFormat,
        quality: Option<u8>,
    ) -> Result<Vec<u8>, LcdError> {
        let r = encode(frame, format, quality);
        if let Ok(bytes) = &r {
            self.stats.frames += 1;
            self.stats.bytes += bytes.len() as u64;
        }
        self.fail(r)
    }

    /// `LCD.getFrame({format, quality})`.
    pub fn get_frame(&mut self, format: &str, quality: Option<u8>) -> Result<Vec<u8>, LcdError> {
        let frame = self.frame("getFrame")?;
        let format = self.format(format)?;
        self.encoded(&frame, format, quality)
    }

    /// `LCD.getSlicedFrame({columns, rows, format, quality})`: one encoded cell per grid
    /// cell, row by row.
    pub fn get_sliced_frame(
        &mut self,
        columns: f64,
        rows: f64,
        format: &str,
        quality: Option<u8>,
    ) -> Result<Vec<Vec<u8>>, LcdError> {
        let frame = self.frame("getSlicedFrame")?;
        let format = self.format(format)?;
        let cells = geometry::slice(&frame, columns, rows);
        let cells = self.fail(cells)?;
        cells.iter().map(|c| self.encoded(c, format, quality)).collect()
    }

    /// `LCD.getImageBuffer(x, y, w, h, {outputWidth, outputHeight, format})`.
    pub fn get_image_buffer(&mut self, req: &ImageRequest, format: &str) -> Result<Vec<u8>, LcdError> {
        let frame = self.frame("getImageBuffer")?;
        let format = self.format(format)?;
        let region = geometry::crop_scale(&frame, req);
        let region = self.fail(region)?;
        self.encoded(&region, format, None)
    }
}

impl Drop for LcdContext {
    fn drop(&mut self) {
        self.detach();
    }
}

/// JPEG quality from JS: a finite number, clamped to 0-100; anything else is the default.
fn quality(q: f64) -> Option<u8> {
    (q.is_finite() && q >= 0.0).then(|| q.min(100.0).round() as u8)
}

/// What an `lcd*` native answers.
enum Answer {
    /// `""`: done.
    Done,
    /// A boolean.
    Flag(bool),
    /// Encoded bytes, as a `Uint8Array`.
    Bytes(Vec<u8>),
    /// Several encoded frames, as an array of `Uint8Array`s.
    Cells(Vec<Vec<u8>>),
    /// `{code, message}`, thrown as an `LcdError` by `js/host.js`.
    Failed(LcdError),
}

impl<T> From<Result<T, LcdError>> for Answer
where
    Answer: From<T>,
{
    fn from(r: Result<T, LcdError>) -> Self {
        r.map_or_else(Answer::Failed, Answer::from)
    }
}

impl From<()> for Answer {
    fn from((): ()) -> Self {
        Answer::Done
    }
}

impl From<Vec<u8>> for Answer {
    fn from(b: Vec<u8>) -> Self {
        Answer::Bytes(b)
    }
}

impl From<Vec<Vec<u8>>> for Answer {
    fn from(c: Vec<Vec<u8>>) -> Self {
        Answer::Cells(c)
    }
}

impl<'js> IntoJs<'js> for Answer {
    fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        let bytes = |b: Vec<u8>| -> rquickjs::Result<Value<'js>> {
            Ok(TypedArray::<u8>::new(ctx.clone(), b)?.into_value())
        };
        match self {
            Answer::Done => "".into_js(ctx),
            Answer::Flag(b) => b.into_js(ctx),
            Answer::Bytes(b) => bytes(b),
            Answer::Cells(cells) => {
                let out = Array::new(ctx.clone())?;
                for (i, cell) in cells.into_iter().enumerate() {
                    out.set(i, bytes(cell)?)?;
                }
                Ok(out.into_value())
            }
            Answer::Failed(e) => {
                let o = Object::new(ctx.clone())?;
                o.set("code", e.code())?;
                o.set("message", e.to_string())?;
                Ok(o.into_value())
            }
        }
    }
}

/// Add the `lcd*` natives to `natives`. Answers: `""` / a `Uint8Array` (or an array of
/// them) on success, `{code, message}` on failure. A panic marks `device` fatal.
pub(crate) fn install<'js>(
    ctx: &Ctx<'js>,
    natives: &Object<'js>,
    lcd: &SharedLcd,
    deadline: &Deadline,
    device: &SharedDevice,
) -> rquickjs::Result<()> {
    macro_rules! native {
        ($name:literal, |$l:ident $(, $arg:ident : $ty:ty)*| $body:expr) => {{
            let shared = Arc::clone(lcd);
            let deadline = deadline.clone();
            let device = Arc::clone(device);
            let f = Function::new(ctx.clone(), move |cx: Ctx<'_>, $($arg: $ty),*| -> rquickjs::Result<Answer> {
                let _blocked = deadline.suspend();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut guard = lock_lcd(&shared);
                    let $l: &mut LcdContext = &mut guard;
                    Answer::from($body)
                }));
                result.map_err(|panic| {
                    let msg = format!("host panic in LCD.{}: {}", $name, panic_text(panic.as_ref()));
                    lock_device(&device).set_fatal(msg.clone());
                    rquickjs::Exception::throw_internal(&cx, &msg)
                })
            })?;
            natives.set($name, f)?;
        }};
    }
    native!("lcdInitialize", |l, width: f64, height: f64, circular: bool| l
        .initialize(width, height, circular));
    native!("lcdInitialized", |l| Answer::Flag(l.initialized()));
    native!("lcdGetFrame", |l, format: String, q: f64| l
        .get_frame(&format, quality(q)));
    native!("lcdGetSlicedFrame", |l,
                                  columns: f64,
                                  rows: f64,
                                  format: String,
                                  q: f64| {
        l.get_sliced_frame(columns, rows, &format, quality(q))
    });
    // `rect`: x, y, width, height, outputWidth, outputHeight (NaN: the region's size).
    native!("lcdGetImageBuffer", |l, rect: Vec<f64>, format: String| {
        let at = |i: usize| rect.get(i).copied().unwrap_or(f64::NAN);
        let size = |v: f64| (!v.is_nan()).then_some(v);
        let req = ImageRequest {
            x: at(0),
            y: at(1),
            width: at(2),
            height: at(3),
            output_width: size(at(4)),
            output_height: size(at(5)),
        };
        l.get_image_buffer(&req, &format)
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_lcd::provider::{mock_face_frame, MockEvent};
    use lightloom_lcd::MockLcdProvider;

    #[test]
    fn frames_need_initialize_and_follow_the_size() {
        let mock = Arc::new(MockLcdProvider::new());
        let mut l = LcdContext::new("dev", "Dev", Some(mock.clone() as Arc<dyn LcdProvider>));
        assert_eq!(l.get_frame("RGB", None).unwrap_err().code(), "NOT_INITIALIZED");
        l.initialize(4.0, 2.0, false).unwrap();
        l.initialize(4.0, 2.0, false).unwrap();
        assert_eq!(l.stats().attaches, 1, "same settings keep the face");
        let rgba = l.get_frame("rgba", None).unwrap();
        assert_eq!(
            rgba,
            encode(&mock_face_frame(4, 2), LcdFormat::Rgba, None).unwrap()
        );
        l.initialize(6.0, 6.0, true).unwrap();
        assert_eq!(l.get_frame("RGB565", None).unwrap().len(), 6 * 6 * 2);
        assert_eq!(l.get_sliced_frame(3.0, 2.0, "RGB", None).unwrap().len(), 6);
        let mut req = ImageRequest {
            x: 1.0,
            y: 1.0,
            width: 2.0,
            height: 2.0,
            output_width: Some(4.0),
            output_height: None,
        };
        assert_eq!(l.get_image_buffer(&req, "RGBA").unwrap().len(), 4 * 2 * 4);
        req.x = 6.0;
        assert_eq!(
            l.get_image_buffer(&req, "RGBA").unwrap_err().code(),
            "OUT_OF_BOUNDS"
        );
        assert_eq!(
            l.get_frame("NZXT::Q565", None).unwrap_err().code(),
            "UNSUPPORTED_FORMAT"
        );
        assert_eq!(l.stats().errors, 3);
        assert!(l
            .stats()
            .last_error
            .as_deref()
            .unwrap()
            .starts_with("UNSUPPORTED_FORMAT"));
        drop(l);
        let ev = mock.events();
        let ids: Vec<_> = ev
            .iter()
            .map(|e| match e {
                MockEvent::Attach { id, .. } => ("attach", *id),
                MockEvent::Detach { id, .. } => ("detach", *id),
            })
            .collect();
        assert_eq!(ids.len(), 4, "{ev:?}");
        assert_eq!(ids[1], ("detach", ids[0].1), "the resize detaches the first face");
        assert_eq!(
            ids[3],
            ("detach", ids[2].1),
            "the end of the session detaches the second"
        );
    }

    #[test]
    fn without_a_provider_the_lcd_is_black() {
        let mut l = LcdContext::new("dev", "Dev", None);
        assert_eq!(
            l.initialize(0.0, 1.0, false).unwrap_err().code(),
            "INVALID_SETTINGS"
        );
        l.initialize(2.0, 2.0, false).unwrap();
        assert_eq!(l.get_frame("RGB", None).unwrap(), vec![0; 16]);
    }

    #[test]
    fn quality_is_clamped_or_defaulted() {
        assert_eq!(quality(50.0), Some(50));
        assert_eq!(quality(500.0), Some(100));
        assert_eq!(quality(-1.0), None);
        assert_eq!(quality(f64::NAN), None);
    }
}
