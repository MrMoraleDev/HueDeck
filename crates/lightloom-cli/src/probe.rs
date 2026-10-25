// SPDX-License-Identifier: GPL-3.0-or-later
//! `lightloom probe msi-gpu-perled`: a supervised hardware test of MSI's
//! per-LED stream on the MSI GeForce RTX 3060 Gaming X Trio LHR
//! (docs/research/msi-gpu-perled.md §5.2), run with the user watching the card.
//!
//! 1. Identify (read only): 0xEC (4 bytes: LED type, module type, firmware)
//!    and 0xED (2 bytes: chip id). Stop on a failed read or an "NV5000"
//!    controller (module type 0x5000 or LED type 0x81), before any write.
//! 2. Enter: 0x2E <- 1.
//! 3. Step: for each of the 34 slots, a 34-slot 0x49 frame with only that
//!    slot lit (red, in the `--order` channel order), then Enter or `--auto`.
//! 4. Colour order: every slot red for 3 s, then blue for 3 s.
//! 5. Short frame: only the first 6 slots (24 bytes), green; the result code.
//! 6. Leave: 0x2E <- 0, always once step 2 was tried: normal end, an error,
//!    Ctrl-C / console close, and a panic.
//!
//! Every write goes through [`Guarded`], which logs it with its bytes and
//! refuses anything but 0x2E <- 0/1 and a raw write starting with 0x49: the
//! save register 0x3F and the mode register 0x22 can never be reached from
//! here. `--dry-run` prints the same transfers without loading NvAPI.

use anyhow::{bail, Context};
use clap::{Args, ValueEnum};
use lightloom_hal::smbus::SmbusBus;
use lightloom_hal::PciIds;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};

/// The controller's 7-bit address.
pub const ADDR: u8 = 0x68;
/// Per-LED stream enable: 1 on, 0 off.
pub const REG_DIRECT: u8 = 0x2E;
/// Per-LED frame register.
pub const REG_FRAME: u8 = 0x49;
/// Firmware info (read only, 4 bytes).
pub const REG_FW_INFO: u8 = 0xEC;
/// Chip id (read only, 2 bytes).
pub const REG_CHIP_ID: u8 = 0xED;
/// Slots MSI always sends.
pub const SLOTS: usize = 34;
/// Slots of the short-frame test.
pub const SHORT_SLOTS: usize = 6;
/// How long each colour-order and short-frame step stays on.
pub const HOLD: Duration = Duration::from_secs(3);

/// The cards this probe accepts (the msi-gpu plugin's list, all four ids).
pub const CARDS: &[PciIds] = &[PciIds {
    vendor: 0x10DE,
    device: 0x2504,
    sub_vendor: 0x1462,
    sub_device: 0x3903,
}];

/// Channel order inside a 4-byte record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Order {
    /// `[R, G, B, 0]`.
    Rgb,
    /// `[B, G, R, 0]` (MSI reverses its input bytes; the best guess).
    Bgr,
}

impl Order {
    /// The record for colour `[r, g, b]`.
    pub fn record(self, [r, g, b]: [u8; 3]) -> [u8; 4] {
        match self {
            Order::Rgb => [r, g, b, 0],
            Order::Bgr => [b, g, r, 0],
        }
    }
}

/// `[0x49, record…]` for `colors`, one record each.
pub fn frame(order: Order, colors: &[[u8; 3]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + 4 * colors.len());
    out.push(REG_FRAME);
    for c in colors {
        out.extend_from_slice(&order.record(*c));
    }
    out
}

/// A 34-slot frame with only `slot` lit in `color`.
pub fn single(order: Order, slot: usize, color: [u8; 3]) -> Vec<u8> {
    let colors: Vec<[u8; 3]> = (0..SLOTS)
        .map(|i| if i == slot { color } else { [0; 3] })
        .collect();
    frame(order, &colors)
}

