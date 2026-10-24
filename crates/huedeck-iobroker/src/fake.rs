// SPDX-License-Identifier: GPL-3.0-or-later
//! Test doubles: a scripted `SmbusPIIX4` or `SmbusI801` module and a global mutex.

use crate::mutex::GlobalMutex;
use crate::pawnio::{NtStatus, PawnExec};
use huedeck_protocol::broker::BrokerError;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
struct State {
    /// `SmbusI801` instead of `SmbusPIIX4`.
    i801: bool,
    /// I801 SMBHSTCFG.SPD_WD.
    spd_wd: bool,
    calls: Vec<(String, Vec<u64>)>,
    devices: BTreeMap<u8, BTreeMap<u8, u8>>,
    port: u8,
    fail: Option<NtStatus>,
    /// DDR4 SPDs (EE1004, 512 bytes) and the page selected on the bus.
    spd: BTreeMap<u8, Vec<u8>>,
    spd_page: u8,
    /// Modules switch the page but do not acknowledge the select.
    nack_page_select: bool,
}

/// A `SmbusPIIX4` module (or, from [`FakePiix4::new_i801`], a `SmbusI801`
/// one: no port select, its own identity, `ioctl_write_protection`) over a
/// register map; clones share state.
#[derive(Clone, Debug, Default)]
pub struct FakePiix4(Arc<Mutex<State>>);

impl FakePiix4 {
    pub fn new() -> Self {
        Self::default()
    }

    /// A `SmbusI801` module (Raptor Lake PCH, I/O base 0xEFA0, SPD_WD set).
    pub fn new_i801() -> Self {
        let fake = Self::default();
        {
            let mut s = fake.state();
            s.i801 = true;
            s.spd_wd = true;
        }
        fake
    }

    /// The I801's SMBHSTCFG.SPD_WD.
    pub fn spd_write_protection(self, on: bool) -> Self {
        self.state().spd_wd = on;
        self
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.lock().unwrap()
    }

    /// A device answering at `addr`.
    pub fn with_device(self, addr: u8, regs: impl IntoIterator<Item = (u8, u8)>) -> Self {
        self.state().devices.insert(addr, regs.into_iter().collect());
        self
    }

    /// Every transfer fails with `status`.
    pub fn failing(self, status: NtStatus) -> Self {
        self.state().fail = Some(status);
        self
    }

    /// From now on every transfer fails with `status` (`None`: none fails).
    pub fn set_failing(&self, status: Option<NtStatus>) {
        self.state().fail = status;
    }

    /// The port the handle (mux) is on.
    pub fn port(&self) -> u8 {
        self.state().port
    }

    /// Every device's registers.
    pub fn devices(&self) -> BTreeMap<u8, BTreeMap<u8, u8>> {
        self.state().devices.clone()
    }

    /// How many calls so far.
    pub fn call_count(&self) -> usize {
        self.state().calls.len()
    }

    /// The calls from the `from`-th on.
    pub fn calls_from(&self, from: usize) -> Vec<(String, Vec<u64>)> {
        self.state().calls.get(from..).unwrap_or_default().to_vec()
    }

    /// The handle (mux) starts on `port`.
    pub fn on_port(self, port: u8) -> Self {
        self.state().port = port;
        self
    }

    /// A DDR4 SPD (EE1004) at `addr` holding `contents` (0xFF-padded to 512).
    pub fn with_spd(self, addr: u8, contents: &[u8]) -> Self {
        let mut bytes = vec![0xFF; 512];
        bytes[..contents.len().min(512)].copy_from_slice(&contents[..contents.len().min(512)]);
        self.state().spd.insert(addr, bytes);
        self
    }

    /// The SPDs switch page on a Set Page Address but answer it with a NACK.
    pub fn nack_page_select(self) -> Self {
        self.state().nack_page_select = true;
        self
    }

    /// The EE1004 page selected on the bus.
    pub fn spd_page(&self) -> u8 {
        self.state().spd_page
    }

    /// Every `(function, input cells)` so far.
    pub fn calls(&self) -> Vec<(String, Vec<u64>)> {
        self.state().calls.clone()
    }

    /// One register of a device.
    pub fn register(&self, addr: u8, reg: u8) -> Option<u8> {
        self.state().devices.get(&addr).and_then(|d| d.get(&reg).copied())
    }
}

