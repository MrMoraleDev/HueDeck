// SPDX-License-Identifier: GPL-3.0-or-later
//! Live hardware modes (Lightloom extension, `crate::hwmode`) end to end on the MSI GPU
//! plugin over a mock SMBus bus: a mode from the session options right after
//! `Initialize()`, no `Render()` while it runs, "Follow effect" back to the canvas,
//! refused choices, a save in between, and never a write of the save register 0x3F
//! (nor a refused one) outside `SaveToDevice`.

use lightloom_engine::{Frame, FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_hal::mock::{MockSmbus, TransactionKind, TransactionRecorder};
use lightloom_hal::{HidBackend, PciIds, SmbusBusKind};
use lightloom_plugin_host::catalog::Catalog;
use lightloom_plugin_host::discover::{scan_buses, ScanBudget};
use lightloom_plugin_host::hwmode::HardwareModeChoice;
use lightloom_plugin_host::manager::{DeviceManager, Present};
use lightloom_plugin_host::save::{SaveAction, SaveChoice};
use lightloom_plugin_host::session::{Control, HardwareModeCall, SessionOptions, SessionReport};
use lightloom_plugin_host::smbus::BusHandle;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn present(rec: &TransactionRecorder) -> Present {
    let mut bus = MockSmbus::new("NVIDIA GeForce RTX 3060", SmbusBusKind::Nvidia, rec.clone())
        .with_pci(PciIds {
            vendor: 0x10DE,
            device: 0x2504,
            sub_vendor: 0x1462,
            sub_device: 0x3903,
        })
        .with_instance("pci01");
    let text = std::fs::read_to_string(repo().join("plugins/msi-gpu/fixtures/expected.json")).unwrap();
    let fixture: serde_json::Value = serde_json::from_str(&text).unwrap();
    let byte = |v: &str| u8::from_str_radix(v.trim_start_matches("0x"), 16).unwrap();
    bus.add_device(
        0x68,
        fixture["startup_registers"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(r, v)| (byte(r), byte(v.as_str().unwrap())))
            .collect::<Vec<_>>(),
    );
    let bus = BusHandle::new(Box::new(bus));
    let catalog = Catalog::scan(&[repo().join("plugins")], Duration::from_secs(10));
    let (present, outcomes) = scan_buses(&catalog, std::slice::from_ref(&bus), &ScanBudget::default());
    assert_eq!(present.len(), 1, "{outcomes:?}");
    let p = present.into_iter().next().unwrap();
    assert_eq!(p.plugin.meta.name, "MSI GPU");
    assert!(p.plugin.meta.lifecycle.set_hardware_mode);
    assert_eq!(p.plugin.meta.hardware_modes.len(), 19);
    p
}

struct Rig {
    manager: DeviceManager,
    frames: Arc<FrameStore>,
}

impl Rig {
    fn new(p: Present, opts: SessionOptions) -> Self {
        let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
        let backend: Arc<dyn HidBackend> = Arc::new(lightloom_hal::hid::NoHid);
        let mut manager =
            DeviceManager::new(backend, Arc::clone(&frames), Box::new(move |_, _| opts.clone()));
        manager.reconcile(vec![p]);
        let rig = Self { manager, frames };
        rig.until("the session running", |r| {
            r.live().state == lightloom_plugin_host::session::SessionState::Running
        });
        rig
    }

    fn live(&self) -> lightloom_plugin_host::session::LiveSnapshot {
        self.manager.sessions().next().unwrap().live()
    }

    fn tick(&self) {
        self.frames.publish(Frame::solid(
            CANVAS_WIDTH,
            CANVAS_HEIGHT,
            Rgb::new(0x12, 0x34, 0x56),
        ));
        std::thread::sleep(Duration::from_millis(5));
    }

    fn until(&self, what: &str, done: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !done(self) {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            self.tick();
        }
    }

    fn set(&self, choice: Option<HardwareModeChoice>) -> Result<(), String> {
        let (reply, rx) = channel();
        assert!(self
            .manager
            .sessions()
            .next()
            .unwrap()
            .control(Control::HardwareMode(HardwareModeCall {
                choice,
                reply: Some(reply),
            })));
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Ok(r) = rx.try_recv() {
                return r;
            }
            assert!(Instant::now() < deadline, "no SetHardwareMode outcome");
            self.tick();
        }
    }

    fn finish(self) -> SessionReport {
        let (mut reports, failed) = self.manager.finish();
        assert!(failed.is_empty(), "{failed:?}");
        reports.pop().unwrap()
    }
}

fn writes(rec: &TransactionRecorder) -> Vec<Vec<u8>> {
    rec.payloads(TransactionKind::SmbusWrite)
}

fn rainbow() -> HardwareModeChoice {
    HardwareModeChoice {
        id: "rainbow".into(),
        colors: Vec::new(),
        speed: Some(100),
        brightness: Some(100),
    }
}

