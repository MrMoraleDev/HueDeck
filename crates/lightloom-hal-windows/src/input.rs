// SPDX-License-Identifier: GPL-3.0-or-later
//! Keyboard, mouse and desktop helpers of the macro runtime
//! (`lightloom_hal::input`) on Windows.
//!
//! * [`WindowsDesktop`]: `SendInput` (keys by virtual-key code or as UTF-16
//!   characters, mouse buttons, the wheel), the clipboard (`CF_UNICODETEXT`),
//!   `ShellExecuteW("open")` for URLs, programs started directly (no shell),
//!   `GetLastInputInfo` and the running process names.
//! * [`LowLevelHooks`]: `WH_KEYBOARD_LL` / `WH_MOUSE_LL` on a thread of their
//!   own that pumps messages. Every event goes to the sink, which answers at
//!   once whether to swallow it; Windows drops a hook whose procedure is slow
//!   (`LowLevelHooksTimeout`), so the sink bounds its wait.
//!
//! Input Lightloom injects itself carries [`INJECTED_MARK`] in `dwExtraInfo`;
//! the hooks let it through without showing it to the sink, so a macro that
//! presses the key it listens for cannot loop. Hooks never log keys.

use lightloom_hal::input::{Desktop, HookEvent, HookSink, InputHook, MouseButton, SynthInput};
use lightloom_hal::{HalError, HalResult};
use std::cell::RefCell;
use std::sync::mpsc::channel;
use std::thread::JoinHandle;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetLastInputInfo, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, LASTINPUTINFO, MOUSEEVENTF_HWHEEL,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP,
    MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED, LLKHF_UP,
    LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, SW_SHOWNORMAL, WHEEL_DELTA, WH_KEYBOARD_LL, WH_MOUSE_LL,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEWHEEL, WM_QUIT,
    WM_RBUTTONDOWN, WM_RBUTTONUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

/// `dwExtraInfo` of the input Lightloom injects ("LLMC").
pub const INJECTED_MARK: usize = 0x4C4C_4D43;
/// `CF_UNICODETEXT`.
const CF_UNICODETEXT: u32 = 13;
/// `XBUTTON1` / `XBUTTON2` in `mouseData`.
const XBUTTON1: u32 = 1;
const XBUTTON2: u32 = 2;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn os_err(what: &str, e: windows::core::Error) -> HalError {
    HalError::Backend(format!("{what}: {}", e.message()))
}

fn keyboard(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: INJECTED_MARK,
            },
        },
    }
}

fn mouse(flags: MOUSE_EVENT_FLAGS, data: u32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: INJECTED_MARK,
            },
        },
    }
}

