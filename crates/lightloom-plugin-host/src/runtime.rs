// SPDX-License-Identifier: GPL-3.0-or-later
//! rquickjs (QuickJS-NG) plugin runtime — M0 spike B.
//!
//! * One [`rquickjs::Runtime`] + [`rquickjs::Context`] per plugin, created and
//!   dropped on a dedicated OS thread ([`run_isolated`]); nothing JS-related
//!   ever crosses a thread boundary.
//! * Memory limit ([`DEFAULT_MEMORY_LIMIT`], 64 MiB) and an interrupt handler
//!   driven by a [`Deadline`] that the caller arms per phase with a [`Budget`]:
//!   JavaScript time (time blocked in `device.pause` and HID I/O is excluded,
//!   [`Deadline::suspend`]) under a wall-clock cap.
//! * [`HostResolver`] / [`HostLoader`]: `@SignalRGB/<name>` and
//!   `@SignalRGB/<name>.js` (plus the `@lightloom/` alias) resolve to the
//!   embedded `prelude/` files; relative imports resolve against the importing
//!   file inside the plugin root (lexically, see [`crate::resolve`]).
//! * The compat host (`js/host.js`) is evaluated before the plugin module:
//!   recording stubs for the dry-run, or the natives of a
//!   [`DeviceContext`](crate::device::DeviceContext) when
//!   [`RuntimeOptions::device`] is set (a real device session). With
//!   [`RuntimeOptions::smbus`] the `bus` natives of a guarded SMBus bus are
//!   added: to the device's (an SMBus device session) or alone (`"scan"`
//!   mode, where only [`PluginRuntime::call_scan`] runs).
//! * An interrupt that QuickJS swallows (a promise job cut off by the
//!   deadline is drained by `execute_pending_job`) is still reported as a
//!   `Timeout`: the deadline remembers that it fired ([`Deadline::fired`]).

use crate::bus::SharedGuard;
use crate::device::SharedDevice;
use crate::exports::{read_exports, ExportsReading};
use crate::resolve::{ModuleResolver, ResolveError, ResolvedModule};
use lightloom_hal::hid::HidEndpointTuple;
use rquickjs::loader::{ImportAttributes, Loader as JsLoader, Resolver};
use rquickjs::{Coerced, Context, Ctx, FromJs, Function, Module, Object, Runtime, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Per-plugin heap cap (synthesis D-rules: 64 MB).
pub const DEFAULT_MEMORY_LIMIT: usize = 64 * 1024 * 1024;
/// QuickJS stack limit; the plugin thread gets [`PLUGIN_THREAD_STACK`].
pub const DEFAULT_MAX_STACK: usize = 4 * 1024 * 1024;
/// OS stack of a plugin thread.
pub const PLUGIN_THREAD_STACK: usize = 16 * 1024 * 1024;

/// Prefix of host-internal module names (never a file path).
pub const INTERNAL_PREFIX: &str = "lightloom:";
/// Internal module standing in for an unknown `@SignalRGB/<x>` import: it
/// throws `HostApiMissing` when evaluated (prelude/modules/README.md rule 4).
const MISSING_PREFIX: &str = "lightloom:missing:";
const HOST_NAME: &str = "lightloom:host/host.js";
const HOST_SOURCE: &str = include_str!("../js/host.js");

/// `prelude/` files embedded at build time, by internal module name.
pub const PRELUDE_MODULES: &[(&str, &str)] = &[
    (
        "lightloom:prelude/Errors.js",
        include_str!("../../../prelude/Errors.js"),
    ),
    (
        "lightloom:prelude/base64.js",
        include_str!("../../../prelude/base64.js"),
    ),
    (
        "lightloom:prelude/BinaryUtils.js",
        include_str!("../../../prelude/BinaryUtils.js"),
    ),
    (
        "lightloom:prelude/createColorArray.js",
        include_str!("../../../prelude/createColorArray.js"),
    ),
    (
        "lightloom:prelude/hexToRgb.js",
        include_str!("../../../prelude/hexToRgb.js"),
    ),
    (
        "lightloom:prelude/HostApiMissing.js",
        include_str!("../../../prelude/HostApiMissing.js"),
    ),
    (
        "lightloom:prelude/modules/udp.js",
        include_str!("../../../prelude/modules/udp.js"),
    ),
    (
        "lightloom:prelude/modules/tcp.js",
        include_str!("../../../prelude/modules/tcp.js"),
    ),
    (
        "lightloom:prelude/modules/serial.js",
        include_str!("../../../prelude/modules/serial.js"),
    ),
    (
        "lightloom:prelude/modules/lcd.js",
        include_str!("../../../prelude/modules/lcd.js"),
    ),
    (
        "lightloom:prelude/modules/permissions.js",
        include_str!("../../../prelude/modules/permissions.js"),
    ),
    (
        "lightloom:prelude/modules/systeminfo.js",
        include_str!("../../../prelude/modules/systeminfo.js"),
    ),
    (
        "lightloom:prelude/modules/DeviceDiscovery.js",
        include_str!("../../../prelude/modules/DeviceDiscovery.js"),
    ),
    (
        "lightloom:prelude/modules/performance.js",
        include_str!("../../../prelude/modules/performance.js"),
    ),
    (
        "lightloom:prelude/modules/appInfo.js",
        include_str!("../../../prelude/modules/appInfo.js"),
    ),
];

/// Rust-backed `@SignalRGB/*` modules (facade in `prelude/modules/`, stub in the stub host).
const NATIVE_MODULES: &[&str] = &[
    "udp",
    "tcp",
    "serial",
    "lcd",
    "permissions",
    "systeminfo",
    "DeviceDiscovery",
    "performance",
    "appInfo",
];

/// Rust-backed modules a device session implements (`js/host.js` `nativeModules`);
/// the others keep their throwing facade there.
const DEVICE_MODULES: &[&str] = &["permissions", "systeminfo"];

/// Prefix of the generated modules that pick a device-session implementation.
const NATIVE_PREFIX: &str = "lightloom:native/";

fn prelude_source(name: &str) -> Option<&'static str> {
    PRELUDE_MODULES.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

/// Internal module name a host shim resolves to.
pub fn host_shim_module(shim: &str, module_stubs: bool) -> String {
    if NATIVE_MODULES.contains(&shim) {
        if module_stubs {
            format!("lightloom:stub/{shim}.js")
        } else if DEVICE_MODULES.contains(&shim) {
            format!("{NATIVE_PREFIX}{shim}.js")
        } else {
            format!("lightloom:prelude/modules/{shim}.js")
        }
    } else {
        format!("lightloom:prelude/{shim}.js")
    }
}

/// Runtime configuration.
#[derive(Clone, Debug)]
pub struct RuntimeOptions {
    /// Heap cap in bytes.
    pub memory_limit: usize,
    /// QuickJS stack cap in bytes.
    pub max_stack_size: usize,
    /// Relative imports may not leave this directory (default: the plugin's own directory).
    pub plugin_root: Option<PathBuf>,
    /// Back the Rust-backed `@SignalRGB/*` modules with recording stubs instead
    /// of the throwing `prelude/modules/*` facades.
    pub module_stubs: bool,
    /// Real device session: back `device.*` with this context's natives (and
    /// make API gaps throw) instead of recording stubs.
    pub device: Option<SharedDevice>,
    /// SMBus plugin: back `bus.*` (and the `Scan(bus)` argument) with this
    /// guarded bus. Without [`RuntimeOptions::device`] the runtime is in scan mode.
    pub smbus: Option<SharedGuard>,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            memory_limit: DEFAULT_MEMORY_LIMIT,
            max_stack_size: DEFAULT_MAX_STACK,
            plugin_root: None,
            module_stubs: true,
            device: None,
            smbus: None,
        }
    }
}