/// A frame of `n` slots all in `color`.
pub fn solid(order: Order, color: [u8; 3], n: usize) -> Vec<u8> {
    frame(order, &vec![color; n])
}

/// `lightloom probe msi-gpu-perled` arguments.
#[derive(Debug, Args)]
pub struct PerLedArgs {
    /// Channel order of each 4-byte record.
    #[arg(long, value_enum, default_value_t = Order::Bgr)]
    pub order: Order,
    /// Step to the next slot after this many milliseconds instead of waiting for Enter.
    #[arg(long, value_name = "MS")]
    pub auto: Option<u64>,
    /// Channel value of a lit colour (1-255).
    #[arg(long, default_value_t = 0x40, value_parser = clap::value_parser!(u8).range(1..))]
    pub brightness: u8,
    /// Print every transfer without loading NvAPI or touching the card.
    #[arg(long)]
    pub dry_run: bool,
}

/// What the probe talks to: the card's bus at [`ADDR`]. Errors are statuses
/// (NvAPI's own on NVIDIA buses).
pub trait ProbeBus {
    /// `len` bytes after register `reg`.
    fn read(&mut self, reg: u8, len: usize) -> Result<Vec<u8>, i32>;
    /// Write Byte Data.
    fn write_byte(&mut self, reg: u8, value: u8) -> Result<(), i32>;
    /// One raw I2C write (first byte = register), no count byte.
    fn write_raw(&mut self, data: &[u8]) -> Result<(), i32>;
}

/// A HAL bus (NvAPI on the card; a mock in tests).
pub struct HalBus(pub Box<dyn SmbusBus>);

impl ProbeBus for HalBus {
    fn read(&mut self, reg: u8, len: usize) -> Result<Vec<u8>, i32> {
        let mut buf = vec![0; len];
        let n = self
            .0
            .read_block(ADDR, reg, &mut buf)
            .map_err(|e| e.plugin_status())?;
        buf.truncate(n);
        Ok(buf)
    }
    fn write_byte(&mut self, reg: u8, value: u8) -> Result<(), i32> {
        self.0
            .write_byte(ADDR, reg, value)
            .map_err(|e| e.plugin_status())
    }
    fn write_raw(&mut self, data: &[u8]) -> Result<(), i32> {
        self.0
            .write_block_no_reg(ADDR, data)
            .map_err(|e| e.plugin_status())
    }
}

/// `--dry-run`: nothing is sent; reads answer zeros (a v1 controller).
pub struct DryRun;

impl ProbeBus for DryRun {
    fn read(&mut self, _: u8, len: usize) -> Result<Vec<u8>, i32> {
        Ok(vec![0; len])
    }
    fn write_byte(&mut self, _: u8, _: u8) -> Result<(), i32> {
        Ok(())
    }
    fn write_raw(&mut self, _: &[u8]) -> Result<(), i32> {
        Ok(())
    }
}

/// The user's side: messages, pacing and stop requests.
pub trait Ui {
    /// One line of output.
    fn say(&mut self, line: &str);
    /// Slot `slot` is lit: wait for the user (or `--auto`). False: stop.
    fn next_slot(&mut self, slot: usize) -> bool;
    /// Keep the current frame for `d`. False: stop requested meanwhile.
    fn hold(&mut self, d: Duration) -> bool;
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The only way the probe writes: logged, and limited to 0x2E <- 0/1 and
/// raw 0x49 frames of at most 34 records.
pub struct Guarded<'a> {
    bus: &'a mut dyn ProbeBus,
    dry_run: bool,
}

impl<'a> Guarded<'a> {
    /// Guard `bus`.
    pub fn new(bus: &'a mut dyn ProbeBus, dry_run: bool) -> Self {
        Self { bus, dry_run }
    }