/// The `INPUT` records of `inputs` (pure; the pointer never moves).
fn records(inputs: &[SynthInput]) -> Vec<INPUT> {
    inputs
        .iter()
        .map(|i| match *i {
            SynthInput::Key { vk, down, extended } => {
                let mut flags = KEYBD_EVENT_FLAGS(0);
                if extended {
                    flags |= KEYEVENTF_EXTENDEDKEY;
                }
                if !down {
                    flags |= KEYEVENTF_KEYUP;
                }
                keyboard(vk, 0, flags)
            }
            SynthInput::Unicode { unit, down } => {
                let mut flags = KEYEVENTF_UNICODE;
                if !down {
                    flags |= KEYEVENTF_KEYUP;
                }
                keyboard(0, unit, flags)
            }
            SynthInput::Button { button, down } => {
                let (flags, data) = match (button, down) {
                    (MouseButton::Left, true) => (MOUSEEVENTF_LEFTDOWN, 0),
                    (MouseButton::Left, false) => (MOUSEEVENTF_LEFTUP, 0),
                    (MouseButton::Right, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
                    (MouseButton::Right, false) => (MOUSEEVENTF_RIGHTUP, 0),
                    (MouseButton::Middle, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
                    (MouseButton::Middle, false) => (MOUSEEVENTF_MIDDLEUP, 0),
                    (MouseButton::X1, true) => (MOUSEEVENTF_XDOWN, XBUTTON1),
                    (MouseButton::X1, false) => (MOUSEEVENTF_XUP, XBUTTON1),
                    (MouseButton::X2, true) => (MOUSEEVENTF_XDOWN, XBUTTON2),
                    (MouseButton::X2, false) => (MOUSEEVENTF_XUP, XBUTTON2),
                };
                mouse(flags, data)
            }
            SynthInput::Wheel { horizontal, notches } => {
                let delta = notches.saturating_mul(WHEEL_DELTA as i32);
                mouse(
                    if horizontal {
                        MOUSEEVENTF_HWHEEL
                    } else {
                        MOUSEEVENTF_WHEEL
                    },
                    delta as u32,
                )
            }
        })
        .collect()
}

/// The Windows [`Desktop`].
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowsDesktop;

impl Desktop for WindowsDesktop {
    fn send(&self, inputs: &[SynthInput]) -> HalResult<()> {
        if inputs.is_empty() {
            return Ok(());
        }
        let records = records(inputs);
        // SAFETY: a slice of initialised INPUT records and their size.
        let sent = unsafe { SendInput(&records, std::mem::size_of::<INPUT>() as i32) };
        if sent as usize == records.len() {
            Ok(())
        } else {
            // UIPI: input to an elevated foreground window is blocked.
            Err(HalError::PermissionDenied(format!(
                "SendInput injected {sent} of {} events (a window of an elevated program has the focus?)",
                records.len()
            )))
        }
    }

    fn set_clipboard(&self, text: &str) -> HalResult<()> {
        let data = wide(text);
        let bytes = data.len() * 2;
        // SAFETY: the clipboard is opened, emptied, given one global block it
        // then owns, and closed; the block is freed only when handing it over fails.
        unsafe {
            OpenClipboard(None).map_err(|e| os_err("OpenClipboard", e))?;
            let result = (|| -> HalResult<()> {
                EmptyClipboard().map_err(|e| os_err("EmptyClipboard", e))?;
                let block: HGLOBAL =
                    GlobalAlloc(GMEM_MOVEABLE, bytes).map_err(|e| os_err("GlobalAlloc", e))?;
                let ptr = GlobalLock(block) as *mut u16;
                if ptr.is_null() {
                    let _ = GlobalFree(Some(block));
                    return Err(HalError::Backend("GlobalLock failed".into()));
                }
                std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
                let _ = GlobalUnlock(block);
                if let Err(e) = SetClipboardData(CF_UNICODETEXT, Some(HANDLE(block.0))) {
                    let _ = GlobalFree(Some(block));
                    return Err(os_err("SetClipboardData", e));
                }
                Ok(())
            })();
            let _ = CloseClipboard();
            result
        }
    }

    fn open(&self, target: &str) -> HalResult<()> {
        let file = wide(target);
        // SAFETY: NUL-terminated strings that outlive the call.
        let r = unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                PCWSTR(file.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        // Values above 32 mean success.
        if r.0 as isize > 32 {
            Ok(())
        } else {
            Err(HalError::Backend(format!(
                "ShellExecuteW failed ({})",
                r.0 as isize
            )))
        }
    }

    fn launch(&self, path: &str, args: &[String], cwd: Option<&str>) -> HalResult<()> {
        let mut cmd = std::process::Command::new(path);
        cmd.args(args);
        if let Some(dir) = cwd.filter(|d| !d.is_empty()) {
            cmd.current_dir(dir);
        }
        cmd.spawn().map(drop).map_err(HalError::Io)
    }

    fn idle_seconds(&self) -> Option<u64> {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        // SAFETY: a LASTINPUTINFO with its size set.
        if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
            return None;
        }
        // SAFETY: no arguments.
        let now = unsafe { GetTickCount() };
        Some(u64::from(now.wrapping_sub(info.dwTime)) / 1000)
    }

    fn processes(&self) -> Option<Vec<String>> {
        crate::system::running_process_names().ok()
    }
}

// ------------------------------------------------------------------ hooks

thread_local! {
    /// The sink of the hooks on this thread.
    static SINK: RefCell<Option<HookSink>> = const { RefCell::new(None) };
}

fn deliver(event: HookEvent) -> bool {
    // A panic must not unwind into user32.
    std::panic::catch_unwind(|| {
        let sink = SINK.with(|s| s.borrow().clone());
        sink.is_some_and(|sink| sink(&event))
    })
    .unwrap_or(false)
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && lparam.0 != 0 {
        // SAFETY: for HC_ACTION, lparam points to the event's KBDLLHOOKSTRUCT.
        let k = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if k.dwExtraInfo != INJECTED_MARK {
            let event = HookEvent::Key {
                vk: k.vkCode as u16,
                released: k.flags.0 & LLKHF_UP.0 != 0,
                extended: k.flags.0 & LLKHF_EXTENDED.0 != 0,
                injected: k.flags.0 & LLKHF_INJECTED.0 != 0,
            };
            if deliver(event) {
                return LRESULT(1);
            }
        }
    }
    // SAFETY: the hook chain's own parameters.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// The hook event of a mouse message (`None`: movement and others).
fn mouse_event(msg: u32, data: u32, injected: bool) -> Option<HookEvent> {
    let button = |button, released| {
        Some(HookEvent::Button {
            button,
            released,
            injected,
        })
    };
    let x = || {
        if (data >> 16) == XBUTTON2 {
            MouseButton::X2
        } else {
            MouseButton::X1
        }
    };
    match msg {
        WM_LBUTTONDOWN => button(MouseButton::Left, false),
        WM_LBUTTONUP => button(MouseButton::Left, true),
        WM_RBUTTONDOWN => button(MouseButton::Right, false),
        WM_RBUTTONUP => button(MouseButton::Right, true),
        WM_MBUTTONDOWN => button(MouseButton::Middle, false),
        WM_MBUTTONUP => button(MouseButton::Middle, true),
        WM_XBUTTONDOWN => button(x(), false),
        WM_XBUTTONUP => button(x(), true),
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let delta = i32::from((data >> 16) as u16 as i16);
            Some(HookEvent::Wheel {
                horizontal: msg == WM_MOUSEHWHEEL,
                notches: delta / WHEEL_DELTA as i32,
                injected,
            })
        }
        _ => None,
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && lparam.0 != 0 {
        // SAFETY: for HC_ACTION, lparam points to the event's MSLLHOOKSTRUCT.
        let m = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        if m.dwExtraInfo != INJECTED_MARK {
            if let Some(event) = mouse_event(wparam.0 as u32, m.mouseData, m.flags & LLMHF_INJECTED != 0) {
                if deliver(event) {
                    return LRESULT(1);
                }
            }
        }
    }
    // SAFETY: the hook chain's own parameters.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

struct HookThread {
    thread_id: u32,
    thread: JoinHandle<()>,
    keyboard: bool,
    mouse: bool,
}

/// `WH_KEYBOARD_LL` / `WH_MOUSE_LL` (see the module docs).
#[derive(Default)]
pub struct LowLevelHooks {
    thread: Option<HookThread>,
}

impl std::fmt::Debug for LowLevelHooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LowLevelHooks")
            .field("active", &self.active())
            .finish()
    }
}

impl LowLevelHooks {
    /// No hook yet.
    pub fn new() -> Self {
        Self::default()
    }
}

fn hook_thread(
    keyboard: bool,
    mouse: bool,
    sink: HookSink,
    ready: std::sync::mpsc::Sender<Result<u32, String>>,
) {
    SINK.with(|s| *s.borrow_mut() = Some(sink));
    // SAFETY: hooks installed and removed on this thread, which pumps
    // messages until WM_QUIT as low-level hooks require.
    unsafe {
        let kb = if keyboard {
            match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0) {
                Ok(h) => Some(h),
                Err(e) => {
                    let _ = ready.send(Err(format!("SetWindowsHookExW(WH_KEYBOARD_LL): {}", e.message())));
                    return;
                }
            }
        } else {
            None
        };
        let ms = if mouse {
            match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0) {
                Ok(h) => Some(h),
                Err(e) => {
                    if let Some(h) = kb {
                        let _ = UnhookWindowsHookEx(h);
                    }
                    let _ = ready.send(Err(format!("SetWindowsHookExW(WH_MOUSE_LL): {}", e.message())));
                    return;
                }
            }
        } else {
            None
        };
        let _ = ready.send(Ok(GetCurrentThreadId()));
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        for h in [kb, ms].into_iter().flatten() {
            let _ = UnhookWindowsHookEx(h);
        }
    }
    SINK.with(|s| *s.borrow_mut() = None);
}

