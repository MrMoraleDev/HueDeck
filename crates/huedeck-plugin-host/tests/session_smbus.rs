// SPDX-License-Identifier: GPL-3.0-or-later
//! SMBus device sessions end to end on a mock bus: discovery (`Scan(bus)` in
//! scan mode) → allowlist → a device session of the first-party MSI GPU and
//! Crucial Ballistix plugins. The expected MSI register writes are the
//! hand-derived ones in `plugins/msi-gpu/fixtures/expected.json` (from
//! OpenRGB's MSIGPUController); the Crucial frame is the one of
//! `plugins/crucial-ballistix/fixtures/expected.json` for a solid colour.

use huedeck_engine::{Frame, FrameStore, Rgb, CANVAS_HEIGHT, CANVAS_WIDTH};
use huedeck_hal::mock::{MockSmbus, TransactionKind, TransactionRecorder};
use huedeck_hal::{HidBackend, PciIds, SmbusBusKind};
use huedeck_plugin_host::catalog::Catalog;
use huedeck_plugin_host::device::WriteMode;
use huedeck_plugin_host::discover::{scan_buses, ScanBudget};
use huedeck_plugin_host::manager::Present;
use huedeck_plugin_host::session::{self, SessionDevice, SessionOptions, SessionReport};
use huedeck_plugin_host::smbus::BusHandle;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture() -> serde_json::Value {
    let text = std::fs::read_to_string(repo().join("plugins/msi-gpu/fixtures/expected.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn byte(v: &serde_json::Value) -> u8 {
    let s = v.as_str().unwrap();
    u8::from_str_radix(s.trim_start_matches("0x"), 16).unwrap()
}

/// `[[reg, value], …]` → SMBus write payloads `[0x68, reg, value]`.
fn writes(fx: &serde_json::Value, name: &str) -> Vec<Vec<u8>> {
    fx[name]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| vec![0x68, byte(&p[0]), byte(&p[1])])
        .collect()
}

const RTX3060: PciIds = PciIds {
    vendor: 0x10DE,
    device: 0x2504,
    sub_vendor: 0x1462,
    sub_device: 0x3903,
};

fn gpu_bus(pci: PciIds, rec: &TransactionRecorder) -> BusHandle {
    let startup = fixture()["startup_registers"].as_object().unwrap().clone();
    let mut bus = MockSmbus::new("NVIDIA GeForce RTX 3060", SmbusBusKind::Nvidia, rec.clone())
        .with_pci(pci)
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

fn catalog() -> Catalog {
    Catalog::scan(&[repo().join("plugins")], Duration::from_secs(10))
}

/// Discover the bus and run the one device found until `frames` frames rendered.
fn run_gpu(rec: &TransactionRecorder, opts: SessionOptions, color: Rgb, frames: u64) -> SessionReport {
    let bus = gpu_bus(RTX3060, rec);
    let (present, outcomes) = scan_buses(&catalog(), std::slice::from_ref(&bus), &ScanBudget::default());
    assert_eq!(present.len(), 1, "{outcomes:?}");
    let p = present.into_iter().next().unwrap();
    run_present(p, opts, color, frames)
}

fn run_present(p: Present, opts: SessionOptions, color: Rgb, frames: u64) -> SessionReport {
    let store = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    store.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, color));
    let backend: Arc<dyn HidBackend> = Arc::new(huedeck_hal::hid::NoHid);
    let handle = session::spawn(p.device, p.plugin, backend, Arc::clone(&store), opts).unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while handle.live().frames < frames && !handle.is_finished() && Instant::now() < deadline {
        store.publish(Frame::solid(CANVAS_WIDTH, CANVAS_HEIGHT, color));
        std::thread::sleep(Duration::from_millis(5));
    }
    handle.stop();
    handle.join().expect("session thread")
}

#[test]
fn msi_gpu_session_writes_static_mode_then_rgb() {
    let fx = fixture();
    let rec = TransactionRecorder::new();
    let report = run_gpu(&rec, SessionOptions::default(), Rgb::new(0x12, 0x34, 0x56), 3);
    assert!(report.is_clean(), "{report:#?}");
    assert!(report.initialized && report.shutdown_called);
    assert_eq!(report.device, "smbus:10de:2504:1462:3903@pci01:68");
    assert_eq!(
        report.endpoint_path.as_deref(),
        Some("NVIDIA GeForce RTX 3060 @0x68")
    );
    let reads: Vec<u8> = rec
        .payloads(TransactionKind::SmbusRead)
        .iter()
        .map(|p| p[1])
        .collect();
    let expected_reads: Vec<u8> = fx["init_reads"].as_array().unwrap().iter().map(byte).collect();
    assert_eq!(reads, expected_reads);
    let mut expected = writes(&fx, "init_writes");
    expected.extend(writes(&fx, "frame_123456"));
    expected.extend(writes(&fx, "shutdown_restore"));
    assert_eq!(rec.payloads(TransactionKind::SmbusWrite), expected);
    let stats = report.smbus.expect("SMBus counters");
    assert_eq!((stats.errors, stats.denied), (0, 0));
}

