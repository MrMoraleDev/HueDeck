// SPDX-License-Identifier: GPL-3.0-or-later
//! System notifications as Windows toasts (`Windows.UI.Notifications`).
//!
//! A toast needs an AppUserModelID (AUMID) that Windows knows a display name
//! for. HueDeck runs as a plain, possibly portable executable, so it has no
//! package identity and no installer-made Start-menu shortcut. The least
//! invasive registration that works for such an executable is a per-user
//! registry key, written the first time a toast is shown (never before,
//! never with administrator rights):
//!
//! ```text
//! HKEY_CURRENT_USER\Software\Classes\AppUserModelId\<AUMID>
//!     DisplayName (REG_SZ) = <display name>
//! ```
//!
//! Windows then shows the toasts under that name in the notification centre
//! (Settings > System > Notifications lists it, where the user can silence
//! it). Deleting the key undoes it. A Start-menu shortcut carrying
//! `System.AppUserModel.ID` would work too, but it would leave a file in the
//! user's Start menu for a portable app.
//!
//! Each toast is shown from its own short-lived thread (WinRT initialised
//! there), so [`Notifier::notify`] never blocks the caller; failures are
//! logged.

use huedeck_hal::notify::{Notification, Notifier};
use huedeck_hal::HalResult;
use std::sync::{Arc, Mutex};
use windows::core::{HSTRING, PCWSTR};
use windows::Data::Xml::Dom::XmlDocument;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE,
    REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

/// HueDeck's AppUserModelID.
pub const HUEDECK_AUMID: &str = "HueDeck.HueDeck";
/// The name toasts appear under.
pub const HUEDECK_DISPLAY_NAME: &str = "HueDeck";

/// Windows toasts under one AUMID.
#[derive(Debug)]
pub struct ToastNotifier {
    aumid: String,
    display_name: String,
    /// The AUMID registration was written (or failed: `Some(Err)`), once per process.
    registered: Arc<Mutex<Option<Result<(), String>>>>,
}

impl ToastNotifier {
    /// Toasts under `aumid`, registered with `display_name` on first use.
    pub fn new(aumid: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            aumid: aumid.into(),
            display_name: display_name.into(),
            registered: Arc::new(Mutex::new(None)),
        }
    }

    /// HueDeck's toasts ([`HUEDECK_AUMID`], [`HUEDECK_DISPLAY_NAME`]).
    pub fn huedeck() -> Self {
        Self::new(HUEDECK_AUMID, HUEDECK_DISPLAY_NAME)
    }
}

impl Notifier for ToastNotifier {
    fn notify(&self, notification: &Notification) -> HalResult<()> {
        let (aumid, name) = (self.aumid.clone(), self.display_name.clone());
        let registered = Arc::clone(&self.registered);
        let xml = toast_xml(&notification.title, &notification.body);
        std::thread::Builder::new()
            .name("huedeck-toast".into())
            .spawn(move || {
                {
                    let mut r = registered.lock().unwrap_or_else(|e| e.into_inner());
                    if r.is_none() {
                        *r = Some(register_aumid(&aumid, &name));
                    }
                    if let Some(Err(e)) = r.as_ref() {
                        tracing::warn!("cannot register the notification sender {aumid}: {e}");
                        return;
                    }
                }
                if let Err(e) = show(&aumid, &xml) {
                    tracing::warn!("cannot show a notification: {e}");
                }
            })?;
        Ok(())
    }
}

/// The toast document: a generic template with a title and one text line.
pub fn toast_xml(title: &str, body: &str) -> String {
    format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
        escape(title),
        escape(body)
    )
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() && c != '\n' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `HKCU\Software\Classes\AppUserModelId\<aumid>`, `DisplayName = <name>`.
fn register_aumid(aumid: &str, name: &str) -> Result<(), String> {
    let subkey = wide(&format!("Software\\Classes\\AppUserModelId\\{aumid}"));
    let mut key = HKEY::default();
    // SAFETY: NUL-terminated key name that outlives the call; valid out pointer.
    let err = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
    };
    if err != ERROR_SUCCESS {
        return Err(std::io::Error::from_raw_os_error(err.0 as i32).to_string());
    }
    let value = wide("DisplayName");
    let data: Vec<u8> = wide(name).iter().flat_map(|u| u.to_le_bytes()).collect();
    // SAFETY: the key is open; value name and data outlive the call.
    let err = unsafe { RegSetValueExW(key, PCWSTR(value.as_ptr()), None, REG_SZ, Some(&data)) };
    // SAFETY: the key was opened above and is closed once.
    unsafe {
        let _ = RegCloseKey(key);
    }
    if err != ERROR_SUCCESS {
        return Err(std::io::Error::from_raw_os_error(err.0 as i32).to_string());
    }
    Ok(())
}

fn show(aumid: &str, xml: &str) -> windows::core::Result<()> {
    // SAFETY: initialises WinRT for this (new, short-lived) thread only.
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }?;
    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(xml))?;
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(aumid))?.Show(&toast)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_text_is_escaped() {
        assert_eq!(
            toast_xml("A & B", "<5 %> \"q\"\u{7}"),
            "<toast><visual><binding template=\"ToastGeneric\"><text>A &amp; B</text>\
             <text>&lt;5 %&gt; &quot;q&quot;</text></binding></visual></toast>"
        );
    }
}