    fn tag(&self) -> &'static str {
        if self.dry_run {
            " (dry run, not sent)"
        } else {
            ""
        }
    }

    fn read(&mut self, ui: &mut dyn Ui, reg: u8, len: usize) -> Result<Vec<u8>, i32> {
        let r = self.bus.read(reg, len);
        match &r {
            Ok(b) => ui.say(&format!("read  0x{ADDR:02X} reg 0x{reg:02X} x{len}: {} -> 0{}", hex(b), self.tag())),
            Err(s) => ui.say(&format!("read  0x{ADDR:02X} reg 0x{reg:02X} x{len}: failed -> {s}")),
        }
        r
    }

    /// `0x2E <- value` (0 or 1 only).
    fn direct(&mut self, ui: &mut dyn Ui, value: u8) -> Result<(), i32> {
        if value > 1 {
            ui.say(&format!("REFUSED: 0x2E <- 0x{value:02X} (only 0 and 1)"));
            return Err(-1);
        }
        let r = self.bus.write_byte(REG_DIRECT, value);
        ui.say(&format!(
            "write 0x{ADDR:02X}: {} -> {}{}",
            hex(&[REG_DIRECT, value]),
            r.err().unwrap_or(0),
            self.tag()
        ));
        r
    }

    /// One raw frame (must start with 0x49, at most 34 records).
    fn frame(&mut self, ui: &mut dyn Ui, data: &[u8]) -> Result<(), i32> {
        if data.first() != Some(&REG_FRAME) || data.len() < 5 || data.len() > 1 + 4 * SLOTS {
            ui.say(&format!("REFUSED: raw write of {} bytes starting {:02X?}", data.len(), data.first()));
            return Err(-1);
        }
        let r = self.bus.write_raw(data);
        ui.say(&format!(
            "write 0x{ADDR:02X} ({} bytes): {} -> {}{}",
            data.len(),
            hex(data),
            r.err().unwrap_or(0),
            self.tag()
        ));
        r
    }

    /// A full frame, tried twice (single NvAPI writes fail now and then).
    fn frame_retry(&mut self, ui: &mut dyn Ui, data: &[u8]) -> anyhow::Result<()> {
        if self.frame(ui, data).is_ok() {
            return Ok(());
        }
        ui.say("frame failed; trying once more");
        self.frame(ui, data)
            .map_err(|s| anyhow::anyhow!("the 0x49 frame failed twice (status {s})"))
    }
}

/// How the probe ended (an error is the `Err` of [`run`]).
#[derive(Debug, PartialEq, Eq)]
pub enum Ended {
    /// Every step ran.
    Done,
    /// The user stopped it (Ctrl-C, console close, `q`).
    Stopped,
}

/// The identification reads: `(led_type, module_type, firmware, chip_id)`.
fn identify(g: &mut Guarded<'_>, ui: &mut dyn Ui) -> anyhow::Result<(u8, u16, u8, u16)> {
    let info = g
        .read(ui, REG_FW_INFO, 4)
        .map_err(|s| anyhow::anyhow!("reading 0xEC failed (status {s}); nothing was written"))?;
    let chip = g
        .read(ui, REG_CHIP_ID, 2)
        .map_err(|s| anyhow::anyhow!("reading 0xED failed (status {s}); nothing was written"))?;
    if info.len() < 4 || chip.len() < 2 {
        bail!("short identification reads; nothing was written");
    }
    Ok((
        info[0],
        u16::from_be_bytes([info[1], info[2]]),
        info[3],
        u16::from_be_bytes([chip[0], chip[1]]),
    ))
}

