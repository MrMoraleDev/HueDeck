// SPDX-License-Identifier: GPL-3.0-or-later
//! Save to devices and shutdown modes (Lightloom extensions, `crate::save`), end to
//! end on a mock SMBus bus: the SMBus `SmbusDeniedRegisters()` open only inside
//! the `SaveToDevice(request)` the user asked for, previews never reach the
//! device, the per-device rate limit, the MSI GPU plugin's save sequence, and
//! `Shutdown(false, mode)` with the `shutdownColor` fallback.

use lightloom_engine::{Frame, FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use lightloom_hal::mock::{MockSmbus, TransactionKind, TransactionRecorder};
use lightloom_hal::{HidBackend, PciIds, SmbusBusKind};
use lightloom_plugin_host::catalog::Catalog;
use lightloom_plugin_host::discover::{scan_buses, ScanBudget};
use lightloom_plugin_host::manager::{DeviceManager, Present};
use lightloom_plugin_host::save::{SaveAction, SaveChoice, SaveOutcome, SaveRefused, ShutdownMode};
use lightloom_plugin_host::session::{Control, SessionOptions, SessionReport};
use lightloom_plugin_host::smbus::BusHandle;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

const RTX3060: PciIds = PciIds {
    vendor: 0x10DE,
    device: 0x2504,
    sub_vendor: 0x1462,
    sub_device: 0x3903,
};

fn fixture() -> serde_json::Value {
    let text = std::fs::read_to_string(repo().join("plugins/msi-gpu/fixtures/expected.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn byte(v: &serde_json::Value) -> u8 {
    u8::from_str_radix(v.as_str().unwrap().trim_start_matches("0x"), 16).unwrap()
}

fn writes(fx: &serde_json::Value, name: &str) -> Vec<Vec<u8>> {
    fx[name]
        .as_array()
        .unwrap_or_else(|| panic!("fixture {name}"))
        .iter()
        .map(|p| vec![0x68, byte(&p[0]), byte(&p[1])])
        .collect()
}

fn gpu_bus(rec: &TransactionRecorder) -> BusHandle {
    let startup = fixture()["startup_registers"].as_object().unwrap().clone();
    let mut bus = MockSmbus::new("NVIDIA GeForce RTX 3060", SmbusBusKind::Nvidia, rec.clone())
        .with_pci(RTX3060)
        .with_instance("pci01");
    bus.add_device(
        0x68,
        startup.iter().map(|(r, v)| {
            (
                u8::from_str_radix(r.trim_start_matches("0x"), 16).unwrap(),
                byte(v),
            )
        }),
    );
    BusHandle::new(Box::new(bus))
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lightloom-save-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// An SMBus plugin for the RTX 3060 at 0x68 with the given extra exports.
fn write_plugin(dir: &Path, body: &str) {
    std::fs::write(
        dir.join("saver.js"),
        format!(
            "export function Name() {{ return 'Saver'; }}\n\
             export function Type() {{ return 'SMBUS'; }}\n\
             export function BrandGPUList() {{ return [{{ Vendor: 0x10DE, Device: 0x2504, SubVendor: 0x1462, SubDevice: 0x3903, Address: 0x68, Name: 'x' }}]; }}\n\
             export function SmbusAddresses() {{ return [0x68]; }}\n\
             export function SmbusDeniedRegisters() {{ return [0x3F]; }}\n\
             export function Scan(bus) {{ return [0x68]; }}\n\
             export function Initialize() {{}}\n\
             {body}\n"
        ),
    )
    .unwrap();
}

fn present(catalog: &Catalog, rec: &TransactionRecorder) -> Present {
    let bus = gpu_bus(rec);
    let (present, outcomes) = scan_buses(catalog, std::slice::from_ref(&bus), &ScanBudget::default());
    assert_eq!(present.len(), 1, "{outcomes:?}");
    present.into_iter().next().unwrap()
}

/// A manager running `p` on a canvas of `color`, with `opts` for the session.
struct Rig {
    manager: DeviceManager,
    frames: Arc<FrameStore>,
    color: Rgb,
    key: String,
}

impl Rig {
    fn new(p: Present, opts: SessionOptions, color: Rgb, interval: Duration) -> Self {
        let frames = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
        frames.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, color));
        let backend: Arc<dyn HidBackend> = Arc::new(lightloom_hal::hid::NoHid);
        let mut manager =
            DeviceManager::new(backend, Arc::clone(&frames), Box::new(move |_, _| opts.clone()))
                .with_save_interval(interval);
        let key = p.device.key.clone();
        manager.reconcile(vec![p]);
        let mut rig = Self {
            manager,
            frames,
            color,
            key,
        };
        rig.wait_frames(2);
        rig
    }

    fn frames_rendered(&self) -> u64 {
        self.manager.sessions().next().map_or(0, |h| h.live().frames)
    }

    fn wait_frames(&mut self, n: u64) {
        let target = self.frames_rendered() + n;
        let deadline = Instant::now() + Duration::from_secs(20);
        while self.frames_rendered() < target && Instant::now() < deadline {
            self.tick();
        }
    }

    fn tick(&self) {
        self.frames
            .publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, self.color));
        std::thread::sleep(Duration::from_millis(5));
    }

    fn call(&mut self, action: SaveAction, choice: SaveChoice) -> Result<SaveOutcome, SaveRefused> {
        let rx = self.manager.save(&self.key.clone(), action, choice)?;
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Ok(o) = rx.try_recv() {
                return Ok(o);
            }
            assert!(Instant::now() < deadline, "no SaveToDevice outcome");
            self.tick();
        }
    }

    fn control(&self, c: Control) {
        assert!(self.manager.sessions().next().unwrap().control(c));
    }

    fn finish(self) -> SessionReport {
        let (mut reports, failed) = self.manager.finish();
        assert!(failed.is_empty(), "{failed:?}");
        reports.pop().unwrap()
    }
}

