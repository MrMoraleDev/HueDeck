// SPDX-License-Identifier: GPL-3.0-or-later
//! The sensor poller: one thread, `lightloom-sensors`, polls the sensor
//! registry ([`lightloom_runtime::sensors`]) at most once a second, and only
//! while something reads sensors: the active effect (its parsed
//! `uses.sensors`, values pushed to the renderer as `engine.sensors`), an LCD
//! face that reads them (pushed to its face renderer, `crate::lcd`), or a
//! client of `GET /v1/sensors` (each request keeps polling on for
//! [`LEASE`]). Otherwise the thread sleeps and no source is touched; the
//! sources (NvAPI included) are opened at the first poll (docs/sensors.md).
//!
//! Read-only: no source writes anything, and the plugin fan API stays a no-op.

use crate::config::SensorsConfig;
use lightloom_api::model::{SensorSourceView, SensorView, SensorsView};
use lightloom_hal::sensors::SensorReading;
use lightloom_runtime::sensors::{SensorRegistry, SourceStatus};
use serde_json::{json, Map, Value};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Shortest poll interval (1 Hz).
pub const MIN_INTERVAL: Duration = Duration::from_millis(1000);
/// How long one `GET /v1/sensors` keeps polling on.
pub const LEASE: Duration = Duration::from_secs(10);
/// How long `GET /v1/sensors` waits for a first poll.
pub const FIRST_POLL_WAIT: Duration = Duration::from_millis(1500);

/// Where an effect's sensor values go (the renderer's `engine` command).
pub type Push = Box<dyn Fn(Map<String, Value>) + Send + Sync>;
/// Opens the sources at the first poll (tests: fakes).
pub type Open = Box<dyn FnOnce() -> SensorRegistry + Send>;

#[derive(Default)]
struct State {
    readings: Vec<SensorReading>,
    sources: Vec<SourceStatus>,
    polled_at: Option<Instant>,
    polls: u64,
    lease_until: Option<Instant>,
    effect: Option<Arc<Push>>,
    /// The LCD faces that read sensors.
    faces: Option<Arc<Push>>,
    stop: bool,
}

impl State {
    fn wanted(&self, now: Instant) -> bool {
        self.effect.is_some() || self.faces.is_some() || self.lease_until.is_some_and(|t| t > now)
    }
}

struct Inner {
    state: Mutex<State>,
    wake: Condvar,
}

fn lock(m: &Mutex<State>) -> std::sync::MutexGuard<'_, State> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The poller (shared by the engine and the API).
pub struct SensorPoller {
    inner: Arc<Inner>,
    enabled: bool,
    interval: Duration,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for SensorPoller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SensorPoller")
            .field("enabled", &self.enabled)
            .field("interval", &self.interval)
            .finish()
    }
}

impl SensorPoller {
    /// A poller for `cfg`; `open` creates the registry at the first poll.
    /// Disabled (`[sensors] enabled = false`): no thread, nothing is read.
    pub fn new(cfg: &SensorsConfig, open: Open) -> Self {
        let interval = Duration::from_millis(cfg.interval_ms).max(MIN_INTERVAL);
        let inner = Arc::new(Inner {
            state: Mutex::new(State::default()),
            wake: Condvar::new(),
        });
        let thread = cfg.enabled.then(|| {
            let inner = Arc::clone(&inner);
            std::thread::Builder::new()
                .name("lightloom-sensors".into())
                .spawn(move || run(&inner, interval, open))
                .ok()
        });
        Self {
            inner,
            enabled: cfg.enabled,
            interval,
            thread: Mutex::new(thread.flatten()),
        }
    }

    /// The active effect reads sensors (`Some`: push its values there) or
    /// not. The last values, when there are any, are pushed at once.
    pub fn set_effect(&self, push: Option<Push>) {
        let push = push.filter(|_| self.enabled).map(Arc::new);
        let mut st = lock(&self.inner.state);
        if let (Some(p), false) = (&push, st.readings.is_empty()) {
            p(engine_data(&st.readings));
        }
        st.effect = push;
        drop(st);
        self.inner.wake.notify_all();
    }

    /// LCD faces read sensors (`Some`: push their values there) or not. The
    /// last values, when there are any, are pushed at once.
    pub fn set_faces(&self, push: Option<Push>) {
        let push = push.filter(|_| self.enabled).map(Arc::new);
        let mut st = lock(&self.inner.state);
        if let (Some(p), false) = (&push, st.readings.is_empty()) {
            p(engine_data(&st.readings));
        }
        st.faces = push;
        drop(st);
        self.inner.wake.notify_all();
    }

