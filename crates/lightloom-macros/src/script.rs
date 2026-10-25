// SPDX-License-Identifier: GPL-3.0-or-later
//! One macro script in its own QuickJS-NG runtime (memory cap, deadline
//! interrupt), the way device plugins run (`lightloom_plugin_host::runtime`).
//!
//! The script is an ES module exporting `Action()` or `Input()`; the host
//! (`js/macro_host.js`) instantiates it with `new` and keeps the instance out
//! of the script's reach. Scripts may not import anything. Every global
//! member reaches Rust through [`Natives`]; time spent inside a native (an
//! engine request, an injection) does not count against the JavaScript
//! budget. A runtime lives and dies on one thread.

use lightloom_plugin_host::runtime::{failure, read_source, Budget, Deadline, JsFailure};
use rquickjs::{Context, Ctx, Function, Module, Object, Runtime};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const HOST_SOURCE: &str = include_str!("../js/macro_host.js");

/// Heap cap of one macro runtime.
pub const MEMORY_LIMIT: usize = 32 * 1024 * 1024;
/// QuickJS stack cap.
pub const MAX_STACK: usize = 2 * 1024 * 1024;
/// OS stack of a macro thread.
pub const THREAD_STACK: usize = 8 * 1024 * 1024;
/// Module evaluation and the constructor.
pub const LOAD_BUDGET: Duration = Duration::from_secs(2);

/// Budget of one handler call: JavaScript time, and wall-clock time with
/// natives included (an engine request waits for the engine).
pub fn call_budget() -> Budget {
    Budget {
        js: Duration::from_secs(2),
        wall: Duration::from_secs(15),
    }
}

/// A native's error: the JavaScript class thrown and its message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeError {
    /// `TypeError`, `MacroRefused`, `MacroError`.
    pub class: &'static str,
    /// The message.
    pub message: String,
}

impl NativeError {
    /// A call the policy refuses (`MacroRefused`).
    pub fn refused(message: impl Into<String>) -> Self {
        Self {
            class: "MacroRefused",
            message: message.into(),
        }
    }

    /// A call that failed (`MacroError`).
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            class: "MacroError",
            message: message.into(),
        }
    }

    /// A bad argument (`TypeError`).
    pub fn type_error(message: impl Into<String>) -> Self {
        Self {
            class: "TypeError",
            message: message.into(),
        }
    }
}

/// What a native answers.
pub type NativeResult = Result<Value, NativeError>;

/// The Rust side of the macro globals (one per script).
pub trait Natives: Send + Sync + 'static {
    /// `system.action(path)`.
    fn action(&self, path: &str) -> NativeResult;
    /// `system.openUrl(url)`.
    fn open_url(&self, url: &str) -> NativeResult;
    /// `system.openApplication(path, args, startIn)`: `args` is an array of
    /// strings or one string (a command line).
    fn open_application(&self, path: &str, args: &Value, cwd: &str) -> NativeResult;
    /// `system.prompt(options, cb)`: the prompt id, or -1 when nobody can answer.
    fn prompt(&self, options: &Value) -> NativeResult;
    /// `system.copyToClipboard(text)`.
    fn clipboard(&self, text: &str) -> NativeResult;
    /// `signalrgb.<method>(...args)`.
    fn signalrgb(&self, method: &str, args: &[Value]) -> NativeResult;
    /// `keys.<op>(arg)`: `press`, `hold`, `release` (a combo or a code),
    /// `paste`, `unicode` (text).
    fn keys(&self, op: &str, arg: &Value) -> NativeResult;
    /// `mouse.click(button)`.
    fn click(&self, arg: &Value) -> NativeResult;
    /// `block.trigger()`.
    fn trigger(&self) -> NativeResult;
    /// `console.<level>(...)`.
    fn log(&self, level: &str, text: &str);
}

/// [`Natives`] that refuse everything: scripts are inspected (listed) with them.
pub struct Inspecting;

impl Natives for Inspecting {
    fn action(&self, _: &str) -> NativeResult {
        Err(refused_inspecting())
    }
    fn open_url(&self, _: &str) -> NativeResult {
        Err(refused_inspecting())
    }
    fn open_application(&self, _: &str, _: &Value, _: &str) -> NativeResult {
        Err(refused_inspecting())
    }
    fn prompt(&self, _: &Value) -> NativeResult {
        Err(refused_inspecting())
    }
    fn clipboard(&self, _: &str) -> NativeResult {
        Err(refused_inspecting())
    }
    fn signalrgb(&self, _: &str, _: &[Value]) -> NativeResult {
        Err(refused_inspecting())
    }
    fn keys(&self, _: &str, _: &Value) -> NativeResult {
        Err(refused_inspecting())
    }
    fn click(&self, _: &Value) -> NativeResult {
        Err(refused_inspecting())
    }
    fn trigger(&self) -> NativeResult {
        Err(refused_inspecting())
    }
    fn log(&self, _: &str, _: &str) {}
}

