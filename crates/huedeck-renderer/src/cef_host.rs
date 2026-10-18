// SPDX-License-Identifier: Apache-2.0 OR MIT
//! CEF integration (feature `cef`): browser process + Chromium subprocesses.
//!
//! Process model: this executable is both the browser process and, via
//! [`cef::execute_process`], every Chromium subprocess (renderer, GPU, utility). Message loop:
//! [`cef::run_message_loop`] on the main thread (CEF's own Win32/glib loop; the browser process
//! has nothing else to do). The run ends when the sink has `--frames` frames (closed from
//! `OnPaint`), on a load error, or when the deadline task fires; `OnBeforeClose` then quits the
//! loop and we call [`cef::shutdown`].
//!
//! Render-process side: the injection script travels in the browser's `extra_info`
//! (`OnBrowserCreated`) and is evaluated in the main frame's V8 context in `OnContextCreated`,
//! i.e. before any page script runs.
//!
//! Serve mode (`--serve`): no frame limit and no deadline; a thread reads control commands
//! from stdin (`huedeck_renderer::control`) and posts each to the UI thread, where it becomes
//! JavaScript in the main frame or a browser call. End of stdin (the daemon is gone) closes
//! the browser like `quit`.

use cef::*;
use huedeck_effects::parse_effect_meta;
use huedeck_effects::EffectMeta;
use huedeck_protocol::shm::{FrameHeader, CANVAS_HEIGHT, CANVAS_WIDTH, SLOT_COUNT};
use huedeck_renderer::cache;
use huedeck_renderer::cli::Args;
use huedeck_renderer::control::{self, Command};
use huedeck_renderer::inject::{injection_script, INJECT_KEY, INJECT_URL};
use huedeck_renderer::ring::RingWriter;
use huedeck_renderer::sink::{FrameSink, PaintOutcome, SinkConfig, SinkReport};
use huedeck_renderer::url::file_url;
use parking_lot::Mutex;
use serde::Serialize;
use std::os::raw::c_int;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

/// Console lines kept for the report.
const CONSOLE_KEEP: usize = 40;
/// `sourceURL` of scripts run for control commands (shows up in console messages).
const CONTROL_URL: &str = "huedeck://renderer/control.js";

/// Mutable run state shared by the handlers (all called on the CEF UI thread).
#[derive(Default)]
struct RunState {
    browser: Option<Browser>,
    create_requested_at: Option<Instant>,
    after_created_at: Option<Instant>,
    load_end: Option<(Instant, i32)>,
    load_error: Option<String>,
    close_requested: bool,
    close_reason: Option<&'static str>,
    timed_out: bool,
    console: Vec<String>,
    console_total: u64,
    console_errors: u64,
}

/// Everything the browser-process handlers need.
struct HostCtx {
    args: Args,
    url: String,
    inject_js: Option<String>,
    sink: Mutex<FrameSink>,
    state: Mutex<RunState>,
}

/// If this process is a Chromium subprocess (`--type=...`), run it and return its exit code.
///
/// `sandbox_info` is the pointer `bootstrap(c).exe` hands to `RunConsoleMain`/`RunWinMain`
/// (sandboxed builds), or null for the plain executable.
pub fn run_subprocess_if_any(sandbox_info: *mut u8) -> Option<i32> {
    // args_os: `std::env::args` panics on a non-Unicode argument.
    if !std::env::args_os()
        .skip(1)
        .any(|a| a.to_string_lossy().starts_with("--type="))
    {
        return None;
    }
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let cef_args = cef::args::Args::new();
    let mut app = HueApp::new(None, Arc::new(Mutex::new(None)));
    let code = execute_process(Some(cef_args.as_main_args()), Some(&mut app), sandbox_info);
    Some(code.max(0))
}

fn request_close(host: &HostCtx, reason: &'static str) {
    let browser = {
        let mut st = host.state.lock();
        if st.close_requested {
            return;
        }
        st.close_requested = true;
        st.close_reason = Some(reason);
        st.browser.clone()
    };
    match browser.and_then(|b| b.host()) {
        Some(bh) => bh.close_browser(1),
        // Browser not created yet (or already gone): nothing to close, just stop the loop.
        None => quit_message_loop(),
    }
}

fn severity_name(level: LogSeverity) -> &'static str {
    match level.get_raw() {
        1 => "verbose",
        2 => "info",
        3 => "warning",
        4 => "error",
        5 => "fatal",
        _ => "log",
    }
}