impl PawnExec for FakePiix4 {
    fn execute(&mut self, function: &str, input: &[u64], output: &mut [u64]) -> Result<(), NtStatus> {
        let mut s = self.state();
        s.calls.push((function.to_string(), input.to_vec()));
        match function {
            "ioctl_piix4_port_sel" => {
                assert!(!s.i801, "SmbusI801 has no ioctl_piix4_port_sel");
                output[0] = u64::from(s.port);
                s.port = input[0] as u8;
                Ok(())
            }
            "ioctl_identity" if s.i801 => {
                output[0] = u64::from_le_bytes(*b"i801\0\0\0\0");
                output[1] = 0xEFA0;
                output[2] = 0x7D25_1462_7A23_8086;
                Ok(())
            }
            "ioctl_write_protection" if s.i801 => {
                output[0] = u64::from(s.spd_wd);
                Ok(())
            }
            "ioctl_identity" => {
                output[0] = u64::from_le_bytes(*b"PIIX4\0\0\0");
                output[1] = if s.port == 1 { 0x0B20 } else { 0x0B00 };
                output[2] = 0x790B_1022_790B_1022;
                Ok(())
            }
            "ioctl_set_sleep_mode" => Ok(()),
            "ioctl_smbus_xfer" => {
                assert_eq!((input.len(), output.len()), (9, 5), "always 9 in / 5 out");
                if let Some(status) = s.fail {
                    return Err(status);
                }
                let (addr, read, cmd, proto) = (input[0] as u8, input[1] == 1, input[2] as u8, input[3]);
                // EE1004: Set Page Address (Send Byte to 0x36 / 0x37) and
                // Read Page Address (Receive Byte at 0x36: ACK on page 0).
                if (addr == 0x36 || addr == 0x37) && proto == 1 && !s.spd.is_empty() {
                    if read {
                        return if addr == 0x36 && s.spd_page == 0 {
                            Ok(())
                        } else {
                            Err(NtStatus::NO_SUCH_DEVICE)
                        };
                    }
                    s.spd_page = addr - 0x36;
                    return if s.nack_page_select {
                        Err(NtStatus::NO_SUCH_DEVICE)
                    } else {
                        Ok(())
                    };
                }
                if let Some(spd) = s.spd.get(&addr) {
                    assert!(read, "the broker never writes an SPD: {input:?}");
                    if s.i801 && s.spd_wd {
                        assert!(read, "SPD_WD: the controller refuses SPD writes");
                    }
                    if proto != 2 {
                        return Err(NtStatus::NOT_SUPPORTED);
                    }
                    output[0] = u64::from(spd[usize::from(s.spd_page) * 256 + usize::from(cmd)]);
                    return Ok(());
                }
                let dev = s.devices.get_mut(&addr).ok_or(NtStatus::NO_SUCH_DEVICE)?;
                match (proto, read) {
                    (0, _) => {}
                    (1, true) => output[0] = u64::from(*dev.get(&0).unwrap_or(&0xFF)),
                    (2, true) => output[0] = u64::from(*dev.get(&cmd).unwrap_or(&0xFF)),
                    (2, false) => {
                        dev.insert(cmd, input[4] as u8);
                    }
                    (3, true) => {
                        let lo = *dev.get(&cmd).unwrap_or(&0xFF);
                        let hi = *dev.get(&cmd.wrapping_add(1)).unwrap_or(&0xFF);
                        output[0] = u64::from(u16::from_le_bytes([lo, hi]));
                    }
                    (3, false) => {
                        let [lo, hi] = (input[4] as u16).to_le_bytes();
                        dev.insert(cmd, lo);
                        dev.insert(cmd.wrapping_add(1), hi);
                    }
                    (5, false) => {
                        let bytes: Vec<u8> = input[4..9].iter().flat_map(|c| c.to_le_bytes()).collect();
                        let len = usize::from(bytes[0]);
                        for (i, b) in bytes[1..=len].iter().enumerate() {
                            dev.insert(cmd.wrapping_add(i as u8), *b);
                        }
                    }
                    _ => return Err(NtStatus::NOT_SUPPORTED),
                }
                Ok(())
            }
            _ => Err(NtStatus::OBJECT_NAME_NOT_FOUND),
        }
    }
}

#[derive(Debug, Default)]
struct MutexState {
    held: bool,
    acquired: u32,
    released: u32,
    contended: bool,
}

/// A global mutex that counts; clones share state.
#[derive(Clone, Debug, Default)]
pub struct FakeMutex(Arc<Mutex<MutexState>>);

impl FakeMutex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Another program holds it from now on.
    pub fn contend(&self, on: bool) {
        self.0.lock().unwrap().contended = on;
    }

    pub fn held(&self) -> bool {
        self.0.lock().unwrap().held
    }

    /// `(acquired, released)` so far.
    pub fn counts(&self) -> (u32, u32) {
        let s = self.0.lock().unwrap();
        (s.acquired, s.released)
    }
}

impl GlobalMutex for FakeMutex {
    fn acquire(&mut self, _timeout_ms: u32) -> Result<(), BrokerError> {
        let mut s = self.0.lock().unwrap();
        assert!(!s.held, "acquired twice without a release");
        if s.contended {
            return Err(BrokerError::BusContended);
        }
        s.held = true;
        s.acquired += 1;
        Ok(())
    }

    fn release(&mut self) {
        let mut s = self.0.lock().unwrap();
        if s.held {
            s.held = false;
            s.released += 1;
        }
    }
}