fn refused_inspecting() -> NativeError {
    NativeError::refused("not available while the script is only being listed")
}

/// What the constructor declared.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Described {
    /// `action` or `input`.
    pub kind: String,
    /// `this.Name`.
    #[serde(default)]
    pub name: Option<String>,
    /// `this.Description`.
    #[serde(default)]
    pub description: Option<String>,
    /// `this.Options` as JSON (`null` when absent).
    #[serde(default)]
    pub options: Value,
    /// Handler functions the instance has.
    #[serde(default)]
    pub handlers: Vec<String>,
}

fn encode(r: NativeResult) -> String {
    match r {
        Ok(v) => serde_json::json!({ "ok": v }).to_string(),
        Err(e) => serde_json::json!({ "err": [e.class, e.message] }).to_string(),
    }
}

fn panic_text(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic".into())
}

fn host_failure(message: impl Into<String>) -> JsFailure {
    JsFailure {
        class: "HostError".into(),
        message: message.into(),
    }
}

fn install<'js>(ctx: &Ctx<'js>, natives: &Arc<dyn Natives>, deadline: &Deadline) -> rquickjs::Result<()> {
    let obj = Object::new(ctx.clone())?;
    macro_rules! native {
        ($name:literal, |$n:ident $(, $arg:ident : $ty:ty)*| $body:expr) => {{
            let shared = Arc::clone(natives);
            let deadline = deadline.clone();
            let f = Function::new(ctx.clone(), move |$($arg: $ty),*| -> String {
                let _blocked = deadline.suspend();
                let $n: &dyn Natives = &*shared;
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body))
                    .unwrap_or_else(|p| encode(Err(NativeError::failed(format!("host panic: {}", panic_text(p.as_ref()))))))
            })?;
            obj.set($name, f)?;
        }};
    }
    let parse = |s: &str| serde_json::from_str::<Value>(s).unwrap_or(Value::Null);
    native!("action", |n, path: String| encode(n.action(&path)));
    native!("openUrl", |n, url: String| encode(n.open_url(&url)));
    native!("openApplication", |n, path: String, args: String, cwd: String| {
        encode(n.open_application(&path, &parse(&args), &cwd))
    });
    native!("prompt", |n, options: String| encode(n.prompt(&parse(&options))));
    native!("clipboard", |n, text: String| encode(n.clipboard(&text)));
    native!("signalrgb", |n, method: String, args: String| {
        let args = match parse(&args) {
            Value::Array(a) => a,
            _ => Vec::new(),
        };
        encode(n.signalrgb(&method, &args))
    });
    native!("keys", |n, op: String, arg: String| encode(
        n.keys(&op, &parse(&arg))
    ));
    native!("click", |n, arg: String| encode(n.click(&parse(&arg))));
    native!("trigger", |n| encode(n.trigger()));
    native!("unsupported", |_n, what: String| encode(Err(
        NativeError::failed(format!("{what}() is not supported by Lightloom"))
    )));
    native!("log", |n, level: String, text: String| {
        n.log(&level, &text);
        encode(Ok(Value::Null))
    });
    ctx.globals().set("__macroNative", obj)
}

/// One loaded macro script.
pub struct ScriptRuntime {
    // Field order matters: the context must drop before the runtime.
    context: Context,
    _runtime: Runtime,
    deadline: Deadline,
}

fn host<'js>(ctx: &Ctx<'js>) -> rquickjs::Result<Object<'js>> {
    ctx.globals().get("__macroHost")
}