wrap_app! {
    struct HueApp {
        host: Option<Arc<HostCtx>>,
        inject: Arc<Mutex<Option<String>>>,
    }

    impl App {
        fn on_before_command_line_processing(
            &self,
            process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            // Only the browser process (empty type) gets our switches; Chromium forwards the
            // relevant ones to its children.
            let is_browser = process_type.map(|t| t.to_string().is_empty()).unwrap_or(true);
            let (Some(host), Some(cl), true) = (self.host.as_ref(), command_line, is_browser) else {
                return;
            };
            for switch in host.args.chromium_switches() {
                match switch.split_once('=') {
                    Some((name, value)) => cl.append_switch_with_value(
                        Some(&CefString::from(name)),
                        Some(&CefString::from(value)),
                    ),
                    None => cl.append_switch(Some(&CefString::from(switch.as_str()))),
                }
            }
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            self.host.as_ref().map(|h| HueBrowserProcessHandler::new(h.clone()))
        }

        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(HueRenderProcessHandler::new(self.inject.clone()))
        }
    }
}

wrap_browser_process_handler! {
    struct HueBrowserProcessHandler {
        host: Arc<HostCtx>,
    }

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            create_browser(&self.host);
        }
    }
}

fn create_browser(host: &Arc<HostCtx>) {
    let window_info = WindowInfo {
        windowless_rendering_enabled: 1,
        // Windowless browsers always use the Alloy runtime style.
        runtime_style: RuntimeStyle::ALLOY,
        ..Default::default()
    };
    let settings = BrowserSettings {
        windowless_frame_rate: host.args.fps as c_int,
        // opaque black, like the lightscripts' own <body> background
        background_color: 0xFF00_0000,
        ..Default::default()
    };
    let mut client = HueClient::new(
        HueRenderHandler::new(host.clone()),
        HueLifeSpanHandler::new(host.clone()),
        HueLoadHandler::new(host.clone()),
        HueDisplayHandler::new(host.clone()),
    );
    let mut extra = dictionary_value_create();
    if let (Some(dict), Some(js)) = (extra.as_mut(), host.inject_js.as_deref()) {
        dict.set_string(Some(&CefString::from(INJECT_KEY)), Some(&CefString::from(js)));
    }
    host.state.lock().create_requested_at = Some(Instant::now());
    let ok = browser_host_create_browser(
        Some(&window_info),
        Some(&mut client),
        Some(&CefString::from(host.url.as_str())),
        Some(&settings),
        extra.as_mut(),
        None,
    );
    if ok != 1 {
        host.state.lock().load_error = Some("browser_host_create_browser returned 0".into());
        quit_message_loop();
        return;
    }
    if !host.args.serve {
        let mut deadline = DeadlineTask::new(host.clone());
        let ms = i64::try_from(host.args.deadline().as_millis()).unwrap_or(i64::MAX);
        post_delayed_task(ThreadId::UI, Some(&mut deadline), ms);
    }
}

/// Serve mode: read control commands from stdin until it closes; each runs on the UI thread.
fn spawn_control_reader(host: Arc<HostCtx>, meta: Arc<EffectMeta>) {
    let spawned = std::thread::Builder::new()
        .name("huedeck-control".into())
        .spawn(move || {
            use std::io::BufRead;
            let stdin = std::io::stdin();
            let mut reader = stdin.lock();
            let mut line = String::new();
            loop {
                line.clear();
                let cmd = match reader.read_line(&mut line) {
                    // End of input: the daemon is gone.
                    Ok(0) | Err(_) => Command::Quit,
                    Ok(_) => match control::parse_line(&line) {
                        Ok(Some(cmd)) => cmd,
                        Ok(None) => continue,
                        Err(e) => {
                            eprintln!("huedeck-renderer: {e}");
                            continue;
                        }
                    },
                };
                let quit = cmd == Command::Quit;
                let mut task = ControlTask::new(host.clone(), meta.clone(), cmd);
                post_task(ThreadId::UI, Some(&mut task));
                if quit {
                    break;
                }
            }
        });
    if let Err(e) = spawned {
        eprintln!("huedeck-renderer: control thread not started ({e}); stop by closing the process");
    }
}

