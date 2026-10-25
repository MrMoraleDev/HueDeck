// SPDX-License-Identifier: GPL-3.0-or-later
//! Wayland: the xdg-desktop-portal ScreenCast interface (`ashpd`) and a
//! PipeWire stream of the shared monitor (`pipewire`).
//!
//! * `open` creates the portal session and selects one monitor, with the
//!   cursor hidden, `persist_mode = ExplicitlyRevoked` and the saved restore
//!   token (`token.rs`). These calls are quick and bounded by a timeout.
//! * `Start` is where the compositor may show its dialog, and the user may
//!   take any time to answer. It runs in the background: until it returns,
//!   `capture` reports "nothing new", so stopping or switching the effect
//!   never waits for the user. With a valid token there is no dialog.
//! * Granted: the new restore token replaces the saved one, and a thread runs
//!   a PipeWire main loop connected to the portal's remote fd, streaming the
//!   granted node. The format offered is BGRx/BGRA/RGBx/RGBA without
//!   modifiers, so compositors send shared-memory buffers, not DMA-BUFs.
//! * Frames are copied (as packed BGRA) only while `capture` waits for one,
//!   so at most `fps` copies a second whatever the monitor refresh rate.
//!   Compositors send a frame only when the screen changes.
//! * Declined (dialog cancelled): this effect run does not ask again; the
//!   next effect that reads the screen does. The stream ending (the user
//!   stopped sharing, the compositor restarted) ends the source, and the
//!   capture thread reopens it: silently with a token, else with a dialog.

use super::pixels::{self, Order};
use super::token::TokenStore;
use crate::screen::sample::{Frame, Rotation};
use crate::screen::FrameSource;
use ashpd::desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType};
use ashpd::desktop::{PersistMode, ResponseError, Session};
use pipewire as pw;
use pw::spa;
use std::os::fd::OwnedFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tokio::sync::oneshot;

/// Bound on the portal calls before the dialog.
const SETUP_TIMEOUT: Duration = Duration::from_secs(10);

/// One `[screen]` configuration's portal state, across reopenings.
pub struct Portal {
    tokens: TokenStore,
    declined: Arc<AtomicBool>,
}

impl Portal {
    pub fn new(tokens: TokenStore) -> Self {
        Self {
            tokens,
            declined: Arc::default(),
        }
    }

    /// Start a session (see the module docs).
    pub fn open(&mut self) -> Result<Box<dyn FrameSource>, String> {
        if self.declined.load(Ordering::Acquire) {
            return Err("screen sharing was declined in the portal dialog; it is asked again at the next effect start".into());
        }
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("lightloom-portal")
            .enable_all()
            .build()
            .map_err(|e| format!("portal runtime: {e}"))?;
        let token = self.tokens.load();
        let restoring = token.is_some();
        let (proxy, session) = rt.block_on(async {
            tokio::time::timeout(SETUP_TIMEOUT, setup(token))
                .await
                .map_err(|_| "the ScreenCast portal did not answer".to_string())?
        })?;
        let (tx, rx) = oneshot::channel();
        let session = Arc::new(session);
        let s = Arc::clone(&session);
        rt.spawn(async move {
            let _ = tx.send(start(&proxy, &s).await);
        });
        Ok(Box::new(Capture {
            state: State::Pending(rx),
            session,
            tokens: self.tokens.clone(),
            declined: Arc::clone(&self.declined),
            restoring,
            rt: Some(rt),
        }))
    }
}

type ScreencastSession = Session<Screencast>;

