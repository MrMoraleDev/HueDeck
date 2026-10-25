// SPDX-License-Identifier: GPL-3.0-or-later
//! Keyboard and mouse for the macro runtime (docs/host-api/macros-system.md):
//! synthetic input and the desktop helpers macros reach ([`Desktop`]), and the
//! opt-in low-level hooks that see the user's keys and clicks ([`InputHook`]).
//!
//! Privacy: a low-level hook sees every keystroke of the session. Backends
//! and consumers process [`HookEvent`]s in memory only and never log which
//! key was pressed (counts at most). Hooks exist only while the user turned
//! them on and an enabled macro input listens.
//!
//! Backends: `lightloom_hal_windows::input` (`SendInput`, the clipboard,
//! `ShellExecuteW`, `GetLastInputInfo`, `WH_KEYBOARD_LL` / `WH_MOUSE_LL`).
//! [`RecordingDesktop`] records what would have happened and does nothing:
//! tests and runs without hardware use it, so they never inject input.

use crate::error::{HalError, HalResult};
use std::sync::{Arc, Mutex};

/// A mouse button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    /// Left.
    Left,
    /// Right.
    Right,
    /// Middle (wheel click).
    Middle,
    /// Back (`XBUTTON1`, "Mouse Button 4").
    X1,
    /// Forward (`XBUTTON2`, "Mouse Button 5").
    X2,
}

/// One synthetic input event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SynthInput {
    /// A key by virtual-key code.
    Key {
        /// Windows virtual-key code.
        vk: u16,
        /// Pressed (`false`: released).
        down: bool,
        /// An extended key (arrows, navigation block, right Ctrl/Alt, ...).
        extended: bool,
    },
    /// One UTF-16 code unit typed as a character (`KEYEVENTF_UNICODE`).
    Unicode {
        /// The code unit.
        unit: u16,
        /// Pressed (`false`: released).
        down: bool,
    },
    /// A mouse button.
    Button {
        /// Which.
        button: MouseButton,
        /// Pressed (`false`: released).
        down: bool,
    },
    /// The wheel, in notches (positive: up / right).
    Wheel {
        /// The horizontal wheel.
        horizontal: bool,
        /// Notches.
        notches: i32,
    },
}

/// What macros may do on the desktop. Every method acts at once and returns.
pub trait Desktop: Send + Sync {
    /// Inject `inputs` in order, as if typed.
    fn send(&self, inputs: &[SynthInput]) -> HalResult<()>;
    /// Put `text` on the clipboard.
    fn set_clipboard(&self, text: &str) -> HalResult<()>;
    /// Open `target` (a URL) with its default handler.
    fn open(&self, target: &str) -> HalResult<()>;
    /// Start the program `path` with `args` (no shell), in `cwd` when given.
    fn launch(&self, path: &str, args: &[String], cwd: Option<&str>) -> HalResult<()>;
    /// Seconds since the last user input; `None` where unknown.
    fn idle_seconds(&self) -> Option<u64>;
    /// Image names of the running processes (`notepad.exe`); `None` where unknown.
    fn processes(&self) -> Option<Vec<String>>;
}

/// A call a [`RecordingDesktop`] received.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopCall {
    /// [`Desktop::send`].
    Send(Vec<SynthInput>),
    /// [`Desktop::set_clipboard`].
    Clipboard(String),
    /// [`Desktop::open`].
    Open(String),
    /// [`Desktop::launch`].
    Launch {
        /// Program.
        path: String,
        /// Arguments.
        args: Vec<String>,
        /// Working directory.
        cwd: Option<String>,
    },
}

