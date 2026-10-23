// SPDX-License-Identifier: GPL-3.0-or-later
//! Network services: the discovery side of a `Type() == "network"` plugin
//! (docs/host-api/network-huedeck.md, "Service context").
//!
//! [`spawn`] starts one thread per plugin with its own QuickJS runtime and
//! [`NetContext`] that:
//!
//! 1. evaluates the plugin and constructs `discovery = new DiscoveryService()`
//!    (the host module's `startService()`), reading its `MDns`,
//!    `UdpBroadcastAddress`/`Port` and `UdpListenPort`;
//! 2. opens the discovery sockets and an [`MdnsBrowser`] for the `MDns` types;
//! 3. calls `Initialize()`, then every [`SERVICE_TICK`] delivers socket events
//!    and asynchronous XHRs (`pumpNet()`), mDNS answers (`Discovered` /
//!    `Removal`) and the dashboard's commands, and every [`UPDATE_EVERY`]
//!    calls `Update()`;
//! 4. on stop, simply ends: the runtime and the sockets go with the thread.
//!
//! What the plugin does with `service.*` lands in a shared [`ServiceState`]:
//! the controllers the dashboard lists and the controllers announced as
//! devices, each change bumping [`ServiceState::generation`]. Settings go
//! through a [`PluginSettings`] store the daemon persists.

use crate::catalog::CatalogEntry;
use crate::device::{panic_text, LogSink, WriteMode};
use crate::meta::PluginType;
use crate::net::mdns::{MdnsBrowser, MdnsEvent};
use crate::net::{lock_net, NetContext, NetRole, SharedNet, SERVICE_TICK};
use crate::runtime::{Budget, PluginRuntime, RuntimeOptions, PLUGIN_THREAD_STACK};
use rquickjs::function::Opt;
use rquickjs::{Ctx, Function, Object};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Period of `discovery.Update()`.
pub const UPDATE_EVERY: Duration = Duration::from_secs(1);
/// Log lines a service keeps for the dashboard.
const LOG_TAIL: usize = 30;
/// Most controllers before the leak warning (SignalRGB's limit).
pub const CONTROLLER_WARNING: usize = 50;
/// Longest controller data accepted (bytes of JSON).
const MAX_CONTROLLER_JSON: usize = 256 << 10;
/// Longest setting value (bytes).
pub const MAX_SETTING_LEN: usize = 64 << 10;

/// Persistent per-plugin settings (`service.getSetting` / `saveSetting`).
pub trait PluginSettings: Send + Sync {
    /// The value of `plugin`'s `id`/`key`.
    fn get(&self, plugin: &str, id: &str, key: &str) -> Option<String>;
    /// Store (`Some`) or delete (`None`) it.
    fn set(&self, plugin: &str, id: &str, key: &str, value: Option<&str>);
}

/// Settings kept in memory only (tests, one-off runs).
#[derive(Debug, Default)]
pub struct MemorySettings(Mutex<BTreeMap<(String, String, String), String>>);

impl PluginSettings for MemorySettings {
    fn get(&self, plugin: &str, id: &str, key: &str) -> Option<String> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(plugin.into(), id.into(), key.into()))
            .cloned()
    }

    fn set(&self, plugin: &str, id: &str, key: &str, value: Option<&str>) {
        let mut m = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let k = (plugin.to_string(), id.to_string(), key.to_string());
        match value {
            Some(v) => {
                m.insert(k, v.to_string());
            }
            None => {
                m.remove(&k);
            }
        }
    }
}

/// One entry of `service.controllers`, as the dashboard sees it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ControllerView {
    /// Its `id`.
    pub id: String,
    /// Its plain data (functions dropped).
    pub data: Value,
    /// It has a `startLink()` method.
    #[serde(rename = "canPair")]
    pub can_pair: bool,
}

/// What a service shows: its controllers, the announced ones, its log.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ServiceState {
    /// Bumped on every change of `controllers` or `announced`.
    pub generation: u64,
    /// The service thread runs.
    pub running: bool,
    /// Why it stopped or could not start.
    pub error: Option<String>,
    /// `IconUrl`.
    pub icon_url: String,
    /// `discovery.forgetController` exists.
    pub can_forget: bool,
    /// `service.controllers`.
    pub controllers: Vec<ControllerView>,
    /// Announced controllers by id: the data their device context gets.
    pub announced: BTreeMap<String, Value>,
    /// Last log lines.
    pub log_tail: VecDeque<String>,
}