#[test]
fn shutdown_restores_gpu_registers() {
    let fx = fixture();
    let rec = TransactionRecorder::new();
    let report = run_gpu(&rec, SessionOptions::default(), Rgb::new(200, 0, 0), 2);
    assert!(report.is_clean(), "{report:#?}");
    let sent = rec.payloads(TransactionKind::SmbusWrite);
    let restore = writes(&fx, "shutdown_restore");
    assert_eq!(
        sent[sent.len() - restore.len()..],
        restore[..],
        "the card's own state, mode last"
    );
    assert!(
        sent.iter().all(|w| w[0] == 0x68 && w[1] != 0x3F),
        "never the flash-save register"
    );
}

#[test]
fn dry_run_sends_nothing() {
    let rec = TransactionRecorder::new();
    let report = run_gpu(
        &rec,
        SessionOptions {
            write_mode: WriteMode::DryRun,
            ..SessionOptions::default()
        },
        Rgb::new(1, 2, 3),
        2,
    );
    assert!(report.is_clean(), "{report:#?}");
    assert!(rec.payloads(TransactionKind::SmbusWrite).is_empty());
    assert_eq!(
        rec.payloads(TransactionKind::SmbusRead).len(),
        12,
        "reads still happen"
    );
    let log = report.smbus.unwrap().dry_run_log;
    assert_eq!(log[0], vec![0x68, 0x36, 0x64]);
    assert!(log.contains(&vec![0x68, 0x30, 1]));
}

