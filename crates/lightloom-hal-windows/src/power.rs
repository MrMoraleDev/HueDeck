// SPDX-License-Identifier: GPL-3.0-or-later
//! System sleep and resume, lock and unlock, display off and on: the Windows
//! [`PowerSource`].
//!
//! A hidden top-level window (never shown) on its own thread receives:
//! * `WM_POWERBROADCAST`: `PBT_APMSUSPEND` (sleep, hibernate; also Modern
//!   Standby once the screen-off delay passes), `PBT_APMRESUMEAUTOMATIC` and
//!   `PBT_APMRESUMESUSPEND` (both arrive on a user wake: the consumer treats a
//!   second resume as a no-op), and `PBT_POWERSETTINGCHANGE` for
//!   `GUID_CONSOLE_DISPLAY_STATE` (off, on, dimmed: dimmed counts as on);
//! * `WM_WTSSESSION_CHANGE`: `WTS_SESSION_LOCK` / `WTS_SESSION_UNLOCK`.
//!
//! Message-only windows (`HWND_MESSAGE`) get no broadcasts, hence the plain
//! hidden window. Windows sends `WM_POWERBROADCAST` and waits for the window
//! procedure, so the sink of [`PowerEvent::Suspend`] runs before the system
//! sleeps; the consumer bounds that wait (Windows allows about two seconds).

use lightloom_hal::power::{PowerEvent, PowerSink, PowerSource};
use lightloom_hal::{HalError, HalResult};
use std::cell::RefCell;
use std::sync::mpsc::channel;
use std::thread::JoinHandle;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    GetLastError, ERROR_CLASS_ALREADY_EXISTS, HANDLE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{
    RegisterPowerSettingNotification, UnregisterPowerSettingNotification, POWERBROADCAST_SETTING,
};
use windows::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
};
use windows::Win32::System::SystemServices::GUID_CONSOLE_DISPLAY_STATE;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, PostThreadMessageW,
    RegisterClassW, TranslateMessage, DEVICE_NOTIFY_WINDOW_HANDLE, MSG, PBT_APMRESUMEAUTOMATIC,
    PBT_APMRESUMESUSPEND, PBT_APMSUSPEND, PBT_POWERSETTINGCHANGE, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_POWERBROADCAST, WM_QUIT, WM_WTSSESSION_CHANGE, WNDCLASSW, WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
};

const CLASS: PCWSTR = w!("LightloomPowerEvents");

thread_local! {
    /// The sink of the window on this thread (one window per thread).
    static SINK: RefCell<Option<PowerSink>> = const { RefCell::new(None) };
}

/// The event a window message stands for.
///
/// # Safety
/// For `PBT_POWERSETTINGCHANGE`, `lparam` is null or points to a
/// `POWERBROADCAST_SETTING` (Windows passes one).
unsafe fn event_of(msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<PowerEvent> {
    match msg {
        WM_POWERBROADCAST => match wparam.0 as u32 {
            PBT_APMSUSPEND => Some(PowerEvent::Suspend),
            PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => Some(PowerEvent::Resume),
            PBT_POWERSETTINGCHANGE if lparam.0 != 0 => {
                // SAFETY (caller): Windows passes the setting with its data.
                let s = unsafe { &*(lparam.0 as *const POWERBROADCAST_SETTING) };
                if s.PowerSetting != GUID_CONSOLE_DISPLAY_STATE || s.DataLength < 1 {
                    return None;
                }
                // 0 off, 1 on, 2 dimmed.
                Some(if s.Data[0] == 0 {
                    PowerEvent::DisplayOff
                } else {
                    PowerEvent::DisplayOn
                })
            }
            _ => None,
        },
        WM_WTSSESSION_CHANGE => match wparam.0 as u32 {
            WTS_SESSION_LOCK => Some(PowerEvent::Lock),
            WTS_SESSION_UNLOCK => Some(PowerEvent::Unlock),
            _ => None,
        },
        _ => None,
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: Windows passes the message's own parameters.
    if let Some(event) = unsafe { event_of(msg, wparam, lparam) } {
        // A panic must not unwind into user32.
        let _ = std::panic::catch_unwind(|| {
            let sink = SINK.with(|s| s.borrow().clone());
            if let Some(sink) = sink {
                sink(event);
            }
        });
    }
    if msg == WM_POWERBROADCAST {
        return LRESULT(1);
    }
    // SAFETY: default handling of a message of this window.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// The window thread and how to end it.
struct Watcher {
    thread_id: u32,
    thread: JoinHandle<()>,
    /// The window's handle (as an integer: `HWND` is not `Send`), for tests.
    #[cfg_attr(not(test), allow(dead_code))]
    hwnd: isize,
}

/// Power and session events from a hidden window (see the module docs).
#[derive(Default)]
pub struct WindowsPower {
    watcher: Option<Watcher>,
}

impl std::fmt::Debug for WindowsPower {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowsPower")
            .field("active", &self.watcher.is_some())
            .finish()
    }
}

impl WindowsPower {
    /// A source that is not watching yet ([`PowerSource::subscribe`] starts it).
    pub fn new() -> Self {
        Self::default()
    }
}

/// Create the window, register for the notifications, pump messages until
/// `WM_QUIT`, clean up. `ready` gets the thread id and window handle, or why it failed.
fn window_thread(sink: PowerSink, ready: std::sync::mpsc::Sender<Result<(u32, isize), String>>) {
    SINK.with(|s| *s.borrow_mut() = Some(sink));
    // SAFETY: plain Win32 calls on this thread's own window; the class, window
    // and registrations are released below before the thread ends.
    unsafe {
        let instance = match GetModuleHandleW(None) {
            Ok(m) => m.into(),
            Err(e) => {
                let _ = ready.send(Err(format!("GetModuleHandleW: {e}")));
                return;
            }
        };
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            lpszClassName: CLASS,
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            let _ = ready.send(Err(format!("RegisterClassW: {:?}", GetLastError())));
            return;
        }
        let hwnd = match CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            w!("Lightloom power events"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                let _ = ready.send(Err(format!("CreateWindowExW: {e}")));
                return;
            }
        };
        // Optional extras: sleep and resume still arrive without them.
        let display = RegisterPowerSettingNotification(
            HANDLE(hwnd.0),
            &GUID_CONSOLE_DISPLAY_STATE,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        )
        .map_err(|e| tracing::warn!("display on/off notifications unavailable: {e}"))
        .ok();
        let session = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION)
            .map_err(|e| tracing::warn!("lock/unlock notifications unavailable: {e}"))
            .is_ok();
        let _ = ready.send(Ok((GetCurrentThreadId(), hwnd.0 as isize)));
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if let Some(h) = display {
            let _ = UnregisterPowerSettingNotification(h);
        }
        if session {
            let _ = WTSUnRegisterSessionNotification(hwnd);
        }
        let _ = DestroyWindow(hwnd);
    }
    SINK.with(|s| *s.borrow_mut() = None);
}

