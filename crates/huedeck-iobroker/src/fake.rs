// SPDX-License-Identifier: GPL-3.0-or-later
//! Test doubles: a scripted `SmbusPIIX4` module and a global mutex.

use crate::mutex::GlobalMutex;
use crate::pawnio::{NtStatus, PawnExec};
use huedeck_protocol::broker::BrokerError;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
struct State {
    calls: Vec<(String, Vec<u64>)>,
    devices: BTreeMap<u8, BTreeMap<u8, u8>>,
    port: u8,
    fail: Option<NtStatus>,
}

/// A `SmbusPIIX4` module over a register map; clones share state.
#[derive(Clone, Debug, Default)]
pub struct FakePiix4(Arc<Mutex<State>>);

impl FakePiix4 {
    pub fn new() -> Self {
        Self::default()
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

    /// The handle (mux) starts on `port`.
    pub fn on_port(self, port: u8) -> Self {
        self.state().port = port;
        self
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
                output[0] = u64::from(s.port);
                s.port = input[0] as u8;
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
