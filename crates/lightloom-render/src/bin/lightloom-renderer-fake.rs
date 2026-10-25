// SPDX-License-Identifier: GPL-3.0-or-later
//! A stand-in for `lightloom-renderer --serve` in the CEF client's tests: no
//! Chromium, just the frame ring and the control channel (stdin, or the unix
//! socket given with `--control unix:PATH`, as on Linux).
//!
//! It accepts the real renderer's arguments (most are ignored) and paints
//! solid frames of `--width` x `--height` (default 320x200) whose red channel
//! counts the frames painted; green is 0x99 with `--face`, else 0. `--prop` values
//! choose the behaviour: `fake_mode=static` paints once, `fake_exit_after=N`
//! exits after N frames (a crash). Every control line received is appended to
//! `<stats>.control.log` (the client's work dir; the ring may live in a
//! private tmpfs dir that is gone after the run), or `<out>.control.log`
//! without `--stats`; `{"cmd":"quit"}` or the end of input ends it.

use lightloom_renderer::ipc::{self, BoundedLine, ControlAddr};
use lightloom_renderer::ring::RingWriter;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn main() {
    let mut out: Option<PathBuf> = None;
    let mut stats: Option<PathBuf> = None;
    let mut control = ControlAddr::Stdin;
    let mut fps = 30u32;
    let (mut width, mut height, mut face) = (320u32, 200u32, false);
    let mut props: Vec<(String, String)> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" => out = args.next().map(PathBuf::from),
            "--stats" => stats = args.next().map(PathBuf::from),
            "--control" => match args.next().map(|a| a.parse::<ControlAddr>()) {
                Some(Ok(addr)) => control = addr,
                other => {
                    eprintln!("bad --control: {other:?}");
                    std::process::exit(2);
                }
            },
            "--fps" => fps = args.next().and_then(|v| v.parse().ok()).unwrap_or(30),
            "--width" => width = args.next().and_then(|v| v.parse().ok()).unwrap_or(320),
            "--height" => height = args.next().and_then(|v| v.parse().ok()).unwrap_or(200),
            "--face" => face = true,
            "--prop" => {
                if let Some((k, v)) = args.next().as_deref().and_then(|p| p.split_once('=')) {
                    props.push((k.to_string(), v.to_string()));
                }
            }
            _ => {}
        }
    }
    let Some(out) = out else {
        eprintln!("--out is required");
        std::process::exit(2);
    };
    let prop = |name: &str| {
        props
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    };
    let paint_once = prop("fake_mode").as_deref() == Some("static");
    let exit_after: Option<u64> = prop("fake_exit_after").and_then(|v| v.parse().ok());

    let mut ring = RingWriter::create_file(&out, width, height).expect("ring");
    let green = if face { 0x99 } else { 0 };
    let done = Arc::new(AtomicBool::new(false));
    let log_path = PathBuf::from(format!(
        "{}.control.log",
        stats.as_ref().unwrap_or(&out).display()
    ));
    // Like the real renderer: the ring exists before the control channel is connected.
    let mut reader = ipc::connect_control(&control).expect("control channel");
    {
        let done = Arc::clone(&done);
        std::thread::spawn(move || {
            let mut log = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)
                .expect("control log");
            let mut buf = Vec::new();
            while let Ok(BoundedLine::Line) = ipc::read_bounded_line(&mut reader, &mut buf, 1 << 20) {
                let line = String::from_utf8_lossy(&buf);
                let _ = writeln!(log, "{line}");
                if line.contains("\"quit\"") {
                    break;
                }
            }
            done.store(true, Ordering::Release);
        });
    }
    let period = Duration::from_secs_f64(1.0 / f64::from(fps.max(1)));
    let mut n: u64 = 0;
    while !done.load(Ordering::Acquire) {
        if !(paint_once && n > 0) {
            n += 1;
            let mut px = vec![0u8; width as usize * height as usize * 4];
            for p in px.as_chunks_mut::<4>().0 {
                p.copy_from_slice(&[0, green, (n % 256) as u8, 255]);
            }
            ring.publish(&px, width, height).expect("publish");
            if exit_after.is_some_and(|max| n >= max) {
                std::process::exit(3);
            }
        }
        std::thread::sleep(period);
    }
}
