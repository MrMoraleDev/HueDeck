// SPDX-License-Identifier: GPL-3.0-or-later
//! Serial device sessions: `Type() == "serial"` plugins and `@SignalRGB/serial`.
//!
//! A serial plugin exports `VendorId()` / `ProductId()` like a HID plugin; the
//! loader matches the USB device by VID:PID and the module binds the COM / tty
//! port that belongs to that USB device (hid-usb.md §3). A [`SerialTarget`] is
//! that device: the backend and the ports whose USB parent it is.
//!
//! Serial ports are shared resources, so the policy here is strict:
//!
//! * only the target's own ports can be opened: `connect({portName})` with a
//!   port of another device is refused (and counted), whatever the plugin
//!   passes;
//! * `availablePorts()` lists the target's ports only, so a plugin that
//!   picks a port from the list (Hyte CNVS) still finds its own;
//! * nothing is ever written to find out what a port is: enumeration comes
//!   from the OS, the plugin's first write goes to its own device.
//!
//! [`install`] adds the `serial*` natives behind the module (`js/host.js`,
//! `serialModule`). Under [`WriteMode::DryRun`] the port is opened (reads
//! reach the device, as for HID) but every write is recorded and reported as
//! sent without reaching it. Blocking natives suspend the JavaScript budget;
//! reads wait in slices so a stop request ends them early.

use crate::device::{lock_device, panic_text, LogSink, SharedDevice, WriteMode};
use crate::runtime::Deadline;
use lightloom_hal::serial::{Parity, SerialBackend, SerialConfig, SerialHandle, SerialPortInfo};
use lightloom_hal::HalError;
use rquickjs::{Ctx, Function, Object};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Longest `serial.read` wait (the documented default is 1000 ms).
pub const MAX_READ_TIMEOUT: Duration = Duration::from_millis(5000);
/// Largest single `serial.write` / `serial.read` (an LCD frame fits).
pub const MAX_TRANSFER: usize = 1 << 20;
/// Serial records kept for the session report.
const SERIAL_LOG_LIMIT: usize = 64;
/// Bytes of one record kept in the report.
const RECORD_BYTES: usize = 256;
/// Wait slice of a read (a stop request ends a read within this).
const READ_SLICE: Duration = Duration::from_millis(100);

/// The USB device a serial session drives: its ports (sorted, the one
/// `Validate(endpoint)` accepts first is bound by `connect()` without a
/// `portName`) and the backend that opens them.
#[derive(Clone)]
pub struct SerialTarget {
    /// Opens the ports.
    pub backend: Arc<dyn SerialBackend>,
    /// The ports of this USB device (never empty).
    pub ports: Vec<SerialPortInfo>,
}

impl std::fmt::Debug for SerialTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SerialTarget")
            .field("ports", &self.ports)
            .finish()
    }
}

/// `serial` key segment of a port: the USB serial number when the device has
/// one, else the port name (stable for a given USB port on Windows).
pub fn port_key_serial(p: &SerialPortInfo) -> String {
    p.serial
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(&p.port_name)
        .to_string()
}

/// Group `ports` into USB devices: `(vid, pid, serial key)` → its ports, in
/// port order. Ports without a USB parent are left out.
pub fn group_ports(ports: Vec<SerialPortInfo>) -> Vec<((u16, u16, String), Vec<SerialPortInfo>)> {
    let mut groups: Vec<((u16, u16, String), Vec<SerialPortInfo>)> = Vec::new();
    for p in ports {
        let Some((vid, pid)) = p.usb_ids() else {
            continue;
        };
        // A composite device without a serial number shows a different
        // instance per interface: group those by port name (one per port).
        let key = (vid, pid, port_key_serial(&p));
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, list)) => list.push(p),
            None => groups.push((key, vec![p])),
        }
    }
    groups
}

