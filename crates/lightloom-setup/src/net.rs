// SPDX-License-Identifier: GPL-3.0-or-later
//! One HTTPS GET with WinHTTP (the system's TLS, proxy settings and
//! certificate store; redirects followed, never from HTTPS to HTTP), for the
//! PawnIO installer download. The body is capped at the pinned size.

use std::time::Duration;
use windows::core::{w, PCWSTR};
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
    INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
};

/// `https://host/path` → `("host", "/path")`; nothing else is accepted.
pub fn split_https_url(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("https://")?;
    let (host, path) = rest.split_at(rest.find('/')?);
    (!host.is_empty() && !host.contains([':', '@'])).then_some((host, path))
}

struct Handle(*mut core::ffi::c_void);

impl Handle {
    fn new(h: *mut core::ffi::c_void, what: &str) -> Result<Self, String> {
        if h.is_null() {
            Err(format!("{what}: {}", std::io::Error::last_os_error()))
        } else {
            Ok(Self(h))
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: a WinHTTP handle opened below, closed once.
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

const TIMEOUT: Duration = Duration::from_secs(30);

/// GET `url` (HTTPS only); fails on a status other than 200 or a body longer than `max_len`.
pub fn download(url: &str, max_len: usize) -> Result<Vec<u8>, String> {
    let (host, path) = split_https_url(url).ok_or_else(|| format!("{url}: not an https:// URL"))?;
    let (whost, wpath) = (wide(host), wide(path));
    let agent = wide(concat!("lightloom-setup/", env!("CARGO_PKG_VERSION")));
    // SAFETY: NUL-terminated agent string; no proxy names (automatic proxy).
    let session = Handle::new(
        unsafe {
            WinHttpOpen(
                PCWSTR(agent.as_ptr()),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            )
        },
        "WinHttpOpen",
    )?;
    let ms = TIMEOUT.as_millis() as i32;
    // SAFETY: valid session handle.
    unsafe { WinHttpSetTimeouts(session.0, ms, ms, ms, ms) }.map_err(|e| format!("timeouts: {e}"))?;
    // SAFETY: valid session; NUL-terminated host name that outlives the call.
    let connect = Handle::new(
        unsafe { WinHttpConnect(session.0, PCWSTR(whost.as_ptr()), INTERNET_DEFAULT_HTTPS_PORT, 0) },
        "connecting",
    )
    .map_err(|e| format!("{host}: {e}"))?;
    // SAFETY: valid connection; NUL-terminated verb and path; default version,
    // no referrer, no accept types.
    let request = Handle::new(
        unsafe {
            WinHttpOpenRequest(
                connect.0,
                w!("GET"),
                PCWSTR(wpath.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            )
        },
        "opening the request",
    )?;
    // SAFETY: valid request handle; no extra headers or body.
    unsafe { WinHttpSendRequest(request.0, None, None, 0, 0, 0) }.map_err(|e| format!("{url}: {e}"))?;
    // SAFETY: valid request handle after a sent request.
    unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) }.map_err(|e| format!("{url}: {e}"))?;
    let mut status = 0u32;
    let mut len = std::mem::size_of::<u32>() as u32;
    // SAFETY: valid request; the buffer is a u32 of the size passed.
    unsafe {
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut len,
            std::ptr::null_mut(),
        )
    }
    .map_err(|e| format!("{url}: status: {e}"))?;
    if status != 200 {
        return Err(format!("{url}: HTTP {status}"));
    }
    let mut body = Vec::with_capacity(max_len);
    loop {
        let mut available = 0u32;
        // SAFETY: valid request; valid out pointer.
        unsafe { WinHttpQueryDataAvailable(request.0, &mut available) }.map_err(|e| format!("{url}: {e}"))?;
        if available == 0 {
            break;
        }
        if body.len() + available as usize > max_len {
            return Err(format!("{url}: larger than the pinned {max_len} bytes"));
        }
        let start = body.len();
        body.resize(start + available as usize, 0);
        let mut read = 0u32;
        // SAFETY: `body[start..]` is writable for `available` bytes; valid out pointer.
        unsafe { WinHttpReadData(request.0, body[start..].as_mut_ptr().cast(), available, &mut read) }
            .map_err(|e| format!("{url}: {e}"))?;
        body.truncate(start + read as usize);
        if read == 0 {
            break;
        }
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_https_urls() {
        assert_eq!(
            split_https_url(lightloom_iobroker::pins::PAWNIO_SETUP.url),
            Some((
                "github.com",
                "/namazso/PawnIO.Setup/releases/download/2.2.0/PawnIO_setup.exe"
            ))
        );
        assert_eq!(split_https_url("http://github.com/x"), None);
        assert_eq!(split_https_url("https://github.com"), None);
        assert_eq!(split_https_url("https:///x"), None);
        assert_eq!(split_https_url("https://evil@github.com/x"), None);
        assert_eq!(split_https_url("https://github.com:8443/x"), None);
    }
}
