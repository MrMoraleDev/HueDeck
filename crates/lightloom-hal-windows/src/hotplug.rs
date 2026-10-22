// SPDX-License-Identifier: GPL-3.0-or-later
//! HID arrival and removal notifications: `CM_Register_Notification` on the
//! HID device-interface class, the Windows [`HotplugSource`].
//!
//! One event per HID collection, carrying what its interface path tells: the
//! path and VID/PID (the serial stays empty, a removed device can no longer
//! be asked). A plug or unplug produces a burst of events (every collection,
//! sometimes twice); consumers wait for the burst to settle, then enumerate
//! again (see `lightloom_plugin_host::manager`).

use lightloom_hal::hotplug::{DeviceClass, DeviceEvent, HotplugSource};
use lightloom_hal::{DeviceKey, HalError, HalResult};
use std::ffi::c_void;
use std::sync::mpsc::{channel, Receiver, Sender};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Register_Notification, CM_Unregister_Notification, CM_NOTIFY_ACTION,
    CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL, CM_NOTIFY_ACTION_DEVICEINTERFACEREMOVAL, CM_NOTIFY_EVENT_DATA,
    CM_NOTIFY_FILTER, CM_NOTIFY_FILTER_0, CM_NOTIFY_FILTER_0_0, CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
    CR_SUCCESS, HCMNOTIFICATION,
};
use windows::Win32::Devices::HumanInterfaceDevice::HidD_GetHidGuid;

/// An active registration and the sender its callback uses.
struct Registration {
    handle: HCMNOTIFICATION,
    context: *mut Sender<DeviceEvent>,
}

/// HID hotplug notifications (see the module docs).
#[derive(Default)]
pub struct HidHotplug {
    registration: Option<Registration>,
}

// SAFETY: the registration handle may be unregistered from any thread, and the
// boxed sender it points to is `Send + Sync`.
unsafe impl Send for HidHotplug {}

impl std::fmt::Debug for HidHotplug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HidHotplug")
            .field("active", &self.registration.is_some())
            .finish()
    }
}

impl HidHotplug {
    /// A watcher that is not watching yet ([`HotplugSource::subscribe`] starts it).
    pub fn new() -> Self {
        Self::default()
    }
}

/// The NUL-terminated symbolic link that follows the event header, bounded by `size`.
///
/// # Safety
/// `data` points to `size` readable bytes of a device-interface event.
unsafe fn symbolic_link(data: *const CM_NOTIFY_EVENT_DATA, size: u32) -> String {
    // SAFETY (caller): the event is readable; the link starts inside it.
    unsafe {
        let start = std::ptr::addr_of!((*data).u.DeviceInterface.SymbolicLink).cast::<u16>();
        let offset = (start as usize).saturating_sub(data as usize);
        let units = (size as usize).saturating_sub(offset) / 2;
        let mut n = 0;
        while n < units && *start.add(n) != 0 {
            n += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(start, n))
    }
}

unsafe extern "system" fn on_notify(
    _handle: HCMNOTIFICATION,
    context: *const c_void,
    action: CM_NOTIFY_ACTION,
    data: *const CM_NOTIFY_EVENT_DATA,
    size: u32,
) -> u32 {
    let arrived = match action {
        CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL => true,
        CM_NOTIFY_ACTION_DEVICEINTERFACEREMOVAL => false,
        _ => return 0,
    };
    if context.is_null() || data.is_null() {
        return 0;
    }
    // A panic must not unwind into cfgmgr32.
    let _ = std::panic::catch_unwind(|| {
        // SAFETY: cfgmgr32 passes the event it describes with `size`; `context` is the
        // sender registered with this callback, alive until unregistration returns.
        let (path, sender) = unsafe { (symbolic_link(data, size), &*context.cast::<Sender<DeviceEvent>>()) };
        let (vid, pid) = crate::probe::path_vid_pid(&path).unwrap_or((0, 0));
        let key = DeviceKey::new(vid, pid, "");
        let class = DeviceClass::Hid;
        let event = if arrived {
            DeviceEvent::Arrived { key, class, path }
        } else {
            DeviceEvent::Removed { key, class, path }
        };
        let _ = sender.send(event);
    });
    0
}

impl HotplugSource for HidHotplug {
    fn subscribe(&mut self) -> HalResult<Receiver<DeviceEvent>> {
        self.stop();
        let (tx, rx) = channel();
        let context = Box::into_raw(Box::new(tx));
        let filter = CM_NOTIFY_FILTER {
            cbSize: std::mem::size_of::<CM_NOTIFY_FILTER>() as u32,
            Flags: 0,
            FilterType: CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
            Reserved: 0,
            u: CM_NOTIFY_FILTER_0 {
                DeviceInterface: CM_NOTIFY_FILTER_0_0 {
                    // SAFETY: plain query of a constant GUID.
                    ClassGuid: unsafe { HidD_GetHidGuid() },
                },
            },
        };
        let mut handle = HCMNOTIFICATION::default();
        // SAFETY: the filter outlives the call; the context stays valid until
        // CM_Unregister_Notification has returned (see `stop`).
        let cr = unsafe {
            CM_Register_Notification(
                &filter,
                Some(context.cast_const().cast()),
                Some(on_notify),
                &mut handle,
            )
        };
        if cr != CR_SUCCESS {
            // SAFETY: not registered, so the callback can never see it.
            drop(unsafe { Box::from_raw(context) });
            return Err(HalError::Backend(format!(
                "CM_Register_Notification failed: {}",
                cr.0
            )));
        }
        self.registration = Some(Registration { handle, context });
        Ok(rx)
    }

    fn stop(&mut self) {
        if let Some(r) = self.registration.take() {
            // SAFETY: unregistering waits for running callbacks, so the sender can be freed after it.
            unsafe {
                let _ = CM_Unregister_Notification(r.handle);
                drop(Box::from_raw(r.context));
            }
        }
    }
}

impl Drop for HidHotplug {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscribes_and_unsubscribes_cleanly() {
        let mut hp = HidHotplug::new();
        let rx = hp.subscribe().expect("register");
        // Re-subscribing replaces the registration; the old channel closes.
        let rx2 = hp.subscribe().expect("register again");
        assert!(matches!(
            rx.recv_timeout(std::time::Duration::from_millis(10)),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
        ));
        hp.stop();
        assert!(rx2.recv().is_err(), "closed after stop");
    }
}
