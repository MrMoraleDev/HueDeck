// SPDX-License-Identifier: GPL-3.0-or-later
//! Console control events: Ctrl-C, Ctrl-Break, window close, log-off, shutdown.
//!
//! Every event requests a stop ([`ConsoleStop::requested`]). Ctrl-C and
//! Ctrl-Break return at once and the application shuts its devices down in
//! its own time. For close, log-off and shutdown, Windows terminates the
//! process as soon as the handler returns, so the handler blocks until the
//! application calls [`ConsoleStop::finished`] (at most [`CLOSE_GRACE`],
//! within the ~5 s Windows allows) so plugins can put devices back first.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;
use windows::core::BOOL;
use windows::Win32::System::Console::{SetConsoleCtrlHandler, CTRL_BREAK_EVENT, CTRL_C_EVENT};

/// Longest the handler of a close/log-off/shutdown event waits for [`ConsoleStop::finished`].
pub const CLOSE_GRACE: Duration = Duration::from_millis(4500);

struct Shared {
    stop: AtomicBool,
    done: Mutex<bool>,
    cv: Condvar,
}

static SHARED: OnceLock<Shared> = OnceLock::new();

fn shared() -> &'static Shared {
    SHARED.get_or_init(|| Shared {
        stop: AtomicBool::new(false),
        done: Mutex::new(false),
        cv: Condvar::new(),
    })
}

unsafe extern "system" fn handler(ctrl_type: u32) -> BOOL {
    let s = shared();
    s.stop.store(true, Ordering::Release);
    if ctrl_type == CTRL_C_EVENT || ctrl_type == CTRL_BREAK_EVENT {
        return BOOL(1);
    }
    // Close, log-off, shutdown: returning ends the process, so wait for the shutdown.
    let guard = s.done.lock().unwrap_or_else(|e| e.into_inner());
    let _ =
        s.cv.wait_timeout_while(guard, CLOSE_GRACE, |done| !*done)
            .unwrap_or_else(|e| e.into_inner());
    BOOL(1)
}

/// The process-wide stop request set by console events.
#[derive(Debug, Clone, Copy)]
pub struct ConsoleStop(());

impl ConsoleStop {
    /// Install the handler (once per process; later calls return the same
    /// handle) and clear any earlier request.
    pub fn install() -> Result<Self, String> {
        static INSTALLED: OnceLock<Result<(), String>> = OnceLock::new();
        INSTALLED
            .get_or_init(|| {
                // SAFETY: `handler` is a plain extern "system" fn with the PHANDLER_ROUTINE signature
                // and lives for the whole process.
                unsafe { SetConsoleCtrlHandler(Some(handler), true) }
                    .map_err(|e| format!("SetConsoleCtrlHandler: {}", e.message()))
            })
            .clone()?;
        let stop = Self(());
        stop.reset();
        Ok(stop)
    }

    /// Whether a console event asked the process to stop.
    pub fn requested(&self) -> bool {
        shared().stop.load(Ordering::Acquire)
    }

    /// Clear the request and the finished mark (start of a new run).
    pub fn reset(&self) {
        let s = shared();
        s.stop.store(false, Ordering::Release);
        *s.done.lock().unwrap_or_else(|e| e.into_inner()) = false;
    }

    /// Devices are back in their previous state: a blocked close/log-off
    /// handler may now return and let Windows end the process.
    pub fn finished(&self) {
        let s = shared();
        *s.done.lock().unwrap_or_else(|e| e.into_inner()) = true;
        s.cv.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test: the handler state is process-wide, parallel tests would race on it.
    #[test]
    fn ctrl_c_returns_at_once_and_close_waits_until_finished() {
        let stop = ConsoleStop::install().expect("install");
        assert!(!stop.requested());
        // SAFETY: calling our own handler directly, as Windows would.
        assert_eq!(unsafe { handler(CTRL_C_EVENT) }, BOOL(1));
        assert!(stop.requested());
        stop.reset();
        assert!(!stop.requested());

        use std::time::Instant;
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let t = std::thread::spawn(move || {
            entered_tx.send(Instant::now()).unwrap();
            // CTRL_CLOSE_EVENT.
            // SAFETY: calling our own handler directly, as Windows would.
            let r = unsafe { handler(2) };
            (r, Instant::now())
        });
        // The handler thread may start late on a loaded machine: wait for it
        // (and for its stop request) instead of assuming a fixed sleep is enough.
        // Its grace period cannot end before `entered + CLOSE_GRACE`.
        let entered = entered_rx
            .recv_timeout(Duration::from_secs(60))
            .expect("the close handler ran");
        let deadline = Instant::now() + Duration::from_secs(60);
        while !stop.requested() {
            assert!(Instant::now() < deadline, "the close handler requested no stop");
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(100));
        // Unless scheduling already ate the whole grace period, the handler is
        // still blocked: only finished() or CLOSE_GRACE lets it return.
        if entered.elapsed() < CLOSE_GRACE {
            assert!(!t.is_finished(), "close blocks until finished()");
        }
        let finished_at = Instant::now();
        stop.finished();
        let (r, returned) = t.join().unwrap();
        assert_eq!(r, BOOL(1));
        // Woken by finished(), not by the grace timeout: it returned before the
        // timeout could fire. Checked when finished() came well inside the grace
        // period (a stall of more than a second meanwhile makes it inconclusive).
        if finished_at + Duration::from_secs(1) < entered + CLOSE_GRACE {
            assert!(returned < entered + CLOSE_GRACE, "returned once finished");
        }
        stop.reset();
    }
}