async fn setup(token: Option<String>) -> Result<(Screencast, ScreencastSession), String> {
    let err = |what: &'static str| move |e: ashpd::Error| format!("ScreenCast portal {what}: {e}");
    let proxy = Screencast::new().await.map_err(err("unavailable"))?;
    let session = proxy
        .create_session(Default::default())
        .await
        .map_err(err("CreateSession"))?;
    proxy
        .select_sources(
            &session,
            SelectSourcesOptions::default()
                .set_cursor_mode(CursorMode::Hidden)
                .set_sources(ashpd::enumflags2::BitFlags::from(SourceType::Monitor))
                .set_multiple(false)
                .set_persist_mode(PersistMode::ExplicitlyRevoked)
                .set_restore_token(token.as_deref()),
        )
        .await
        .and_then(|r| r.response())
        .map_err(err("SelectSources"))?;
    Ok((proxy, session))
}

/// What `Start` granted.
struct Granted {
    fd: OwnedFd,
    node: u32,
    size: Option<(i32, i32)>,
    token: Option<String>,
}

enum StartError {
    Declined,
    Failed(String),
}

async fn start(proxy: &Screencast, session: &ScreencastSession) -> Result<Granted, StartError> {
    let streams = match proxy
        .start(session, None, Default::default())
        .await
        .and_then(|r| r.response())
    {
        Ok(s) => s,
        Err(ashpd::Error::Response(ResponseError::Cancelled)) => return Err(StartError::Declined),
        Err(e) => return Err(StartError::Failed(format!("ScreenCast portal Start: {e}"))),
    };
    let stream = streams
        .streams()
        .first()
        .ok_or_else(|| StartError::Failed("the portal shared no stream".into()))?;
    let fd = proxy
        .open_pipe_wire_remote(session, Default::default())
        .await
        .map_err(|e| StartError::Failed(format!("ScreenCast portal OpenPipeWireRemote: {e}")))?;
    Ok(Granted {
        fd,
        node: stream.pipe_wire_node_id(),
        size: stream.size(),
        token: streams.restore_token().map(str::to_string),
    })
}

enum State {
    Pending(oneshot::Receiver<Result<Granted, StartError>>),
    Streaming {
        node: u32,
        size: Option<(i32, i32)>,
        shared: Arc<Shared>,
        thread: PwThread,
    },
}

/// A portal session (pending or streaming).
struct Capture {
    state: State,
    session: Arc<ScreencastSession>,
    tokens: TokenStore,
    declined: Arc<AtomicBool>,
    restoring: bool,
    rt: Option<tokio::runtime::Runtime>,
}

impl Drop for Capture {
    fn drop(&mut self) {
        // The PipeWire thread first, then the portal session.
        if let State::Streaming { thread, .. } = &mut self.state {
            thread.stop();
        }
        if let Some(rt) = self.rt.take() {
            let session = Arc::clone(&self.session);
            rt.block_on(async {
                let _ = tokio::time::timeout(Duration::from_secs(2), session.close()).await;
            });
            rt.shutdown_timeout(Duration::from_millis(500));
        }
    }
}

impl FrameSource for Capture {
    fn describe(&self) -> String {
        match &self.state {
            State::Pending(_) if self.restoring => {
                "the ScreenCast portal (restoring the saved choice)".into()
            }
            State::Pending(_) => "the ScreenCast portal (waiting for the user to choose a monitor)".into(),
            State::Streaming { node, size, .. } => match size {
                Some((w, h)) => format!("PipeWire node {node} ({w}x{h}) from the ScreenCast portal"),
                None => format!("PipeWire node {node} from the ScreenCast portal"),
            },
        }
    }

