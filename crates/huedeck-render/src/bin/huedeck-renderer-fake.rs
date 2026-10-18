// SPDX-License-Identifier: Apache-2.0 OR MIT
//! A stand-in for `huedeck-renderer --serve` in the CEF client's tests: no
//! Chromium, just the frame ring and the control pipe.
//!
//! It accepts the real renderer's arguments (most are ignored) and paints
//! solid frames whose red channel counts the frames painted. `--prop` values
//! choose the behaviour: `fake_mode=static` paints once, `fake_exit_after=N`
//! exits after N frames (a crash). Every control line received is appended to
//! `<out>.control.log`; `{"cmd":"quit"}` or the end of stdin ends it.

use huedeck_renderer::ring::RingWriter;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn main() {
    let mut out: Option<PathBuf> = None;
    let mut fps = 30u32;
    let mut props: Vec<(String, String)> = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" => out = args.next().map(PathBuf::from),
            "--fps" => fps = args.next().and_then(|v| v.parse().ok()).unwrap_or(30),
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

    let mut ring = RingWriter::create_file(&out, 320, 200).expect("ring");
    let done = Arc::new(AtomicBool::new(false));
    let log_path = PathBuf::from(format!("{}.control.log", out.display()));
    {
        let done = Arc::clone(&done);
        std::thread::spawn(move || {
            let stdin = std::io::stdin();
            let mut log = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)
                .expect("control log");
            for line in stdin.lock().lines() {
                let Ok(line) = line else { break };
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
            let mut px = vec![0u8; 320 * 200 * 4];
            for p in px.as_chunks_mut::<4>().0 {
                p.copy_from_slice(&[0, 0, (n % 256) as u8, 255]);
            }
            ring.publish(&px, 320, 200).expect("publish");
            if exit_after.is_some_and(|max| n >= max) {
                std::process::exit(3);
            }
        }
        std::thread::sleep(period);
    }
}