    /// `GET /v1/sensors`: keep polling on for [`LEASE`] and answer the last
    /// values, waiting up to [`FIRST_POLL_WAIT`] when there are none yet.
    pub fn view(&self) -> SensorsView {
        self.view_waiting(FIRST_POLL_WAIT)
    }

    fn view_waiting(&self, wait: Duration) -> SensorsView {
        let now = Instant::now();
        let mut st = lock(&self.inner.state);
        if self.enabled {
            st.lease_until = Some(now + LEASE);
            self.inner.wake.notify_all();
            if st.polled_at.is_none() {
                let polls = st.polls;
                st = self
                    .inner
                    .wake
                    .wait_timeout_while(st, wait, |s| s.polls == polls && !s.stop)
                    .map(|(g, _)| g)
                    .unwrap_or_else(|e| e.into_inner().0);
            }
        }
        SensorsView {
            enabled: self.enabled,
            polling: self.enabled && st.wanted(Instant::now()),
            interval_ms: self.interval.as_millis() as u64,
            age_ms: st.polled_at.map(|t| t.elapsed().as_millis() as u64),
            sensors: st.readings.iter().map(view).collect(),
            sources: st
                .sources
                .iter()
                .map(|s| SensorSourceView {
                    name: s.name.clone(),
                    sensors: s.sensors,
                    error: s.error.clone(),
                })
                .collect(),
        }
    }

    /// Stop the thread (daemon shutdown).
    pub fn stop(&self) {
        lock(&self.inner.state).stop = true;
        self.inner.wake.notify_all();
        let thread = self.thread.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(t) = thread {
            let _ = t.join();
        }
    }
}

impl Drop for SensorPoller {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run(inner: &Inner, interval: Duration, open: Open) {
    let mut open = Some(open);
    let mut registry: Option<SensorRegistry> = None;
    loop {
        // Sleep until somebody reads sensors.
        let mut st = lock(&inner.state);
        loop {
            if st.stop {
                return;
            }
            let now = Instant::now();
            if st.wanted(now) {
                break;
            }
            st = match st.lease_until.filter(|t| *t > now) {
                Some(t) => inner
                    .wake
                    .wait_timeout(st, t - now)
                    .map_or_else(|e| e.into_inner().0, |(g, _)| g),
                None => inner.wake.wait(st).unwrap_or_else(|e| e.into_inner()),
            };
        }
        drop(st);
        let reg = registry.get_or_insert_with(|| {
            open.take()
                .map(|f| f())
                .unwrap_or_else(|| SensorRegistry::new(Vec::new()))
        });
        let readings = reg.poll();
        let mut st = lock(&inner.state);
        st.sources = reg.sources().to_vec();
        st.readings = readings;
        st.polled_at = Some(Instant::now());
        st.polls += 1;
        let pushes: Vec<Arc<Push>> = st.effect.iter().chain(st.faces.iter()).cloned().collect();
        let data = (!pushes.is_empty()).then(|| engine_data(&st.readings));
        drop(st);
        inner.wake.notify_all();
        if let Some(data) = data {
            for push in pushes {
                push(data.clone());
            }
        }
        // Wait out the interval (a stop ends it early).
        let st = lock(&inner.state);
        let _ = inner.wake.wait_timeout_while(st, interval, |s| !s.stop);
    }
}

fn view(r: &SensorReading) -> SensorView {
    SensorView {
        name: r.name.clone(),
        value: r.value,
        units: r.units.clone(),
        min: r.min,
        max: r.max,
        kind: r.kind.label().into(),
    }
}

/// `{"sensors": {name: {value, units, min, max, type}}}` for the renderer
/// (`engine.getSensorValue(name)` returns the inner object).
pub fn engine_data(readings: &[SensorReading]) -> Map<String, Value> {
    let sensors: Map<String, Value> = readings
        .iter()
        .map(|r| {
            (
                r.name.clone(),
                json!({
                    "value": r.value,
                    "units": r.units,
                    "min": r.min,
                    "max": r.max,
                    "type": r.kind.label(),
                }),
            )
        })
        .collect();
    let mut data = Map::new();
    data.insert("sensors".into(), Value::Object(sensors));
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_hal::sensors::{SensorBackend, SensorKind};
    use lightloom_hal::HalResult;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Counts its polls; reports `CPU Load` = the poll number.
    struct Counting(Arc<AtomicUsize>);

    impl SensorBackend for Counting {
        fn name(&self) -> &str {
            "fake"
        }
        fn poll(&mut self) -> HalResult<Vec<SensorReading>> {
            let n = self.0.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(vec![SensorReading {
                name: "CPU Load".into(),
                value: n as f64,
                units: "%".into(),
                min: 0.0,
                max: 100.0,
                kind: SensorKind::Load,
            }])
        }
    }

    fn poller(cfg: &SensorsConfig) -> (SensorPoller, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let polls = Arc::new(AtomicUsize::new(0));
        let opened = Arc::new(AtomicUsize::new(0));
        let (p, o) = (Arc::clone(&polls), Arc::clone(&opened));
        let open: Open = Box::new(move || {
            o.fetch_add(1, Ordering::SeqCst);
            SensorRegistry::new(vec![Box::new(Counting(p))])
        });
        (SensorPoller::new(cfg, open), polls, opened)
    }

    #[test]
    fn idle_until_somebody_reads() {
        let (p, polls, opened) = poller(&SensorsConfig::default());
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            opened.load(Ordering::SeqCst),
            0,
            "no source opened while nobody reads"
        );
        let v = p.view();
        assert!(v.enabled && v.polling);
        assert_eq!(v.interval_ms, 1000);
        assert_eq!(v.sensors.len(), 1, "the first poll is waited for");
        assert_eq!(v.sensors[0].kind, "Load");
        assert_eq!(v.sources[0].name, "fake");
        assert_eq!(opened.load(Ordering::SeqCst), 1);
        // At most 1 Hz: a second view soon after polls nothing new.
        p.view();
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        p.stop();
    }