    fn capture(&mut self, timeout: Duration, f: &mut dyn FnMut(&Frame<'_>)) -> Result<bool, String> {
        if let State::Pending(rx) = &mut self.state {
            match rx.try_recv() {
                Err(oneshot::error::TryRecvError::Empty) => {
                    thread::sleep(timeout);
                    return Ok(false);
                }
                Err(oneshot::error::TryRecvError::Closed) => {
                    return Err("the portal request was dropped".into())
                }
                Ok(Err(StartError::Declined)) => {
                    self.declined.store(true, Ordering::Release);
                    self.tokens.save(None);
                    return Err("screen sharing was declined in the portal dialog".into());
                }
                Ok(Err(StartError::Failed(e))) => {
                    // A token the portal no longer accepts must not be retried.
                    self.tokens.save(None);
                    return Err(e);
                }
                Ok(Ok(g)) => {
                    self.tokens.save(g.token.as_deref());
                    let shared = Arc::new(Shared::default());
                    let thread = PwThread::spawn(g.fd, g.node, Arc::clone(&shared))?;
                    tracing::info!("screen: the portal shares PipeWire node {}", g.node);
                    self.state = State::Streaming {
                        node: g.node,
                        size: g.size,
                        shared,
                        thread,
                    };
                }
            }
        }
        let State::Streaming { shared, .. } = &self.state else {
            return Ok(false);
        };
        let Some(image) = shared.next(timeout)? else {
            return Ok(false);
        };
        let frame = Frame::new(
            &image.bgra,
            image.width,
            image.height,
            image.width as usize * 4,
            Rotation::Identity,
        );
        let ok = frame.map(|fr| f(&fr)).is_some();
        shared.recycle(image.bgra);
        if ok {
            Ok(true)
        } else {
            Err(format!("bad PipeWire frame {}x{}", image.width, image.height))
        }
    }
}

/// A packed BGRA image.
struct Image {
    bgra: Vec<u8>,
    width: u32,
    height: u32,
}

/// Between the PipeWire thread and the capture thread.
#[derive(Default)]
struct Shared {
    slot: Mutex<Slot>,
    ready: Condvar,
}

#[derive(Default)]
struct Slot {
    /// The capture thread waits for a frame.
    want: bool,
    frame: Option<Image>,
    /// A buffer to reuse.
    spare: Vec<u8>,
    ended: Option<String>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Slot> {
        self.slot.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Wait up to `timeout` for the next frame.
    fn next(&self, timeout: Duration) -> Result<Option<Image>, String> {
        let mut slot = self.lock();
        slot.want = true;
        let (mut slot, _) = self
            .ready
            .wait_timeout_while(slot, timeout, |s| s.frame.is_none() && s.ended.is_none())
            .unwrap_or_else(|e| e.into_inner());
        if let Some(why) = slot.ended.clone() {
            return Err(why);
        }
        Ok(slot.frame.take())
    }

    fn recycle(&self, buf: Vec<u8>) {
        self.lock().spare = buf;
    }

    fn end(&self, why: String) {
        let mut slot = self.lock();
        slot.ended.get_or_insert(why);
        self.ready.notify_all();
    }

    /// Store a frame if one is wanted (PipeWire thread).
    fn offer(&self, src: &[u8], width: u32, height: u32, stride: usize, order: Order) {
        let mut slot = self.lock();
        if !slot.want {
            return;
        }
        let mut buf = std::mem::take(&mut slot.spare);
        if pixels::to_bgra(src, width as usize, height as usize, stride, order, &mut buf) {
            slot.frame = Some(Image {
                bgra: buf,
                width,
                height,
            });
            slot.want = false;
            self.ready.notify_all();
        } else {
            slot.spare = buf;
        }
    }
}

/// The PipeWire main loop thread.
struct PwThread {
    quit: pw::channel::Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl PwThread {
    fn spawn(fd: OwnedFd, node: u32, shared: Arc<Shared>) -> Result<Self, String> {
        let (quit, rx) = pw::channel::channel::<()>();
        let thread = thread::Builder::new()
            .name("lightloom-pipewire".into())
            .spawn(move || {
                if let Err(e) = run(fd, node, &shared, rx) {
                    shared.end(format!("PipeWire: {e}"));
                } else {
                    shared.end("the PipeWire stream ended".into());
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            quit,
            thread: Some(thread),
        })
    }

    fn stop(&mut self) {
        let _ = self.quit.send(());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for PwThread {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The negotiated format.
#[derive(Clone, Copy)]
struct Negotiated {
    order: Order,
    width: u32,
    height: u32,
}

/// The `EnumFormat` offered: 32-bit RGB formats, any size and rate.
fn format_pod() -> Result<Vec<u8>, String> {
    use spa::param::format::{FormatProperties, MediaSubtype, MediaType};
    use spa::param::video::VideoFormat;
    use spa::pod::{self, serialize::PodSerializer, Value};
    use spa::utils::{Fraction, Rectangle, SpaTypes};
    let obj = pod::object!(
        SpaTypes::ObjectParamFormat,
        spa::param::ParamType::EnumFormat,
        pod::property!(FormatProperties::MediaType, Id, MediaType::Video),
        pod::property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
        pod::property!(
            FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            VideoFormat::BGRx,
            VideoFormat::BGRx,
            VideoFormat::BGRA,
            VideoFormat::RGBx,
            VideoFormat::RGBA
        ),
        pod::property!(
            FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            Rectangle {
                width: 1920,
                height: 1080
            },
            Rectangle { width: 1, height: 1 },
            Rectangle {
                width: 16384,
                height: 16384
            }
        ),
        pod::property!(
            FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            Fraction { num: 30, denom: 1 },
            Fraction { num: 0, denom: 1 },
            Fraction { num: 1000, denom: 1 }
        ),
    );
    PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &Value::Object(obj))
        .map(|(c, _)| c.into_inner())
        .map_err(|e| format!("format pod: {e:?}"))
}

fn run(fd: OwnedFd, node: u32, shared: &Arc<Shared>, quit: pw::channel::Receiver<()>) -> Result<(), String> {
    pw::init();
    let err = |what: &'static str| move |e: pw::Error| format!("{what}: {e}");
    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(err("main loop"))?;
    let context = pw::context::ContextRc::new(&mainloop, None).map_err(err("context"))?;
    let core = context
        .connect_fd_rc(fd, None)
        .map_err(err("connecting to the portal's remote"))?;
    let ml = mainloop.clone();
    let _quit = quit.attach(mainloop.loop_(), move |()| ml.quit());

    let (ml, sh) = (mainloop.clone(), Arc::clone(shared));
    let _core_listener = core
        .add_listener_local()
        .error(move |id, _seq, res, message| {
            if id == pw::core::PW_ID_CORE {
                sh.end(format!("PipeWire core error {res}: {message}"));
                ml.quit();
            }
        })
        .register();

    let stream = pw::stream::StreamRc::new(
        core,
        "lightloom-screen",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )
    .map_err(err("stream"))?;
    let (ml, sh_state, sh_frames) = (mainloop.clone(), Arc::clone(shared), Arc::clone(shared));
    let _listener = stream
        .add_local_listener_with_user_data(None::<Negotiated>)
        .state_changed(move |_, _, old, new| match new {
            pw::stream::StreamState::Error(e) => {
                sh_state.end(format!("stream error: {e}"));
                ml.quit();
            }
            pw::stream::StreamState::Unconnected => {
                sh_state.end(format!("the stream disconnected (was {old:?})"));
                ml.quit();
            }
            _ => {}
        })
        .param_changed(|_, format, id, param| {
            let Some(param) = param else { return };
            if id != spa::param::ParamType::Format.as_raw() {
                return;
            }
            let mut info = spa::param::video::VideoInfoRaw::default();
            if info.parse(param).is_err() {
                return;
            }
            let order = match info.format() {
                spa::param::video::VideoFormat::BGRx | spa::param::video::VideoFormat::BGRA => Order::Bgrx,
                spa::param::video::VideoFormat::RGBx | spa::param::video::VideoFormat::RGBA => Order::Rgbx,
                other => {
                    tracing::warn!("screen: unexpected PipeWire format {other:?}");
                    *format = None;
                    return;
                }
            };
            let size = info.size();
            tracing::debug!(
                "screen: PipeWire format {:?} {}x{}",
                info.format(),
                size.width,
                size.height
            );
            *format = Some(Negotiated {
                order,
                width: size.width,
                height: size.height,
            });
        })
        .process(move |stream, format| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let Some(fmt) = *format else { return };
            let Some(data) = buffer.datas_mut().first_mut() else {
                return;
            };
            let chunk = data.chunk();
            let (offset, size, stride) = (chunk.offset() as usize, chunk.size() as usize, chunk.stride());
            // An empty chunk: a cursor-only or corrupted buffer.
            if size == 0 {
                return;
            }
            let stride = usize::try_from(stride)
                .ok()
                .filter(|&s| s > 0)
                .unwrap_or(fmt.width as usize * 4);
            if let Some(bytes) = data.data() {
                let end = (offset + size).min(bytes.len());
                if offset < end {
                    sh_frames.offer(&bytes[offset..end], fmt.width, fmt.height, stride, fmt.order);
                }
            }
        })
        .register()
        .map_err(err("stream listener"))?;

    let format = format_pod()?;
    let pod = spa::pod::Pod::from_bytes(&format).ok_or("format pod")?;
    stream
        .connect(
            spa::utils::Direction::Input,
            Some(node),
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut [pod],
        )
        .map_err(err("connecting the stream"))?;
    mainloop.run();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_copied_only_when_wanted() {
        let shared = Shared::default();
        let px = [1u8, 2, 3, 4, 5, 6, 7, 8];
        // Nobody waits: nothing is copied.
        shared.offer(&px, 2, 1, 8, Order::Rgbx);
        assert!(shared.lock().frame.is_none());
        // A wait with no frame times out empty and leaves the wish standing.
        assert!(shared.next(Duration::from_millis(5)).unwrap().is_none());
        shared.offer(&px, 2, 1, 8, Order::Rgbx);
        let image = shared.next(Duration::ZERO).unwrap().unwrap();
        assert_eq!(image.bgra, [3, 2, 1, 4, 7, 6, 5, 8]);
        assert_eq!((image.width, image.height), (2, 1));
        shared.recycle(image.bgra);
        assert_eq!(shared.lock().spare.len(), 8);
        // The end is reported to the waiting side.
        shared.end("gone".into());
        shared.end("later".into());
        assert_eq!(shared.next(Duration::ZERO).err().as_deref(), Some("gone"));
    }

    #[test]
    fn a_frame_arrives_while_waiting() {
        let shared = Arc::new(Shared::default());
        let producer = Arc::clone(&shared);
        let t = thread::spawn(move || {
            while !producer.lock().want {
                thread::yield_now();
            }
            producer.offer(&[9, 9, 9, 9], 1, 1, 4, Order::Bgrx);
        });
        let image = shared.next(Duration::from_secs(5)).unwrap().expect("a frame");
        t.join().unwrap();
        assert_eq!(image.bgra, [9, 9, 9, 9]);
    }

    #[test]
    fn the_format_pod_serializes() {
        let bytes = format_pod().unwrap();
        assert!(spa::pod::Pod::from_bytes(&bytes).is_some());
    }

    /// Manual check in a Wayland session (never run by `cargo test`): asks
    /// the compositor to share a monitor (a dialog the first time; the
    /// restore token goes to the temp directory), then waits for 30 frames
    /// and prints their size. Nothing of the image is printed or kept.
    /// `cargo test -p lightloom-inputs portal -- --ignored --nocapture`
    #[test]
    #[ignore = "asks the compositor for screen sharing"]
    fn share_a_monitor_through_the_portal() {
        let file = std::env::temp_dir().join("lightloom-portal-test-token");
        let mut portal = Portal::new(TokenStore::new(Some(file), None));
        let mut source = portal.open().expect("open");
        let (mut frames, mut size) = (0, (0, 0));
        let end = std::time::Instant::now() + Duration::from_secs(120);
        while frames < 30 && std::time::Instant::now() < end {
            let got = source
                .capture(Duration::from_millis(100), &mut |f| size = f.display_size())
                .expect("capture");
            frames += u32::from(got);
        }
        println!("{}: {frames} frames of {size:?}", source.describe());
        assert!(frames > 0);
    }
}