/// Run one control command (UI thread).
fn run_command(host: &HostCtx, meta: &EffectMeta, cmd: &Command) {
    let browser = host.state.lock().browser.clone();
    let exec = |js: &str| {
        if let Some(frame) = browser.as_ref().and_then(|b| b.main_frame()) {
            frame.execute_java_script(Some(&CefString::from(js)), Some(&CefString::from(CONTROL_URL)), 1);
        }
    };
    match cmd {
        Command::Quit => request_close(host, "quit"),
        Command::Fps { value } => {
            if let Some(bh) = browser.as_ref().and_then(|b| b.host()) {
                bh.set_windowless_frame_rate(*value as c_int);
            }
        }
        Command::Set { values } => {
            let (js, skipped) = control::set_script(meta, values);
            if !skipped.is_empty() {
                eprintln!("huedeck-renderer: ignored control values: {}", skipped.join(", "));
            }
            exec(&js);
        }
        Command::Event { sender, event } => exec(&control::event_script(sender, event)),
        Command::Tap { x, y } => exec(&control::tap_script(*x, *y)),
    }
}

wrap_task! {
    struct ControlTask {
        host: Arc<HostCtx>,
        meta: Arc<EffectMeta>,
        cmd: Command,
    }

    impl Task {
        fn execute(&self) {
            run_command(&self.host, &self.meta, &self.cmd);
        }
    }
}

wrap_task! {
    struct DeadlineTask {
        host: Arc<HostCtx>,
    }

    impl Task {
        fn execute(&self) {
            if !self.host.sink.lock().is_done() {
                self.host.state.lock().timed_out = true;
                request_close(&self.host, "timeout");
            }
        }
    }
}

wrap_client! {
    struct HueClient {
        render: RenderHandler,
        life_span: LifeSpanHandler,
        load: LoadHandler,
        display: DisplayHandler,
    }

    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(self.render.clone())
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(self.life_span.clone())
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            Some(self.load.clone())
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(self.display.clone())
        }
    }
}

wrap_render_handler! {
    struct HueRenderHandler {
        host: Arc<HostCtx>,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(rect) = rect {
                rect.x = 0;
                rect.y = 0;
                rect.width = CANVAS_WIDTH as c_int;
                rect.height = CANVAS_HEIGHT as c_int;
            }
        }

        fn screen_info(&self, _browser: Option<&mut Browser>, screen_info: Option<&mut ScreenInfo>) -> c_int {
            // Device scale factor 1 so OnPaint buffers are exactly 320x200 on HiDPI desktops.
            let Some(info) = screen_info else {
                return 0;
            };
            let full = Rect {
                x: 0,
                y: 0,
                width: CANVAS_WIDTH as c_int,
                height: CANVAS_HEIGHT as c_int,
            };
            info.device_scale_factor = 1.0;
            info.depth = 32;
            info.depth_per_component = 8;
            info.is_monochrome = 0;
            info.rect = full.clone();
            info.available_rect = full;
            1
        }

        fn on_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: c_int,
            height: c_int,
        ) {
            let at = Instant::now();
            if type_ != PaintElementType::VIEW || buffer.is_null() || width <= 0 || height <= 0 {
                return;
            }
            let len = width as usize * height as usize * 4;
            // SAFETY: CEF documents the OnPaint buffer as width*height*4 bytes of BGRA, valid
            // for the duration of the callback; it is only read here and copied into the ring.
            let pixels = unsafe { std::slice::from_raw_parts(buffer, len) };
            let outcome = self.host.sink.lock().on_paint(pixels, width as u32, height as u32, at);
            if outcome == PaintOutcome::Done {
                request_close(&self.host, "frames-complete");
            }
        }
    }
}

wrap_life_span_handler! {
    struct HueLifeSpanHandler {
        host: Arc<HostCtx>,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            let close_now = {
                let mut st = self.host.state.lock();
                st.browser = browser.cloned();
                st.after_created_at = Some(Instant::now());
                st.close_requested
            };
            if close_now {
                if let Some(bh) = self.host.state.lock().browser.clone().and_then(|b| b.host()) {
                    bh.close_browser(1);
                }
            }
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            // Drop our reference before shutdown, then leave run_message_loop.
            self.host.state.lock().browser = None;
            quit_message_loop();
        }
    }
}

