// SPDX-License-Identifier: GPL-3.0-or-later
//! The macro runtime: script folders, the user's configuration, one worker
//! thread (with its own QuickJS runtime) per running script, the event bus
//! and the opt-in input hooks.
//!
//! * Scripts live in `<config>/macroscripts/{Actions,Inputs}` (and SignalRGB's
//!   bundled `Macroscripts` folder while its library is on). The folders are
//!   watched (polled every [`WATCH_EVERY`]); a change lists them again and
//!   restarts the running scripts.
//! * Nothing runs while macros are off (the default). With macros on, every
//!   enabled Input runs; an Action starts on its first use (the dashboard's
//!   Run button, or an Input's `block.trigger()` running the Input's chain of
//!   Actions) and keeps running.
//! * Events reach Inputs on a bus: `onApplication` (effect and layout
//!   changes, lock, sleep and resume from the engine; idle time and process
//!   start/end polled here), `onKey` / `onMouse` from the low-level hooks and
//!   from device plugins (`keyboard.sendEvent`, `mouse.sendEvent`). A hook
//!   event waits at most [`HOOK_WAIT`] for the scripts' answer to swallow it.
//! * The hooks are installed only while macros are on, the user allowed the
//!   hook ([`MacroConfig::keyboard_hook`], [`MacroConfig::mouse_hook`], off by
//!   default) and an enabled Input has the handler. Key events are processed
//!   in memory only: nothing here logs a key, and the console output and
//!   error messages of `onKey` / `onMouse` are not logged either (they may
//!   hold typed text).
//! * [`EMERGENCY_STOP`] (seen by the keyboard hook) turns macros off.

use crate::host::{signalrgb, EngineCall, MacroHost, Prompt, SignalRgbState};
use crate::keys::{self, Modifiers};
use crate::options::{self, OptionSpec};
use crate::script::{self, Described, NativeError, NativeResult, Natives, ScriptRuntime};
use lightloom_hal::input::{Desktop, HookEvent, InputHook, SynthInput};
use lightloom_plugin_host::input::{InputSink, PluginInput};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

/// How long a hook event waits for the scripts' answer (Windows removes a
/// low-level hook that answers too slowly).
pub const HOOK_WAIT: Duration = Duration::from_millis(40);
/// How often the script folders are looked at.
pub const WATCH_EVERY: Duration = Duration::from_secs(2);
/// How often `System Idle Time` is sent.
pub const IDLE_EVERY: Duration = Duration::from_secs(1);
/// How often running processes are compared.
pub const PROCESSES_EVERY: Duration = Duration::from_secs(2);
/// The key combination that turns macros off.
pub const EMERGENCY_STOP: &str = "Ctrl + Alt + Shift + F12";
/// Events queued per script before new ones are dropped.
pub const QUEUE: usize = 64;
/// Most scripts listed per folder.
pub const MAX_SCRIPTS: usize = 200;
/// Largest script file.
pub const MAX_FILE: u64 = 1024 * 1024;
/// Most Actions in an Input's chain.
pub const MAX_CHAIN: usize = 16;
/// Most prompts waiting for an answer.
pub const MAX_PROMPTS: usize = 16;
/// URL schemes `system.openUrl` opens besides `http(s)` and `mailto`: chat and
/// media apps the bundled scripts open (`discord://`, `slack://`, ...).
pub const APP_SCHEMES: &[&str] = &["discord", "slack", "spotify", "steam", "msteams", "zoommtg"];

/// Action or Input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// `export function Action()`: `onExecute()`.
    Action,
    /// `export function Input()`: event handlers, `block.trigger()`.
    Input,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Action => "action",
            Kind::Input => "input",
        }
    }
}

/// A folder of scripts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ScriptDir {
    /// `user` or `signalrgb`.
    pub source: String,
    /// What it holds.
    pub kind: Kind,
    /// Where.
    pub path: PathBuf,
}

impl ScriptDir {
    /// `<root>/Actions` and `<root>/Inputs` of `source`.
    pub fn pair(source: &str, root: &Path) -> [ScriptDir; 2] {
        [
            ScriptDir {
                source: source.into(),
                kind: Kind::Action,
                path: root.join("Actions"),
            },
            ScriptDir {
                source: source.into(),
                kind: Kind::Input,
                path: root.join("Inputs"),
            },
        ]
    }
}

/// The user's macro settings (persisted by the daemon).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MacroConfig {
    /// Macros run (off by default).
    pub enabled: bool,
    /// The low-level keyboard hook may be installed (off by default).
    pub keyboard_hook: bool,
    /// The low-level mouse hook may be installed (off by default).
    pub mouse_hook: bool,
    /// `system.openApplication` may start programs (off by default).
    pub allow_launch: bool,
    /// Per script.
    pub scripts: BTreeMap<String, ScriptConfig>,
}

/// One script's settings.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScriptConfig {
    /// An Input runs (Actions run when used).
    pub enabled: bool,
    /// Option values by property.
    pub options: Map<String, Value>,
    /// An Input's chain: the Actions `block.trigger()` runs, in order.
    pub actions: Vec<String>,
}

/// A listed script.
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptInfo {
    /// `<source>:<kind>:<file stem>`.
    pub id: String,
    /// Action or Input.
    pub kind: Kind,
    /// `user` or `signalrgb`.
    pub source: String,
    /// The file.
    pub path: PathBuf,
    /// `this.Name` (the file stem when missing).
    pub name: String,
    /// `this.Description`.
    pub description: String,
    /// `this.Options`.
    pub options: Vec<OptionSpec>,
    /// Handler functions.
    pub handlers: Vec<String>,
    /// Why it cannot run.
    pub error: Option<String>,
    /// Option descriptors that were ignored.
    pub warnings: Vec<String>,
}

/// A refused macro request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MacroError {
    /// No such script or prompt.
    NotFound(String),
    /// A bad value.
    Invalid(String),
    /// Not now (macros are off, the script cannot run).
    Conflict(String),
}