impl ServiceState {
    fn log(&mut self, line: String) {
        if self.log_tail.len() >= LOG_TAIL {
            self.log_tail.pop_front();
        }
        self.log_tail.push_back(line);
    }
}

/// What the dashboard asks a service to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServiceCommand {
    /// *Add by IP*: `discovery.forceDiscover(ip)` (or a synthetic `Discovered`).
    Add(String),
    /// *Pair*: the controller's `startLink()`.
    Pair(String),
    /// *Forget*: `discovery.forgetController(id)`.
    Forget(String),
}

/// The natives behind `service.*` (shared by the runtime and the thread).
#[derive(Clone)]
pub struct ServiceNatives {
    plugin: String,
    settings: Arc<dyn PluginSettings>,
    state: Arc<Mutex<ServiceState>>,
    log: Option<LogSink>,
}

impl std::fmt::Debug for ServiceNatives {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceNatives")
            .field("plugin", &self.plugin)
            .finish_non_exhaustive()
    }
}

fn lock_state(s: &Mutex<ServiceState>) -> MutexGuard<'_, ServiceState> {
    s.lock().unwrap_or_else(|e| e.into_inner())
}

impl ServiceNatives {
    /// Natives for `plugin` (its `Name()`, the settings namespace).
    pub fn new(plugin: impl Into<String>, settings: Arc<dyn PluginSettings>, log: Option<LogSink>) -> Self {
        Self {
            plugin: plugin.into(),
            settings,
            state: Arc::new(Mutex::new(ServiceState::default())),
            log,
        }
    }

    /// The state the natives write.
    pub fn state(&self) -> Arc<Mutex<ServiceState>> {
        Arc::clone(&self.state)
    }

    fn log(&self, text: &str) {
        let line: String = text.chars().take(1000).collect();
        lock_state(&self.state).log(line.clone());
        if let Some(sink) = &self.log {
            sink(&line);
        }
    }
}

/// Install the service natives (`log`, `svc*`) on `natives`.
pub(crate) fn install<'js>(
    ctx: &Ctx<'js>,
    natives: &Object<'js>,
    svc: &ServiceNatives,
) -> rquickjs::Result<()> {
    macro_rules! native {
        ($name:literal, |$s:ident $(, $arg:ident : $ty:ty)*| $body:expr) => {{
            let shared = svc.clone();
            let f = Function::new(ctx.clone(), move |cx: Ctx<'_>, $($arg: $ty),*| -> rquickjs::Result<_> {
                let $s: &ServiceNatives = &shared;
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)).map_err(|panic| {
                    let msg = format!("host panic in service.{}: {}", $name, panic_text(panic.as_ref()));
                    rquickjs::Exception::throw_internal(&cx, &msg)
                })
            })?;
            natives.set($name, f)?;
        }};
    }
    native!("log", |s, text: String| s.log(&text));
    native!("svcControllers", |s, json: String| {
        let list: Vec<ControllerView> = serde_json::from_str(&json).unwrap_or_default();
        let mut st = lock_state(&s.state);
        if st.controllers != list {
            st.controllers = list;
            st.generation += 1;
        }
    });
    native!("svcAnnounce", |s, id: String, json: String| {
        if json.len() > MAX_CONTROLLER_JSON {
            s.log(&format!(
                "announceController(): controller {id} is too large ({} bytes)",
                json.len()
            ));
            return false;
        }
        let Ok(data) = serde_json::from_str::<Value>(&json) else {
            return false;
        };
        let mut st = lock_state(&s.state);
        if st.announced.get(&id) != Some(&data) {
            st.announced.insert(id, data);
            st.generation += 1;
        }
        true
    });
    native!("svcSuppress", |s, id: String| {
        let mut st = lock_state(&s.state);
        if st.announced.remove(&id).is_some() {
            st.generation += 1;
        }
    });
    native!("svcGetSetting", |s, id: String, key: String| s
        .settings
        .get(&s.plugin, &id, &key));
    native!(
        "svcSaveSetting",
        |s, id: String, key: String, value: Opt<String>| {
            match value.0 {
                Some(v) if v.len() > MAX_SETTING_LEN => {
                    s.log(&format!(
                        "saveSetting(): {id}/{key} is longer than {MAX_SETTING_LEN} bytes: not saved"
                    ));
                }
                v => s.settings.set(&s.plugin, &id, &key, v.as_deref()),
            }
        }
    );
    Ok(())
}

