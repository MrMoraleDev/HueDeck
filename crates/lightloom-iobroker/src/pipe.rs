// SPDX-License-Identifier: GPL-3.0-or-later
//! The broker's named pipe, `\\.\pipe\lightloom-iobroker`.
//!
//! Frames are `u32 LE length` + a postcard message, at most
//! `MAX_FRAME_BYTES`. One pipe instance, created once and reused, serves one
//! client at a time on the broker's worker thread (so the global SMBus mutex
//! is always taken and released on the same thread). Remote clients are
//! rejected; the DACL lets in SYSTEM and the one interactive user recorded by
//! `lightloom-setup` (its SID on the service command line). Users get read and
//! write but not `FILE_CREATE_PIPE_INSTANCE`, so they cannot add instances.
//!
//! While a `BusLock` lease holds the global SMBus mutex the broker never
//! blocks on the pipe: it reads a request only once the whole frame has
//! arrived ([`frame_complete`]), and a client that stalls mid-frame loses its
//! leases (and the mutex) when they run out. Between two looks at the pipe
//! it yields for the first [`LEASE_SPIN`] ([`idle`]), then sleeps.
//!
//! Deferred (ADR-0004): a per-boot token proving the client is Lightloom. Until
//! then the DACL is the gate, and the broker's static policy (deny ranges,
//! write allowlist, rate limit) holds whatever the client sends.

use std::io;
#[cfg(windows)]
use std::io::{Read, Write};
use std::time::Duration;

pub use lightloom_protocol::broker::PIPE_NAME;
/// Read + write for a client, without `FILE_APPEND_DATA` (= `FILE_CREATE_PIPE_INSTANCE`):
/// `(FILE_GENERIC_READ | FILE_GENERIC_WRITE) & !FILE_APPEND_DATA`.
pub const CLIENT_ACCESS: u32 = 0x0012_019B;

use lightloom_protocol::broker::MAX_FRAME_BYTES;
pub use lightloom_protocol::broker::{read_frame, write_frame};

/// Whether a whole frame is waiting: `header` holds the first (up to 4)
/// bytes waiting, `available` how many are waiting in all. A malformed
/// length counts as complete, so [`read_frame`] rejects it at once.
pub fn frame_complete(header: &[u8], available: u32) -> bool {
    let Ok(len) = <[u8; 4]>::try_from(header.get(..4).unwrap_or(header)) else {
        return false;
    };
    let len = u32::from_le_bytes(len) as usize;
    if len == 0 || len > MAX_FRAME_BYTES {
        return true;
    }
    available as usize >= 4 + len
}

/// While a lease holds the mutex, how long the broker only yields between
/// looks at the pipe before it sleeps. The client's next request (the rest
/// of a locked sequence) normally arrives within microseconds; a 1 ms sleep
/// lasts a whole timer tick on Windows (up to 15.6 ms), which made every
/// transfer of a locked sequence wait that long.
pub const LEASE_SPIN: Duration = Duration::from_millis(2);

/// What the broker does between two looks at the pipe while a lease holds
/// the mutex.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Idle {
    /// Give up the time slice and look again at once.
    Yield,
    /// Sleep (a timer tick or more).
    Sleep(Duration),
}

/// [`Idle`] after waiting `waited` for the next request under a lease:
/// yield for [`LEASE_SPIN`], then sleep 1 ms at a time (lease ends are
/// still noticed within a tick).
pub fn idle(waited: Duration) -> Idle {
    if waited < LEASE_SPIN {
        Idle::Yield
    } else {
        Idle::Sleep(Duration::from_millis(1))
    }
}

/// The pipe's DACL: SYSTEM full control, and the client `sid` (a SID string
/// such as `S-1-5-21-…`), or interactive users (`IU`) for a development console run.
pub fn pipe_sddl(client_sid: Option<&str>) -> Result<String, String> {
    let client = match client_sid {
        Some(sid) => {
            let ok = sid.starts_with("S-1-")
                && sid.len() <= 184
                && sid[4..]
                    .split('-')
                    .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
            if !ok {
                return Err(format!("not a SID: {sid:?}"));
            }
            sid
        }
        None => "IU",
    };
    Ok(format!("D:P(A;;GA;;;SY)(A;;0x{CLIENT_ACCESS:08X};;;{client})"))
}

#[cfg(windows)]
pub use win::{serve, wake};