/// Time limits of one call into the plugin.
///
/// Plugins block in host calls on purpose: `device.pause()` between packets,
/// HID reads waiting for a reply, device setup run lazily from `Render()`
/// (Razer mice probe nine transaction ids 400 ms apart). Only JavaScript time
/// counts against `js`, so such a call completes; `wall` still bounds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    /// JavaScript time; time blocked in host calls is excluded.
    pub js: Duration,
    /// Wall-clock cap, blocked time included (at least `js`).
    pub wall: Duration,
}

impl From<Duration> for Budget {
    /// The same limit for JavaScript and wall-clock time.
    fn from(d: Duration) -> Self {
        Self { js: d, wall: d }
    }
}

/// Watchdog deadline shared with the interrupt handler.
///
/// The interrupt handler calls [`Deadline::poll`]. Once that has interrupted
/// the engine, [`Deadline::fired`] stays true until the next [`Deadline::arm`],
/// so a cut-off promise job (whose exception `execute_pending_job` drains) is
/// still reported as a timeout.
#[derive(Clone, Debug, Default)]
pub struct Deadline(Arc<DeadlineState>);

#[derive(Debug, Default)]
struct Window {
    /// End of the JavaScript budget, pushed back by suspended time.
    soft: Option<Instant>,
    /// Wall-clock cap.
    hard: Option<Instant>,
    /// Set by whoever may cancel the call (a session asked to stop).
    stop: Option<Arc<AtomicBool>>,
    /// When the stop request was first seen.
    stop_seen: Option<Instant>,
}

/// A call still running this long after a stop request is interrupted; one
/// that ends sooner completes normally (its packets are whole).
pub const STOP_GRACE: Duration = Duration::from_millis(500);

#[derive(Debug, Default)]
struct DeadlineState {
    window: Mutex<Window>,
    fired: AtomicBool,
}

impl Deadline {
    fn window(&self) -> MutexGuard<'_, Window> {
        self.0.window.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Arm: the engine is interrupted once `budget` has elapsed from now
    /// (limits too large to represent mean no limit).
    pub fn arm(&self, budget: impl Into<Budget>) {
        let b = budget.into();
        let now = Instant::now();
        *self.window() = Window {
            soft: now.checked_add(b.js),
            hard: now.checked_add(b.wall.max(b.js)),
            stop: None,
            stop_seen: None,
        };
        self.0.fired.store(false, Ordering::Relaxed);
    }

    /// [`Deadline::arm`], and interrupt as soon as `stop` is set.
    pub fn arm_until(&self, budget: impl Into<Budget>, stop: Arc<AtomicBool>) {
        self.arm(budget);
        self.window().stop = Some(stop);
    }

    /// Whether the armed call was asked to stop ([`Deadline::arm_until`]).
    pub fn stop_requested(&self) -> bool {
        self.window()
            .stop
            .as_ref()
            .is_some_and(|s| s.load(Ordering::Acquire))
    }

    /// Disarm (keeps [`Deadline::fired`] until the next arm).
    pub fn clear(&self) {
        *self.window() = Window::default();
    }

    /// Whether the armed budget or cap has passed, or a stop request is
    /// older than [`STOP_GRACE`].
    pub fn expired(&self) -> bool {
        let mut w = self.window();
        let now = Instant::now();
        if w.soft.is_some_and(|d| now >= d) || w.hard.is_some_and(|d| now >= d) {
            return true;
        }
        if w.stop.as_ref().is_some_and(|s| s.load(Ordering::Acquire)) {
            let seen = *w.stop_seen.get_or_insert(now);
            return now.duration_since(seen) >= STOP_GRACE;
        }
        false
    }