impl PowerSource for WindowsPower {
    fn subscribe(&mut self, sink: PowerSink) -> HalResult<()> {
        self.stop();
        let (tx, rx) = channel();
        let thread = std::thread::Builder::new()
            .name("lightloom-power".into())
            .spawn(move || window_thread(sink, tx))
            .map_err(|e| HalError::Backend(format!("power events thread: {e}")))?;
        match rx.recv() {
            Ok(Ok((thread_id, hwnd))) => {
                self.watcher = Some(Watcher {
                    thread_id,
                    thread,
                    hwnd,
                });
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(HalError::Backend(e))
            }
            Err(_) => {
                let _ = thread.join();
                Err(HalError::Backend("power events thread ended".into()))
            }
        }
    }

    fn stop(&mut self) {
        if let Some(w) = self.watcher.take() {
            // SAFETY: posting to a thread that runs a message loop (or has ended: then it fails).
            if unsafe { PostThreadMessageW(w.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) }.is_ok() {
                let _ = w.thread.join();
            }
        }
    }
}

impl Drop for WindowsPower {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn messages_map_to_events() {
        let ev = |msg, w: u32| unsafe { event_of(msg, WPARAM(w as usize), LPARAM(0)) };
        assert_eq!(ev(WM_POWERBROADCAST, PBT_APMSUSPEND), Some(PowerEvent::Suspend));
        assert_eq!(
            ev(WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC),
            Some(PowerEvent::Resume)
        );
        assert_eq!(
            ev(WM_POWERBROADCAST, PBT_APMRESUMESUSPEND),
            Some(PowerEvent::Resume)
        );
        assert_eq!(ev(WM_POWERBROADCAST, PBT_POWERSETTINGCHANGE), None, "no data");
        assert_eq!(ev(WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK), Some(PowerEvent::Lock));
        assert_eq!(
            ev(WM_WTSSESSION_CHANGE, WTS_SESSION_UNLOCK),
            Some(PowerEvent::Unlock)
        );
        assert_eq!(ev(WM_WTSSESSION_CHANGE, 1), None);

        let mut s = POWERBROADCAST_SETTING {
            PowerSetting: GUID_CONSOLE_DISPLAY_STATE,
            DataLength: 4,
            Data: [0],
        };
        let p = LPARAM(&s as *const _ as isize);
        let display = |p| unsafe { event_of(WM_POWERBROADCAST, WPARAM(PBT_POWERSETTINGCHANGE as usize), p) };
        assert_eq!(display(p), Some(PowerEvent::DisplayOff));
        s.Data[0] = 2;
        let p = LPARAM(&s as *const _ as isize);
        assert_eq!(display(p), Some(PowerEvent::DisplayOn), "dimmed counts as on");
    }

    #[test]
    fn the_hidden_window_delivers_and_stops_cleanly() {
        use windows::Win32::UI::WindowsAndMessaging::SendMessageW;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut p = WindowsPower::new();
        let s = Arc::clone(&seen);
        p.subscribe(Arc::new(move |e| s.lock().unwrap().push(e)))
            .expect("hidden window");
        // What Windows sends before sleeping, to this process's own window.
        // Its own window by handle: a lookup by class name could find a running lightloomd's
        // window and send that daemon a fake suspend.
        let hwnd = HWND(p.watcher.as_ref().expect("watching").hwnd as *mut _);
        // SAFETY: this test's own window, alive until `stop`.
        unsafe {
            let r = SendMessageW(
                hwnd,
                WM_POWERBROADCAST,
                Some(WPARAM(PBT_APMSUSPEND as usize)),
                Some(LPARAM(0)),
            );
            assert_eq!(r, LRESULT(1));
        }
        // Windows also sends the display state after the registration.
        let sleeps = || {
            let s = seen.lock().unwrap();
            s.iter()
                .filter(|e| **e != PowerEvent::DisplayOn)
                .copied()
                .collect::<Vec<_>>()
        };
        assert_eq!(sleeps(), [PowerEvent::Suspend]);
        // Subscribing again replaces the window.
        p.subscribe(Arc::new(|_| {})).expect("again");
        p.stop();
        p.stop();
        assert_eq!(sleeps(), [PowerEvent::Suspend]);
    }
}