impl ScriptRuntime {
    /// Create the runtime, install the globals over `natives`, evaluate the
    /// module at `path` and instantiate its constructor.
    pub fn load(path: &Path, natives: Arc<dyn Natives>) -> Result<(Self, Described), JsFailure> {
        let source = read_source(path).map_err(|e| JsFailure {
            class: "IoError".into(),
            message: e.to_string(),
        })?;
        let runtime = Runtime::new().map_err(|e| host_failure(e.to_string()))?;
        runtime.set_memory_limit(MEMORY_LIMIT);
        runtime.set_max_stack_size(MAX_STACK);
        let deadline = Deadline::default();
        let d = deadline.clone();
        runtime.set_interrupt_handler(Some(Box::new(move || d.poll())));
        let context = Context::full(&runtime).map_err(|e| host_failure(e.to_string()))?;
        let this = Self {
            context,
            _runtime: runtime,
            deadline,
        };
        let name = path.to_string_lossy().into_owned();
        this.deadline.arm(LOAD_BUDGET);
        let described = this.context.with(|ctx| -> Result<String, JsFailure> {
            let d = &this.deadline;
            install(&ctx, &natives, d).map_err(|e| failure(&ctx, e, d))?;
            ctx.eval::<(), _>(HOST_SOURCE).map_err(|e| failure(&ctx, e, d))?;
            let run = || -> rquickjs::Result<String> {
                let (module, promise) = Module::declare(ctx.clone(), name.as_str(), source)?.eval()?;
                promise.finish::<()>()?;
                let ns = module.namespace()?;
                let json: String = host(&ctx)?.get::<_, Function>("load")?.call((ns,))?;
                while ctx.execute_pending_job() {}
                Ok(json)
            };
            match run().map_err(|e| failure(&ctx, e, d)) {
                Ok(_) if d.fired() => Err(JsFailure {
                    class: "Timeout".into(),
                    message: "interrupted (loading the script exceeded its deadline)".into(),
                }),
                r => r,
            }
        });
        this.deadline.clear();
        let described: Described =
            serde_json::from_str(&described?).map_err(|e| host_failure(format!("describe: {e}")))?;
        Ok((this, described))
    }

    fn call<T>(
        &self,
        what: &str,
        budget: Budget,
        f: impl for<'js> FnOnce(&Ctx<'js>, &Object<'js>) -> rquickjs::Result<T>,
    ) -> Result<T, JsFailure> {
        let d = &self.deadline;
        d.arm(budget);
        let r = self.context.with(|ctx| {
            let run = || -> rquickjs::Result<T> {
                let h = host(&ctx)?;
                let v = f(&ctx, &h)?;
                while ctx.execute_pending_job() {}
                Ok(v)
            };
            match run().map_err(|e| failure(&ctx, e, d)) {
                Ok(_) if d.fired() => Err(JsFailure {
                    class: "Timeout".into(),
                    message: format!("interrupted ({what} exceeded its deadline)"),
                }),
                r => r,
            }
        });
        d.clear();
        r
    }

    /// Write option values onto the instance (`this.<property>`).
    pub fn set_options(&self, values: &Map<String, Value>) -> Result<(), JsFailure> {
        let json = Value::Object(values.clone()).to_string();
        self.call("setOptions", Budget::from(Duration::from_secs(1)), |_, h| {
            h.get::<_, Function>("setOptions")?.call::<_, ()>((json,))
        })
    }

    /// `onExecute()`.
    pub fn execute(&self) -> Result<(), JsFailure> {
        self.call("onExecute", call_budget(), |_, h| {
            h.get::<_, Function>("execute")?.call::<_, ()>(())
        })
    }

    /// `handler(event)`; `true` when it returned `true` (swallow the input).
    pub fn handle(&self, handler: &str, event: &Value) -> Result<bool, JsFailure> {
        let json = event.to_string();
        let handler = handler.to_string();
        self.call("a handler", call_budget(), move |_, h| {
            h.get::<_, Function>("handle")?
                .call::<_, i32>((handler, json))
                .map(|r| r == 1)
        })
    }

    /// Complete prompt `id` with `text` (`None`: cancelled).
    pub fn answer(&self, id: u64, text: Option<&str>) -> Result<(), JsFailure> {
        let text = text.map(str::to_string);
        self.call("a prompt callback", call_budget(), move |ctx, h| {
            let value = match text {
                Some(t) => rquickjs::String::from_str(ctx.clone(), &t)?.into_value(),
                None => rquickjs::Value::new_null(ctx.clone()),
            };
            h.get::<_, Function>("answer")?.call::<_, ()>((id as f64, value))
        })
    }

    /// The deadline (a stop request cuts the current call off).
    pub fn deadline(&self) -> &Deadline {
        &self.deadline
    }
}

/// Load `path` with refusing natives on a thread of its own and report what
/// it declares (the macro list).
pub fn inspect(path: &Path) -> Result<Described, JsFailure> {
    let path = path.to_path_buf();
    std::thread::scope(|s| {
        std::thread::Builder::new()
            .name("lightloom-macro-scan".into())
            .stack_size(THREAD_STACK)
            .spawn_scoped(s, move || {
                ScriptRuntime::load(&path, Arc::new(Inspecting)).map(|(_, d)| d)
            })
            .map_err(|e| host_failure(e.to_string()))?
            .join()
            .unwrap_or_else(|p| Err(host_failure(format!("panic: {}", panic_text(p.as_ref())))))
    })
}