    /// While the returned guard lives, time does not count against the
    /// JavaScript budget (the wall-clock cap still runs): wrap host calls that block.
    pub fn suspend(&self) -> Suspended<'_> {
        Suspended {
            deadline: self,
            since: Instant::now(),
        }
    }

    /// Interrupt-handler hook: `true` (and remembered) once the deadline has passed.
    pub fn poll(&self) -> bool {
        let expired = self.expired();
        if expired {
            self.0.fired.store(true, Ordering::Relaxed);
        }
        expired
    }

    /// Whether the interrupt handler cut the engine off since the last [`Deadline::arm`].
    pub fn fired(&self) -> bool {
        self.0.fired.load(Ordering::Relaxed)
    }
}

/// See [`Deadline::suspend`].
#[must_use = "the budget is suspended only while the guard lives"]
pub struct Suspended<'a> {
    deadline: &'a Deadline,
    since: Instant,
}

impl Drop for Suspended<'_> {
    fn drop(&mut self) {
        let blocked = self.since.elapsed();
        let mut w = self.deadline.window();
        if let Some(soft) = w.soft {
            w.soft = soft.checked_add(blocked);
        }
    }
}

/// Specifiers the resolver refused (`import:<specifier>`), shared with the caller.
pub type GapLog = Arc<Mutex<Vec<String>>>;

/// Plugin module names whose imports the resolver was asked to resolve.
///
/// QuickJS resolves a module's imports only after the whole module parsed and
/// compiled (`js_resolve_module` runs after `js_create_function`), so a name in
/// this set means "that file compiled"; a later failure is in a dependency.
pub type BaseLog = Arc<Mutex<BTreeSet<String>>>;

/// Module resolver: host shims, prelude-internal relative imports, plugin-relative imports.
pub struct HostResolver {
    plugin: ModuleResolver,
    module_stubs: bool,
    gaps: GapLog,
    bases: BaseLog,
}

impl HostResolver {
    /// Resolver for plugins under `root`.
    pub fn new(root: &Path, module_stubs: bool, gaps: GapLog) -> Self {
        Self {
            plugin: ModuleResolver::new(root),
            module_stubs,
            gaps,
            bases: BaseLog::default(),
        }
    }

    /// Record every plugin module whose imports get resolved into `bases`.
    pub fn with_base_log(mut self, bases: BaseLog) -> Self {
        self.bases = bases;
        self
    }

    fn gap(&self, specifier: &str) {
        if let Ok(mut g) = self.gaps.lock() {
            g.push(format!("import:{specifier}"));
        }
    }

    fn internal_relative(base: &str, name: &str) -> String {
        let mut parts: Vec<&str> = base[INTERNAL_PREFIX.len()..].split('/').collect();
        parts.pop();
        for seg in name.split('/') {
            match seg {
                "." | "" => {}
                ".." => {
                    parts.pop();
                }
                s => parts.push(s),
            }
        }
        format!("{INTERNAL_PREFIX}{}", parts.join("/"))
    }

    /// Resolve without an engine context (used by [`Resolver::resolve`] and tests).
    ///
    /// Host-internal names (`lightloom:…`) are only reachable from host-internal
    /// modules; plugin code gets `@SignalRGB/*`, `@lightloom/*` and relative files.
    pub fn resolve_name(&self, base: &str, name: &str) -> Result<String, String> {
        if base.starts_with(INTERNAL_PREFIX) {
            if name.starts_with(INTERNAL_PREFIX) {
                return Ok(name.to_string());
            }
            if name.starts_with("./") || name.starts_with("../") {
                let joined = Self::internal_relative(base, name);
                return if prelude_source(&joined).is_some() {
                    Ok(joined)
                } else {
                    Err(format!("unknown prelude module {joined}"))
                };
            }
            return Err(format!("prelude module {base} imports unsupported \"{name}\""));
        }
        if let Ok(mut b) = self.bases.lock() {
            b.insert(base.to_string());
        }
        match self.plugin.resolve(Path::new(base), name) {
            Ok(ResolvedModule::HostShim(shim)) => Ok(host_shim_module(&shim, self.module_stubs)),
            Ok(ResolvedModule::File(p)) => Ok(p.to_string_lossy().into_owned()),
            // prelude/modules/README.md rule 4: an unknown host module is an
            // API gap whose import throws HostApiMissing at evaluation time.
            Err(ResolveError::UnknownHostModule(_)) => {
                self.gap(name);
                Ok(format!("{MISSING_PREFIX}{name}"))
            }
            Err(e) => {
                self.gap(name);
                Err(e.to_string())
            }
        }
    }
}

impl Resolver for HostResolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        self.resolve_name(base, name)
            .map_err(|msg| rquickjs::Error::new_resolving_message(base, name, msg))
    }
}

/// Module loader: embedded prelude, generated stub modules, plugin files.
#[derive(Debug, Default)]
pub struct HostLoader;

/// Read a plugin source file, dropping a UTF-8 BOM.
pub fn read_source(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut bytes = std::fs::read(path)?;
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        bytes.drain(..3);
    }
    Ok(bytes)
}