#[test]
fn run_phase_cannot_touch_an_address_no_scan_found() {
    let rec = TransactionRecorder::new();
    let bus = gpu_bus(RTX3060, &rec);
    let plugin = catalog()
        .smbus_plugins()
        .find(|e| e.meta.name == "MSI GPU")
        .unwrap()
        .clone();
    // A device the scan never reported: its address is not on the allowlist.
    let report = run_present(
        Present {
            device: SessionDevice::smbus(bus, 0x68),
            plugin,
        },
        SessionOptions::default(),
        Rgb::new(1, 2, 3),
        2,
    );
    assert!(rec.snapshot().is_empty(), "nothing reached the bus");
    let stats = report.smbus.as_ref().unwrap();
    assert!(stats.denied >= 12, "{stats:?}");
    assert!(!report.is_clean());
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("huedeck-smbus-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A minimal SMBus plugin for the RTX 3060 at 0x68 with the given Render body.
fn write_plugin(dir: &Path, file: &str, name: &str, scan: &str, render: &str) {
    std::fs::write(
        dir.join(file),
        format!(
            "export function Name() {{ return '{name}'; }}\n\
             export function Type() {{ return 'SMBUS'; }}\n\
             export function BrandGPUList() {{ return [{{ Vendor: 0x10DE, Device: 0x2504, SubVendor: 0x1462, SubDevice: 0x3903, Address: 0x68, Name: 'x' }}]; }}\n\
             export function SmbusAddresses() {{ return [0x68, 0x69]; }}\n\
             export function Scan(bus) {{ {scan} }}\n\
             export function Initialize() {{}}\n\
             export function Render() {{ {render} }}\n\
             export function Shutdown() {{}}\n"
        ),
    )
    .unwrap();
}

#[test]
fn render_leaving_lock_is_force_released() {
    let dir = scratch("lock");
    write_plugin(
        &dir,
        "locker.js",
        "Locker",
        "return [0x68];",
        "bus.TransactionLock(); bus.WriteByte(0x30, 1);",
    );
    let catalog = Catalog::scan(std::slice::from_ref(&dir), Duration::from_secs(10));
    let rec = TransactionRecorder::new();
    let bus = gpu_bus(RTX3060, &rec);
    let (present, _) = scan_buses(&catalog, std::slice::from_ref(&bus), &ScanBudget::default());
    let report = run_present(
        present[0].clone(),
        SessionOptions::default(),
        Rgb::new(1, 1, 1),
        3,
    );
    assert!(report.is_clean(), "{report:#?}");
    assert!(report.frames >= 3);
    assert_eq!(
        report
            .log_tail
            .iter()
            .filter(|l| l.contains("Transaction Lock wasn't unlocked"))
            .count(),
        1,
        "logged once: {:?}",
        report.log_tail
    );
    // Released after every Render(): another guard can take the bus at once.
    let mut other = huedeck_plugin_host::smbus::GuardedBus::new(
        bus,
        huedeck_plugin_host::smbus::Phase::Run { addr: 0x68 },
        WriteMode::Live,
    );
    assert!(other.lock());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn denied_registers_are_refused_by_the_host() {
    let dir = scratch("denied");
    write_plugin(
        &dir,
        "saver.js",
        "Saver",
        "return [0x68];",
        "bus.WriteByte(0x3F, 1); bus.WriteByte(0x30, 2);",
    );
    let path = dir.join("saver.js");
    let source = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        format!(
            "{source}export function SmbusDeniedRegisters() {{ return [0x3F]; }}
"
        ),
    )
    .unwrap();
    let catalog = Catalog::scan(std::slice::from_ref(&dir), Duration::from_secs(10));
    let rec = TransactionRecorder::new();
    let bus = gpu_bus(RTX3060, &rec);
    let (present, _) = scan_buses(&catalog, std::slice::from_ref(&bus), &ScanBudget::default());
    assert_eq!(present[0].plugin.meta.smbus_denied_registers, vec![0x3F]);
    let report = run_present(
        present[0].clone(),
        SessionOptions::default(),
        Rgb::new(1, 1, 1),
        2,
    );
    let stats = report.smbus.clone().expect("smbus stats");
    assert!(stats.denied >= 2, "{stats:?}");
    let sent = rec.payloads(TransactionKind::SmbusWrite);
    assert!(
        sent.iter().all(|p| p[1] != 0x3F),
        "MSI_GPU_REG_SAVE never sent: {sent:?}"
    );
    assert!(sent.contains(&vec![0x68, 0x30, 2]), "other registers still are");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn msi_gpu_plugin_declares_its_save_register() {
    let catalog = catalog();
    let msi = catalog
        .smbus_plugins()
        .find(|e| e.meta.name == "MSI GPU")
        .expect("MSI GPU plugin");
    assert_eq!(msi.meta.smbus_denied_registers, vec![0x3F]);
}

#[test]
fn scan_offers_gpu_plugin_only_matching_card() {
    let catalog = catalog();
    let rec = TransactionRecorder::new();
    let other_card = gpu_bus(
        PciIds {
            sub_device: 0x3904,
            ..RTX3060
        },
        &rec,
    );
    let (present, outcomes) = scan_buses(&catalog, &[other_card], &ScanBudget::default());
    assert!(present.is_empty() && outcomes.is_empty(), "{outcomes:?}");
    let (present, outcomes) = scan_buses(&catalog, &[gpu_bus(RTX3060, &rec)], &ScanBudget::default());
    assert_eq!(present.len(), 1);
    assert_eq!(outcomes[0].plugin, "MSI GPU");
    assert_eq!(outcomes[0].addrs, vec![0x68]);
    assert!(rec.snapshot().is_empty(), "the MSI scan does no I/O");
}

#[test]
fn scan_results_build_allowlist() {
    let dir = scratch("allow");
    // Returns an address outside its window and a duplicate: both dropped.
    write_plugin(&dir, "a.js", "A", "return [0x68, 0x68, 0x50, 'x'];", "");
    let catalog = Catalog::scan(std::slice::from_ref(&dir), Duration::from_secs(10));
    let rec = TransactionRecorder::new();
    let bus = gpu_bus(RTX3060, &rec);
    assert!(bus.allowed().is_empty());
    let (present, outcomes) = scan_buses(&catalog, std::slice::from_ref(&bus), &ScanBudget::default());
    assert_eq!(outcomes[0].addrs, vec![0x68]);
    assert_eq!(present.len(), 1);
    assert_eq!(bus.allowed().into_iter().collect::<Vec<_>>(), vec![0x68]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scan_throwing_plugin_is_skipped_for_that_bus() {
    let dir = scratch("throw");
    write_plugin(&dir, "a.js", "Thrower", "throw new Error('boom');", "");
    write_plugin(&dir, "b.js", "NotArray", "return 0x68;", "");
    write_plugin(
        &dir,
        "c.js",
        "Prober",
        "return bus.ReadByte(0x68, 0x22) === 0x08 ? [0x68] : [];",
        "",
    );
    let catalog = Catalog::scan(std::slice::from_ref(&dir), Duration::from_secs(10));
    let rec = TransactionRecorder::new();
    let bus = gpu_bus(RTX3060, &rec);
    let (present, outcomes) = scan_buses(&catalog, std::slice::from_ref(&bus), &ScanBudget::default());
    assert_eq!(outcomes.len(), 3);
    assert!(
        outcomes[0].error.as_deref().unwrap().contains("boom"),
        "{outcomes:?}"
    );
    assert!(outcomes[1]
        .error
        .as_deref()
        .unwrap()
        .contains("Scan() must return an array."));
    assert_eq!(present.len(), 1);
    assert_eq!(present[0].plugin.meta.name, "Prober");
    assert_eq!(
        rec.payloads(TransactionKind::SmbusRead),
        vec![vec![0x68, 0x22, 0x08]]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn two_plugins_claiming_one_address_first_wins() {
    let dir = scratch("claim");
    write_plugin(&dir, "a.js", "First", "return [0x68];", "");
    write_plugin(&dir, "b.js", "Second", "return [0x68, 0x69];", "");
    let catalog = Catalog::scan(std::slice::from_ref(&dir), Duration::from_secs(10));
    let rec = TransactionRecorder::new();
    let (present, outcomes) = scan_buses(&catalog, &[gpu_bus(RTX3060, &rec)], &ScanBudget::default());
    let owners: Vec<(String, String)> = present
        .iter()
        .map(|p| (p.device.key.clone(), p.plugin.meta.name.clone()))
        .collect();
    assert_eq!(
        owners,
        vec![
            ("smbus:10de:2504:1462:3903@pci01:68".into(), "First".into()),
            ("smbus:10de:2504:1462:3903@pci01:69".into(), "Second".into()),
        ]
    );
    assert!(outcomes[1].error.as_deref().unwrap().contains("0x68"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn bus_arguments_are_validated_like_signalrgb() {
    let dir = scratch("args");
    write_plugin(
        &dir,
        "args.js",
        "Args",
        "return [0x68];",
        "try { bus.WriteByte(300, 1); } catch (e) { device.log(e.name + ': ' + e.message); }\n\
         try { bus.WriteBlock(0x03, 1, 'x'); } catch (e) { device.log(e.name + ': ' + e.message); }\n\
         device.log('fixed ' + bus.Type() + ' ' + bus.GetAddress() + ' ' + bus.IsNvidiaBus() + ' ' + String(bus).startsWith('I2CBusWrapperFixedAddress'));\n\
         try { bus.WriteQuick(); } catch (e) { device.log('quick ' + e.name); }\n\
         bus.WriteBlock(0x03, 2, [7, 8, 9]);",
    );
    let catalog = Catalog::scan(std::slice::from_ref(&dir), Duration::from_secs(10));
    let rec = TransactionRecorder::new();
    let bus = gpu_bus(RTX3060, &rec);
    let (present, _) = scan_buses(&catalog, std::slice::from_ref(&bus), &ScanBudget::default());
    let report = run_present(
        present[0].clone(),
        SessionOptions::default(),
        Rgb::new(1, 1, 1),
        1,
    );
    let has = |needle: &str| report.log_tail.iter().any(|l| l.contains(needle));
    assert!(
        has("RangeError: WriteByte(): Register must be a number between 0 and 255"),
        "{:?}",
        report.log_tail
    );
    assert!(
        has("TypeError: WriteBlock(): Data must be an array."),
        "{:?}",
        report.log_tail
    );
    assert!(has("fixed Fixed 104 true true"), "{:?}", report.log_tail);
    assert!(
        has("quick HostApiMissing"),
        "no Quick on the fixed bus: {:?}",
        report.log_tail
    );
    // Data longer than `len` is clipped (logged once); the count byte is the length sent.
    assert!(rec
        .payloads(TransactionKind::SmbusWrite)
        .contains(&vec![0x68, 0x03, 2, 7, 8]));
    let _ = std::fs::remove_dir_all(&dir);
}

// ------------------------------------------------------------------ Crucial Ballistix (RAM)

const PIIX4: PciIds = PciIds {
    vendor: 0x1022,
    device: 0x790B,
    sub_vendor: 0x1022,
    sub_device: 0x790B,
};

/// A DDR4 SPD (EE1004, 512 bytes) of a module from JEP106 `b320` / `b321`
/// (bytes 320-321 as stored: continuation count with parity, id with parity).
fn ddr4_spd(b320: u8, b321: u8, part: &str) -> Vec<u8> {
    let mut spd = vec![0u8; 512];
    spd[..3].copy_from_slice(&[0x23, 0x11, 0x0C]);
    spd[320] = b320;
    spd[321] = b321;
    let mut p = part.as_bytes().to_vec();
    p.resize(20, b' ');
    spd[329..349].copy_from_slice(&p);
    spd
}

/// The SPD of a Crucial Ballistix module: Crucial Technology, JEP106 bank 6 id 0x9B.
fn ballistix_spd() -> Vec<u8> {
    ddr4_spd(0x85, 0x9B, "BL8G32C16U4WL.M8FE1")
}

/// An AMD PIIX4 bus with Crucial modules (ENE-style indexed register file:
/// identification bytes 0xA0..0xAF = 0..15, "Micron" at 0x1025) at `addrs`,
/// and their SPDs at 0x50, 0x51, ... (the host's SPD check).
fn ram_bus(addrs: &[u8], rec: &TransactionRecorder, fail_block: bool) -> BusHandle {
    ram_bus_with_log(addrs, rec, fail_block).0
}

/// [`ram_bus`] and the sizes of the batches the bus was given.
fn ram_bus_with_log(
    addrs: &[u8],
    rec: &TransactionRecorder,
    fail_block: bool,
) -> (BusHandle, Arc<std::sync::Mutex<Vec<usize>>>) {
    let mut bus = MockSmbus::new("AMD PIIX4 Bus - 0x0b00", SmbusBusKind::Amd, rec.clone())
        .with_pci(PIIX4)
        .with_instance("0b00");
    for i in 0..addrs.len() {
        bus.add_ddr4_spd(0x50 + i as u8, &ballistix_spd());
    }
    let batches = bus.batch_log();
    for &a in addrs {
        if fail_block {
            bus = bus.fail_block_writes(a);
        }
        let ident = (0..16u8).map(|i| (0xA0 + i, i));
        let micron = b"Micron\0"
            .iter()
            .enumerate()
            .map(|(i, b)| (0x1025 + i as u16, *b));
        let version = b"BL8G32C16U4WL\0"
            .iter()
            .enumerate()
            .map(|(i, b)| (0x1000 + i as u16, *b));
        bus.add_indexed_device(a, ident, micron.chain(version));
    }
    (BusHandle::new(Box::new(bus)), batches)
}

/// Discover the RAM bus and run the module at 0x39 for `frames` frames.
fn run_ram(rec: &TransactionRecorder, fail_block: bool, color: Rgb, frames: u64) -> SessionReport {
    let bus = ram_bus(&[0x39, 0x3A], rec, fail_block);
    let (present, outcomes) = scan_buses(&catalog(), std::slice::from_ref(&bus), &ScanBudget::default());
    let keys: Vec<&str> = present.iter().map(|p| p.device.key.as_str()).collect();
    assert_eq!(
        keys,
        vec![
            "smbus:1022:790b:1022:790b@0b00:39",
            "smbus:1022:790b:1022:790b@0b00:3a"
        ],
        "{outcomes:?}"
    );
    assert_eq!(outcomes[0].plugin, "Crucial Ballistix DRAM");
    rec.clear();
    let p = present.into_iter().next().unwrap();
    run_present(p, SessionOptions::default(), color, frames)
}

/// The writes of the first frame: the transactions between the bus lock
/// whose first transfer sets the red plane's pointer (0x8300) and its
/// release. (Initialize holds the bus around each register access too.)
fn locked_group(rec: &TransactionRecorder) -> Vec<Vec<u8>> {
    let all = rec.snapshot();
    let start = (0..all.len())
        .find(|&i| {
            all[i].kind == TransactionKind::SmbusLock
                && !all[i].bytes.is_empty()
                && all.get(i + 1).is_some_and(|t| t.bytes == pointer(0x8300))
        })
        .expect("a frame under TransactionLock");
    let len = all[start + 1..]
        .iter()
        .position(|t| t.kind == TransactionKind::SmbusLock)
        .expect("released");
    all[start + 1..start + 1 + len]
        .iter()
        .inspect(|t| assert_eq!(t.kind, TransactionKind::SmbusWrite, "{t:?}"))
        .map(|t| t.bytes.clone())
        .collect()
}

/// `WriteWord(0x00, reg byte-swapped)` to 0x39: the register pointer.
fn pointer(reg: u16) -> Vec<u8> {
    let [hi, lo] = reg.to_be_bytes();
    vec![0x39, 0x00, hi, lo]
}

#[test]
fn crucial_session_frame_is_three_locked_block_writes() {
    let rec = TransactionRecorder::new();
    let report = run_ram(&rec, false, Rgb::new(0x12, 0x34, 0x56), 3);
    assert!(report.is_clean(), "{report:#?}");
    assert_eq!(
        report.endpoint_path.as_deref(),
        Some("AMD PIIX4 Bus - 0x0b00 @0x39")
    );
    let block = |v: u8| [vec![0x39, 0x03, 8], vec![v; 8]].concat();
    assert_eq!(
        locked_group(&rec),
        vec![
            pointer(0x8300),
            block(0x12),
            pointer(0x8340),
            block(0x34),
            pointer(0x8380),
            block(0x56)
        ]
    );
    let all = rec.snapshot();
    assert!(
        all.iter()
            .all(|t| t.bytes.is_empty() || t.kind == TransactionKind::SmbusLock || t.bytes[0] == 0x39),
        "only the module's own address"
    );
    // Initialize entered static mode: 0x820F <- 0xCF through the pointer.
    let writes = rec.payloads(TransactionKind::SmbusWrite);
    let mode = writes
        .iter()
        .position(|w| *w == pointer(0x820F))
        .expect("mode pointer");
    assert_eq!(writes[mode + 1], vec![0x39, 0x01, 0xCF]);
}

#[test]
fn crucial_block_failure_falls_back_to_bytes() {
    let rec = TransactionRecorder::new();
    let report = run_ram(&rec, true, Rgb::new(1, 2, 3), 2);
    // The frame goes out as one batch (SmbusBatchWrites): its first block
    // write fails, the rest is dropped, TransactionUnlock() reports it and
    // the plugin sends the planes again byte by byte, under a new lock.
    let groups = locked_groups(&rec);
    let first = groups
        .iter()
        .position(|g| g.first() == Some(&pointer(0x8300)))
        .expect("the frame");
    assert_eq!(
        groups[first],
        vec![pointer(0x8300)],
        "the failed block write is not recorded"
    );
    let bytes = |v: u8| vec![vec![0x39, 0x01, v]; 8];
    let mut expected = vec![pointer(0x8300)];
    expected.extend(bytes(1));
    expected.push(pointer(0x8340));
    expected.extend(bytes(2));
    expected.push(pointer(0x8380));
    expected.extend(bytes(3));
    assert_eq!(groups[first + 1], expected);
    let stats = report.smbus.expect("SMBus counters");
    assert_eq!(
        (stats.errors, stats.denied, stats.contended),
        (1, 0, 0),
        "the one failed block write"
    );
}

/// The writes between each bus lock and its release.
fn locked_groups(rec: &TransactionRecorder) -> Vec<Vec<Vec<u8>>> {
    let mut groups = Vec::new();
    let mut open: Option<Vec<Vec<u8>>> = None;
    for t in rec.snapshot() {
        match (t.kind, &mut open) {
            (TransactionKind::SmbusLock, None) if !t.bytes.is_empty() => open = Some(Vec::new()),
            (TransactionKind::SmbusLock, Some(_)) => groups.extend(open.take()),
            (TransactionKind::SmbusWrite, Some(g)) => g.push(t.bytes),
            _ => {}
        }
    }
    groups
}

#[test]
fn crucial_frame_is_one_batch() {
    let rec = TransactionRecorder::new();
    let (bus, batches) = ram_bus_with_log(&[0x39], &rec, false);
    let (present, outcomes) = scan_buses(&catalog(), std::slice::from_ref(&bus), &ScanBudget::default());
    assert_eq!(present.len(), 1, "{outcomes:?}");
    assert!(
        batches.lock().unwrap().is_empty(),
        "Scan(bus) sends its writes one by one"
    );
    let report = run_present(
        present.into_iter().next().unwrap(),
        SessionOptions::default(),
        Rgb::new(9, 8, 7),
        3,
    );
    assert!(report.is_clean(), "{report:#?}");
    let sizes = batches.lock().unwrap().clone();
    // Initialize: 16 version reads (pointer write + read: one batch each),
    // the effect mode (8 writes), 8 LEDs (16 writes each); then the frame:
    // 3 pointers + 3 block writes.
    // Shutdown: the 8 LEDs again, then the mode.
    let mut expected = vec![2; 16];
    expected.push(8);
    expected.extend([16; 8]);
    expected.push(6);
    expected.extend([16; 8]);
    expected.push(8);
    assert_eq!(sizes, expected);
    let stats = report.smbus.unwrap();
    assert_eq!(stats.batches as usize, sizes.len());
}

#[test]
fn two_modules_on_one_bus_take_turns() {
    let rec = TransactionRecorder::new();
    let bus = ram_bus(&[0x39, 0x3A], &rec, false);
    let (present, outcomes) = scan_buses(&catalog(), std::slice::from_ref(&bus), &ScanBudget::default());
    assert_eq!(present.len(), 2, "{outcomes:?}");
    rec.clear();
    let store = Arc::new(FrameStore::new(CANVAS_WIDTH, CANVAS_HEIGHT));
    let backend: Arc<dyn HidBackend> = Arc::new(huedeck_hal::hid::NoHid);
    let handles: Vec<_> = present
        .into_iter()
        .map(|p| {
            session::spawn(
                p.device,
                p.plugin,
                Arc::clone(&backend),
                Arc::clone(&store),
                SessionOptions::default(),
            )
            .unwrap()
        })
        .collect();
    // A new colour every few milliseconds: every Render() writes a frame.
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut shade = 0u8;
    while handles.iter().any(|h| h.live().frames < 8) && Instant::now() < deadline {
        shade = shade.wrapping_add(17);
        store.publish(Frame::solid(
            CANVAS_WIDTH,
            CANVAS_HEIGHT,
            Rgb::new(shade, 0, 255 - shade),
        ));
        std::thread::sleep(Duration::from_millis(5));
    }
    let reports: Vec<SessionReport> = handles
        .into_iter()
        .map(|h| {
            h.stop();
            h.join().expect("session thread")
        })
        .collect();
    for r in &reports {
        assert!(r.frames >= 8, "both sessions render: {r:#?}");
        let s = r.smbus.as_ref().unwrap();
        assert_eq!((s.errors, s.denied, s.contended), (0, 0, 0), "{} {s:?}", r.device);
    }
    // Each module's locked sequences went out whole: no transfer of the
    // other module between a lock and its release.
    let mut owner: Option<u8> = None;
    for t in rec.snapshot() {
        match t.kind {
            TransactionKind::SmbusLock if !t.bytes.is_empty() => owner = Some(0),
            TransactionKind::SmbusLock => owner = None,
            TransactionKind::SmbusWrite | TransactionKind::SmbusRead => {
                if let Some(o) = owner.as_mut() {
                    if *o == 0 {
                        *o = t.bytes[0];
                    }
                    assert_eq!(*o, t.bytes[0], "interleaved inside a locked sequence");
                }
            }
            _ => {}
        }
    }
    let written: std::collections::BTreeSet<u8> = rec
        .payloads(TransactionKind::SmbusWrite)
        .iter()
        .map(|w| w[0])
        .collect();
    assert_eq!(written.into_iter().collect::<Vec<_>>(), vec![0x39, 0x3A]);
}

#[test]
fn ram_plugins_scan_only_where_the_spd_names_their_maker() {
    let catalog = catalog();
    let scan = |spd: Option<Vec<u8>>| {
        let rec = TransactionRecorder::new();
        let mut bus = MockSmbus::new("AMD PIIX4 Bus - 0x0b00", SmbusBusKind::Amd, rec.clone())
            .with_pci(PIIX4)
            .with_instance("0b00");
        if let Some(spd) = spd {
            bus.add_ddr4_spd(0x50, &spd);
        }
        let ident = (0..16u8).map(|i| (0xA0 + i, i));
        let micron = b"Micron\0"
            .iter()
            .enumerate()
            .map(|(i, b)| (0x1025 + i as u16, *b));
        bus.add_indexed_device(0x39, ident, micron);
        let (present, outcomes) =
            scan_buses(&catalog, &[BusHandle::new(Box::new(bus))], &ScanBudget::default());
        let touched_rgb = rec.snapshot().iter().any(|t| t.bytes.first() == Some(&0x39));
        (present.len(), outcomes, touched_rgb)
    };
    // Crucial Technology and Micron Technology modules: scanned, the slot handed over.
    for spd in [ballistix_spd(), ddr4_spd(0x80, 0x2C, "MTA8ATF1G64AZ")] {
        let (found, outcomes, touched) = scan(Some(spd));
        assert_eq!(found, 1, "{outcomes:?}");
        assert!(touched);
        let slots = outcomes[0].spd_slots.as_ref().unwrap();
        assert_eq!(slots[0].address, 0x50);
    }
    // Kingston (bank 2), no SPD at all, DDR5: the plugin never runs.
    let mut ddr5 = vec![0u8; 512];
    ddr5[..3].copy_from_slice(&[0x30, 0x10, 0x12]);
    for (spd, why) in [
        (Some(ddr4_spd(0x01, 0x98, "KF432C16BB/8")), "no module from"),
        (None, "no SPD answered"),
        (Some(ddr5), "DDR5"),
    ] {
        let (found, outcomes, touched) = scan(spd);
        assert_eq!(found, 0);
        assert!(!touched, "the RGB controller's addresses were never touched");
        let e = outcomes[0].error.as_deref().unwrap();
        assert!(e.contains(why), "{e}");
    }
}

#[test]
fn ram_plugin_is_not_offered_gpu_buses_nor_gpu_plugin_ram_buses() {
    let rec = TransactionRecorder::new();
    let catalog = catalog();
    let (_, outcomes) = scan_buses(&catalog, &[ram_bus(&[0x39], &rec, false)], &ScanBudget::default());
    let plugins: Vec<&str> = outcomes.iter().map(|o| o.plugin.as_str()).collect();
    assert_eq!(plugins, vec!["Crucial Ballistix DRAM"]);
    let (_, outcomes) = scan_buses(&catalog, &[gpu_bus(RTX3060, &rec)], &ScanBudget::default());
    let plugins: Vec<&str> = outcomes.iter().map(|o| o.plugin.as_str()).collect();
    assert_eq!(plugins, vec!["MSI GPU"]);
}

/// The EE1004 page select (a Send Byte to 0x36 / 0x37) is the host's own
/// step of its SPD check: a plugin, even a RAM plugin offered the bus, has
/// no member that reaches it and every write it tries in 0x30-0x37 or
/// 0x50-0x57 is refused.
#[test]
fn plugins_cannot_select_spd_pages_or_write_the_spd() {
    let dir = scratch("spd-pages");
    std::fs::write(
        dir.join("ram.js"),
        "export function Name() { return 'Sneaky RAM'; }\n\
         export function Type() { return 'SMBUS'; }\n\
         export function DeviceType() { return 'ram'; }\n\
         export function SmbusBuses() { return ['amd']; }\n\
         export function SmbusAddresses() { return [0x36, 0x37, 0x50, 0x51]; }\n\
         export function SpdManufacturers() { return [{ bank: 6, id: 0x9B, name: 'Crucial Technology' }]; }\n\
         export function Scan(bus) {\n\
           bus.log('members ' + Object.keys(bus).filter((k) => /spd|page|ee1004/i.test(k)).join(','));\n\
           bus.log('slots ' + JSON.stringify(bus.SpdSlots().map((s) => [s.address, s.memoryType, s.jedecBank, s.jedecId])));\n\
           const st = [bus.WriteByte(0x36, 0, 0), bus.WriteQuick(0x37), bus.WriteByte(0x50, 0, 0), bus.WriteWord(0x51, 0, 0)];\n\
           bus.log('statuses ' + st.join(','));\n\
           bus.log('read ' + bus.ReadByte(0x50, 2));\n\
           return [];\n\
         }\n\
         export function Initialize() {}\n\
         export function Render() {}\n\
         export function Shutdown() {}\n",
    )
    .unwrap();
    let catalog = Catalog::scan(std::slice::from_ref(&dir), Duration::from_secs(10));
    let rec = TransactionRecorder::new();
    let mut bus = MockSmbus::new("AMD PIIX4 Bus - 0x0b00", SmbusBusKind::Amd, rec.clone())
        .with_pci(PIIX4)
        .with_instance("0b00");
    bus.add_ddr4_spd(0x50, &ballistix_spd());
    let (_, outcomes) = scan_buses(&catalog, &[BusHandle::new(Box::new(bus))], &ScanBudget::default());
    assert_eq!(outcomes.len(), 1);
    let log = &outcomes[0].log;
    let has = |needle: &str| log.iter().any(|l| l == needle || l.contains(needle));
    assert!(has("members SpdSlots"), "only the read-only slot list: {log:?}");
    assert!(has("slots [[80,\"DDR4\",6,155]]"), "{log:?}");
    assert!(has("statuses -1,-1,-1,-1"), "{log:?}");
    assert!(has("read 12"), "SPD reads are fine: {log:?}");
    assert!(has("SPD / EEPROM / write-protect / PMIC address"), "{log:?}");
    // On the wire: the host's page selects (page 1 then page 0, for the
    // manufacturer id and the part number) and nothing the plugin asked.
    let writes = rec.payloads(TransactionKind::SmbusWrite);
    assert_eq!(writes.len(), 4, "{writes:?}");
    assert!(
        writes
            .chunks(2)
            .all(|w| w == [vec![0x37, 0x00], vec![0x36, 0x00]]),
        "{writes:?}"
    );
    assert!(rec.payloads(TransactionKind::SmbusWriteQuick).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