/// Run the probe on `bus`. Once the enter write was tried, `0x2E <- 0` is
/// the last write on every path (tried twice if it fails).
pub fn run(bus: &mut dyn ProbeBus, args: &PerLedArgs, ui: &mut dyn Ui) -> anyhow::Result<Ended> {
    let mut g = Guarded::new(bus, args.dry_run);
    let (led_type, module, fw, chip) = identify(&mut g, ui)?;
    ui.say(&format!(
        "LED type 0x{led_type:02X}, module type 0x{module:04X}, firmware 0x{fw:02X}, chip id 0x{chip:04X}"
    ));
    if module == 0x5000 || led_type == 0x81 {
        bail!("an NV5000 controller: it takes another protocol; nothing was written");
    }
    ui.say("v1 controller: entering the per-LED stream (0x2E <- 1)");
    let entered = g.direct(ui, 1);
    let result = match entered {
        Err(s) => Err(anyhow::anyhow!("entering the stream (0x2E <- 1) failed with status {s}")),
        Ok(()) => match catch_unwind(AssertUnwindSafe(|| steps(&mut g, args, ui))) {
            Ok(r) => r,
            Err(_) => Err(anyhow::anyhow!("the probe panicked")),
        },
    };
    ui.say("leaving the per-LED stream (0x2E <- 0)");
    if g.direct(ui, 0).is_err() && g.direct(ui, 0).is_err() {
        ui.say("WARNING: leaving failed twice: the card may stay in per-LED mode until a power cycle");
        return Err(result.err().unwrap_or_else(|| anyhow::anyhow!("leaving the stream (0x2E <- 0) failed")));
    }
    result
}

fn steps(g: &mut Guarded<'_>, args: &PerLedArgs, ui: &mut dyn Ui) -> anyhow::Result<Ended> {
    let v = args.brightness;
    let (red, blue, green) = ([v, 0, 0], [0, 0, v], [0, v, 0]);
    let order = args.order;
    ui.say(&format!(
        "step mode: one slot at a time, red, order {order:?}: record {}",
        hex(&order.record(red))
    ));
    for slot in 0..SLOTS {
        g.frame_retry(ui, &single(order, slot, red))?;
        if !ui.next_slot(slot) {
            return Ok(Ended::Stopped);
        }
    }
    for (name, color) in [("red", red), ("blue", blue)] {
        ui.say(&format!(
            "colour order: all {SLOTS} slots {name} = RGB {}, sent as {} per record ({order:?}) for {} s",
            hex(&color),
            hex(&order.record(color)),
            HOLD.as_secs()
        ));
        g.frame_retry(ui, &solid(order, color, SLOTS))?;
        if !ui.hold(HOLD) {
            return Ok(Ended::Stopped);
        }
    }
    ui.say(&format!(
        "short frame: only the first {SHORT_SLOTS} slots ({} data bytes) green; the rest were blue",
        4 * SHORT_SLOTS
    ));
    let status = g.frame(ui, &solid(order, green, SHORT_SLOTS)).err().unwrap_or(0);
    ui.say(&format!("short frame result code: {status}"));
    if !ui.hold(HOLD) {
        return Ok(Ended::Stopped);
    }
    Ok(Ended::Done)
}

/// The card's bus: the one NVIDIA GPU whose four PCI ids are in [`CARDS`].
fn open_card() -> anyhow::Result<HalBus> {
    let hub = lightloom_nvapi::NvapiHub::shared().context("NvAPI is not available")?;
    let mut found = Vec::new();
    let mut others = Vec::new();
    for bus in hub.buses() {
        let d = bus.descriptor().clone();
        if CARDS.contains(&d.pci) {
            found.push(bus);
        } else {
            others.push(format!("{} ({})", d.name, d.key()));
        }
    }
    match found.len() {
        1 => Ok(HalBus(Box::new(found.remove(0)))),
        0 => bail!(
            "no MSI GeForce RTX 3060 Gaming X Trio LHR (10DE:2504 1462:3903) found; other cards are refused: [{}]",
            others.join(", ")
        ),
        n => bail!("{n} matching cards found; the probe drives exactly one"),
    }
}

/// The console: Enter / `q`, Ctrl-C and console close.
struct Console {
    interrupt: crate::run::Interrupt,
    lines: std::sync::mpsc::Receiver<String>,
    auto: Option<Duration>,
}