impl InputHook for LowLevelHooks {
    fn start(&mut self, keyboard: bool, mouse: bool, sink: HookSink) -> HalResult<()> {
        self.stop();
        if !keyboard && !mouse {
            return Ok(());
        }
        let (tx, rx) = channel();
        let thread = std::thread::Builder::new()
            .name("lightloom-input-hooks".into())
            .spawn(move || hook_thread(keyboard, mouse, sink, tx))
            .map_err(HalError::Io)?;
        match rx.recv() {
            Ok(Ok(thread_id)) => {
                self.thread = Some(HookThread {
                    thread_id,
                    thread,
                    keyboard,
                    mouse,
                });
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(HalError::Backend(e))
            }
            Err(_) => {
                let _ = thread.join();
                Err(HalError::Backend("the hook thread ended".into()))
            }
        }
    }

    fn stop(&mut self) {
        if let Some(t) = self.thread.take() {
            // SAFETY: posting WM_QUIT to the hook thread's queue.
            let _ = unsafe { PostThreadMessageW(t.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
            let _ = t.thread.join();
        }
    }

    fn active(&self) -> (bool, bool) {
        self.thread
            .as_ref()
            .map_or((false, false), |t| (t.keyboard, t.mouse))
    }
}

impl Drop for LowLevelHooks {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pure conversions only: nothing here calls SendInput or installs a hook.

    #[test]
    fn records_carry_the_mark_and_the_flags() {
        let r = records(&[
            SynthInput::Key {
                vk: 0x27,
                down: true,
                extended: true,
            },
            SynthInput::Key {
                vk: 0x27,
                down: false,
                extended: true,
            },
            SynthInput::Unicode {
                unit: 0x20AC,
                down: true,
            },
            SynthInput::Button {
                button: MouseButton::X2,
                down: false,
            },
            SynthInput::Wheel {
                horizontal: false,
                notches: -2,
            },
        ]);
        // SAFETY (test): reading the union member each record was built with.
        unsafe {
            assert_eq!(r[0].Anonymous.ki.dwFlags, KEYEVENTF_EXTENDEDKEY);
            assert_eq!(r[1].Anonymous.ki.dwFlags, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP);
            assert_eq!(r[0].Anonymous.ki.dwExtraInfo, INJECTED_MARK);
            assert_eq!(r[2].Anonymous.ki.wScan, 0x20AC);
            assert_eq!(r[2].Anonymous.ki.dwFlags, KEYEVENTF_UNICODE);
            assert_eq!(r[3].Anonymous.mi.dwFlags, MOUSEEVENTF_XUP);
            assert_eq!(r[3].Anonymous.mi.mouseData, XBUTTON2);
            assert_eq!(r[4].Anonymous.mi.mouseData as i32, -240);
        }
    }

    #[test]
    fn mouse_messages_map_to_events() {
        assert_eq!(
            mouse_event(WM_XBUTTONDOWN, XBUTTON2 << 16, false),
            Some(HookEvent::Button {
                button: MouseButton::X2,
                released: false,
                injected: false
            })
        );
        assert_eq!(
            mouse_event(WM_MOUSEWHEEL, (120u32) << 16, true),
            Some(HookEvent::Wheel {
                horizontal: false,
                notches: 1,
                injected: true
            })
        );
        assert_eq!(
            mouse_event(WM_MOUSEWHEEL, ((-120i16) as u16 as u32) << 16, false),
            Some(HookEvent::Wheel {
                horizontal: false,
                notches: -1,
                injected: false
            })
        );
        assert_eq!(mouse_event(0x0200, 0, false), None);
    }
}