impl JsLoader for HostLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js, rquickjs::module::Declared>> {
        if let Some(shim) = name
            .strip_prefix("lightloom:stub/")
            .and_then(|s| s.strip_suffix(".js"))
        {
            let src = format!("export default globalThis.__lightloomHost.modules[{shim:?}];\n");
            return Module::declare(ctx.clone(), name, src);
        }
        if let Some(shim) = name
            .strip_prefix(NATIVE_PREFIX)
            .and_then(|s| s.strip_suffix(".js"))
            .filter(|s| DEVICE_MODULES.contains(s))
        {
            // The device host's implementation, else the throwing facade.
            let src = format!(
                "import facade from \"lightloom:prelude/modules/{shim}.js\";\n\
                 const real = globalThis.__lightloomHost.nativeModules[{shim:?}];\n\
                 export default real === undefined ? facade : real;\n"
            );
            return Module::declare(ctx.clone(), name, src);
        }
        if let Some(spec) = name.strip_prefix(MISSING_PREFIX) {
            let src = format!(
                "import {{ HostApiMissing }} from \"lightloom:prelude/HostApiMissing.js\";\n\
                 throw new HostApiMissing({spec:?}, {{ hint: \"no such host module\" }});\n\
                 export default undefined;\n"
            );
            return Module::declare(ctx.clone(), name, src);
        }
        if let Some(src) = prelude_source(name) {
            return Module::declare(ctx.clone(), name, src);
        }
        if name.starts_with(INTERNAL_PREFIX) {
            return Err(rquickjs::Error::new_loading_message(name, "unknown host module"));
        }
        let src = read_source(Path::new(name))
            .map_err(|e| rquickjs::Error::new_loading_message(name, e.to_string()))?;
        Module::declare(ctx.clone(), name, src)
    }
}

/// A JS exception (or host error) flattened to text.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct JsFailure {
    /// `error.name` (`TypeError`, `HostApiMissing`, …) or a host class:
    /// `Timeout`, `OutOfMemory`, `PromisePending`, `Throw` (a non-Error value
    /// was thrown), `HostError`, `IoError`, `HostPanic`.
    pub class: String,
    /// `error.message`.
    pub message: String,
}

impl std::fmt::Display for JsFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.class, self.message)
    }
}

impl JsFailure {
    fn timeout(during: &str) -> Self {
        JsFailure {
            class: "Timeout".into(),
            message: format!("interrupted ({during} exceeded its deadline)"),
        }
    }

    fn stopped(during: &str) -> Self {
        JsFailure {
            class: STOPPED.into(),
            message: format!("interrupted ({during} was running when the session was asked to stop)"),
        }
    }

    /// The call was cut off by a stop request, not by its budget.
    pub fn is_stop(&self) -> bool {
        self.class == STOPPED
    }

    /// The failure of a call `deadline` cut off.
    fn cut_off(deadline: &Deadline, during: &str) -> Self {
        if deadline.stop_requested() {
            Self::stopped(during)
        } else {
            Self::timeout(during)
        }
    }
}

/// [`JsFailure::class`] of a call interrupted by a stop request.
pub const STOPPED: &str = "Stopped";

/// Turn an rquickjs error into a [`JsFailure`], consuming the pending exception.
///
/// Classes added by the host: `Timeout` (the deadline fired during the
/// phase), `OutOfMemory`, `PromisePending` (an awaited promise can never
/// settle: no job left to run), `HostError` (anything else on the Rust side).
pub fn failure(ctx: &Ctx<'_>, err: rquickjs::Error, deadline: &Deadline) -> JsFailure {
    let mut f = match err {
        rquickjs::Error::Exception => {
            let v = ctx.catch();
            exception_failure(ctx, v)
        }
        rquickjs::Error::WouldBlock => JsFailure {
            class: "PromisePending".into(),
            message: "the returned promise never settled (no pending jobs left)".into(),
        },
        other => JsFailure {
            class: "HostError".into(),
            message: other.to_string(),
        },
    };
    if deadline.fired() || (f.message.contains("interrupted") && deadline.expired()) {
        if !f.message.contains("interrupted") {
            f.message = format!("interrupted; {}", f.message);
        }
        f.class = if deadline.stop_requested() {
            STOPPED
        } else {
            "Timeout"
        }
        .into();
    } else if f.message.contains("out of memory") {
        f.class = "OutOfMemory".into();
    }
    f
}

/// `r.ok()`, clearing the exception a failed engine call leaves pending so it
/// cannot be picked up by a later, unrelated [`failure`].
fn ok_or_catch<T>(ctx: &Ctx<'_>, r: rquickjs::Result<T>) -> Option<T> {
    r.map_err(|e| {
        if matches!(e, rquickjs::Error::Exception) {
            drop(ctx.catch());
        }
    })
    .ok()
}

fn exception_failure<'js>(ctx: &Ctx<'js>, v: Value<'js>) -> JsFailure {
    if let Some(o) = v.as_object() {
        let field = |key: &str| {
            ok_or_catch(ctx, o.get::<_, Value>(key))
                .filter(|x| !x.is_undefined() && !x.is_null())
                .and_then(|x| ok_or_catch(ctx, Coerced::<String>::from_js(ctx, x)))
                .map(|c| c.0)
        };
        match (field("name"), field("message")) {
            (Some(class), message) => {
                return JsFailure {
                    class,
                    message: message.unwrap_or_default(),
                }
            }
            // `throw { message: "…" }`: not an Error, but keep its text.
            (None, Some(message)) => {
                return JsFailure {
                    class: "Throw".into(),
                    message,
                }
            }
            (None, None) => {}
        }
    }
    let text = ok_or_catch(ctx, Coerced::<String>::from_js(ctx, v))
        .map(|c| c.0)
        .unwrap_or_else(|| "<unprintable exception>".into());
    JsFailure {
        class: "Throw".into(),
        message: text,
    }
}