impl std::fmt::Display for MacroError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MacroError::NotFound(m) | MacroError::Invalid(m) | MacroError::Conflict(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for MacroError {}

// ------------------------------------------------------------------ views

/// One option with its current value.
#[derive(Clone, Debug, Serialize)]
pub struct OptionView {
    /// The descriptor.
    #[serde(flatten)]
    pub spec: OptionSpec,
    /// The value the script gets.
    pub value: Value,
}

/// One script in [`MacrosView`].
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptView {
    /// Id.
    pub id: String,
    /// Action or Input.
    pub kind: Kind,
    /// `user` or `signalrgb`.
    pub source: String,
    /// File name.
    pub file: String,
    /// Name.
    pub name: String,
    /// Description.
    pub description: String,
    /// Options.
    pub options: Vec<OptionView>,
    /// Handler functions.
    pub handlers: Vec<String>,
    /// An Input is enabled.
    pub enabled: bool,
    /// An Input's chain of Actions.
    pub actions: Vec<String>,
    /// Its thread runs.
    pub running: bool,
    /// Why it cannot run (or stopped).
    pub error: Option<String>,
    /// Ignored options.
    pub warnings: Vec<String>,
}

/// Which hooks are installed.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct HooksView {
    /// Keyboard.
    pub keyboard: bool,
    /// Mouse.
    pub mouse: bool,
}

/// `GET /v1/macros`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacrosView {
    /// Macros run.
    pub enabled: bool,
    /// The keyboard hook is allowed.
    pub keyboard_hook: bool,
    /// The mouse hook is allowed.
    pub mouse_hook: bool,
    /// Macros may start programs.
    pub allow_launch: bool,
    /// This system has low-level hooks.
    pub hooks_available: bool,
    /// Installed now.
    pub hooks: HooksView,
    /// Why a wanted hook is not installed.
    pub hook_error: Option<String>,
    /// [`EMERGENCY_STOP`].
    pub emergency_stop: &'static str,
    /// Script folders.
    pub folders: Vec<ScriptDir>,
    /// Every script.
    pub scripts: Vec<ScriptView>,
}

// ------------------------------------------------------------------ core

enum Cmd {
    Execute,
    Handle {
        handler: &'static str,
        event: Value,
        reply: Option<SyncSender<bool>>,
    },
    Options(Map<String, Value>),
    Answer {
        id: u64,
        text: Option<String>,
    },
}

#[derive(Clone)]
struct Link {
    kind: Kind,
    handlers: Vec<String>,
    tx: SyncSender<Cmd>,
}

/// State shared by the manager, the workers, the hook thread and plugin threads.
struct Core {
    host: Arc<dyn MacroHost>,
    desktop: Arc<dyn Desktop>,
    enabled: AtomicBool,
    allow_launch: AtomicBool,
    links: Mutex<BTreeMap<String, Link>>,
    prompts: Mutex<BTreeMap<u64, String>>,
    next_prompt: AtomicU64,
    triggers: Mutex<Vec<String>>,
    modifiers: Mutex<Modifiers>,
    srgb: SignalRgbState,
    emergency: AtomicBool,
    dropped: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Core {
    /// Send `event` to every running Input with `handler`; with `wait`, collect
    /// their answers for that long and return whether one asked to swallow it.
    fn deliver(&self, handler: &'static str, event: &Value, wait: Option<Duration>) -> bool {
        if !self.enabled.load(Ordering::Acquire) {
            return false;
        }
        let targets: Vec<SyncSender<Cmd>> = lock(&self.links)
            .values()
            .filter(|l| l.kind == Kind::Input && l.handlers.iter().any(|h| h == handler))
            .map(|l| l.tx.clone())
            .collect();
        let mut replies: Vec<Receiver<bool>> = Vec::new();
        for tx in targets {
            let (reply, rx) = match wait {
                Some(_) => {
                    let (t, r) = sync_channel(1);
                    (Some(t), Some(r))
                }
                None => (None, None),
            };
            match tx.try_send(Cmd::Handle {
                handler,
                event: event.clone(),
                reply,
            }) {
                Ok(()) => replies.extend(rx),
                Err(TrySendError::Full(_)) => {
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                }
                Err(TrySendError::Disconnected(_)) => {}
            }
        }
        let Some(wait) = wait else {
            return false;
        };
        let end = Instant::now() + wait;
        let mut swallow = false;
        for rx in replies {
            let left = end.saturating_duration_since(Instant::now());
            if rx.recv_timeout(left).unwrap_or(false) {
                swallow = true;
            }
        }
        swallow
    }

    /// A low-level hook event; `true` swallows it.
    fn hook(&self, e: &HookEvent) -> bool {
        match *e {
            HookEvent::Key {
                vk,
                released,
                extended,
                injected,
            } => {
                let mut m = lock(&self.modifiers);
                m.track(vk, released);
                if !released && vk == keys::vk::F12 && m.control && m.alt && m.shift {
                    drop(m);
                    self.enabled.store(false, Ordering::Release);
                    self.emergency.store(true, Ordering::Release);
                    return false;
                }
                let name = keys::name_of(vk, extended).unwrap_or("");
                let data = json!({
                    "key": name,
                    "keyCode": vk,
                    "released": released,
                    "shift": m.shift,
                    "control": m.control,
                    "alt": m.alt,
                    "win": m.win,
                    "keyString": m.key_string(vk, name),
                    "injected": injected,
                });
                drop(m);
                self.deliver(
                    "onKey",
                    &envelope("Key", "Key Press", "SignalRGB", "Input", data),
                    Some(HOOK_WAIT),
                )
            }
            HookEvent::Button {
                button,
                released,
                injected,
            } => {
                let (name, code) = keys::button_event(button, released);
                self.deliver(
                    "onMouse",
                    &mouse_event(name, code, released, injected),
                    Some(HOOK_WAIT),
                )
            }
            HookEvent::Wheel {
                horizontal,
                notches,
                injected,
            } => {
                let (name, code) = keys::wheel_event(horizontal, notches);
                self.deliver(
                    "onMouse",
                    &mouse_event(name, code, false, injected),
                    Some(HOOK_WAIT),
                )
            }
        }
    }

    /// An event of a device plugin's `keyboard` / `mouse` feature.
    fn plugin(&self, input: PluginInput) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }
        match input {
            PluginInput::Key {
                key,
                key_code,
                released,
            } => {
                let m = *lock(&self.modifiers);
                let data = json!({
                    "key": key,
                    "keyCode": key_code,
                    "released": released,
                    "shift": m.shift,
                    "control": m.control,
                    "alt": m.alt,
                    "win": m.win,
                    "keyString": m.key_string(u16::try_from(key_code).unwrap_or(0), &key),
                    "injected": true,
                });
                self.deliver(
                    "onKey",
                    &envelope("Key", "Key Press", "SignalRGB", "Input", data),
                    None,
                );
            }
            PluginInput::Button {
                name,
                button_code,
                released,
            } => {
                self.deliver("onMouse", &mouse_event(&name, button_code, released, true), None);
            }
            PluginInput::KeyHid { vk, released } => {
                let _ = self.desktop.send(&[SynthInput::Key {
                    vk,
                    down: !released,
                    extended: keys::extended(vk),
                }]);
            }
            PluginInput::ButtonHid {
                vk,
                released,
                xbutton,
            } => {
                if let Some(button) = keys::hid_button(vk, xbutton) {
                    let _ = self.desktop.send(&[SynthInput::Button {
                        button,
                        down: !released,
                    }]);
                }
            }
        }
    }
}