/// What a serial session did, for its report.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct SerialStats {
    /// The port of the last successful `connect`.
    pub port: Option<String>,
    /// Its baud rate.
    pub baud_rate: Option<u32>,
    /// Successful `connect` calls.
    pub connects: u64,
    /// Failed `connect` calls (the OS refused, the port is busy, bad settings).
    pub connect_errors: u64,
    /// `connect({portName})` calls naming a port of another device (refused).
    pub refused: u64,
    /// Successful writes (dry-run ones included).
    pub writes: u64,
    /// Bytes written (dry-run ones included).
    pub bytes_written: u64,
    /// Writes held back by [`WriteMode::DryRun`].
    pub dry_run: u64,
    /// Failed writes (none connected, OS error).
    pub write_errors: u64,
    /// Reads that returned data.
    pub reads: u64,
    /// Bytes read.
    pub bytes_read: u64,
    /// Reads that timed out without data.
    pub read_timeouts: u64,
    /// Failed reads.
    pub read_errors: u64,
    /// I/O failures because the port went away (the port is then closed).
    pub disconnects: u64,
    /// Text of the most recent failure.
    pub last_error: Option<String>,
}

impl SerialStats {
    /// Failed serial operations of any kind.
    pub fn errors(&self) -> u64 {
        self.connect_errors + self.refused + self.write_errors + self.read_errors
    }
}

/// One recorded serial operation.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct SerialRecord {
    /// `"write"` or `"read"`.
    pub kind: &'static str,
    /// The port.
    pub port: String,
    /// Bytes written or read (the first [`RECORD_BYTES`]).
    pub bytes: Vec<u8>,
    /// Bytes in all.
    pub len: usize,
    /// The write was held back by [`WriteMode::DryRun`].
    pub dry_run: bool,
}

/// The serial part of a session report.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct SerialReport {
    /// Counters.
    pub stats: SerialStats,
    /// Dry-run writes and the first reads.
    pub io_log: Vec<SerialRecord>,
}

/// The serial side of one session.
pub struct SerialContext {
    backend: Arc<dyn SerialBackend>,
    ports: Vec<SerialPortInfo>,
    /// Index into `ports` of the port `connect()` binds without a `portName`.
    default_port: usize,
    handle: Option<Box<dyn SerialHandle>>,
    mode: WriteMode,
    stats: SerialStats,
    io_log: Vec<SerialRecord>,
    log_sink: Option<LogSink>,
}

impl std::fmt::Debug for SerialContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SerialContext")
            .field("ports", &self.ports.len())
            .field("connected", &self.port_name())
            .field("mode", &self.mode)
            .finish()
    }
}

/// A [`SerialContext`] shared between the session and the natives.
pub type SharedSerial = Arc<Mutex<SerialContext>>;

/// Lock a [`SharedSerial`], recovering from a poisoned lock.
pub fn lock_serial(s: &SharedSerial) -> MutexGuard<'_, SerialContext> {
    s.lock().unwrap_or_else(|e| e.into_inner())
}

