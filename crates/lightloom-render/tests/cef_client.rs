// SPDX-License-Identifier: GPL-3.0-or-later
//! The CEF client against a stand-in renderer (`lightloom-renderer-fake`): frames
//! flow into the store, a static page is kept alive, a crashed renderer is
//! restarted, control values reach the renderer (over stdin on Windows, over a
//! unix socket elsewhere).

use lightloom_engine::{FrameStore, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_render::{CefRenderer, CefRendererConfig, RendererClient};
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn wait_until(what: &str, secs: u64, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn work_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lightloom-cef-client-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn client(name: &str, props: &[(&str, Value)]) -> (CefRenderer, PathBuf) {
    let dir = work_dir(name);
    let cfg = CefRendererConfig {
        renderer: PathBuf::from(env!("CARGO_BIN_EXE_lightloom-renderer-fake")),
        effect: PathBuf::from("unused.html"),
        fps: 60,
        properties: props
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<Map<_, _>>(),
        software_compositing: false,
        work_dir: dir.clone(),
    };
    (CefRenderer::new(cfg), dir)
}

#[test]
fn frames_flow_and_controls_reach_the_renderer() {
    let (mut r, dir) = client("flow", &[]);
    let store = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    r.start(Arc::clone(&store)).unwrap();
    wait_until("frames", 20, || r.status().frames >= 5);
    let (_, frame) = store.latest();
    assert!(frame.pixel(0, 0).r > 0, "the fake paints the frame count in red");
    wait_until("the control pipe", 20, || {
        r.set_properties(json!({"speed": 3}).as_object().unwrap().clone())
    });
    assert!(r.push_engine_data(json!({"audio": {"level": -20}}).as_object().unwrap().clone()));
    // An input source pushes from its own thread through a sink.
    let sink = r.engine_sink();
    let pushed =
        std::thread::spawn(move || sink.push(json!({"audio": {"width": 0.25}}).as_object().unwrap().clone()))
            .join()
            .unwrap();
    assert!(pushed);
    let st = r.status();
    r.stop();
    let ring = std::path::PathBuf::from(st.ring.clone().unwrap());
    let control = st.control.clone().unwrap();
    if cfg!(unix) {
        // A private per-run dir (tmpfs on Linux) holds the ring and the control socket; it is
        // gone once the run ended.
        assert!(control.starts_with("unix:"), "{control}");
        let run_dir = ring.parent().unwrap().to_path_buf();
        assert!(!run_dir.exists(), "{} survived the run", run_dir.display());
        if cfg!(target_os = "linux") && std::path::Path::new("/dev/shm").is_dir() {
            assert!(
                !ring.starts_with(&dir),
                "ring {} is not on a tmpfs",
                ring.display()
            );
        }
    } else {
        assert_eq!(control, "stdin");
        assert_eq!(ring, dir.join("frames-1.shm"));
        assert!(!ring.exists(), "the ring file is removed after the run");
    }
    let log = std::fs::read_to_string(dir.join("renderer-report.json.control.log")).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(log.contains("\"set\"") && log.contains("\"speed\":3"), "{log}");
    assert!(
        log.contains("\"engine\"") && log.contains("\"level\":-20"),
        "{log}"
    );
    assert!(log.contains("\"width\":0.25"), "{log}");
    assert!(log.contains("\"quit\""), "stop sends quit: {log}");
    let st = r.status();
    assert_eq!((st.starts, st.running), (1, false));
}

#[test]
fn a_static_page_is_kept_alive() {
    let (mut r, dir) = client("static", &[("fake_mode", json!("static"))]);
    let store = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    r.start(Arc::clone(&store)).unwrap();
    wait_until("the one frame", 20, || r.status().frames == 1);
    let seq = store.seq();
    wait_until("republished frames", 20, || store.seq() >= seq + 5);
    r.stop();
    let _ = std::fs::remove_dir_all(&dir);
    let st = r.status();
    assert_eq!(st.frames, 1);
    assert!(st.republished >= 5, "{st:?}");
}

#[test]
fn a_crashed_renderer_is_restarted_holding_the_last_frame() {
    let (mut r, dir) = client("crash", &[("fake_exit_after", json!(5))]);
    let store = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    r.start(Arc::clone(&store)).unwrap();
    wait_until("a restart", 60, || r.status().starts >= 2);
    let st = r.status();
    assert!(st.last_exit.as_deref().unwrap_or("").contains("exited"), "{st:?}");
    // However late the client looked, the frames the first renderer painted
    // before it died were read from its ring before the restart.
    assert!(st.frames >= 1, "{st:?}");
    // While the renderer was gone the last frame was held, not replaced by black.
    let (_, frame) = store.latest();
    assert!(frame.pixel(0, 0).r > 0);
    wait_until("the replacement's first frame", 60, || {
        r.status().max_recovery_ms.is_some()
    });
    let ms = r.status().max_recovery_ms.unwrap();
    assert!(
        (900.0..10_000.0).contains(&ms),
        "recovery took {ms} ms (restart delay 1 s)"
    );
    r.stop();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_renderer_is_reported() {
    let dir = work_dir("missing");
    let mut r = CefRenderer::new(CefRendererConfig {
        renderer: dir.join("no-such-renderer.exe"),
        effect: PathBuf::from("unused.html"),
        fps: 30,
        properties: Map::new(),
        software_compositing: false,
        work_dir: dir.clone(),
    });
    let store = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    r.start(store).unwrap();
    wait_until("the start error", 20, || r.status().last_exit.is_some());
    r.stop();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(r.status().last_exit.unwrap().contains("cannot start"));
}