/// The envelope every handler gets (docs/host-api/macros-system.md, critic
/// pass: `EventHandler`, `Sender`, `SenderType`, `EventType`, the payload).
/// The script host adds `Data` as a second name of `data`.
pub fn envelope(handler: &str, event_type: &str, sender: &str, sender_type: &str, data: Value) -> Value {
    json!({
        "EventHandler": handler,
        "EventType": event_type,
        "Sender": sender,
        "SenderType": sender_type,
        "data": data,
    })
}

fn mouse_event(button: &str, code: i32, released: bool, injected: bool) -> Value {
    envelope(
        "Mouse",
        "Mouse",
        "SignalRGB",
        "Input",
        json!({ "button": button, "buttonCode": code, "released": released, "injected": injected }),
    )
}

// ------------------------------------------------------------------ workers

/// The natives of one running script.
struct WorkerNatives {
    core: Arc<Core>,
    id: String,
    name: String,
    kind: Kind,
    /// Inside `onKey` / `onMouse`: console lines are not logged.
    private: AtomicBool,
    hidden_noted: AtomicBool,
}

/// Split one command-line string into arguments (double quotes group, `\"` is a quote).
pub fn split_args(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
                any = true;
            }
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(cur);
    }
    out
}

fn has_control(s: &str) -> bool {
    s.chars().any(char::is_control)
}