wrap_load_handler! {
    struct HueLoadHandler {
        host: Arc<HostCtx>,
    }

    impl LoadHandler {
        fn on_load_end(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, http_status_code: c_int) {
            if frame.map(|f| f.is_main() != 0).unwrap_or(false) {
                self.host.state.lock().load_end = Some((Instant::now(), http_status_code));
            }
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            error_code: Errorcode,
            error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            if error_code == Errorcode::ABORTED || !frame.map(|f| f.is_main() != 0).unwrap_or(false) {
                return;
            }
            let text = error_text.map(|t| t.to_string()).unwrap_or_default();
            let url = failed_url.map(|u| u.to_string()).unwrap_or_default();
            self.host.state.lock().load_error = Some(format!("{error_code:?} {text} ({url})"));
            request_close(&self.host, "load-error");
        }
    }
}

wrap_display_handler! {
    struct HueDisplayHandler {
        host: Arc<HostCtx>,
    }

    impl DisplayHandler {
        fn on_console_message(
            &self,
            _browser: Option<&mut Browser>,
            level: LogSeverity,
            message: Option<&CefString>,
            source: Option<&CefString>,
            line: c_int,
        ) -> c_int {
            let msg = message.map(|m| m.to_string()).unwrap_or_default();
            let src = source.map(|s| s.to_string()).unwrap_or_default();
            let entry = format!("[{}] {src}:{line} {msg}", severity_name(level));
            if self.host.args.verbose {
                eprintln!("console {entry}");
            }
            let mut st = self.host.state.lock();
            st.console_total += 1;
            if level.get_raw() >= LogSeverity::ERROR.get_raw() {
                st.console_errors += 1;
            }
            if st.console.len() < CONSOLE_KEEP {
                st.console.push(entry);
            }
            0
        }
    }
}

wrap_render_process_handler! {
    struct HueRenderProcessHandler {
        inject: Arc<Mutex<Option<String>>>,
    }

    impl RenderProcessHandler {
        fn on_browser_created(&self, _browser: Option<&mut Browser>, extra_info: Option<&mut DictionaryValue>) {
            let Some(dict) = extra_info else {
                return;
            };
            let key = CefString::from(INJECT_KEY);
            if dict.has_key(Some(&key)) != 0 {
                let js = CefString::from(&dict.string(Some(&key))).to_string();
                *self.inject.lock() = Some(js);
            }
        }

        fn on_context_created(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            context: Option<&mut V8Context>,
        ) {
            let (Some(frame), Some(context)) = (frame, context) else {
                return;
            };
            if frame.is_main() == 0 {
                return;
            }
            let Some(js) = self.inject.lock().clone() else {
                return;
            };
            let mut retval = None;
            let mut exception = None;
            let ok = context.eval(
                Some(&CefString::from(js.as_str())),
                Some(&CefString::from(INJECT_URL)),
                1,
                Some(&mut retval),
                Some(&mut exception),
            );
            if ok == 0 {
                let msg = exception
                    .map(|e| CefString::from(&e.message()).to_string())
                    .unwrap_or_else(|| "unknown error".into());
                // Render-process stderr goes nowhere; a console error reaches the browser's
                // OnConsoleMessage and thus the JSON report (`console`, `console_errors`).
                let text = format!("huedeck: engine injection failed: {msg}");
                // A JSON string is a valid JS string literal.
                let quoted = serde_json::to_string(&text)
                    .unwrap_or_else(|_| "\"huedeck: engine injection failed\"".into());
                let mut retval = None;
                let mut exception = None;
                context.eval(
                    Some(&CefString::from(format!("console.error({quoted});").as_str())),
                    Some(&CefString::from(INJECT_URL)),
                    1,
                    Some(&mut retval),
                    Some(&mut exception),
                );
            }
        }
    }
}

#[derive(Serialize)]
struct ModeReport {
    cef_version: String,
    software_compositing: bool,
    disable_gpu: bool,
    sandbox: bool,
    /// DNS blocked (`--allow-network` not given).
    network_blocked: bool,
    message_loop: &'static str,
    chromium_switches: Vec<String>,
    injected: bool,
}

#[derive(Serialize)]
struct TimingReport {
    cef_initialize_ms: f64,
    browser_created_ms: Option<f64>,
    load_end_ms: Option<f64>,
    first_paint_ms: Option<f64>,
    run_total_ms: f64,
}

#[derive(Serialize)]
struct ShmReport {
    path: String,
    bytes: usize,
    width: u32,
    height: u32,
    slots: usize,
}