/// A [`Desktop`] that only records (tests, runs without hardware): no input
/// reaches the OS, nothing opens or starts. Idle time and processes are what
/// the test sets.
#[derive(Debug, Default)]
pub struct RecordingDesktop {
    calls: Mutex<Vec<DesktopCall>>,
    idle: Mutex<Option<u64>>,
    processes: Mutex<Option<Vec<String>>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl RecordingDesktop {
    /// Nothing recorded yet.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Every call so far.
    pub fn calls(&self) -> Vec<DesktopCall> {
        lock(&self.calls).clone()
    }

    /// Every synthetic input so far, flattened.
    pub fn inputs(&self) -> Vec<SynthInput> {
        lock(&self.calls)
            .iter()
            .filter_map(|c| match c {
                DesktopCall::Send(i) => Some(i.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// Forget the calls.
    pub fn clear(&self) {
        lock(&self.calls).clear();
    }

    /// What [`Desktop::idle_seconds`] answers.
    pub fn set_idle(&self, seconds: Option<u64>) {
        *lock(&self.idle) = seconds;
    }

    /// What [`Desktop::processes`] answers.
    pub fn set_processes(&self, names: Option<Vec<String>>) {
        *lock(&self.processes) = names;
    }
}

impl Desktop for RecordingDesktop {
    fn send(&self, inputs: &[SynthInput]) -> HalResult<()> {
        lock(&self.calls).push(DesktopCall::Send(inputs.to_vec()));
        Ok(())
    }

    fn set_clipboard(&self, text: &str) -> HalResult<()> {
        lock(&self.calls).push(DesktopCall::Clipboard(text.to_string()));
        Ok(())
    }

    fn open(&self, target: &str) -> HalResult<()> {
        lock(&self.calls).push(DesktopCall::Open(target.to_string()));
        Ok(())
    }

    fn launch(&self, path: &str, args: &[String], cwd: Option<&str>) -> HalResult<()> {
        lock(&self.calls).push(DesktopCall::Launch {
            path: path.to_string(),
            args: args.to_vec(),
            cwd: cwd.map(str::to_string),
        });
        Ok(())
    }

    fn idle_seconds(&self) -> Option<u64> {
        *lock(&self.idle)
    }

    fn processes(&self) -> Option<Vec<String>> {
        lock(&self.processes).clone()
    }
}

/// A platform without any of it.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoDesktop;

impl Desktop for NoDesktop {
    fn send(&self, _inputs: &[SynthInput]) -> HalResult<()> {
        Err(HalError::Unsupported("synthetic input"))
    }

    fn set_clipboard(&self, _text: &str) -> HalResult<()> {
        Err(HalError::Unsupported("the clipboard"))
    }

    fn open(&self, _target: &str) -> HalResult<()> {
        Err(HalError::Unsupported("opening URLs"))
    }

    fn launch(&self, _path: &str, _args: &[String], _cwd: Option<&str>) -> HalResult<()> {
        Err(HalError::Unsupported("starting programs"))
    }

    fn idle_seconds(&self) -> Option<u64> {
        None
    }

    fn processes(&self) -> Option<Vec<String>> {
        None
    }
}

/// What a low-level hook saw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookEvent {
    /// A key went down or up.
    Key {
        /// Virtual-key code (side-specific for modifiers: `VK_LSHIFT`, ...).
        vk: u16,
        /// Released.
        released: bool,
        /// An extended key (`Num Enter` vs `Enter`).
        extended: bool,
        /// Synthesised by a program (`LLKHF_INJECTED`).
        injected: bool,
    },
    /// A mouse button went down or up.
    Button {
        /// Which.
        button: MouseButton,
        /// Released.
        released: bool,
        /// Synthesised by a program (`LLMHF_INJECTED`).
        injected: bool,
    },
    /// The wheel turned.
    Wheel {
        /// The horizontal wheel.
        horizontal: bool,
        /// Positive: up / right.
        notches: i32,
        /// Synthesised by a program.
        injected: bool,
    },
}

/// Receives hook events on the hook thread and answers at once whether to
/// swallow the event (`true`: other programs never see it). The OS removes a
/// hook that answers too slowly, so the sink bounds its own wait.
pub type HookSink = Arc<dyn Fn(&HookEvent) -> bool + Send + Sync>;

/// Low-level keyboard and mouse hooks.
pub trait InputHook: Send {
    /// Install the keyboard and/or mouse hook (replacing the current ones).
    fn start(&mut self, keyboard: bool, mouse: bool, sink: HookSink) -> HalResult<()>;
    /// Remove every hook.
    fn stop(&mut self);
    /// Which hooks are installed: `(keyboard, mouse)`.
    fn active(&self) -> (bool, bool);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recorder_only_records() {
        let d = RecordingDesktop::new();
        d.send(&[SynthInput::Key {
            vk: 0x41,
            down: true,
            extended: false,
        }])
        .unwrap();
        d.set_clipboard("x").unwrap();
        d.open("https://example.com").unwrap();
        d.launch("notepad.exe", &["a".into()], None).unwrap();
        assert_eq!(d.calls().len(), 4);
        assert_eq!(d.inputs().len(), 1);
        assert_eq!(d.idle_seconds(), None);
        d.set_idle(Some(5));
        assert_eq!(d.idle_seconds(), Some(5));
        d.clear();
        assert!(d.calls().is_empty());
        assert!(NoDesktop.send(&[]).is_err());
    }
}