/// Stub-host state read back after a run.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HostRecord {
    /// `object.member` → access count, for members outside the documented surface.
    pub gaps: BTreeMap<String, u64>,
    /// `controller.field` / `discovery.field` → read count, for plugin-defined
    /// data the stub objects do not carry (not host API gaps).
    #[serde(default)]
    pub fields: BTreeMap<String, u64>,
    /// `object.member` → call count, for documented host members.
    #[serde(default)]
    pub calls: BTreeMap<String, u64>,
    /// Lines written through `console.*`, `device.log`, `bus.log`, `service.log`.
    #[serde(rename = "consoleLines")]
    pub console_lines: u64,
    /// The first 20 of those lines.
    #[serde(rename = "logTail")]
    pub log_tail: Vec<String>,
    /// What the plugin reported through the `battery` feature (when it added it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battery: Option<StubBattery>,
}

/// The stub `battery` global's values after a run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StubBattery {
    /// `setBatteryLevel` (`0..=100`), `None` when never called.
    pub level: Option<u8>,
    /// `setBatteryState` (0..=5).
    pub state: i32,
}

/// One plugin loaded into its own runtime (lives on the plugin thread).
pub struct PluginRuntime {
    // Field order matters: the context must drop before the runtime.
    context: Context,
    runtime: Runtime,
    deadline: Deadline,
    gaps: GapLog,
    bases: BaseLog,
    path: PathBuf,
}

/// `globalThis.__lightloomHost` (installed non-writable by the host module).
fn host_object<'js>(ctx: &Ctx<'js>) -> rquickjs::Result<Object<'js>> {
    ctx.globals().get("__lightloomHost")
}

/// The evaluated plugin namespace kept by the host module.
fn plugin_namespace<'js>(ctx: &Ctx<'js>) -> rquickjs::Result<Object<'js>> {
    host_object(ctx)?.get::<_, Function>("namespace")?.call(())
}

/// Outcome of [`PluginRuntime::load`].
pub struct Loaded {
    /// Whether the file itself compiled: false only for a `SyntaxError` raised
    /// before any of its imports was resolved (dependency, link and run-time
    /// errors leave it true).
    pub parsed: bool,
    /// Module failure (parse, link or top-level throw).
    pub error: Option<JsFailure>,
    /// Namespace reading when evaluation succeeded.
    pub exports: Option<ExportsReading>,
}

impl PluginRuntime {
    /// Create the runtime and context and install the stub host.
    pub fn new(path: &Path, opts: &RuntimeOptions) -> Result<Self, JsFailure> {
        let host_err = |e: rquickjs::Error| JsFailure {
            class: "HostError".into(),
            message: e.to_string(),
        };
        let runtime = Runtime::new().map_err(host_err)?;
        runtime.set_memory_limit(opts.memory_limit);
        runtime.set_max_stack_size(opts.max_stack_size);
        let deadline = Deadline::default();
        let d = deadline.clone();
        runtime.set_interrupt_handler(Some(Box::new(move || d.poll())));
        let root = opts
            .plugin_root
            .clone()
            .or_else(|| path.parent().map(Path::to_path_buf))
            .unwrap_or_default();
        let gaps: GapLog = Arc::default();
        let bases: BaseLog = Arc::default();
        runtime.set_loader(
            HostResolver::new(&root, opts.module_stubs, gaps.clone()).with_base_log(bases.clone()),
            HostLoader,
        );
        let context = Context::full(&runtime).map_err(host_err)?;
        let this = Self {
            context,
            runtime,
            deadline,
            gaps,
            bases,
            path: path.to_path_buf(),
        };
        let plugin = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        this.context.with(|ctx| -> Result<(), JsFailure> {
            let d = &this.deadline;
            let cfg = Object::new(ctx.clone()).map_err(|e| failure(&ctx, e, d))?;
            cfg.set("plugin", plugin).map_err(|e| failure(&ctx, e, d))?;
            if let Some(device) = &opts.device {
                cfg.set("mode", "device").map_err(|e| failure(&ctx, e, d))?;
                crate::device::install(&ctx, device, d).map_err(|e| failure(&ctx, e, d))?;
            }
            if let Some(guard) = &opts.smbus {
                let install = || -> rquickjs::Result<()> {
                    if opts.device.is_none() {
                        cfg.set("mode", "scan")?;
                        ctx.globals().set("__lightloomNative", Object::new(ctx.clone())?)?;
                    }
                    let natives: Object = ctx.globals().get("__lightloomNative")?;
                    crate::bus::install_bus(&ctx, &natives, guard, d, opts.device.as_ref())
                };
                install().map_err(|e| failure(&ctx, e, d))?;
            }
            ctx.globals()
                .set("__lightloomConfig", cfg)
                .map_err(|e| failure(&ctx, e, d))?;
            Module::evaluate(ctx.clone(), HOST_NAME, HOST_SOURCE)
                .and_then(|p| p.finish::<()>())
                .map_err(|e| failure(&ctx, e, d))
        })?;
        Ok(this)
    }

    /// The watchdog of this runtime.
    pub fn deadline(&self) -> &Deadline {
        &self.deadline
    }

    /// Bytes currently allocated by the engine.
    pub fn memory_used(&self) -> i64 {
        self.runtime.memory_usage().malloc_size
    }