impl Console {
    fn new(auto: Option<u64>) -> Self {
        let (tx, lines) = std::sync::mpsc::channel();
        if auto.is_none() {
            // Stdin is read on its own thread so a stop request is seen while waiting.
            std::thread::spawn(move || {
                for line in std::io::stdin().lines() {
                    let Ok(line) = line else { break };
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            });
        }
        Self {
            interrupt: crate::run::Interrupt::install(),
            lines,
            auto: auto.map(Duration::from_millis),
        }
    }
}

impl Ui for Console {
    fn say(&mut self, line: &str) {
        println!("{line}");
    }

    fn next_slot(&mut self, slot: usize) -> bool {
        if let Some(d) = self.auto {
            println!("slot {slot} lit; next in {} ms", d.as_millis());
            return self.hold(d);
        }
        println!("slot {slot} lit: note which LED shows it, then press Enter (q + Enter stops)");
        loop {
            if self.interrupt.requested() {
                println!("stop requested");
                return false;
            }
            match self.lines.recv_timeout(Duration::from_millis(50)) {
                Ok(line) => return !line.trim().eq_ignore_ascii_case("q"),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    println!("standard input closed; stopping (use --auto <ms> without a console)");
                    return false;
                }
            }
        }
    }

    fn hold(&mut self, d: Duration) -> bool {
        let end = Instant::now() + d;
        while Instant::now() < end {
            if self.interrupt.requested() {
                println!("stop requested");
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        !self.interrupt.requested()
    }
}

/// `lightloom probe msi-gpu-perled`.
pub fn run_cli(args: &PerLedArgs) -> anyhow::Result<crate::run::Outcome> {
    let mut console = Console::new(args.auto);
    let ended = if args.dry_run {
        println!("dry run: NvAPI is not loaded; reads answer zeros");
        run(&mut DryRun, args, &mut console)
    } else {
        // Not beside lightloomd, SignalRGB, OpenRGB or MSI's service; held until the end.
        let _claim = crate::run::claim_smbus_now().map_err(|why| anyhow::anyhow!("GPU I2C is not free: {why}"))?;
        let mut bus = open_card()?;
        println!("card: {}", bus.0.descriptor().name);
        run(&mut bus, args, &mut console)
    };
    console.interrupt.finished();
    Ok(match ended? {
        Ended::Done => crate::run::Outcome::Clean,
        Ended::Stopped => crate::run::Outcome::Failed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_hal::mock::{MockSmbus, TransactionKind, TransactionRecorder};
    use lightloom_hal::SmbusBusKind;

    fn args() -> PerLedArgs {
        PerLedArgs {
            order: Order::Bgr,
            auto: Some(0),
            brightness: 0x40,
            dry_run: false,
        }
    }

    /// Answers at once; stops after `stop_after` slots, if set.
    #[derive(Default)]
    struct TestUi {
        lines: Vec<String>,
        stop_after: Option<usize>,
        stop_in_hold: bool,
    }

    impl Ui for TestUi {
        fn say(&mut self, line: &str) {
            self.lines.push(line.to_string());
        }
        fn next_slot(&mut self, slot: usize) -> bool {
            self.stop_after.is_none_or(|n| slot + 1 < n)
        }
        fn hold(&mut self, _: Duration) -> bool {
            !self.stop_in_hold
        }
    }

    /// A mock card at 0x68 with `regs`; `fail_frame` fails raw writes from that one on.
    struct Card {
        bus: HalBus,
        rec: TransactionRecorder,
        raw: usize,
        fail_frame: Option<usize>,
        fail_reads: bool,
        panic_frame: Option<usize>,
    }

    impl Card {
        fn new(regs: &[(u8, u8)]) -> Self {
            let rec = TransactionRecorder::new();
            let mut bus = MockSmbus::new("NVIDIA GeForce RTX 3060", SmbusBusKind::Nvidia, rec.clone());
            bus.add_device(ADDR, regs.iter().copied());
            Self {
                bus: HalBus(Box::new(bus)),
                rec,
                raw: 0,
                fail_frame: None,
                fail_reads: false,
                panic_frame: None,
            }
        }

        /// Every write as `[addr, bytes…]`.
        fn writes(&self) -> Vec<Vec<u8>> {
            self.rec.payloads(TransactionKind::SmbusWrite)
        }
    }

    impl ProbeBus for Card {
        fn read(&mut self, reg: u8, len: usize) -> Result<Vec<u8>, i32> {
            if self.fail_reads {
                return Err(-5);
            }
            self.bus.read(reg, len)
        }
        fn write_byte(&mut self, reg: u8, value: u8) -> Result<(), i32> {
            self.bus.write_byte(reg, value)
        }
        fn write_raw(&mut self, data: &[u8]) -> Result<(), i32> {
            self.raw += 1;
            if self.panic_frame == Some(self.raw) {
                panic!("test panic");
            }
            if self.fail_frame.is_some_and(|n| self.raw >= n) {
                return Err(-1);
            }
            self.bus.write_raw(data)
        }
    }

    const V1: &[(u8, u8)] = &[(0xEC, 0x01), (0xED, 0x01), (0xEE, 0x02), (0xEF, 0x03)];

    /// Only 0x2E <- 0/1 and raw 0x49 frames; the last write is 0x2E <- 0.
    fn assert_safe(card: &Card) {
        let w = card.writes();
        assert!(!w.is_empty());
        for p in &w {
            assert_eq!(p[0], ADDR);
            match p[1] {
                REG_DIRECT => assert!(p.len() == 3 && p[2] <= 1, "{p:02X?}"),
                REG_FRAME => assert!(p.len() > 3, "{p:02X?}"),
                other => panic!("write to register 0x{other:02X}"),
            }
        }
        assert_eq!(w.last().unwrap(), &vec![ADDR, REG_DIRECT, 0x00], "leaves last");
    }

    #[test]
    fn records_follow_the_order() {
        assert_eq!(Order::Bgr.record([1, 2, 3]), [3, 2, 1, 0]);
        assert_eq!(Order::Rgb.record([1, 2, 3]), [1, 2, 3, 0]);
        let f = single(Order::Bgr, 2, [0x40, 0, 0]);
        assert_eq!(f.len(), 1 + 4 * SLOTS);
        assert_eq!(f[0], REG_FRAME);
        assert_eq!(&f[1 + 8..1 + 12], &[0, 0, 0x40, 0]);
        assert!(f.iter().enumerate().all(|(i, b)| i == 0 || i == 11 || *b == 0));
        let s = solid(Order::Rgb, [0, 0x40, 0], SHORT_SLOTS);
        assert_eq!(s.len(), 25);
        assert_eq!(&s[1..5], &[0, 0x40, 0, 0]);
    }

    #[test]
    fn full_run_writes_only_the_stream_registers_and_leaves_last() {
        let mut card = Card::new(V1);
        let mut ui = TestUi::default();
        assert_eq!(run(&mut card, &args(), &mut ui).unwrap(), Ended::Done);
        assert_safe(&card);
        let w = card.writes();
        assert_eq!(w[0], vec![ADDR, REG_DIRECT, 0x01]);
        // 34 step frames, red and blue, the short frame.
        let frames: Vec<&Vec<u8>> = w.iter().filter(|p| p[1] == REG_FRAME).collect();
        assert_eq!(frames.len(), SLOTS + 3);
        assert_eq!(frames[0].len(), 1 + 1 + 4 * SLOTS);
        assert_eq!(&frames[0][2..6], &[0, 0, 0x40, 0], "slot 0 red, B,G,R");
        assert_eq!(&frames[SLOTS][2..6], &[0, 0, 0x40, 0], "all red");
        assert_eq!(&frames[SLOTS + 1][2..6], &[0x40, 0, 0, 0], "all blue");
        assert_eq!(frames[SLOTS + 2].len(), 1 + 1 + 4 * SHORT_SLOTS);
        assert!(ui.lines.iter().any(|l| l == "short frame result code: 0"));
        assert!(ui.lines.iter().any(|l| l.starts_with("LED type 0x01, module type 0x0102, firmware 0x03")));
    }

    #[test]
    fn a_stop_mid_way_still_leaves() {
        for (stop_after, stop_in_hold) in [(Some(3), false), (None, true)] {
            let mut card = Card::new(V1);
            let mut ui = TestUi {
                stop_after,
                stop_in_hold,
                ..TestUi::default()
            };
            assert_eq!(run(&mut card, &args(), &mut ui).unwrap(), Ended::Stopped);
            assert_safe(&card);
        }
    }

    #[test]
    fn a_failing_frame_or_a_panic_still_leaves() {
        let mut card = Card::new(V1);
        card.fail_frame = Some(5);
        let err = run(&mut card, &args(), &mut TestUi::default()).unwrap_err();
        assert!(err.to_string().contains("failed twice"), "{err}");
        assert_safe(&card);

        let mut card = Card::new(V1);
        card.panic_frame = Some(2);
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let r = run(&mut card, &args(), &mut TestUi::default());
        std::panic::set_hook(hook);
        assert!(r.unwrap_err().to_string().contains("panicked"));
        assert_safe(&card);
    }

    #[test]
    fn nv5000_or_failed_reads_write_nothing() {
        for regs in [
            &[(0xEC, 0x01), (0xED, 0x50), (0xEE, 0x00), (0xEF, 0x01)][..],
            &[(0xEC, 0x81), (0xED, 0x00), (0xEE, 0x00), (0xEF, 0x01)][..],
        ] {
            let mut card = Card::new(regs);
            let err = run(&mut card, &args(), &mut TestUi::default()).unwrap_err();
            assert!(err.to_string().contains("NV5000"), "{err}");
            assert!(card.writes().is_empty());
        }
        let mut card = Card::new(V1);
        card.fail_reads = true;
        assert!(run(&mut card, &args(), &mut TestUi::default()).is_err());
        assert!(card.writes().is_empty());
    }

    #[test]
    fn the_guard_refuses_other_registers() {
        let mut card = Card::new(V1);
        let mut ui = TestUi::default();
        let mut g = Guarded::new(&mut card, false);
        assert_eq!(g.frame(&mut ui, &[0x3F, 1, 2, 3, 4]), Err(-1));
        assert_eq!(g.frame(&mut ui, &[0x22, 1, 2, 3, 4]), Err(-1));
        assert_eq!(g.frame(&mut ui, &vec![REG_FRAME; 2 + 4 * SLOTS]), Err(-1), "over 34 records");
        assert_eq!(g.direct(&mut ui, 2), Err(-1));
        assert!(card.writes().is_empty());
    }

    #[test]
    fn dry_run_logs_every_transfer() {
        let mut ui = TestUi::default();
        let a = PerLedArgs {
            dry_run: true,
            order: Order::Rgb,
            ..args()
        };
        assert_eq!(run(&mut DryRun, &a, &mut ui).unwrap(), Ended::Done);
        let writes: Vec<&String> = ui.lines.iter().filter(|l| l.starts_with("write")).collect();
        assert_eq!(writes.len(), 1 + SLOTS + 3 + 1);
        assert!(writes.iter().all(|l| l.ends_with("(dry run, not sent)")));
        assert!(writes[1].contains("49 40 00 00 00 00"), "{}", writes[1]);
        assert_eq!(writes.last().unwrap().as_str(), "write 0x68: 2E 00 -> 0 (dry run, not sent)");
    }
}
