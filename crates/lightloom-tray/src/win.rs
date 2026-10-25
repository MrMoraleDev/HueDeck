// SPDX-License-Identifier: GPL-3.0-or-later
//! The Windows side: a hidden window that owns one `Shell_NotifyIconW`
//! icon, the popup menu, a 3 s status poll for the tooltip and starting
//! the daemon.
//!
//! Everything runs on the one GUI thread. The menu loop (`TrackPopupMenu`)
//! dispatches messages while it is open, so the window procedure only
//! borrows the state when nothing else holds it (a timer tick during the
//! menu is skipped).

use lightloom_tray::{
    ico_image, menu_with_profiles, perform, tooltip, Action, Api, Autostart, Desktop, Entry, HttpApi,
    Outcome, Status, WinRunKey, WINDOW_CLASS,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    GetLastError, ERROR_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_WARNING, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconFromResourceEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
    DispatchMessageW, GetCursorPos, GetMessageW, GetSystemMetrics, LoadIconW, PostMessageW, PostQuitMessage,
    RegisterClassW, RegisterWindowMessageW, SetForegroundWindow, SetMenuDefaultItem, SetTimer,
    TrackPopupMenu, TranslateMessage, HICON, HMENU, IDI_APPLICATION, LR_DEFAULTCOLOR, MF_CHECKED, MF_GRAYED,
    MF_POPUP, MF_SEPARATOR, MF_STRING, MSG, SM_CXSMICON, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_CLOSE, WM_CONTEXTMENU, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP,
    WM_TIMER, WNDCLASSW,
};

/// The icon's callback message.
const WM_TRAY: u32 = WM_APP + 1;
const TIMER_POLL: usize = 1;
const POLL_MS: u32 = 3000;
const ICON_ID: u32 = 1;
/// The app icon (`packaging/wix/lightloom.ico`).
const ICO: &[u8] = include_bytes!("../../../packaging/wix/lightloom.ico");

/// Explorer's broadcast after it restarts: the icon has to be added again.
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `s` into a fixed UTF-16 field, cut to fit, NUL-terminated.
fn fill(dst: &mut [u16], s: &str) {
    let n = dst.len() - 1;
    let mut i = 0;
    for u in s.encode_utf16().take(n) {
        dst[i] = u;
        i += 1;
    }
    dst[i..].fill(0);
}

/// The browser and the programs next to the tray.
struct WinDesktop {
    dir: Option<PathBuf>,
}

impl WinDesktop {
    fn sibling(&self, name: &str) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join(name)).filter(|p| p.is_file())
    }
}

impl Desktop for WinDesktop {
    fn open_url(&self, url: &str) {
        let _ = std::process::Command::new("explorer").arg(url).spawn();
    }

    fn start_and_open(&self) -> bool {
        use std::os::windows::process::CommandExt;
        /// A console of its own, never shown.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // lightloomw keeps the daemon's log; a bare lightloomd is the fallback.
        if let Some(w) = self.sibling("lightloomw.exe") {
            return std::process::Command::new(w).arg("--open").spawn().is_ok();
        }
        let Some(d) = self.sibling("lightloomd.exe") else {
            return false;
        };
        let mut cmd = std::process::Command::new(&d);
        cmd.arg("--open")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW);
        if let Some(dir) = d.parent() {
            cmd.current_dir(dir);
        }
        cmd.spawn().is_ok()
    }
}

struct App {
    hwnd: HWND,
    icon: HICON,
    api: HttpApi,
    autostart: Autostart<WinRunKey>,
    desktop: WinDesktop,
    tip: String,
}

