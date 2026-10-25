// SPDX-License-Identifier: GPL-3.0-or-later
//! The CEF client against the real `lightloom-renderer` (built with `--features cef`): frames of a
//! real Chromium page flow through the shm ring into the store, and control commands reach the
//! page over the platform's control channel (stdin on Windows, a unix socket on Linux).
//!
//! Ignored by default: it needs the CEF build of the renderer, with the CEF runtime files next to
//! it. CI's `renderer-cef` jobs run it with
//! `LIGHTLOOM_RENDERER_EXE=target/debug/lightloom-renderer cargo test -p lightloom-render --test cef_real -- --ignored`.
//! On Linux it runs without a display (Ozone `headless`); set `LIGHTLOOM_OZONE_PLATFORM=x11` to
//! run it under Xvfb instead.

use lightloom_engine::{FrameStore, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_render::{CefRenderer, CefRendererConfig, RendererClient};
use serde_json::{json, Map};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
#[ignore = "needs lightloom-renderer built with --features cef (LIGHTLOOM_RENDERER_EXE)"]
fn the_real_renderer_paints_and_takes_controls() {
    let exe = PathBuf::from(
        std::env::var_os("LIGHTLOOM_RENDERER_EXE")
            .expect("set LIGHTLOOM_RENDERER_EXE to the CEF build of lightloom-renderer"),
    );
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let effect = Path::new(env!("CARGO_MANIFEST_DIR")).join("../lightloom-renderer/testdata/spike.html");
    let effect = std::fs::canonicalize(&effect).expect("spike.html");
    let work_dir = std::env::temp_dir().join(format!("lightloom-cef-real-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work_dir);
    let mut r = CefRenderer::new(CefRendererConfig {
        renderer: exe,
        effect,
        fps: 30,
        properties: Map::new(),
        // CI runners have no GPU; Ozone headless is software-only anyway.
        software_compositing: true,
        work_dir: work_dir.clone(),
        face: None,
    });
    let store = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    r.start(Arc::clone(&store)).unwrap();

    let log = || std::fs::read_to_string(work_dir.join("renderer.log")).unwrap_or_default();
    let deadline = Instant::now() + Duration::from_secs(120);
    while r.status().frames < 30 {
        assert!(
            Instant::now() < deadline,
            "fewer than 30 frames in 120 s: {:?}\nrenderer.log:\n{}",
            r.status(),
            log()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let (_, frame) = store.latest();
    let lit = (0..CANVAS_WIDTH)
        .step_by(8)
        .flat_map(|x| (0..CANVAS_HEIGHT).step_by(8).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            let p = frame.pixel(x, y);
            u16::from(p.r) + u16::from(p.g) + u16::from(p.b) > 30
        })
        .count();
    assert!(
        lit > 50,
        "spike.html paints a gradient; only {lit} sampled pixels are lit"
    );

    // The control channel is up (a socket is accepted once the renderer connected).
    let accepted = Instant::now() + Duration::from_secs(20);
    while !r.set_properties(json!({"speed": 7}).as_object().unwrap().clone()) {
        assert!(Instant::now() < accepted, "control channel never opened");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(r.tap(10.0, 20.0));
    let frames = r.status().frames;
    let more = Instant::now() + Duration::from_secs(30);
    while r.status().frames < frames + 10 {
        assert!(
            Instant::now() < more,
            "the page stopped painting after controls: {}",
            log()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let st = r.status();
    r.stop();
    let st_after = r.status();
    let report = std::fs::read_to_string(work_dir.join("renderer-report.json")).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&work_dir);
    assert_eq!(st.starts, 1, "the renderer was restarted: {st:?}");
    if cfg!(unix) {
        assert!(st.control.as_deref().unwrap_or("").starts_with("unix:"), "{st:?}");
    }
    // `quit` makes the renderer close its browser and write its report. The client kills a
    // renderer that has not exited 5 s after `quit`; a debug CEF build can take that long to
    // shut down (measured 2-6 s on Windows), so the report is only checked when it got there.
    let exit = st_after.last_exit.clone().unwrap_or_default();
    if report.is_empty() {
        assert!(
            exit.contains("killed"),
            "no report, yet the renderer exited: {exit}"
        );
        eprintln!("renderer killed before its report was written: {exit}");
    } else {
        assert!(
            report.contains("\"close_reason\": \"quit\""),
            "{st_after:?}\n{report}"
        );
        assert!(report.contains("\"ok\": true"), "{report}");
    }
}