#[cfg(windows)]
mod win {
    use super::*;
    use crate::mutex::GlobalMutex;
    use crate::pawnio::PawnExec;
    use crate::server::Server;
    use lightloom_protocol::broker::{BrokerError, BrokerRequest, BrokerResponse};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{
        CloseHandle, LocalFree, ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE, HANDLE, HLOCAL,
    };
    use windows::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FlushFileBuffers, ReadFile, WriteFile, FILE_FLAGS_AND_ATTRIBUTES,
        FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_SHARE_NONE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
    };
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PeekNamedPipe, PIPE_READMODE_BYTE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn os_err(e: windows::core::Error) -> io::Error {
        io::Error::other(e)
    }

    /// The one pipe instance (server end).
    struct Pipe(HANDLE);

    impl Drop for Pipe {
        fn drop(&mut self) {
            // SAFETY: the handle came from CreateNamedPipeW and is closed once.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    impl Pipe {
        fn create(sddl: &str) -> io::Result<Self> {
            let wsddl = wide(sddl);
            let mut sd = PSECURITY_DESCRIPTOR::default();
            // SAFETY: NUL-terminated SDDL; `sd` receives a LocalAlloc'ed
            // descriptor freed after CreateNamedPipeW.
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    PCWSTR(wsddl.as_ptr()),
                    SDDL_REVISION_1,
                    &mut sd,
                    None,
                )
            }
            .map_err(os_err)?;
            let sa = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: sd.0,
                bInheritHandle: false.into(),
            };
            let name = wide(PIPE_NAME);
            let buffer = (MAX_FRAME_BYTES + 4) as u32;
            // SAFETY: NUL-terminated name; `sa` and its descriptor outlive the
            // call. FIRST_PIPE_INSTANCE fails if another process already owns the name.
            let h = unsafe {
                CreateNamedPipeW(
                    PCWSTR(name.as_ptr()),
                    PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    1,
                    buffer,
                    buffer,
                    0,
                    Some(&sa),
                )
            };
            // SAFETY: `sd` came from ConvertStringSecurityDescriptorToSecurityDescriptorW, freed once.
            unsafe {
                let _ = LocalFree(Some(HLOCAL(sd.0)));
            }
            if h.is_invalid() {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(h))
        }

        /// Wait for a client.
        fn connect(&self) -> io::Result<()> {
            // SAFETY: valid pipe handle; synchronous (no OVERLAPPED).
            match unsafe { ConnectNamedPipe(self.0, None) } {
                Ok(()) => Ok(()),
                Err(e) if e.code() == ERROR_PIPE_CONNECTED.to_hresult() => Ok(()),
                Err(e) => Err(os_err(e)),
            }
        }

        fn disconnect(&self) {
            // SAFETY: valid pipe handle owned by self.
            unsafe {
                let _ = FlushFileBuffers(self.0);
                let _ = DisconnectNamedPipe(self.0);
            }
        }

        /// Whether a whole request frame is waiting (nothing is consumed).
        fn frame_waiting(&self) -> io::Result<bool> {
            let mut header = [0u8; 4];
            let (mut read, mut avail) = (0u32, 0u32);
            // SAFETY: valid pipe handle; `header` is writable for the 4 bytes
            // passed; both counters are valid out pointers. Peeking consumes nothing.
            unsafe {
                PeekNamedPipe(
                    self.0,
                    Some(header.as_mut_ptr().cast()),
                    header.len() as u32,
                    Some(&mut read),
                    Some(&mut avail),
                    None,
                )
            }
            .map_err(os_err)?;
            Ok(frame_complete(&header[..read as usize], avail))
        }
    }

    impl Read for &Pipe {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let mut n = 0u32;
            // SAFETY: valid pipe handle; `buf` is writable for its length; synchronous.
            match unsafe { ReadFile(self.0, Some(buf), Some(&mut n), None) } {
                Ok(()) => Ok(n as usize),
                // A closed client (ERROR_BROKEN_PIPE) reads as end of stream.
                Err(_) if n == 0 => Ok(0),
                Err(e) => Err(os_err(e)),
            }
        }
    }

    impl Write for &Pipe {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let mut n = 0u32;
            // SAFETY: valid pipe handle; `buf` readable for its length; synchronous.
            unsafe { WriteFile(self.0, Some(buf), Some(&mut n), None) }.map_err(os_err)?;
            Ok(n as usize)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// Serve clients one after the other until `stop` is set (then [`wake`]
    /// the blocked accept). Every request goes to `server` on this thread.
    pub fn serve<E: PawnExec, M: GlobalMutex>(
        server: &mut Server<E, M>,
        client_sid: Option<&str>,
        stop: &AtomicBool,
    ) -> io::Result<()> {
        let sddl = pipe_sddl(client_sid).map_err(io::Error::other)?;
        let pipe = Pipe::create(&sddl)?;
        tracing::info!("listening on {PIPE_NAME}");
        while !stop.load(Ordering::SeqCst) {
            pipe.connect()?;
            if stop.load(Ordering::SeqCst) {
                break;
            }
            tracing::info!("client connected");
            if let Err(e) = session(server, &pipe) {
                tracing::warn!("client session ended: {e}");
            }
            server.client_gone();
            pipe.disconnect();
        }
        Ok(())
    }

    fn session<E: PawnExec, M: GlobalMutex>(server: &mut Server<E, M>, pipe: &Pipe) -> io::Result<()> {
        loop {
            // While a lease holds the SMBus mutex, never block: wait for a
            // whole frame, ending the leases that run out meanwhile. Once none
            // is left the mutex is free and a blocking read harms no one.
            let waiting_since = Instant::now();
            while let Some(deadline) = server.lease_deadline() {
                if pipe.frame_waiting()? {
                    break;
                }
                let now = Instant::now();
                if now >= deadline {
                    server.expire(now);
                    continue;
                }
                match idle(now.saturating_duration_since(waiting_since)) {
                    Idle::Yield => std::thread::yield_now(),
                    Idle::Sleep(d) => std::thread::sleep(d),
                }
            }
            let Some(bytes) = read_frame(&mut &*pipe)? else {
                return Ok(());
            };
            let resp = match lightloom_protocol::decode::<BrokerRequest>(&bytes) {
                Ok(req) => server.handle(&req, Instant::now()),
                Err(e) => BrokerResponse {
                    id: 0,
                    result: Err(BrokerError::BadRequest(e.to_string())),
                },
            };
            let out = lightloom_protocol::encode(&resp).map_err(io::Error::other)?;
            write_frame(&mut &*pipe, &out)?;
        }
    }

    /// Connect to the pipe once so a blocked [`serve`] notices `stop`.
    pub fn wake() {
        let name = wide(PIPE_NAME);
        // SAFETY: NUL-terminated name; the handle is closed right away.
        if let Ok(h) = unsafe {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        } {
            // SAFETY: handle just opened above, closed once.
            unsafe {
                let _ = CloseHandle(h);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightloom_protocol::broker::{BrokerOp, BrokerRequest};

    #[test]
    fn frame_round_trip() {
        let req = BrokerRequest::new(
            3,
            BrokerOp::BusLock {
                bus: 0,
                lease_ms: 100,
            },
        );
        let payload = lightloom_protocol::encode(&req).unwrap();
        let mut wire = Vec::new();
        write_frame(&mut wire, &payload).unwrap();
        assert_eq!(&wire[..4], &(payload.len() as u32).to_le_bytes());
        let mut r = &wire[..];
        let back = read_frame(&mut r).unwrap().unwrap();
        assert_eq!(lightloom_protocol::decode::<BrokerRequest>(&back).unwrap(), req);
        assert_eq!(read_frame(&mut r).unwrap(), None, "clean end of stream");
    }

    #[test]
    fn oversized_frame_rejected() {
        let mut wire = ((MAX_FRAME_BYTES + 1) as u32).to_le_bytes().to_vec();
        wire.extend(vec![0u8; MAX_FRAME_BYTES + 1]);
        let err = read_frame(&mut &wire[..]).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(
            read_frame(&mut &0u32.to_le_bytes()[..]).is_err(),
            "empty frames too"
        );
        assert!(write_frame(&mut Vec::new(), &vec![0; MAX_FRAME_BYTES + 1]).is_err());
        assert!(
            read_frame(&mut &[5u8, 0, 0, 0, 1][..]).is_err(),
            "truncated payload"
        );
    }

    #[test]
    fn under_a_lease_the_next_request_is_not_waited_for_with_a_timer_tick() {
        assert_eq!(idle(Duration::ZERO), Idle::Yield);
        assert_eq!(idle(LEASE_SPIN - Duration::from_micros(1)), Idle::Yield);
        assert_eq!(idle(LEASE_SPIN), Idle::Sleep(Duration::from_millis(1)));
    }

    #[test]
    fn frame_complete_needs_the_whole_frame() {
        let header = 5u32.to_le_bytes();
        assert!(!frame_complete(&[], 0));
        assert!(!frame_complete(&header[..3], 3), "partial length prefix");
        assert!(!frame_complete(&header, 4), "length only");
        assert!(!frame_complete(&header, 8), "truncated body");
        assert!(frame_complete(&header, 9));
        assert!(frame_complete(&header, 20), "more queued behind it");
        assert!(
            frame_complete(&0u32.to_le_bytes(), 4),
            "bad length: read and rejected"
        );
        assert!(frame_complete(&((MAX_FRAME_BYTES + 1) as u32).to_le_bytes(), 4));
    }

    #[test]
    fn dacl_names_system_and_one_user() {
        assert_eq!(
            pipe_sddl(Some("S-1-5-21-1-2-3-1001")).unwrap(),
            "D:P(A;;GA;;;SY)(A;;0x0012019B;;;S-1-5-21-1-2-3-1001)"
        );
        assert!(pipe_sddl(None).unwrap().ends_with(";;;IU)"));
        assert!(
            pipe_sddl(Some("S-1-5-21-1)(A;;GA;;;WD")).is_err(),
            "no SDDL injection"
        );
        assert!(pipe_sddl(Some("WD")).is_err());
    }
}