fn same_port(a: &str, b: &str) -> bool {
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

impl SerialContext {
    /// The context of `target` (nothing opened yet).
    pub fn new(target: &SerialTarget, mode: WriteMode) -> Self {
        Self {
            backend: Arc::clone(&target.backend),
            ports: target.ports.clone(),
            default_port: 0,
            handle: None,
            mode,
            stats: SerialStats::default(),
            io_log: Vec::new(),
            log_sink: None,
        }
    }

    /// Wrap for sharing with the natives.
    pub fn shared(self) -> SharedSerial {
        Arc::new(Mutex::new(self))
    }

    /// Where plugin-visible warnings go (the device log).
    pub fn set_log_sink(&mut self, sink: Option<LogSink>) {
        self.log_sink = sink;
    }

    /// The port `connect()` binds without a `portName` (an index into the ports).
    pub fn set_default_port(&mut self, index: usize) {
        if index < self.ports.len() {
            self.default_port = index;
        }
    }

    /// The device's ports.
    pub fn ports(&self) -> &[SerialPortInfo] {
        &self.ports
    }

    /// Counters so far.
    pub fn stats(&self) -> &SerialStats {
        &self.stats
    }

    /// The report part.
    pub fn report(&self) -> SerialReport {
        SerialReport {
            stats: self.stats.clone(),
            io_log: self.io_log.clone(),
        }
    }

    fn log(&self, line: &str) {
        if let Some(sink) = &self.log_sink {
            sink(line);
        }
    }

    fn record(&mut self, kind: &'static str, bytes: &[u8], dry_run: bool) {
        if self.io_log.len() < SERIAL_LOG_LIMIT {
            let port = self
                .handle
                .as_ref()
                .map(|h| h.port_name().to_string())
                .unwrap_or_default();
            self.io_log.push(SerialRecord {
                kind,
                port,
                bytes: bytes[..bytes.len().min(RECORD_BYTES)].to_vec(),
                len: bytes.len(),
                dry_run,
            });
        }
    }

    fn fail(&mut self, what: &str, e: &HalError) {
        let text = format!("serial.{what}: {e}");
        self.stats.last_error = Some(text);
        if matches!(e, HalError::Disconnected) {
            self.stats.disconnects += 1;
            if let Some(h) = self.handle.take() {
                self.log(&format!(
                    "serial: {} went away; the port is closed",
                    h.port_name()
                ));
            }
        }
    }

    /// The port named `name` among the device's ports; `""` = the default port.
    fn find_port(&self, name: &str) -> Option<&SerialPortInfo> {
        if name.is_empty() {
            return self.ports.get(self.default_port);
        }
        self.ports.iter().find(|p| same_port(&p.port_name, name))
    }

    /// `serial.connect(...)`: open `port` (`""` = the device's own port) with
    /// `config`, closing the port open before. Ports of other devices are refused.
    pub fn connect(&mut self, port: &str, config: &SerialConfig) -> bool {
        let Some(name) = self.find_port(port).map(|p| p.port_name.clone()) else {
            self.stats.refused += 1;
            self.stats.last_error = Some(format!("serial.connect: {port} is not a port of this device"));
            self.log(&format!(
                "serial.connect: {port} does not belong to this device; Lightloom only opens the device's own ports"
            ));
            return false;
        };
        self.handle = None;
        match self.backend.open(&name, config) {
            Ok(h) => {
                self.handle = Some(h);
                self.stats.connects += 1;
                self.stats.port = Some(name);
                self.stats.baud_rate = Some(config.baud_rate);
                true
            }
            Err(e) => {
                self.stats.connect_errors += 1;
                self.log(&format!("serial.connect: cannot open {name}: {e}"));
                self.fail("connect", &e);
                false
            }
        }
    }

    /// `serial.disconnect()`.
    pub fn disconnect(&mut self) {
        self.handle = None;
    }

    /// `serial.isConnected()`.
    pub fn is_connected(&self) -> bool {
        self.handle.is_some()
    }

    /// `serial.getPortName()`: the open port, `""` when none is.
    pub fn port_name(&self) -> String {
        self.handle
            .as_ref()
            .map(|h| h.port_name().to_string())
            .unwrap_or_default()
    }

    /// `serial.getBaudRate()`: of the open port, `0` when none is.
    pub fn baud_rate(&self) -> u32 {
        self.handle.as_ref().map_or(0, |h| h.config().baud_rate)
    }

    /// `serial.write(bytes)`: bytes written, `0` on failure (plugins test for falsy).
    pub fn write(&mut self, data: &[u8]) -> usize {
        if data.is_empty() {
            return 0;
        }
        let Some(handle) = self.handle.as_mut() else {
            self.stats.write_errors += 1;
            self.stats.last_error = Some("serial.write: not connected".into());
            return 0;
        };
        if self.mode == WriteMode::DryRun {
            self.stats.writes += 1;
            self.stats.dry_run += 1;
            self.stats.bytes_written += data.len() as u64;
            self.record("write", data, true);
            return data.len();
        }
        match handle.write(data) {
            Ok(n) => {
                self.stats.writes += 1;
                self.stats.bytes_written += n as u64;
                n
            }
            Err(e) => {
                self.stats.write_errors += 1;
                self.fail("write", &e);
                0
            }
        }
    }

    /// `serial.read(maxBytes?, timeoutMs)`: up to `max` bytes (`None` = what
    /// is waiting, or what arrives first), waiting at most `timeout`; empty on
    /// timeout or failure. `cancel` ends the wait early (stop request, deadline).
    pub fn read(&mut self, max: Option<usize>, timeout: Duration, cancel: &dyn Fn() -> bool) -> Vec<u8> {
        let Some(handle) = self.handle.as_mut() else {
            self.stats.read_errors += 1;
            self.stats.last_error = Some("serial.read: not connected".into());
            return Vec::new();
        };
        let want = match max {
            Some(n) => n.min(MAX_TRANSFER),
            None => match handle.bytes_to_read() {
                Ok(0) => MAX_TRANSFER,
                Ok(n) => n.min(MAX_TRANSFER),
                Err(e) => {
                    self.stats.read_errors += 1;
                    self.fail("read", &e);
                    return Vec::new();
                }
            },
        };
        if want == 0 {
            return Vec::new();
        }
        // `None` returns what the first bytes bring; `Some(n)` keeps reading
        // until `n` bytes arrived or the time is up.
        let mut buf = vec![0u8; want.min(64 * 1024)];
        let mut out = Vec::new();
        let end = Instant::now() + timeout.min(MAX_READ_TIMEOUT);
        let result = loop {
            let left = end.saturating_duration_since(Instant::now());
            let slice = left.min(READ_SLICE).as_millis() as u32;
            let room = (want - out.len()).min(buf.len());
            match handle.read(&mut buf[..room], slice) {
                Ok(n) if n > 0 => {
                    out.extend_from_slice(&buf[..n]);
                    if max.is_none() || out.len() >= want {
                        break Ok(());
                    }
                }
                Ok(_) => {}
                Err(e) => break Err(e),
            }
            if left.is_zero() || cancel() {
                break Ok(());
            }
        };
        if let Err(e) = result {
            self.stats.read_errors += 1;
            self.fail("read", &e);
            return out;
        }
        if out.is_empty() {
            self.stats.read_timeouts += 1;
        } else {
            self.stats.reads += 1;
            self.stats.bytes_read += out.len() as u64;
            self.record("read", &out, false);
        }
        out
    }

    /// `serial.availablePorts()`: the device's own ports.
    pub fn ports_json(&self) -> serde_json::Value {
        serde_json::Value::Array(self.ports.iter().map(port_json).collect())
    }

    /// `serial.getDeviceInfo(portName?)`: the port (`""` = the open one, else
    /// the default one), `None` for a port of another device.
    pub fn info_json(&self, port: &str) -> Option<serde_json::Value> {
        let name = if port.is_empty() {
            self.port_name()
        } else {
            port.to_string()
        };
        self.find_port(&name).map(port_json)
    }
}

/// A port as `availablePorts()` / `getDeviceInfo()` show it.
fn port_json(p: &SerialPortInfo) -> serde_json::Value {
    serde_json::json!({
        "portName": p.port_name,
        "vendorId": p.vid.unwrap_or(0),
        "productId": p.pid.unwrap_or(0),
        "serialNumber": p.serial.clone().unwrap_or_default(),
        // Keyed by the USB serial number (Turzx `getDeviceProperties(deviceInfo.deviceId)`).
        "deviceId": port_key_serial(p),
        "manufacturer": p.manufacturer.clone().unwrap_or_default(),
        "product": p.product.clone().unwrap_or_default(),
        "interface": p.interface.map_or(-1, i32::from),
    })
}

fn to_bytes(values: &[i32]) -> Vec<u8> {
    values.iter().take(MAX_TRANSFER).map(|&v| v as u8).collect()
}

/// Add the `serial*` natives to `natives`. A panic marks `device` fatal.
pub(crate) fn install<'js>(
    ctx: &Ctx<'js>,
    natives: &Object<'js>,
    serial: &SharedSerial,
    deadline: &Deadline,
    device: &SharedDevice,
) -> rquickjs::Result<()> {
    // As in device.rs: blocking natives suspend the JavaScript budget, answer
    // `else` at once past the deadline, and turn a panic into a JS InternalError.
    macro_rules! native {
        ($name:literal else $fallback:expr, |$s:ident, $dl:ident $(, $arg:ident : $ty:ty)*| $body:expr) => {{
            let shared = Arc::clone(serial);
            let deadline = deadline.clone();
            let device = Arc::clone(device);
            let f = Function::new(ctx.clone(), move |cx: Ctx<'_>, $($arg: $ty),*| -> rquickjs::Result<_> {
                if deadline.expired() {
                    return Ok($fallback);
                }
                let _blocked = deadline.suspend();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut guard = lock_serial(&shared);
                    let $s: &mut SerialContext = &mut guard;
                    let $dl: &Deadline = &deadline;
                    $body
                }));
                result.map_err(|panic| {
                    let msg = format!("host panic in serial.{}: {}", $name, panic_text(panic.as_ref()));
                    lock_device(&device).set_fatal(msg.clone());
                    rquickjs::Exception::throw_internal(&cx, &msg)
                })
            })?;
            natives.set($name, f)?;
        }};
    }
    native!("serialConnect" else false, |s, _d, port: String, baud: f64, data_bits: i32, stop_bits: i32, parity: i32| {
        let parity = match parity {
            1 => Parity::Odd,
            2 => Parity::Even,
            _ => Parity::None,
        };
        let config = SerialConfig {
            baud_rate: if baud.is_finite() && baud >= 1.0 { baud.min(f64::from(u32::MAX)) as u32 } else { 0 },
            data_bits: u8::try_from(data_bits).unwrap_or(0),
            stop_bits: u8::try_from(stop_bits).unwrap_or(0),
            parity,
            ..SerialConfig::default()
        };
        let port: String = port.chars().take(256).collect();
        s.connect(&port, &config)
    });
    native!("serialDisconnect" else (), |s, _d| s.disconnect());
    native!("serialIsConnected" else false, |s, _d| s.is_connected());
    native!("serialWrite" else 0, |s, _d, data: Vec<i32>| s.write(&to_bytes(&data)) as i32);
    native!("serialRead" else Vec::new(), |s, d, max: i32, timeout: f64| {
        let max = usize::try_from(max).ok();
        let timeout = if timeout.is_finite() && timeout > 0.0 {
            Duration::from_micros((timeout.min(1e7) * 1000.0) as u64)
        } else {
            Duration::ZERO
        };
        let cancel = || d.stop_requested() || d.expired();
        s.read(max, timeout, &cancel).into_iter().map(i32::from).collect::<Vec<i32>>()
    });
    native!("serialPorts" else String::from("[]"), |s, _d| s.ports_json().to_string());
    native!("serialInfo" else String::new(), |s, _d, port: String| {
        s.info_json(&port).map(|v| v.to_string()).unwrap_or_default()
    });
    native!("serialPortName" else String::new(), |s, _d| s.port_name());
    native!("serialBaudRate" else 0.0, |s, _d| f64::from(s.baud_rate()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_hal::mock::MockSerial;

    fn target(m: MockSerial) -> (Arc<MockSerial>, SerialTarget) {
        let m = Arc::new(m);
        let ports = m.available_ports().unwrap();
        (Arc::clone(&m), SerialTarget { backend: m, ports })
    }

    #[test]
    fn ports_group_by_usb_device() {
        let ports = vec![
            MockSerial::usb_port("COM3", 0x16D5, 0x2A08, "N1", Some(0)),
            MockSerial::usb_port("COM4", 0x16D5, 0x2A08, "N1", Some(2)),
            MockSerial::usb_port("COM5", 0x16D5, 0x2A08, "N2", Some(0)),
            SerialPortInfo {
                port_name: "COM1".into(),
                ..SerialPortInfo::default()
            },
        ];
        let groups = group_ports(ports);
        assert_eq!(groups.len(), 2, "the platform UART is not a USB device");
        assert_eq!(groups[0].0, (0x16D5, 0x2A08, "N1".to_string()));
        assert_eq!(groups[0].1.len(), 2);
        let mut no_serial = MockSerial::usb_port("COM9", 0x1A86, 0x7523, "", None);
        no_serial.serial = None;
        assert_eq!(port_key_serial(&no_serial), "COM9");
    }

    #[test]
    fn only_the_devices_own_ports_open() {
        let (m, t) = target(
            MockSerial::new()
                .add_port(MockSerial::usb_port("COM3", 0x1A86, 0x7523, "A", Some(0)))
                .add_port(MockSerial::usb_port("COM8", 0x0403, 0x6001, "MODEM", Some(0))),
        );
        // The session's target holds only its own device's port.
        let own = SerialTarget {
            ports: t.ports[..1].to_vec(),
            ..t
        };
        let mut s = SerialContext::new(&own, WriteMode::Live);
        assert!(!s.connect("COM8", &SerialConfig::default()));
        assert_eq!(s.stats().refused, 1);
        assert_eq!(lock_port_open_count(&m, "COM8"), 0, "never opened");
        assert!(s.connect("", &SerialConfig::default()));
        assert_eq!(s.port_name(), "COM3");
        assert!(s.connect("com3", &SerialConfig::default()) || !cfg!(windows));
        assert_eq!(s.ports_json().as_array().unwrap().len(), 1);
        assert!(s.info_json("COM8").is_none());
        assert_eq!(s.info_json("").unwrap()["deviceId"], "A");
    }

    fn lock_port_open_count(m: &MockSerial, port: &str) -> usize {
        m.port(port).unwrap().lock().unwrap().open_count
    }

    #[test]
    fn write_read_and_timeouts() {
        let (m, t) =
            target(MockSerial::new().add_port(MockSerial::usb_port("COM3", 0x1A86, 0x7523, "A", Some(0))));
        m.reply("COM3", b"PING", b"PONG!").unwrap();
        let mut s = SerialContext::new(&t, WriteMode::Live);
        assert_eq!(s.write(b"x"), 0, "not connected");
        assert!(s.connect("", &SerialConfig::default()));
        assert_eq!(s.write(b"PING"), 4);
        let never = || false;
        assert_eq!(s.read(Some(4), Duration::from_millis(200), &never), b"PONG");
        assert_eq!(s.read(None, Duration::from_millis(200), &never), b"!");
        let t0 = Instant::now();
        assert!(s.read(Some(1), Duration::from_millis(50), &never).is_empty());
        assert!(t0.elapsed() >= Duration::from_millis(50));
        // A cancelled wait ends within one slice.
        let t0 = Instant::now();
        assert!(s.read(Some(1), Duration::from_secs(4), &|| true).is_empty());
        assert!(t0.elapsed() < Duration::from_millis(1000));
        let st = s.stats().clone();
        assert_eq!(
            (st.writes, st.reads, st.read_timeouts, st.write_errors),
            (1, 2, 2, 1)
        );
        // Unplugged: the failure closes the port, isConnected() turns false.
        m.set_unplugged("COM3", true).unwrap();
        assert_eq!(s.write(b"PING"), 0);
        assert!(!s.is_connected());
        assert_eq!(s.stats().disconnects, 1);
        m.set_unplugged("COM3", false).unwrap();
        assert!(
            s.connect("COM3", &SerialConfig::default()),
            "reconnects after a replug"
        );
    }

    #[test]
    fn dry_run_records_writes_and_sends_nothing() {
        let (m, t) =
            target(MockSerial::new().add_port(MockSerial::usb_port("COM3", 0x1A86, 0x7523, "A", Some(0))));
        let mut s = SerialContext::new(&t, WriteMode::DryRun);
        assert!(s.connect("", &SerialConfig::default()));
        assert_eq!(s.write(&[1, 2, 3]), 3);
        let writes = m.recorder().payloads(lightloom_hal::mock::TransactionKind::Write);
        assert!(writes.is_empty(), "held back");
        let r = s.report();
        assert_eq!(r.stats.dry_run, 1);
        assert_eq!(r.io_log[0].bytes, [1, 2, 3]);
        assert!(r.io_log[0].dry_run);
    }
}