/// How a service runs.
#[derive(Clone)]
pub struct ServiceOptions {
    /// Settings store.
    pub settings: Arc<dyn PluginSettings>,
    /// Receives every log line.
    pub log_sink: Option<LogSink>,
    /// Module evaluation.
    pub load_budget: Duration,
    /// Each call into the plugin.
    pub call_budget: Budget,
}

impl Default for ServiceOptions {
    fn default() -> Self {
        Self {
            settings: Arc::new(MemorySettings::default()),
            log_sink: None,
            load_budget: Duration::from_secs(10),
            call_budget: Budget {
                js: Duration::from_secs(2),
                wall: Duration::from_secs(15),
            },
        }
    }
}

/// A running service.
pub struct ServiceHandle {
    name: String,
    plugin: CatalogEntry,
    stop: Arc<AtomicBool>,
    commands: Sender<ServiceCommand>,
    state: Arc<Mutex<ServiceState>>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for ServiceHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceHandle")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl ServiceHandle {
    /// The plugin's `Name()`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The plugin.
    pub fn plugin(&self) -> &CatalogEntry {
        &self.plugin
    }

    /// A copy of the state.
    pub fn state(&self) -> ServiceState {
        lock_state(&self.state).clone()
    }

    /// The state's generation (cheap change check).
    pub fn generation(&self) -> u64 {
        lock_state(&self.state).generation
    }

    /// Queue a dashboard command; `false` when the service has ended.
    pub fn command(&self, c: ServiceCommand) -> bool {
        self.commands.send(c).is_ok()
    }

    /// Ask the thread to end.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }

    /// Stop and wait at most `timeout` (a thread stuck in a plugin call is left behind).
    pub fn shutdown(mut self, timeout: Duration) {
        self.stop();
        let end = Instant::now() + timeout;
        if let Some(t) = self.thread.take() {
            while !t.is_finished() && Instant::now() < end {
                std::thread::sleep(Duration::from_millis(10));
            }
            if t.is_finished() {
                let _ = t.join();
            }
        }
    }
}

impl Drop for ServiceHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The discovery settings `startService()` reports.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    #[serde(default)]
    icon_url: String,
    #[serde(default)]
    mdns: Vec<String>,
    #[serde(default)]
    broadcast_address: String,
    #[serde(default)]
    broadcast_port: u32,
    #[serde(default)]
    listen_port: u32,
    #[serde(default)]
    can_forget: bool,
}

/// Start the service of the network plugin `plugin`.
pub fn spawn(plugin: CatalogEntry, opts: ServiceOptions) -> std::io::Result<ServiceHandle> {
    let name = plugin.meta.name.clone();
    let natives = ServiceNatives::new(name.clone(), Arc::clone(&opts.settings), opts.log_sink.clone());
    let state = natives.state();
    lock_state(&state).running = true;
    let stop = Arc::new(AtomicBool::new(false));
    let (commands, rx) = channel();
    let thread = {
        let (plugin, stop, state) = (plugin.clone(), Arc::clone(&stop), Arc::clone(&state));
        std::thread::Builder::new()
            .name(format!("huedeck-service {name}"))
            .stack_size(PLUGIN_THREAD_STACK)
            .spawn(move || {
                let error = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(&plugin, &opts, natives, &stop, &rx)
                })) {
                    Ok(r) => r.err(),
                    Err(p) => Some(format!("service thread panicked: {}", panic_text(p.as_ref()))),
                };
                let mut st = lock_state(&state);
                st.running = false;
                if let Some(e) = error {
                    st.log(e.clone());
                    st.error = Some(e);
                }
                st.announced.clear();
                st.generation += 1;
            })?
    };
    Ok(ServiceHandle {
        name,
        plugin,
        stop,
        commands,
        state,
        thread: Some(thread),
    })
}