impl App {
    fn data(&self, flags: windows::Win32::UI::Shell::NOTIFY_ICON_DATA_FLAGS) -> NOTIFYICONDATAW {
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: ICON_ID,
            uFlags: flags,
            uCallbackMessage: WM_TRAY,
            hIcon: self.icon,
            ..Default::default()
        };
        fill(&mut nid.szTip, &self.tip);
        nid
    }

    fn add_icon(&self) {
        let nid = self.data(NIF_MESSAGE | NIF_ICON | NIF_TIP);
        // SAFETY: a fully initialised NOTIFYICONDATAW for our own window.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_ADD, &nid);
        }
    }

    fn remove_icon(&self) {
        let nid = self.data(Default::default());
        // SAFETY: as in add_icon.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        }
    }

    /// Poll the daemon; update the tooltip when it changed.
    fn refresh(&mut self) -> Status {
        let status = self.api.status();
        let tip = tooltip(status);
        if tip != self.tip {
            self.tip = tip;
            let nid = self.data(NIF_TIP);
            // SAFETY: as in add_icon.
            unsafe {
                let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
            }
        }
        status
    }

    /// A balloon on the icon (a refused command).
    fn warn(&self, message: &str) {
        let mut nid = self.data(NIF_INFO);
        fill(&mut nid.szInfoTitle, "Lightloom");
        fill(&mut nid.szInfo, message);
        nid.dwInfoFlags = NIIF_WARNING;
        // SAFETY: as in add_icon.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }

    fn run(&mut self, action: Action) {
        match perform(action, &self.api, &self.autostart, &self.desktop) {
            Ok(Outcome::Exit) => {
                self.remove_icon();
                // SAFETY: ends this thread's message loop.
                unsafe { PostQuitMessage(0) };
            }
            Ok(Outcome::Stay) => {
                self.refresh();
            }
            Err(e) => self.warn(&e),
        }
    }

    fn show_menu(&mut self) {
        let status = self.refresh();
        let (profiles, current) = if status == Status::NotRunning {
            (Vec::new(), None)
        } else {
            self.api.profiles()
        };
        let entries = menu_with_profiles(
            status,
            (self.autostart.enabled(), self.autostart.available()),
            &profiles,
            current.as_deref(),
        );
        // SAFETY: the menu is built, shown and destroyed here, on the window's
        // thread; the window is ours.
        let chosen = unsafe {
            let Ok(m) = build_menu(&entries) else {
                return;
            };
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            // Without this the menu does not close when the user clicks elsewhere.
            let _ = SetForegroundWindow(self.hwnd);
            let id = TrackPopupMenu(
                m,
                TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY,
                pt.x,
                pt.y,
                None,
                self.hwnd,
                None,
            );
            let _ = PostMessageW(Some(self.hwnd), WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(m);
            id.0
        };
        if let Some(action) = u16::try_from(chosen).ok().and_then(Action::from_id) {
            self.run(action);
        }
    }
}

/// The popup menu for `entries` (submenus are destroyed with it).
unsafe fn build_menu(entries: &[Entry]) -> windows::core::Result<HMENU> {
    let m = CreatePopupMenu()?;
    for e in entries {
        match e {
            Entry::Separator => AppendMenuW(m, MF_SEPARATOR, 0, PCWSTR::null())?,
            Entry::Item {
                action,
                label,
                checked,
                enabled,
                default,
            } => {
                let mut flags = MF_STRING;
                if *checked {
                    flags |= MF_CHECKED;
                }
                if !*enabled {
                    flags |= MF_GRAYED;
                }
                let text = wide(label);
                AppendMenuW(m, flags, usize::from(action.id()), PCWSTR(text.as_ptr()))?;
                if *default {
                    let _ = SetMenuDefaultItem(m, u32::from(action.id()), 0);
                }
            }
            Entry::Submenu {
                label,
                enabled,
                items,
            } => {
                let sub = build_menu(items)?;
                let mut flags = MF_POPUP | MF_STRING;
                if !*enabled {
                    flags |= MF_GRAYED;
                }
                let text = wide(label);
                AppendMenuW(m, flags, sub.0 as usize, PCWSTR(text.as_ptr()))?;
            }
        }
    }
    Ok(m)
}

/// Run `f` on the state, unless something already holds it.
fn with_app(f: impl FnOnce(&mut App)) {
    APP.with(|a| {
        if let Ok(mut a) = a.try_borrow_mut() {
            if let Some(app) = a.as_mut() {
                f(app);
            }
        }
    });
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            match (lp.0 & 0xffff) as u32 {
                WM_LBUTTONUP => with_app(|a| a.run(Action::OpenDashboard)),
                WM_RBUTTONUP | WM_CONTEXTMENU => with_app(App::show_menu),
                _ => {}
            }
            LRESULT(0)
        }
        // `lightloomw --stop` (the installer is about to replace the files).
        WM_CLOSE => {
            with_app(|a| a.remove_icon());
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_TIMER if wp.0 == TIMER_POLL => {
            with_app(|a| {
                a.refresh();
            });
            LRESULT(0)
        }
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            with_app(|a| a.add_icon());
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn load_icon() -> HICON {
    // SAFETY: plain metric and icon calls; the image bytes outlive the call.
    unsafe {
        let px = GetSystemMetrics(SM_CXSMICON).max(16);
        if let Some(img) = ico_image(ICO, px as u32) {
            if let Ok(icon) = CreateIconFromResourceEx(img, true, 0x0003_0000, px, px, LR_DEFAULTCOLOR) {
                return icon;
            }
        }
        LoadIconW(None, IDI_APPLICATION).unwrap_or_default()
    }
}

/// Run the tray until Quit; the process exit code.
pub fn run() -> i32 {
    // SAFETY: a named mutex, kept (never closed) for the life of the process.
    let mutex = unsafe { CreateMutexW(None, false, w!("Local\\Lightloom.Tray")) };
    // SAFETY: reads this thread's last error right after the call.
    if mutex.is_ok() && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return 0; // one tray per session
    }

    let (data, _) = lightloom_runtime::dirs::from_env();
    let dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(PathBuf::from));
    let desktop = WinDesktop { dir };
    let command = desktop
        .sibling("lightloomw.exe")
        .map(|w| Autostart::<WinRunKey>::command_for(&w));

    // SAFETY: registering a class and creating a hidden window owned by this
    // thread; the class name is a static string.
    let hwnd = unsafe {
        let Ok(module) = GetModuleHandleW(None) else {
            return 1;
        };
        let hinst = HINSTANCE(module.0);
        let class = WINDOW_CLASS;
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            lpszClassName: class,
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            return 1;
        }
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);
        match CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("Lightloom"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinst),
            None,
        ) {
            Ok(h) => h,
            Err(_) => return 1,
        }
    };

    let mut app = App {
        hwnd,
        icon: load_icon(),
        api: HttpApi::new(data),
        autostart: Autostart::new(WinRunKey, command),
        desktop,
        tip: tooltip(Status::NotRunning),
    };
    app.tip = tooltip(app.api.status());
    app.add_icon();
    APP.with(|a| *a.borrow_mut() = Some(app));

    // SAFETY: the standard message loop of this thread's window.
    unsafe {
        SetTimer(Some(hwnd), TIMER_POLL, POLL_MS, None);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    APP.with(|a| {
        if let Some(app) = a.borrow_mut().take() {
            app.remove_icon();
        }
    });
    0
}