#[test]
fn a_hardware_mode_from_the_options_stops_frames_until_follow_effect() {
    let rec = TransactionRecorder::new();
    let rig = Rig::new(
        present(&rec),
        SessionOptions {
            hardware_mode: Some(rainbow()),
            ..SessionOptions::default()
        },
    );
    rig.until("the mode applied", |r| r.live().hardware_mode);
    // Initialize (Direct), then the mode: brightness, speed max (0x01), Rainbow (0x08).
    let sent = writes(&rec);
    assert_eq!(
        sent[sent.len() - 3..],
        [
            vec![0x68, 0x36, 100],
            vec![0x68, 0x38, 0x01],
            vec![0x68, 0x22, 0x08]
        ]
    );
    // No Render() while the card runs its own effect.
    for _ in 0..20 {
        rig.tick();
    }
    assert_eq!(rig.live().frames, 0);
    assert_eq!(writes(&rec).len(), sent.len(), "nothing sent while in the mode");

    // Follow effect: Direct again, then frames.
    rig.set(None).unwrap();
    assert!(!rig.live().hardware_mode);
    rig.until("frames again", |r| r.live().frames >= 2);
    let after = writes(&rec);
    assert_eq!(
        after[sent.len()..sent.len() + 2],
        [vec![0x68, 0x36, 100], vec![0x68, 0x22, 0x13]]
    );
    assert!(
        after.iter().any(|w| w[1..] == [0x30, 0x12]),
        "the frame's colour reaches the card"
    );

    let report = rig.finish();
    assert!(report.is_clean(), "{report:#?}");
    assert!(
        writes(&rec).iter().all(|w| w[1] != 0x3F),
        "never the save register"
    );
    assert_eq!(report.smbus.unwrap().denied, 0, "nothing refused either");
}

#[test]
fn choices_are_checked_and_a_save_returns_to_the_mode() {
    let rec = TransactionRecorder::new();
    let mut rig = Rig::new(present(&rec), SessionOptions::default());
    rig.until("frames", |r| r.live().frames >= 2);

    // Refused before the plugin runs: unknown mode, too many colours, bad colour.
    let bad = |c: HardwareModeChoice| rig.set(Some(c)).unwrap_err();
    assert!(bad(HardwareModeChoice {
        id: "disco".into(),
        ..Default::default()
    })
    .contains("no hardware mode"));
    assert!(bad(HardwareModeChoice {
        id: "breathing".into(),
        colors: vec!["#010203".into(), "#040506".into()],
        ..Default::default()
    })
    .contains("1 colour"));
    assert!(!rig.live().hardware_mode);
    // Follow effect while following the canvas does nothing.
    let before = writes(&rec).len();
    rig.set(None).unwrap();

    rig.set(Some(HardwareModeChoice {
        id: "breathing".into(),
        colors: vec!["#AABBCC".into()],
        speed: Some(0),
        brightness: Some(40),
    }))
    .unwrap();
    assert!(rig.live().hardware_mode);
    let sent = writes(&rec);
    let mode: Vec<Vec<u8>> = sent[before..]
        .iter()
        .filter(|w| [0x36, 0x38, 0x22, 0x30, 0x31, 0x32].contains(&w[1]))
        .cloned()
        .collect();
    assert!(
        mode.ends_with(&[
            vec![0x68, 0x36, 40],
            vec![0x68, 0x38, 0x04],
            vec![0x68, 0x22, 0x04],
            vec![0x68, 0x30, 0xAA],
            vec![0x68, 0x31, 0xBB],
            vec![0x68, 0x32, 0xCC],
        ]),
        "{mode:?}"
    );

    // A preview in the mode: nothing reaches the card; the mode is set again after it.
    let key = rig.manager.sessions().next().unwrap().key().to_string();
    let rx = rig
        .manager
        .save(&key, SaveAction::Preview, SaveChoice::default())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let outcome = loop {
        if let Ok(o) = rx.try_recv() {
            break o;
        }
        assert!(Instant::now() < deadline);
        rig.tick();
    };
    assert!(outcome.ok, "{outcome:?}");
    rig.until("the mode set again", |_| {
        writes(&rec).last().is_some_and(|w| w[1..] == [0x32, 0xCC])
    });
    assert!(rig.live().hardware_mode);
    let frames = rig.live().frames;
    for _ in 0..10 {
        rig.tick();
    }
    assert_eq!(rig.live().frames, frames, "still no frames");

    let report = rig.finish();
    assert!(
        writes(&rec).iter().all(|w| w[1] != 0x3F),
        "never the save register"
    );
    assert_eq!(report.smbus.unwrap().denied, 0);
    assert!(
        report.notes.iter().any(|n| n.contains("no hardware mode")),
        "{:?}",
        report.notes
    );
}