fn call(
    rt: &PluginRuntime,
    natives: &ServiceNatives,
    function: &str,
    arg: Option<&str>,
    opts: &ServiceOptions,
    stop: &Arc<AtomicBool>,
) -> Option<String> {
    match rt.call_host(function, arg, opts.call_budget, Some(stop)) {
        Ok(r) => r,
        Err(e) if e.is_stop() => None,
        Err(e) => {
            natives.log(&format!("{function}: {e}"));
            None
        }
    }
}

fn run(
    plugin: &CatalogEntry,
    opts: &ServiceOptions,
    natives: ServiceNatives,
    stop: &Arc<AtomicBool>,
    commands: &Receiver<ServiceCommand>,
) -> Result<(), String> {
    if plugin.meta.plugin_type != PluginType::Network {
        return Err("not a network plugin".into());
    }
    let mut net = NetContext::new(NetRole::Service, WriteMode::Live);
    let sink_natives = natives.clone();
    net.set_log_sink(Some(Arc::new(move |line: &str| sink_natives.log(line))));
    let net: SharedNet = net.shared();
    let ropts = RuntimeOptions {
        plugin_root: Some(plugin.root.clone()),
        module_stubs: false,
        net: Some(Arc::clone(&net)),
        service: Some(natives.clone()),
        ..RuntimeOptions::default()
    };
    let rt = PluginRuntime::new(&plugin.meta.source_path, &ropts).map_err(|e| e.to_string())?;
    let loaded = rt.load(opts.load_budget);
    if let Some(e) = loaded.error {
        return Err(format!("load: {e}"));
    }
    let config = rt
        .call_host("startService", None, opts.call_budget, Some(stop))
        .map_err(|e| format!("new DiscoveryService(): {e}"))?
        .and_then(|j| serde_json::from_str::<Config>(&j).ok())
        .unwrap_or_default();
    {
        let mut st = lock_state(&natives.state);
        st.icon_url = config.icon_url.clone();
        st.can_forget = config.can_forget;
    }
    let broadcast = (config.broadcast_port > 0).then(|| {
        let ip = config
            .broadcast_address
            .parse::<IpAddr>()
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::BROADCAST));
        SocketAddr::new(ip, config.broadcast_port.min(65535) as u16)
    });
    let listen = (config.listen_port > 0 && config.listen_port <= 65535).then_some(config.listen_port as u16);
    for note in lock_net(&net).open_discovery(broadcast, listen) {
        natives.log(&note);
    }
    let mut browser = (!config.mdns.is_empty()).then(|| MdnsBrowser::new(&config.mdns));
    if let Some(e) = browser.as_ref().and_then(|b| b.error()) {
        natives.log(e);
    }

    call(
        &rt,
        &natives,
        "callDiscovery",
        Some(r#"{"name":"Initialize","args":[]}"#),
        opts,
        stop,
    );
    let mut next_update = Instant::now() + UPDATE_EVERY;
    while !stop.load(Ordering::Acquire) {
        let tick = Instant::now() + SERVICE_TICK;
        while let Ok(c) = commands.try_recv() {
            let (function, arg) = match c {
                ServiceCommand::Add(ip) => ("addByIp", ip),
                ServiceCommand::Pair(id) => ("pairController", id),
                ServiceCommand::Forget(id) => ("forgetController", id),
            };
            call(&rt, &natives, function, Some(&arg), opts, stop);
        }
        call(&rt, &natives, "pumpNet", None, opts, stop);
        if let Some(b) = &mut browser {
            for ev in b.poll(Instant::now()) {
                let (name, s) = match ev {
                    MdnsEvent::Found(s) => ("Discovered", s),
                    MdnsEvent::Lost(s) => ("Removal", s),
                };
                let arg = serde_json::json!({ "name": name, "args": [s.to_value()] }).to_string();
                call(&rt, &natives, "callDiscovery", Some(&arg), opts, stop);
            }
        }
        if Instant::now() >= next_update {
            next_update = Instant::now() + UPDATE_EVERY;
            call(
                &rt,
                &natives,
                "callDiscovery",
                Some(r#"{"name":"Update","args":[]}"#),
                opts,
                stop,
            );
        }
        let left = crate::net::until(tick);
        if !left.is_zero() {
            std::thread::sleep(left);
        }
    }
    Ok(())
}