    /// Parse and evaluate the plugin module under `budget`, then read its namespace.
    pub fn load(&self, budget: Duration) -> Loaded {
        let source = match read_source(&self.path) {
            Ok(s) => s,
            Err(e) => {
                return Loaded {
                    parsed: false,
                    error: Some(JsFailure {
                        class: "IoError".into(),
                        message: e.to_string(),
                    }),
                    exports: None,
                }
            }
        };
        let name = self.path.to_string_lossy().into_owned();
        let deadline = &self.deadline;
        deadline.arm(budget);
        let out = self.context.with(|ctx| {
            let fail = |error: JsFailure, parsed: bool| Loaded {
                parsed,
                error: Some(error),
                exports: None,
            };
            let declared = match Module::declare(ctx.clone(), name.as_str(), source) {
                Ok(m) => m,
                Err(e) => {
                    let f = failure(&ctx, e, deadline);
                    // A resolver call from this module means the file itself
                    // compiled and the failure lies in a dependency.
                    let compiled = self.bases.lock().is_ok_and(|b| b.contains(&name));
                    let parsed = compiled || f.class != "SyntaxError";
                    return fail(f, parsed);
                }
            };
            // The whole module graph compiled; from here on failures are link
            // errors (`SyntaxError: Could not find export ...`) or evaluation errors.
            let module = match declared.eval().and_then(|(m, p)| p.finish::<()>().map(|_| m)) {
                Ok(m) => m,
                Err(e) => return fail(failure(&ctx, e, deadline), true),
            };
            let read = module.namespace().and_then(|ns| {
                let host = host_object(&ctx)?;
                let json: String = host.get::<_, Function>("readMeta")?.call((ns.clone(),))?;
                // Keep the namespace for lifecycle calls, out of reach of plugin code.
                host.get::<_, Function>("setNamespace")?.call::<_, ()>((ns,))?;
                Ok(json)
            });
            match read {
                // A promise job of the module body ran into the deadline.
                Ok(_) if deadline.fired() => fail(JsFailure::timeout("module evaluation"), true),
                Ok(json) => {
                    let v: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
                    Loaded {
                        parsed: true,
                        error: None,
                        exports: Some(read_exports(&self.path, &v)),
                    }
                }
                Err(e) => fail(failure(&ctx, e, deadline), true),
            }
        });
        deadline.clear();
        out
    }

    /// Bind ControllableParameters defaults as globals and set `device.productId()`, under `budget`.
    pub fn prepare_device(
        &self,
        vendor_id: u16,
        product_id: u16,
        budget: impl Into<Budget>,
    ) -> Result<(), JsFailure> {
        let deadline = &self.deadline;
        deadline.arm(budget);
        let r = self.context.with(|ctx| {
            let run = || -> rquickjs::Result<()> {
                let host = host_object(&ctx)?;
                let ns = plugin_namespace(&ctx)?;
                host.get::<_, Function>("setIds")?
                    .call::<_, ()>((vendor_id, product_id))?;
                let params: Value = match ns.get::<_, Value>("ControllableParameters")? {
                    v if v.is_function() => v
                        .as_function()
                        .map(|f| f.call::<_, Value>(()))
                        .transpose()?
                        .unwrap_or(v),
                    v => v,
                };
                host.get::<_, Function>("applyParameters")?
                    .call::<_, ()>((params,))
            };
            match run().map_err(|e| failure(&ctx, e, deadline)) {
                Ok(()) if deadline.fired() => Err(JsFailure::timeout("ControllableParameters")),
                r => r,
            }
        });
        deadline.clear();
        r
    }

    /// Call a lifecycle export (`Initialize`, `Shutdown`, `Render`, …) under `budget`.
    /// A missing (or non-function) export is `Ok(false)`. Promise jobs the call
    /// queued are run to completion inside the same budget.
    pub fn call_export(
        &self,
        export: &str,
        args: CallArgs,
        budget: impl Into<Budget>,
    ) -> Result<bool, JsFailure> {
        self.call(export, args, budget.into(), None)
    }

    /// [`PluginRuntime::call_export`], cut off as soon as `stop` is set (the
    /// failure is then [`JsFailure::is_stop`]); `device.pause()` ends early too.
    pub fn call_export_until(
        &self,
        export: &str,
        args: CallArgs,
        budget: impl Into<Budget>,
        stop: &Arc<AtomicBool>,
    ) -> Result<bool, JsFailure> {
        self.call(export, args, budget.into(), Some(Arc::clone(stop)))
    }

    fn call(
        &self,
        export: &str,
        args: CallArgs,
        budget: Budget,
        stop: Option<Arc<AtomicBool>>,
    ) -> Result<bool, JsFailure> {
        let deadline = &self.deadline;
        match stop {
            Some(stop) => deadline.arm_until(budget, stop),
            None => deadline.arm(budget),
        }
        let r = self.context.with(|ctx| {
            let run = || -> rquickjs::Result<bool> {
                let ns = plugin_namespace(&ctx)?;
                let Some(f) = ns.get::<_, Value>(export)?.into_function() else {
                    return Ok(false);
                };
                let ret: Value = match args {
                    CallArgs::None => f.call(())?,
                    CallArgs::Bool(b) => f.call((b,))?,
                    CallArgs::FreeBus => {
                        let bus: Value = host_object(&ctx)?.get::<_, Function>("freeBus")?.call(())?;
                        f.call((bus,))?
                    }
                };
                // Drive promises an async export may have returned.
                if let Some(p) = ret.as_promise() {
                    p.finish::<Value>()?;
                }
                // Exceptions of queued jobs are drained here (unhandled
                // rejections); a job cut off by the deadline shows up in `fired`.
                while ctx.execute_pending_job() {}
                Ok(true)
            };
            match run().map_err(|e| failure(&ctx, e, deadline)) {
                Ok(_) if deadline.fired() => Err(JsFailure::cut_off(deadline, export)),
                r => r,
            }
        });
        deadline.clear();
        r
    }