/// Whether `system.openUrl` may open `url`: `Ok(None)` to open it, `Ok(Some(path))`
/// for a `lightloom:` action, `Err` when refused.
pub fn url_policy(url: &str) -> Result<Option<String>, NativeError> {
    let url = url.trim();
    if url.is_empty() || url.len() > 2048 || has_control(url) {
        return Err(NativeError::type_error("openUrl(): not a URL"));
    }
    let scheme = url
        .split_once(':')
        .map(|(s, _)| s.to_ascii_lowercase())
        .unwrap_or_default();
    match scheme.as_str() {
        "http" | "https" | "mailto" => Ok(None),
        "lightloom" => Ok(Some(url.to_string())),
        s if APP_SCHEMES.contains(&s) => Ok(None),
        _ => Err(NativeError::refused(format!(
            "openUrl(): {} links are not opened by macros (http, https, mailto and {} are)",
            if scheme.is_empty() {
                "these".to_string()
            } else {
                format!("{scheme}:")
            },
            APP_SCHEMES
                .iter()
                .map(|s| format!("{s}:"))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

impl WorkerNatives {
    fn send(&self, inputs: &[SynthInput]) -> NativeResult {
        self.core
            .desktop
            .send(inputs)
            .map(|_| Value::Null)
            .map_err(|e| NativeError::failed(e.to_string()))
    }

    fn log_line(&self, line: &str) {
        self.core.host.log(&self.name, line);
    }
}

fn key_arg(arg: &Value) -> Result<Vec<keys::Key>, NativeError> {
    match arg {
        Value::String(s) => keys::combo(s).map_err(|e| NativeError::type_error(format!("(): {e}"))),
        Value::Number(n) => keys::code(n.as_f64().unwrap_or(-1.0))
            .map(|k| vec![k])
            .map_err(|e| NativeError::type_error(format!("(): {e}"))),
        _ => Err(NativeError::type_error(
            "(): Key Parameter must be a key name, or a virtual key code!",
        )),
    }
}

impl Natives for WorkerNatives {
    fn action(&self, path: &str) -> NativeResult {
        // SignalRGB's own scheme is accepted for the paths both routers share.
        let path = path.trim();
        let path = path
            .strip_prefix("signalrgb://")
            .map(str::to_string)
            .unwrap_or_else(|| path.to_string());
        if let Err(e) = self.core.host.call(EngineCall::Action(path)) {
            self.log_line(&format!("action(): {e}"));
        }
        Ok(Value::Null)
    }

    fn open_url(&self, url: &str) -> NativeResult {
        match url_policy(url)? {
            Some(action) => self.action(&action),
            None => self
                .core
                .desktop
                .open(url.trim())
                .map(|_| Value::Null)
                .map_err(|e| NativeError::failed(format!("openUrl(): {e}"))),
        }
    }

    fn open_application(&self, path: &str, args: &Value, cwd: &str) -> NativeResult {
        if !self.core.allow_launch.load(Ordering::Acquire) {
            return Err(NativeError::refused(
                "openApplication(): macros may not start programs (turn on \"Let macros start programs\" in the Macros page)",
            ));
        }
        let path = path.trim();
        if path.is_empty() || has_control(path) || has_control(cwd) {
            return Err(NativeError::type_error(
                "openApplication(): Parameter must be a string!",
            ));
        }
        let args: Vec<String> = match args {
            Value::Array(a) => a
                .iter()
                .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string))
                .collect(),
            Value::String(s) => split_args(s),
            _ => Vec::new(),
        };
        let cwd = (!cwd.trim().is_empty()).then(|| cwd.trim());
        self.log_line(&format!("starting {path}"));
        self.core
            .desktop
            .launch(path, &args, cwd)
            .map(|_| Value::Null)
            .map_err(|e| NativeError::failed(format!("openApplication(): {e}")))
    }

    fn prompt(&self, options: &Value) -> NativeResult {
        let field = |k: &str| -> String {
            options
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or("")
                .chars()
                .take(500)
                .collect()
        };
        let id = self.core.next_prompt.fetch_add(1, Ordering::Relaxed) + 1;
        let prompt = Prompt {
            id,
            script: self.name.clone(),
            title: field("title"),
            message: field("message"),
            placeholder: field("placeholder"),
            confirm_text: field("confirmText"),
            cancel_text: field("cancelText"),
        };
        {
            let mut p = lock(&self.core.prompts);
            if p.len() >= MAX_PROMPTS {
                return Ok(Value::from(-1));
            }
            p.insert(id, self.id.clone());
        }
        if self.core.host.show_prompt(&prompt) {
            Ok(Value::from(id))
        } else {
            lock(&self.core.prompts).remove(&id);
            Ok(Value::from(-1))
        }
    }

    fn clipboard(&self, text: &str) -> NativeResult {
        self.core
            .desktop
            .set_clipboard(text)
            .map(|_| Value::Null)
            .map_err(|e| NativeError::failed(format!("copyToClipboard(): {e}")))
    }

    fn signalrgb(&self, method: &str, args: &[Value]) -> NativeResult {
        let log = |l: &str| self.log_line(l);
        signalrgb(&*self.core.host, &self.core.srgb, method, args, &log)
    }

    fn keys(&self, op: &str, arg: &Value) -> NativeResult {
        let text = || arg.as_str().unwrap_or_default().to_string();
        match op {
            "press" => self.send(&keys::press(&key_arg(arg)?)),
            "hold" => self.send(&keys::hold(&key_arg(arg)?)),
            "release" => self.send(&keys::release(&key_arg(arg)?)),
            "paste" => {
                self.clipboard(&text())?;
                self.send(&keys::paste())
            }
            "unicode" => self.send(&keys::unicode(&text())),
            _ => Err(NativeError::type_error(format!("keys.{op} is not a function"))),
        }
    }

    fn click(&self, arg: &Value) -> NativeResult {
        let name = match arg {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => String::new(),
        };
        let c = keys::click(&name).map_err(|e| NativeError::type_error(format!("click(): {e}")))?;
        self.send(&keys::click_input(c))
    }

    fn trigger(&self) -> NativeResult {
        if self.kind != Kind::Input {
            self.log_line("block.trigger() does nothing in an Action");
            return Ok(Value::Null);
        }
        lock(&self.core.triggers).push(self.id.clone());
        Ok(Value::Null)
    }

    fn log(&self, level: &str, text: &str) {
        if self.private.load(Ordering::Acquire) {
            if !self.hidden_noted.swap(true, Ordering::AcqRel) {
                self.log_line(
                    "console output inside onKey / onMouse is not logged (it could hold typed text)",
                );
            }
            return;
        }
        let text: String = text.chars().take(500).collect();
        match level {
            "log" | "info" | "debug" | "trace" => self.log_line(&text),
            _ => self.log_line(&format!("{level}: {text}")),
        }
    }
}

struct WorkerThread {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    failed: Arc<Mutex<Option<String>>>,
}

impl WorkerThread {
    fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Stop and wait a moment; a thread stuck in a call is left to its deadline.
    fn shutdown(mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = self.thread.take() {
            let end = Instant::now() + Duration::from_millis(500);
            while !t.is_finished() && Instant::now() < end {
                std::thread::sleep(Duration::from_millis(5));
            }
            if t.is_finished() {
                let _ = t.join();
            }
        }
    }
}

fn worker_main(
    path: PathBuf,
    natives: Arc<WorkerNatives>,
    rx: Receiver<Cmd>,
    stop: Arc<AtomicBool>,
    options: Map<String, Value>,
    failed: Arc<Mutex<Option<String>>>,
) {
    let fail = |what: &str, e: &dyn std::fmt::Display| {
        let line = format!("{what}: {e}");
        natives.log_line(&line);
        *lock(&failed) = Some(line);
    };
    let rt = match ScriptRuntime::load(&path, natives.clone()) {
        Ok((rt, _)) => rt,
        Err(e) => return fail("failed to start", &e),
    };
    if let Err(e) = rt.set_options(&options) {
        return fail("options", &e);
    }
    while !stop.load(Ordering::Acquire) {
        let cmd = match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(c) => c,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        match cmd {
            Cmd::Execute => {
                if let Err(e) = rt.execute() {
                    natives.log_line(&format!("onExecute failed: {e}"));
                }
            }
            Cmd::Options(values) => {
                if let Err(e) = rt.set_options(&values) {
                    natives.log_line(&format!("options: {e}"));
                }
            }
            Cmd::Answer { id, text } => {
                if let Err(e) = rt.answer(id, text.as_deref()) {
                    natives.log_line(&format!("prompt callback failed: {e}"));
                }
            }
            Cmd::Handle {
                handler,
                event,
                reply,
            } => {
                let private = handler == "onKey" || handler == "onMouse";
                natives.private.store(private, Ordering::Release);
                let r = rt.handle(handler, &event);
                natives.private.store(false, Ordering::Release);
                let swallow = match r {
                    Ok(s) => s,
                    Err(e) if private => {
                        // The message may quote the key: only the class.
                        natives.log_line(&format!("{handler} failed: {} (message not logged)", e.class));
                        false
                    }
                    Err(e) => {
                        natives.log_line(&format!("{handler} failed: {e}"));
                        false
                    }
                };
                if let Some(reply) = reply {
                    let _ = reply.try_send(swallow);
                }
            }
        }
    }
}

// ------------------------------------------------------------------ manager

/// The macro runtime (see the module docs). Owned by the engine thread.
pub struct MacroManager {
    core: Arc<Core>,
    dirs: Vec<ScriptDir>,
    scripts: Vec<ScriptInfo>,
    config: MacroConfig,
    threads: BTreeMap<String, WorkerThread>,
    errors: BTreeMap<String, String>,
    hook: Option<Box<dyn InputHook>>,
    hook_error: Option<String>,
    signature: Vec<(PathBuf, Option<SystemTime>, u64)>,
    last_watch: Instant,
    last_idle: Instant,
    last_processes: Instant,
    processes: Option<BTreeSet<String>>,
    changed: bool,
}

impl MacroManager {
    /// A manager over `host` and `desktop`, with this system's hooks (`None`:
    /// no hooks) and the saved `config`. Nothing is listed until [`MacroManager::set_dirs`].
    pub fn new(
        host: Arc<dyn MacroHost>,
        desktop: Arc<dyn Desktop>,
        hook: Option<Box<dyn InputHook>>,
        config: MacroConfig,
    ) -> Self {
        let core = Arc::new(Core {
            host,
            desktop,
            enabled: AtomicBool::new(config.enabled),
            allow_launch: AtomicBool::new(config.allow_launch),
            links: Mutex::default(),
            prompts: Mutex::default(),
            next_prompt: AtomicU64::new(0),
            triggers: Mutex::default(),
            modifiers: Mutex::default(),
            srgb: SignalRgbState::default(),
            emergency: AtomicBool::new(false),
            dropped: AtomicU64::new(0),
        });
        Self {
            core,
            dirs: Vec::new(),
            scripts: Vec::new(),
            config,
            threads: BTreeMap::new(),
            errors: BTreeMap::new(),
            hook,
            hook_error: None,
            signature: Vec::new(),
            last_watch: Instant::now(),
            last_idle: Instant::now(),
            last_processes: Instant::now(),
            processes: None,
            changed: false,
        }
    }

    /// Where device plugins' `keyboard` / `mouse` events go.
    pub fn input_sink(&self) -> InputSink {
        let core = Arc::clone(&self.core);
        Arc::new(move |_device: &str, input| core.plugin(input))
    }

    /// The settings to persist.
    pub fn config(&self) -> &MacroConfig {
        &self.config
    }

    /// Whether the settings changed since the last call (persist them).
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// The listed scripts.
    pub fn scripts(&self) -> &[ScriptInfo] {
        &self.scripts
    }

    /// Hook events for the tests' hook (what a real hook's sink does).
    pub fn hook_event(&self, e: &HookEvent) -> bool {
        self.core.hook(e)
    }

    /// Set the script folders, list them and start what should run.
    pub fn set_dirs(&mut self, dirs: Vec<ScriptDir>) {
        if dirs != self.dirs {
            self.dirs = dirs;
            self.rescan();
        }
    }

    /// The signature of the folders (file, modified, length).
    fn folder_signature(&self) -> Vec<(PathBuf, Option<SystemTime>, u64)> {
        let mut sig = Vec::new();
        for d in &self.dirs {
            sig.push((d.path.clone(), None, 0));
            let Ok(rd) = std::fs::read_dir(&d.path) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("js")) {
                    let meta = e.metadata().ok();
                    sig.push((
                        p,
                        meta.as_ref().and_then(|m| m.modified().ok()),
                        meta.map_or(0, |m| m.len()),
                    ));
                }
            }
        }
        sig.sort();
        sig
    }

    /// List the folders again and restart the running scripts.
    pub fn rescan(&mut self) {
        self.signature = self.folder_signature();
        self.last_watch = Instant::now();
        let mut scripts = Vec::new();
        for d in &self.dirs {
            let Ok(rd) = std::fs::read_dir(&d.path) else {
                continue;
            };
            let mut files: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("js")))
                .collect();
            files.sort();
            for path in files.into_iter().take(MAX_SCRIPTS) {
                scripts.push(inspect(d, &path));
            }
        }
        let count = scripts.len();
        self.scripts = scripts;
        self.errors.clear();
        self.stop_all();
        self.sync();
        tracing::debug!(scripts = count, "macro scripts listed");
    }

    /// Look at the folders now (normally every [`WATCH_EVERY`] from [`MacroManager::tick`]);
    /// `true` when they changed and were listed again.
    pub fn watch(&mut self) -> bool {
        self.last_watch = Instant::now();
        if self.folder_signature() != self.signature {
            self.core.host.log("", "macro scripts changed: listed again");
            self.rescan();
            return true;
        }
        false
    }

    fn info(&self, id: &str) -> Option<&ScriptInfo> {
        self.scripts.iter().find(|s| s.id == id)
    }

    fn script_config(&self, id: &str) -> ScriptConfig {
        self.config.scripts.get(id).cloned().unwrap_or_default()
    }

    fn options_of(&self, info: &ScriptInfo) -> Map<String, Value> {
        options::effective(&info.options, &self.script_config(&info.id).options)
    }

    fn start(&mut self, id: &str) -> Result<(), MacroError> {
        if self.threads.contains_key(id) {
            return Ok(());
        }
        let info = self
            .info(id)
            .ok_or_else(|| MacroError::NotFound(format!("no macro script {id}")))?
            .clone();
        if let Some(e) = &info.error {
            return Err(MacroError::Conflict(format!("{}: {e}", info.name)));
        }
        let (tx, rx) = sync_channel(QUEUE);
        let natives = Arc::new(WorkerNatives {
            core: Arc::clone(&self.core),
            id: info.id.clone(),
            name: info.name.clone(),
            kind: info.kind,
            private: AtomicBool::new(false),
            hidden_noted: AtomicBool::new(false),
        });
        let stop = Arc::new(AtomicBool::new(false));
        let failed: Arc<Mutex<Option<String>>> = Arc::default();
        let options = self.options_of(&info);
        let thread = {
            let (path, stop, failed) = (info.path.clone(), Arc::clone(&stop), Arc::clone(&failed));
            std::thread::Builder::new()
                .name("lightloom-macro".into())
                .stack_size(script::THREAD_STACK)
                .spawn(move || worker_main(path, natives, rx, stop, options, failed))
                .map_err(|e| MacroError::Conflict(format!("cannot start {}: {e}", info.name)))?
        };
        lock(&self.core.links).insert(
            info.id.clone(),
            Link {
                kind: info.kind,
                handlers: info.handlers.clone(),
                tx,
            },
        );
        self.threads.insert(
            info.id.clone(),
            WorkerThread {
                stop,
                thread: Some(thread),
                failed,
            },
        );
        self.errors.remove(id);
        Ok(())
    }

    fn stop(&mut self, id: &str) {
        lock(&self.core.links).remove(id);
        lock(&self.core.prompts).retain(|_, s| s != id);
        if let Some(t) = self.threads.remove(id) {
            t.shutdown();
        }
    }

    fn stop_all(&mut self) {
        let ids: Vec<String> = self.threads.keys().cloned().collect();
        for id in ids {
            self.stop(&id);
        }
    }

    /// Start the enabled Inputs (macros on), stop what should not run, and
    /// install or remove the hooks.
    fn sync(&mut self) {
        let on = self.config.enabled;
        self.core.enabled.store(on, Ordering::Release);
        self.core
            .allow_launch
            .store(self.config.allow_launch, Ordering::Release);
        if !on {
            self.stop_all();
        } else {
            let wanted: Vec<String> = self
                .scripts
                .iter()
                .filter(|s| s.kind == Kind::Input && s.error.is_none() && self.script_config(&s.id).enabled)
                .map(|s| s.id.clone())
                .collect();
            let running: Vec<String> = self.threads.keys().cloned().collect();
            for id in running {
                let keep = self
                    .info(&id)
                    .is_some_and(|s| s.kind == Kind::Action || wanted.contains(&id));
                if !keep {
                    self.stop(&id);
                }
            }
            for id in wanted {
                if !self.threads.contains_key(&id) && !self.errors.contains_key(&id) {
                    if let Err(e) = self.start(&id) {
                        self.errors.insert(id, e.to_string());
                    }
                }
            }
        }
        self.sync_hooks();
    }

    fn running_with(&self, handler: &str) -> bool {
        let links = lock(&self.core.links);
        links
            .values()
            .any(|l| l.kind == Kind::Input && l.handlers.iter().any(|h| h == handler))
    }

    fn sync_hooks(&mut self) {
        let keyboard = self.config.enabled && self.config.keyboard_hook && self.running_with("onKey");
        let mouse = self.config.enabled && self.config.mouse_hook && self.running_with("onMouse");
        let Some(hook) = self.hook.as_mut() else {
            self.hook_error =
                (keyboard || mouse).then(|| "this system has no keyboard and mouse hooks".into());
            return;
        };
        if hook.active() == (keyboard, mouse) {
            return;
        }
        if !keyboard && !mouse {
            hook.stop();
            self.core.host.log("", "input hooks removed");
            return;
        }
        let core = Arc::clone(&self.core);
        match hook.start(keyboard, mouse, Arc::new(move |e| core.hook(e))) {
            Ok(()) => {
                self.hook_error = None;
                let which = match (keyboard, mouse) {
                    (true, true) => "keyboard and mouse hooks",
                    (true, false) => "keyboard hook",
                    _ => "mouse hook",
                };
                self.core.host.log("", &format!("{which} installed for macros"));
            }
            Err(e) => {
                self.hook_error = Some(format!("the input hooks could not be installed: {e}"));
                self.core
                    .host
                    .log("", self.hook_error.as_deref().unwrap_or_default());
            }
        }
    }

    /// Change the settings; `None` leaves one as it is.
    pub fn update_settings(
        &mut self,
        enabled: Option<bool>,
        keyboard_hook: Option<bool>,
        mouse_hook: Option<bool>,
        allow_launch: Option<bool>,
    ) {
        let before = self.config.clone();
        if let Some(v) = enabled {
            self.config.enabled = v;
        }
        if let Some(v) = keyboard_hook {
            self.config.keyboard_hook = v;
        }
        if let Some(v) = mouse_hook {
            self.config.mouse_hook = v;
        }
        if let Some(v) = allow_launch {
            self.config.allow_launch = v;
        }
        if self.config != before {
            self.changed = true;
            if self.config.enabled != before.enabled {
                self.core.host.log(
                    "",
                    if self.config.enabled {
                        "macros on"
                    } else {
                        "macros off"
                    },
                );
                self.errors.clear();
            }
            self.sync();
        }
    }

    /// Change one script's settings.
    pub fn update_script(
        &mut self,
        id: &str,
        enabled: Option<bool>,
        values: Option<Map<String, Value>>,
        actions: Option<Vec<String>>,
    ) -> Result<(), MacroError> {
        let info = self
            .info(id)
            .ok_or_else(|| MacroError::NotFound(format!("no macro script {id}")))?
            .clone();
        let mut cfg = self.script_config(id);
        if let Some(values) = values {
            for (k, v) in values {
                let spec = info
                    .options
                    .iter()
                    .find(|o| o.property == k)
                    .ok_or_else(|| MacroError::Invalid(format!("{} has no option {k}", info.name)))?;
                let v = options::coerce(spec, &v).map_err(MacroError::Invalid)?;
                cfg.options.insert(k, v);
            }
        }
        if let Some(actions) = actions {
            if info.kind != Kind::Input {
                return Err(MacroError::Invalid("only an Input has a chain of Actions".into()));
            }
            if actions.len() > MAX_CHAIN {
                return Err(MacroError::Invalid(format!(
                    "at most {MAX_CHAIN} Actions per Input"
                )));
            }
            for a in &actions {
                if !self.scripts.iter().any(|s| &s.id == a && s.kind == Kind::Action) {
                    return Err(MacroError::Invalid(format!("no Action {a}")));
                }
            }
            cfg.actions = actions;
        }
        if let Some(on) = enabled {
            if info.kind != Kind::Input {
                return Err(MacroError::Invalid(
                    "Actions are not enabled: they run from an Input or the Run button".into(),
                ));
            }
            if on && info.error.is_some() {
                return Err(MacroError::Conflict(format!(
                    "{} cannot run: {}",
                    info.name,
                    info.error.clone().unwrap_or_default()
                )));
            }
            cfg.enabled = on;
        }
        if self.config.scripts.get(id) != Some(&cfg) {
            self.config.scripts.insert(id.to_string(), cfg);
            self.changed = true;
            let values = self.options_of(&info);
            if let Some(link) = lock(&self.core.links).get(id) {
                let _ = link.tx.try_send(Cmd::Options(values));
            }
            self.errors.remove(id);
            self.sync();
        }
        Ok(())
    }

    /// Run an Action now (macros must be on).
    pub fn run(&mut self, id: &str) -> Result<(), MacroError> {
        let info = self
            .info(id)
            .ok_or_else(|| MacroError::NotFound(format!("no macro script {id}")))?;
        if info.kind != Kind::Action {
            return Err(MacroError::Invalid("only an Action runs on request".into()));
        }
        if !self.config.enabled {
            return Err(MacroError::Conflict("macros are off: turn them on first".into()));
        }
        self.execute(id)
    }

    fn execute(&mut self, id: &str) -> Result<(), MacroError> {
        if self.threads.get(id).is_some_and(WorkerThread::finished) {
            self.stop(id);
        }
        self.start(id)?;
        let tx = lock(&self.core.links).get(id).map(|l| l.tx.clone());
        match tx.map(|tx| tx.try_send(Cmd::Execute)) {
            Some(Ok(())) => Ok(()),
            Some(Err(TrySendError::Full(_))) => Err(MacroError::Conflict(format!("{id} is busy"))),
            _ => Err(MacroError::Conflict(format!("{id} is not running"))),
        }
    }

    /// Answer prompt `id` (`None`: cancelled).
    pub fn answer(&mut self, id: u64, text: Option<String>) -> Result<(), MacroError> {
        let script = lock(&self.core.prompts)
            .remove(&id)
            .ok_or_else(|| MacroError::NotFound(format!("no open prompt {id}")))?;
        let text = text.map(|t| t.chars().take(options::MAX_TEXT).collect());
        if let Some(link) = lock(&self.core.links).get(&script) {
            let _ = link.tx.try_send(Cmd::Answer { id, text });
        }
        Ok(())
    }

    /// An `onApplication` event.
    pub fn application(&self, event_type: &str, sender: &str, sender_type: &str, data: Value) {
        let e = envelope("Application", event_type, sender, sender_type, data);
        self.core.deliver("onApplication", &e, None);
    }

    /// The effect shown changed (`Effect Changed`, `LastEffectName`).
    pub fn effect_changed(&self, title: &str) {
        self.core.srgb.effect_changed(title);
        self.application(
            "Effect Changed",
            "LightingManager",
            "Lighting",
            json!({ "EffectName": title }),
        );
    }

    /// The layout changed (`Layout Changed`).
    pub fn layout_changed(&self, name: &str) {
        self.application(
            "Layout Changed",
            "LayoutManager",
            "Layouts",
            json!({ "LayoutName": name }),
        );
    }

    /// The session was locked or unlocked (`Session Lock`).
    pub fn session_lock(&self, locked: bool) {
        self.application("Session Lock", "SignalRGB", "Input", json!({ "locked": locked }));
    }

    /// The system goes to sleep or woke up (`System Suspend`, `System Resume`).
    pub fn system_power(&self, suspend: bool) {
        let t = if suspend {
            "System Suspend"
        } else {
            "System Resume"
        };
        self.application(t, "SignalRGB", "System", json!({}));
    }

    /// Housekeeping, called often by the engine: the emergency stop, triggered
    /// chains, ended scripts, the folder watch and the polled events.
    pub fn tick(&mut self) {
        if self.core.emergency.swap(false, Ordering::AcqRel) {
            self.config.enabled = false;
            self.changed = true;
            self.sync();
            self.core.host.log(
                "",
                &format!("macros turned off by the emergency stop ({EMERGENCY_STOP}); turn them on again in the Macros page"),
            );
        }
        let triggered: Vec<String> = std::mem::take(&mut *lock(&self.core.triggers));
        for input in triggered {
            if !self.config.enabled {
                break;
            }
            for action in self.script_config(&input).actions {
                if let Err(e) = self.execute(&action) {
                    self.core.host.log("", &format!("chain of {input}: {e}"));
                }
            }
        }
        let ended: Vec<String> = self
            .threads
            .iter()
            .filter(|(_, t)| t.finished())
            .map(|(id, _)| id.clone())
            .collect();
        for id in ended {
            let why = self
                .threads
                .get(&id)
                .and_then(|t| lock(&t.failed).clone())
                .unwrap_or_else(|| "stopped".into());
            self.stop(&id);
            self.errors.insert(id, why);
            self.sync_hooks();
        }
        if self.last_watch.elapsed() >= WATCH_EVERY {
            self.watch();
        }
        if !self.config.enabled || !self.running_with("onApplication") {
            self.processes = None;
            return;
        }
        if self.last_idle.elapsed() >= IDLE_EVERY {
            self.last_idle = Instant::now();
            if let Some(idle) = self.core.desktop.idle_seconds() {
                self.application(
                    "System Idle Time",
                    "SignalRGB",
                    "Input",
                    json!({ "idleTime": idle }),
                );
            }
        }
        if self.last_processes.elapsed() >= PROCESSES_EVERY {
            self.last_processes = Instant::now();
            self.poll_processes();
        }
    }

    /// Compare the running processes with the last look (`Process Started`, `Process Ended`).
    pub fn poll_processes(&mut self) {
        let Some(now) = self.core.desktop.processes() else {
            return;
        };
        let now: BTreeSet<String> = now.into_iter().collect();
        if let Some(before) = &self.processes {
            for name in now.difference(before) {
                self.application(
                    "Process Started",
                    "SignalRGB",
                    "Process",
                    json!({ "exeName": name }),
                );
            }
            for name in before.difference(&now) {
                self.application(
                    "Process Ended",
                    "SignalRGB",
                    "Process",
                    json!({ "exeName": name }),
                );
            }
        }
        self.processes = Some(now);
    }

    /// Events dropped because a script's queue was full.
    pub fn dropped(&self) -> u64 {
        self.core.dropped.load(Ordering::Relaxed)
    }

    /// `GET /v1/macros`.
    pub fn view(&self) -> MacrosView {
        let (keyboard, mouse) = self.hook.as_ref().map_or((false, false), |h| h.active());
        let links = lock(&self.core.links);
        MacrosView {
            enabled: self.config.enabled,
            keyboard_hook: self.config.keyboard_hook,
            mouse_hook: self.config.mouse_hook,
            allow_launch: self.config.allow_launch,
            hooks_available: self.hook.is_some(),
            hooks: HooksView { keyboard, mouse },
            hook_error: self.hook_error.clone(),
            emergency_stop: EMERGENCY_STOP,
            folders: self.dirs.clone(),
            scripts: self
                .scripts
                .iter()
                .map(|s| {
                    let cfg = self.script_config(&s.id);
                    let values = options::effective(&s.options, &cfg.options);
                    ScriptView {
                        id: s.id.clone(),
                        kind: s.kind,
                        source: s.source.clone(),
                        file: s
                            .path
                            .file_name()
                            .map(|f| f.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        name: s.name.clone(),
                        description: s.description.clone(),
                        options: s
                            .options
                            .iter()
                            .map(|o| OptionView {
                                spec: o.clone(),
                                value: values.get(&o.property).cloned().unwrap_or(Value::Null),
                            })
                            .collect(),
                        handlers: s.handlers.clone(),
                        enabled: cfg.enabled,
                        actions: cfg.actions,
                        running: links.contains_key(&s.id),
                        error: s.error.clone().or_else(|| self.errors.get(&s.id).cloned()),
                        warnings: s.warnings.clone(),
                    }
                })
                .collect(),
        }
    }

    /// Stop every script and remove the hooks.
    pub fn shutdown(&mut self) {
        self.core.enabled.store(false, Ordering::Release);
        if let Some(h) = self.hook.as_mut() {
            h.stop();
        }
        self.stop_all();
    }
}