#[derive(Serialize)]
struct RunReport {
    ok: bool,
    effect: String,
    title: String,
    url: String,
    mode: ModeReport,
    timing: TimingReport,
    #[serde(flatten)]
    sink: SinkReport,
    shm: ShmReport,
    cef_cache_dir: String,
    /// What happened to the cache dir before the run (created / adopted / wiped [+ stale]).
    cef_cache_prepared: String,
    cef_cache_removed: bool,
    timed_out: bool,
    close_reason: Option<&'static str>,
    load_http_status: Option<i32>,
    load_error: Option<String>,
    skipped_controls: Vec<String>,
    console_messages: u64,
    console_errors: u64,
    console: Vec<String>,
    errors: Vec<String>,
}

fn ms_since(start: Instant, t: Option<Instant>) -> Option<f64> {
    t.map(|t| (t.saturating_duration_since(start).as_secs_f64() * 1_000_000.0).round() / 1000.0)
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

fn fail(msg: impl std::fmt::Display) -> u8 {
    eprintln!("huedeck-renderer: {msg}");
    2
}

/// Browser process: render `args.effect` and report. Returns the process exit code: 0 all
/// frames in time, 1 too few frames / timeout / load error, 2 setup error.
///
/// `sandbox_info`: see [`run_subprocess_if_any`]; null runs Chromium unsandboxed.
pub fn run(args: Args, sandbox_info: *mut u8) -> u8 {
    let run_start = Instant::now();
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    let effect = match std::fs::canonicalize(&args.effect) {
        Ok(p) => p,
        Err(e) => return fail(format!("effect {}: {e}", args.effect.display())),
    };
    let html = match std::fs::read(&effect) {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(e) => return fail(format!("effect {}: {e}", effect.display())),
    };
    let meta = parse_effect_meta(&html);
    let (inject_js, skipped_controls) = injection_script(&meta, &args.prop_values());
    let url = file_url(&effect);

    let ring = match RingWriter::create_file(&args.out, CANVAS_WIDTH, CANVAS_HEIGHT) {
        Ok(r) => r,
        Err(e) => return fail(format!("shm {}: {e}", args.out.display())),
    };
    let sink = FrameSink::new(
        ring,
        SinkConfig {
            frames: args.frames,
            fps: args.fps,
            snapshot_frames: args.snapshot_frames.clone(),
            raw_intervals: args.raw_intervals,
            serve: args.serve,
        },
    );

    // Chromium user-data dir, one per --out ring: a dir shared by two live renderers would make
    // the second hand its command line to the first (process singleton) and exit. Every run
    // starts from an empty profile; only a dir carrying our marker is ever wiped (cache.rs).
    let root_cache = args
        .cef_cache
        .clone()
        .unwrap_or_else(|| with_suffix(&args.out, ".cef-cache"));
    let cache_prepared = match cache::prepare(&root_cache) {
        Ok(cache::Prepared::Created) => "created".to_string(),
        Ok(cache::Prepared::Adopted) => "adopted (was empty)".to_string(),
        Ok(cache::Prepared::Wiped { stale }) if stale.is_empty() => "wiped".to_string(),
        Ok(cache::Prepared::Wiped { stale }) => {
            format!("wiped, {} entries still locked: {stale:?}", stale.len())
        }
        Err(e) => return fail(format!("cef cache: {e}")),
    };
    let cef_log = args
        .cef_log
        .clone()
        .unwrap_or_else(|| with_suffix(&args.out, ".cef.log"));

    let host = Arc::new(HostCtx {
        url: url.clone(),
        inject_js: (!args.no_inject).then_some(inject_js),
        sink: Mutex::new(sink),
        state: Mutex::new(RunState::default()),
        args,
    });

    let cef_args = cef::args::Args::new();
    let mut app = HueApp::new(Some(host.clone()), Arc::new(Mutex::new(None)));
    let code = execute_process(Some(cef_args.as_main_args()), Some(&mut app), sandbox_info);
    if code >= 0 {
        // Not expected without --type, but honour CEF's verdict.
        return code.clamp(0, 255) as u8;
    }
    let sandboxed = !sandbox_info.is_null();

    let settings = Settings {
        // Chromium sandbox: on Windows CEF only supports it through bootstrap(c).exe + a client
        // DLL, which passes `sandbox_info` (see README); the plain exe runs unsandboxed.
        no_sandbox: c_int::from(!sandboxed),
        windowless_rendering_enabled: 1,
        // Our own CLI flags must not be read as Chromium switches / startup URLs.
        command_line_args_disabled: 1,
        root_cache_path: CefString::from(root_cache.to_string_lossy().as_ref()),
        log_file: CefString::from(cef_log.to_string_lossy().as_ref()),
        log_severity: LogSeverity::WARNING,
        background_color: 0xFF00_0000,
        locale: CefString::from("en-US"),
        ..Default::default()
    };
    let init_start = Instant::now();
    if initialize(
        Some(cef_args.as_main_args()),
        Some(&settings),
        Some(&mut app),
        sandbox_info,
    ) != 1
    {
        return fail(format!(
            "cef initialize failed (cef exit code {})",
            get_exit_code()
        ));
    }
    let cef_initialize_ms = (init_start.elapsed().as_secs_f64() * 1_000_000.0).round() / 1000.0;

    if host.args.serve {
        spawn_control_reader(host.clone(), Arc::new(meta.clone()));
    }
    run_message_loop();
    shutdown();
    // The browser process keeps the profile's LevelDB locks (Session Storage, shared_proto_db,
    // Site Characteristics Database) until it exits, also after CefShutdown, so this usually
    // removes all but those (measured: 20 retries over 2 s did not help). The marker survives
    // with them and the next run with this --out wipes the rest.
    let cache_removed = cache::remove(&root_cache, 1, std::time::Duration::ZERO);

    // ---- report ----
    let args = &host.args;
    let sink = host.sink.lock();
    let st = host.state.lock();
    let (sink_report, mut errors) = sink.finish(args.png.as_deref());
    if let Err(e) = sink.ring().flush() {
        errors.push(format!("shm flush: {e}"));
    }
    if let Some(e) = sink.last_error() {
        errors.push(format!("last ring error: {e}"));
    }
    let created = st.create_requested_at.unwrap_or(run_start);
    let geometry = FrameHeader::new(CANVAS_WIDTH, CANVAS_HEIGHT);
    // A serving renderer ends when told to; it succeeded if the page loaded and painted.
    let ok = if args.serve {
        sink.frames() > 0 && st.load_error.is_none()
    } else {
        sink.is_done() && !st.timed_out && st.load_error.is_none()
    };
    let report = RunReport {
        ok,
        effect: effect.display().to_string(),
        title: meta.title.clone(),
        url,
        mode: ModeReport {
            cef_version: String::from_utf8_lossy(
                sys::CEF_VERSION.strip_suffix(b"\0").unwrap_or(sys::CEF_VERSION),
            )
            .into_owned(),
            software_compositing: args.software_compositing,
            disable_gpu: args.disable_gpu,
            sandbox: sandboxed,
            network_blocked: !args.allow_network,
            message_loop: "cef::run_message_loop (main thread)",
            chromium_switches: args.chromium_switches(),
            injected: !args.no_inject,
        },
        timing: TimingReport {
            cef_initialize_ms,
            browser_created_ms: ms_since(created, st.after_created_at),
            load_end_ms: ms_since(created, st.load_end.map(|(t, _)| t)),
            first_paint_ms: ms_since(created, sink.first_paint()),
            run_total_ms: (run_start.elapsed().as_secs_f64() * 1_000_000.0).round() / 1000.0,
        },
        sink: sink_report,
        shm: ShmReport {
            path: args.out.display().to_string(),
            bytes: geometry.total_bytes(),
            width: CANVAS_WIDTH,
            height: CANVAS_HEIGHT,
            slots: SLOT_COUNT,
        },
        cef_cache_dir: root_cache.display().to_string(),
        cef_cache_prepared: cache_prepared,
        cef_cache_removed: cache_removed,
        timed_out: st.timed_out,
        close_reason: st.close_reason,
        load_http_status: st.load_end.map(|(_, s)| s),
        load_error: st.load_error.clone(),
        skipped_controls,
        console_messages: st.console_total,
        console_errors: st.console_errors,
        console: st.console.clone(),
        errors,
    };
    let json = serde_json::to_string_pretty(&report).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"));
    println!("{json}");
    if let Some(path) = &args.stats {
        if let Err(e) = std::fs::write(path, format!("{json}\n")) {
            eprintln!("huedeck-renderer: stats {}: {e}", path.display());
        }
    }
    u8::from(!ok)
}