const SAVER: &str = "\
export function SaveCapabilities() { return { static: true, effects: ['spectrum'] }; }\n\
export function Render() { bus.WriteByte(0x3F, 9); bus.WriteByte(0x30, device.color(0, 0)[0]); }\n\
export function SaveToDevice(request) {\n\
  const [r, g, b] = request.colors[0];\n\
  bus.WriteByte(0x31, r);\n\
  if (request.action === 'save') { bus.WriteByte(0x3F, 1); }\n\
  return { effect: request.effect || 'static', zones: [{ name: 'GPU', color: '#' + [r, g, b].map((v) => v.toString(16).padStart(2, '0')).join('') }] };\n\
}\n\
export function Shutdown() {}";

#[test]
fn the_save_register_opens_only_inside_save_to_device() {
    let dir = scratch("window");
    write_plugin(&dir, SAVER);
    let catalog = Catalog::scan(std::slice::from_ref(&dir), Duration::from_secs(10));
    let rec = TransactionRecorder::new();
    let p = present(&catalog, &rec);
    assert!(p.plugin.meta.save_caps.is_some() && p.plugin.meta.lifecycle.save_to_device);
    let mut rig = Rig::new(
        p,
        SessionOptions::default(),
        Rgb::new(0x40, 0x50, 0x60),
        Duration::from_secs(60),
    );

    // A preview runs the plugin dry: nothing reaches the bus, the result says what would be stored.
    let before = rec.payloads(TransactionKind::SmbusWrite).len();
    let preview = rig.call(SaveAction::Preview, SaveChoice::default()).unwrap();
    assert!(preview.ok && preview.dry_run, "{preview:?}");
    assert_eq!(preview.result["zones"][0]["color"], "#405060");
    let sent = rec.payloads(TransactionKind::SmbusWrite);
    assert!(
        sent[before..].iter().all(|w| w[1] != 0x31 && w[1] != 0x3F),
        "a preview sends nothing: {sent:?}"
    );

    // The save: 0x3F goes through exactly once, inside SaveToDevice.
    let saved = rig.call(SaveAction::Save, SaveChoice::default()).unwrap();
    assert!(saved.ok && !saved.dry_run, "{saved:?}");
    rig.wait_frames(2);
    let sent = rec.payloads(TransactionKind::SmbusWrite);
    let commits: Vec<&Vec<u8>> = sent.iter().filter(|w| w[1] == 0x3F).collect();
    assert_eq!(
        commits,
        vec![&vec![0x68, 0x3F, 1]],
        "only SaveToDevice's write: {sent:?}"
    );
    let at = sent.iter().position(|w| w[1] == 0x3F).unwrap();
    assert_eq!(
        sent[at - 1],
        vec![0x68, 0x31, 0x40],
        "the colour of the frame shown"
    );

    // One save per device per interval; a preview is still fine.
    assert!(matches!(
        rig.manager
            .save(&rig.key.clone(), SaveAction::Save, SaveChoice::default()),
        Err(SaveRefused::TooSoon { .. })
    ));
    assert!(rig.manager.save_wait_secs(&rig.key) > 0);
    assert!(rig.call(SaveAction::Preview, SaveChoice::default()).unwrap().ok);
    // What the plugin does not advertise is refused by the session.
    let other = rig
        .call(
            SaveAction::Preview,
            SaveChoice {
                effect: Some("breathing".into()),
            },
        )
        .unwrap();
    assert!(!other.ok && other.error.unwrap().contains("breathing"));

    let report = rig.finish();
    assert_eq!(report.saves, 1);
    let stats = report.smbus.unwrap();
    assert!(
        stats.denied >= 2,
        "Render()'s 0x3F writes stay refused: {stats:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_rate_limit_lets_a_save_through_again_after_the_interval() {
    let dir = scratch("limit");
    write_plugin(&dir, SAVER);
    let catalog = Catalog::scan(std::slice::from_ref(&dir), Duration::from_secs(10));
    let rec = TransactionRecorder::new();
    let mut rig = Rig::new(
        present(&catalog, &rec),
        SessionOptions::default(),
        Rgb::new(1, 2, 3),
        Duration::from_millis(300),
    );
    assert!(rig.call(SaveAction::Save, SaveChoice::default()).unwrap().ok);
    assert!(rig.call(SaveAction::Save, SaveChoice::default()).is_err());
    std::thread::sleep(Duration::from_millis(350));
    assert!(rig.call(SaveAction::Save, SaveChoice::default()).unwrap().ok);
    assert_eq!(rig.finish().saves, 2);
    let commits = rec
        .payloads(TransactionKind::SmbusWrite)
        .into_iter()
        .filter(|w| w[1] == 0x3F)
        .count();
    assert_eq!(commits, 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn plugins_without_save_to_device_are_refused() {
    let dir = scratch("nosave");
    write_plugin(&dir, "export function Render() {}\nexport function Shutdown() {}");
    let catalog = Catalog::scan(std::slice::from_ref(&dir), Duration::from_secs(10));
    let rec = TransactionRecorder::new();
    let mut rig = Rig::new(
        present(&catalog, &rec),
        SessionOptions::default(),
        Rgb::new(1, 2, 3),
        Duration::from_secs(60),
    );
    let o = rig.call(SaveAction::Save, SaveChoice::default()).unwrap();
    assert!(!o.ok && o.error.unwrap().contains("cannot store"));
    assert!(matches!(
        rig.manager
            .save("smbus:nope", SaveAction::Save, SaveChoice::default()),
        Err(SaveRefused::NotRunning(_))
    ));
    rig.finish();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn msi_gpu_saves_the_static_colour_with_openrgbs_sequence() {
    let fx = fixture();
    let catalog = Catalog::scan(&[repo().join("plugins")], Duration::from_secs(10));
    let rec = TransactionRecorder::new();
    let p = present(&catalog, &rec);
    assert_eq!(p.plugin.meta.name, "MSI GPU");
    let mut rig = Rig::new(
        p,
        SessionOptions::default(),
        Rgb::new(0x12, 0x34, 0x56),
        Duration::from_secs(60),
    );
    let preview = rig.call(SaveAction::Preview, SaveChoice::default()).unwrap();
    assert!(preview.ok, "{preview:?}");
    assert_eq!(preview.result["zones"][0]["color"], "#123456");
    let before = rec.payloads(TransactionKind::SmbusWrite).len();
    let saved = rig.call(SaveAction::Save, SaveChoice::default()).unwrap();
    assert!(saved.ok, "{saved:?}");
    let sent = rec.payloads(TransactionKind::SmbusWrite);
    let mut expected = writes(&fx, "save_static_123456");
    expected.extend(writes(&fx, "save_resume"));
    assert_eq!(sent[before..before + expected.len()], expected[..]);
    let spectrum = rig.manager.save(
        &rig.key.clone(),
        SaveAction::Save,
        SaveChoice {
            effect: Some("spectrum".into()),
        },
    );
    assert!(matches!(spectrum, Err(SaveRefused::TooSoon { .. })));
    let report = rig.finish();
    assert!(report.is_clean(), "{report:#?}");
    // The restore on exit puts back what was saved, not the state before it.
    let sent = rec.payloads(TransactionKind::SmbusWrite);
    let restore = writes(&fx, "shutdown_after_save");
    assert_eq!(sent[sent.len() - restore.len()..], restore[..]);
}

const MODES: &str = "\
export function ControllableParameters() { return [{ property: 'shutdownColor', label: 'Shutdown Color', type: 'color', default: '#000000' }]; }\n\
export function Render() {}\n\
export function Shutdown(suspending, mode) {\n\
  bus.WriteByte(0x30, parseInt(shutdownColor.slice(1, 3), 16));\n\
  bus.WriteByte(0x31, ({ restore: 1, keep: 2, off: 3 })[mode] || 0);\n\
}";

fn shutdown_writes(
    body: &str,
    mode: ShutdownMode,
    live: Option<ShutdownMode>,
) -> (Vec<Vec<u8>>, SessionReport) {
    let dir = scratch(&format!("modes-{}-{}", mode.as_str(), body.len()));
    write_plugin(&dir, body);
    let catalog = Catalog::scan(std::slice::from_ref(&dir), Duration::from_secs(10));
    let rec = TransactionRecorder::new();
    let opts = SessionOptions {
        shutdown_mode: mode,
        ..SessionOptions::default()
    };
    let mut rig = Rig::new(
        present(&catalog, &rec),
        opts,
        Rgb::new(0xAB, 0, 0),
        Duration::from_secs(60),
    );
    if let Some(m) = live {
        rig.control(Control::ShutdownMode(m));
        rig.wait_frames(2);
    }
    let report = rig.finish();
    let _ = std::fs::remove_dir_all(&dir);
    (rec.payloads(TransactionKind::SmbusWrite), report)
}

#[test]
fn shutdown_modes_reach_the_plugin_or_go_through_shutdown_color() {
    // No ShutdownModes(): "keep" and "off" become shutdownColor; the mode still comes along.
    let (w, r) = shutdown_writes(MODES, ShutdownMode::Keep, None);
    assert_eq!(w, vec![vec![0x68, 0x30, 0xAB], vec![0x68, 0x31, 2]]);
    assert_eq!(r.shutdown_mode, ShutdownMode::Keep);
    assert!(
        r.notes.iter().any(|n| n.contains("shutdownColor")),
        "{:?}",
        r.notes
    );
    let (w, _) = shutdown_writes(MODES, ShutdownMode::Off, None);
    assert_eq!(w, vec![vec![0x68, 0x30, 0], vec![0x68, 0x31, 3]]);
    // "Restore" leaves the parameters alone.
    let (w, _) = shutdown_writes(MODES, ShutdownMode::Restore, None);
    assert_eq!(w, vec![vec![0x68, 0x30, 0], vec![0x68, 0x31, 1]]);
    // A plugin that implements the mode gets it as is, its shutdownColor untouched.
    let native = format!("{MODES}\nexport function ShutdownModes() {{ return ['keep', 'off']; }}");
    let (w, r) = shutdown_writes(&native, ShutdownMode::Restore, Some(ShutdownMode::Keep));
    assert_eq!(
        w,
        vec![vec![0x68, 0x30, 0], vec![0x68, 0x31, 2]],
        "changed while running"
    );
    assert_eq!(r.shutdown_mode, ShutdownMode::Keep);
    // Neither: the plugin restores as before, and the report says so.
    let (_, r) = shutdown_writes(
        "export function Render() {}\nexport function Shutdown() {}",
        ShutdownMode::Off,
        None,
    );
    assert!(
        r.notes.iter().any(|n| n.contains("not supported")),
        "{:?}",
        r.notes
    );
}