    #[test]
    fn effects_get_pushes() {
        let (p, _, _) = poller(&SensorsConfig::default());
        let got: Arc<Mutex<Vec<Map<String, Value>>>> = Arc::default();
        let g = Arc::clone(&got);
        p.set_effect(Some(Box::new(move |d| g.lock().unwrap().push(d))));
        let t = Instant::now();
        while got.lock().unwrap().is_empty() && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(10));
        }
        let first = got.lock().unwrap()[0].clone();
        assert_eq!(
            first["sensors"]["CPU Load"],
            json!({"value": 1.0, "units": "%", "min": 0.0, "max": 100.0, "type": "Load"})
        );
        // A new effect gets the last values at once.
        let again: Arc<Mutex<usize>> = Arc::default();
        let a = Arc::clone(&again);
        p.set_effect(Some(Box::new(move |_| *a.lock().unwrap() += 1)));
        assert_eq!(*again.lock().unwrap(), 1);
        p.set_effect(None);
        p.stop();
    }

    #[test]
    fn faces_get_pushes_alongside_the_effect() {
        let (p, _, _) = poller(&SensorsConfig::default());
        let faces: Arc<Mutex<usize>> = Arc::default();
        let effect: Arc<Mutex<usize>> = Arc::default();
        let (f, e) = (Arc::clone(&faces), Arc::clone(&effect));
        p.set_faces(Some(Box::new(move |d| {
            assert!(d["sensors"]["CPU Load"]["value"].is_number());
            *f.lock().unwrap() += 1;
        })));
        p.set_effect(Some(Box::new(move |_| *e.lock().unwrap() += 1)));
        let t = Instant::now();
        while (*faces.lock().unwrap() == 0 || *effect.lock().unwrap() == 0)
            && t.elapsed() < Duration::from_secs(5)
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(*faces.lock().unwrap() >= 1 && *effect.lock().unwrap() >= 1);
        p.set_faces(None);
        p.set_effect(None);
        p.stop();
    }

    #[test]
    fn disabled_reads_nothing() {
        let cfg = SensorsConfig {
            enabled: false,
            ..SensorsConfig::default()
        };
        let (p, polls, opened) = poller(&cfg);
        let v = p.view_waiting(Duration::from_millis(50));
        assert!(!v.enabled && !v.polling && v.sensors.is_empty());
        p.set_effect(Some(Box::new(|_| panic!("disabled sensors never push"))));
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(
            (polls.load(Ordering::SeqCst), opened.load(Ordering::SeqCst)),
            (0, 0)
        );
    }

    #[test]
    fn interval_is_at_least_one_second() {
        let cfg = SensorsConfig {
            interval_ms: 10,
            ..SensorsConfig::default()
        };
        let (p, _, _) = poller(&cfg);
        assert_eq!(p.view_waiting(Duration::ZERO).interval_ms, 1000);
    }
}