impl Drop for MacroManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// List one file.
fn inspect(dir: &ScriptDir, path: &Path) -> ScriptInfo {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut info = ScriptInfo {
        id: format!("{}:{}:{stem}", dir.source, dir.kind.as_str()),
        kind: dir.kind,
        source: dir.source.clone(),
        path: path.to_path_buf(),
        name: stem.clone(),
        description: String::new(),
        options: Vec::new(),
        handlers: Vec::new(),
        error: None,
        warnings: Vec::new(),
    };
    if std::fs::metadata(path).map_or(true, |m| m.len() > MAX_FILE) {
        info.error = Some(format!("larger than {} KiB", MAX_FILE / 1024));
        return info;
    }
    let d: Described = match script::inspect(path) {
        Ok(d) => d,
        Err(e) => {
            info.error = Some(e.to_string());
            return info;
        }
    };
    let label = if d.kind == "action" { "Action" } else { "Input" };
    info.handlers = d.handlers.clone();
    info.options = options::parse(&d.options, &mut info.warnings);
    info.description = d.description.clone().unwrap_or_default();
    match &d.name {
        Some(n) if !n.trim().is_empty() => info.name = n.trim().to_string(),
        _ => info.error = Some(format!("{label} doesn't have property: Name")),
    }
    let has = |h: &str| d.handlers.iter().any(|x| x == h);
    if info.error.is_none() && d.description.is_none() {
        info.error = Some(format!("{label} doesn't have property: Description"));
    }
    if info.error.is_none() {
        info.error = match (dir.kind, d.kind.as_str()) {
            (Kind::Action, "input") => Some("an Input script in the Actions folder".into()),
            (Kind::Input, "action") => Some("an Action script in the Inputs folder".into()),
            (Kind::Action, _) if !has("onExecute") => Some("Missing execute callback.".into()),
            (Kind::Input, _) if !["onApplication", "onKey", "onMouse"].iter().any(|h| has(h)) => {
                Some("Input has no handler (onApplication, onKey or onMouse)".into())
            }
            _ => None,
        };
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lines_split() {
        assert_eq!(
            split_args("-NoExit -Command cd ~"),
            vec!["-NoExit", "-Command", "cd", "~"]
        );
        assert_eq!(split_args(r#"-d "C:\My Dir" x"#), vec!["-d", r"C:\My Dir", "x"]);
        assert_eq!(split_args(r#"a \"b\" """#), vec!["a", "\"b\"", ""]);
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn url_policy_allows_links_and_refuses_the_rest() {
        assert_eq!(url_policy("https://example.com"), Ok(None));
        assert_eq!(url_policy("discord://open"), Ok(None));
        assert_eq!(url_policy("mailto:a@b"), Ok(None));
        assert_eq!(
            url_policy("lightloom://app/raiseWindow"),
            Ok(Some("lightloom://app/raiseWindow".into()))
        );
        for bad in [
            "file:///C:/Windows/System32/cmd.exe",
            "ms-msdt:/id",
            "search-ms:query=x",
            "C:\\x.exe",
            "javascript:alert(1)",
        ] {
            let e = url_policy(bad).unwrap_err();
            assert_eq!(e.class, "MacroRefused", "{bad}");
        }
        assert_eq!(url_policy("http://a\nb").unwrap_err().class, "TypeError");
    }

    #[test]
    fn envelopes_carry_the_handler_fields() {
        let e = envelope(
            "Application",
            "Process Started",
            "SignalRGB",
            "Process",
            json!({"exeName": "a.exe"}),
        );
        assert_eq!(e["EventHandler"], "Application");
        assert_eq!(e["data"]["exeName"], "a.exe");
    }
}