    /// `Scan(bus)` with the free-address bus under `budget`: the addresses it
    /// returned that lie in `window`, without duplicates, at most
    /// [`MAX_SCAN_RESULTS`]. A plugin without `Scan`, or one returning anything
    /// but an array, fails.
    pub fn call_scan(&self, window: &BTreeSet<u8>, budget: impl Into<Budget>) -> Result<Vec<u8>, JsFailure> {
        let deadline = &self.deadline;
        deadline.arm(budget);
        let r = self.context.with(|ctx| {
            let not_array = || JsFailure {
                class: "TypeError".into(),
                message: "Scan() must return an array.".into(),
            };
            let run = || -> rquickjs::Result<Result<Vec<u8>, JsFailure>> {
                let ns = plugin_namespace(&ctx)?;
                let Some(f) = ns.get::<_, Value>("Scan")?.into_function() else {
                    return Ok(Err(JsFailure {
                        class: "HostError".into(),
                        message: "the plugin exports no Scan(bus)".into(),
                    }));
                };
                let bus: Value = host_object(&ctx)?.get::<_, Function>("freeBus")?.call(())?;
                let mut ret: Value = f.call((bus,))?;
                if let Some(p) = ret.as_promise() {
                    ret = p.finish::<Value>()?;
                }
                while ctx.execute_pending_job() {}
                let Some(list) = ret.as_array() else {
                    return Ok(Err(not_array()));
                };
                let mut found = Vec::new();
                for v in list.iter::<Value>() {
                    let Some(n) = v?.as_number() else { continue };
                    if n.fract() != 0.0 || !(0.0..=127.0).contains(&n) {
                        continue;
                    }
                    let a = n as u8;
                    if window.contains(&a) && !found.contains(&a) && found.len() < MAX_SCAN_RESULTS {
                        found.push(a);
                    }
                }
                Ok(Ok(found))
            };
            match run().map_err(|e| failure(&ctx, e, deadline)) {
                Ok(_) if deadline.fired() => Err(JsFailure::timeout("Scan")),
                Ok(r) => r,
                Err(e) => Err(e),
            }
        });
        deadline.clear();
        r
    }

    /// `Validate(endpoint)` under `budget`: the truthiness of its return value.
    /// A plugin without `Validate` accepts every endpoint.
    pub fn validate(
        &self,
        endpoint: &HidEndpointTuple,
        budget: impl Into<Budget>,
    ) -> Result<bool, JsFailure> {
        let deadline = &self.deadline;
        deadline.arm(budget);
        let r = self.context.with(|ctx| {
            let run = || -> rquickjs::Result<bool> {
                let ns = plugin_namespace(&ctx)?;
                let Some(f) = ns.get::<_, Value>("Validate")?.into_function() else {
                    return Ok(true);
                };
                let ep = Object::new(ctx.clone())?;
                ep.set("interface", endpoint.interface)?;
                ep.set("usage", endpoint.usage as i32)?;
                ep.set("usage_page", endpoint.usage_page as i32)?;
                ep.set("collection", endpoint.collection as i32)?;
                let ret: Value = f.call((ep,))?;
                while ctx.execute_pending_job() {}
                Ok(Coerced::<bool>::from_js(&ctx, ret)?.0)
            };
            match run().map_err(|e| failure(&ctx, e, deadline)) {
                Ok(_) if deadline.fired() => Err(JsFailure::timeout("Validate")),
                r => r,
            }
        });
        deadline.clear();
        r
    }

    /// Assign globals from a JSON object (`{"LightingMode": "Forced"}`): parameter
    /// overrides applied after [`PluginRuntime::prepare_device`].
    pub fn set_globals(
        &self,
        values: &serde_json::Value,
        budget: impl Into<Budget>,
    ) -> Result<(), JsFailure> {
        let json = values.to_string();
        let deadline = &self.deadline;
        deadline.arm(budget);
        let r = self.context.with(|ctx| {
            let run = || -> rquickjs::Result<()> {
                let v = ctx.json_parse(json)?;
                host_object(&ctx)?
                    .get::<_, Function>("setGlobals")?
                    .call::<_, ()>((v,))
            };
            run().map_err(|e| failure(&ctx, e, deadline))
        });
        deadline.clear();
        r
    }

    /// Stub-host record plus specifiers the resolver refused.
    pub fn record(&self) -> HostRecord {
        // Host code only, but it iterates Maps whose prototype a plugin could patch.
        self.deadline.arm(Duration::from_secs(1));
        let mut rec = self.context.with(|ctx| {
            host_object(&ctx)
                .and_then(|h| h.get::<_, Function>("report"))
                .and_then(|f| f.call::<_, String>(()))
                .ok()
                .and_then(|s| serde_json::from_str::<HostRecord>(&s).ok())
                .unwrap_or_default()
        });
        self.deadline.clear();
        if let Ok(g) = self.gaps.lock() {
            for k in g.iter() {
                *rec.gaps.entry(k.clone()).or_default() += 1;
            }
        }
        rec
    }
}

/// Most devices one `Scan(bus)` may report on one bus.
pub const MAX_SCAN_RESULTS: usize = 8;

/// Arguments for [`PluginRuntime::call_export`].
#[derive(Clone, Copy, Debug)]
pub enum CallArgs {
    /// `()`.
    None,
    /// `(bool)` — `Shutdown(SystemSuspending)`.
    Bool(bool),
    /// `(bus)` — `Scan(FreeAddressBus)`.
    FreeBus,
}

/// Run `f` on a fresh OS thread (scoped, large stack) and return its result.
/// Every runtime the closure creates lives and dies on that thread.
pub fn run_isolated<T, F>(f: F) -> std::thread::Result<T>
where
    T: Send,
    F: FnOnce() -> T + Send,
{
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .name("lightloom-plugin".into())
            .stack_size(PLUGIN_THREAD_STACK)
            .spawn_scoped(s, f)
            .map_err(|e| Box::new(e.to_string()) as Box<dyn std::any::Any + Send>)?
            .join()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_arms_clears_and_remembers_firing() {
        let d = Deadline::default();
        assert!(!d.expired() && !d.poll() && !d.fired());
        d.arm(Duration::ZERO);
        assert!(d.expired());
        assert!(!d.fired(), "only the interrupt handler's poll marks a firing");
        assert!(d.poll() && d.fired());
        d.clear();
        assert!(!d.expired() && !d.poll());
        assert!(d.fired(), "a firing survives clear() until the next arm()");
        d.arm(Duration::from_secs(60));
        assert!(!d.fired() && !d.poll());
        // Budgets beyond Instant's range mean "no deadline" instead of a panic.
        d.arm(Duration::MAX);
        assert!(!d.expired());
    }

    #[test]
    fn a_stop_request_expires_an_abortable_deadline_only() {
        let d = Deadline::default();
        let stop = Arc::new(AtomicBool::new(false));
        d.arm_until(Duration::from_secs(60), Arc::clone(&stop));
        assert!(!d.expired() && !d.stop_requested());
        stop.store(true, Ordering::Release);
        assert!(d.stop_requested());
        assert!(!d.expired(), "a running call gets STOP_GRACE to finish");
        std::thread::sleep(STOP_GRACE + Duration::from_millis(20));
        assert!(d.expired());
        assert!(JsFailure::cut_off(&d, "Render").is_stop());
        d.arm(Duration::from_secs(60));
        assert!(!d.expired() && !d.stop_requested(), "plain arms ignore the flag");
        assert!(!JsFailure::cut_off(&d, "Shutdown").is_stop());
    }

    #[test]
    fn suspended_time_extends_the_js_budget_but_not_the_cap() {
        let d = Deadline::default();
        d.arm(Budget {
            js: Duration::from_millis(40),
            wall: Duration::from_millis(400),
        });
        {
            let _blocked = d.suspend();
            std::thread::sleep(Duration::from_millis(80));
        }
        assert!(
            !d.expired(),
            "80 ms blocked do not use the 40 ms JavaScript budget"
        );
        std::thread::sleep(Duration::from_millis(60));
        assert!(d.expired(), "unsuspended time does");
        d.arm(Budget {
            js: Duration::from_millis(40),
            wall: Duration::from_millis(60),
        });
        {
            let _blocked = d.suspend();
            std::thread::sleep(Duration::from_millis(80));
        }
        assert!(d.expired(), "the wall-clock cap still applies");
        d.arm(Duration::from_millis(30));
        assert!(!d.expired());
    }

    #[test]
    fn resolver_maps_host_shims_and_prelude_relatives() {
        let gaps = GapLog::default();
        let r = HostResolver::new(Path::new("/plugins"), false, gaps.clone());
        let base = "/plugins/Vendor/X.js";
        assert_eq!(
            r.resolve_name(base, "@SignalRGB/Errors.js").unwrap(),
            "lightloom:prelude/Errors.js"
        );
        assert_eq!(
            r.resolve_name(base, "@SignalRGB/base64").unwrap(),
            "lightloom:prelude/base64.js"
        );
        assert_eq!(
            r.resolve_name(base, "@SignalRGB/udp").unwrap(),
            "lightloom:prelude/modules/udp.js"
        );
        assert_eq!(
            r.resolve_name("lightloom:prelude/modules/udp.js", "../HostApiMissing.js")
                .unwrap(),
            "lightloom:prelude/HostApiMissing.js"
        );
        // Unknown host modules resolve to a module that throws HostApiMissing.
        assert_eq!(
            r.resolve_name(base, "@SignalRGB/nope").unwrap(),
            "lightloom:missing:@SignalRGB/nope"
        );
        // Plugins cannot reach host-internal modules directly.
        assert!(r.resolve_name(base, "lightloom:prelude/BinaryUtils.js").is_err());
        assert!(r.resolve_name(base, "lightloom:missing:x").is_err());
        assert!(r.resolve_name(base, "fs").is_err());
        assert_eq!(
            gaps.lock().unwrap().as_slice(),
            [
                "import:@SignalRGB/nope",
                "import:lightloom:prelude/BinaryUtils.js",
                "import:lightloom:missing:x",
                "import:fs"
            ]
        );
        let stubbed = HostResolver::new(Path::new("/plugins"), true, GapLog::default());
        assert_eq!(
            stubbed.resolve_name(base, "@SignalRGB/lcd").unwrap(),
            "lightloom:stub/lcd.js"
        );
        // Every shim the resolver can produce exists (device-session modules are
        // generated, over a facade that must exist).
        for shim in crate::meta::HOST_MODULES
            .iter()
            .chain(crate::meta::LIGHTLOOM_ONLY_MODULES)
        {
            let n = host_shim_module(shim, false);
            match n.strip_prefix(NATIVE_PREFIX) {
                Some(native) => {
                    assert!(DEVICE_MODULES.contains(&native.trim_end_matches(".js")), "{n}");
                    let facade = format!("lightloom:prelude/modules/{native}");
                    assert!(prelude_source(&facade).is_some(), "{facade}");
                }
                None => assert!(prelude_source(&n).is_some(), "{n}"),
            }
        }
        assert_eq!(
            host_shim_module("permissions", false),
            "lightloom:native/permissions.js"
        );
        assert_eq!(host_shim_module("lcd", false), "lightloom:prelude/modules/lcd.js");
    }
}
