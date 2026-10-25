// SPDX-License-Identifier: GPL-3.0-or-later
//! Where the frame ring lives and how the control channel is carried, per platform.
//!
//! Both sides use this module: `lightloom-renderer` (writer, control reader) and the daemon's
//! `lightloom-render::CefRenderer` (reader, control writer). The wire formats do not change with
//! the platform: the ring is the `lightloom_protocol::shm` layout and the control channel carries
//! the JSON lines of [`crate::control`].
//!
//! **Shared memory.** The ring is a memory-mapped file ([`crate::ring`]). Where that file sits
//! decides whether it is "real" shared memory:
//!
//! * Linux: a tmpfs, never a disk. [`shm_base_dir`] prefers `$XDG_RUNTIME_DIR` (per user, 0700,
//!   tmpfs under systemd), then `/dev/shm` (what glibc's `shm_open` uses: `shm_open("/x")` is
//!   `open("/dev/shm/x")`), then the caller's fallback. A file there is the same object
//!   `shm_open` would give, but both processes can open it by path, so the renderer keeps its
//!   `--out FILE` interface. `memfd_create` was not used: a memfd has no name, so the daemon
//!   would have to pass the descriptor across `exec` (`pre_exec` + `dup2`, i.e. `unsafe` in a
//!   `forbid(unsafe_code)` crate) or open `/proc/<pid>/fd/<n>` of the renderer (subject to
//!   Yama's ptrace scope). Every run gets a [`PrivateDir`] (mode 0700, unique name) so nothing
//!   another user put into world-writable `/dev/shm` is ever opened or truncated.
//! * Windows: the caller's work dir. A mapped file whose pages stay in the cache manager;
//!   unchanged from M4.
//!
//! **Control channel.** [`ControlAddr`]: the renderer's stdin (Windows, and the default
//! everywhere for manual runs) or a unix domain socket (Linux, what the daemon uses). With a
//! socket the daemon binds a [`ControlListener`] inside the run's [`PrivateDir`] (so only the
//! same user can connect), passes `--control unix:PATH`, and the renderer connects before CEF
//! starts. Advantages over a stdin pipe on Linux: the accepted connection proves the renderer
//! got through its own start-up, launch wrappers that do not forward stdin (AppImage runtimes,
//! `flatpak-spawn`, zypak) do not matter, and the renderer's stdin stays free. End of stream
//! means the daemon is gone, exactly as end of stdin did.

use std::ffi::OsString;
use std::fmt;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

/// File name of the ring inside a [`PrivateDir`].
pub const RING_FILE: &str = "frames.shm";
/// File name of the control socket inside a [`PrivateDir`].
pub const CONTROL_SOCKET: &str = "control.sock";
/// `sockaddr_un.sun_path` holds 108 bytes on Linux, including the terminating NUL.
pub const MAX_SOCKET_PATH: usize = 107;

/// Base directory for shared-memory rings on this platform (see the module docs).
pub fn shm_base_dir(fallback: &Path) -> PathBuf {
    if cfg!(target_os = "linux") {
        shm_base_from(
            std::env::var_os("XDG_RUNTIME_DIR"),
            Path::new("/dev/shm").is_dir(),
            fallback,
        )
    } else {
        fallback.to_path_buf()
    }
}

/// [`shm_base_dir`]'s Linux choice with its inputs made explicit (testable anywhere):
/// `$XDG_RUNTIME_DIR` when set to an absolute path that is a directory, else `/dev/shm` when
/// it exists, else `fallback`.
pub fn shm_base_from(runtime_dir: Option<OsString>, dev_shm_is_dir: bool, fallback: &Path) -> PathBuf {
    if let Some(dir) = runtime_dir.map(PathBuf::from) {
        // The XDG spec requires an absolute path; a relative or stale value is ignored.
        if dir.is_absolute() && dir.is_dir() {
            return dir;
        }
    }
    if dev_shm_is_dir {
        return PathBuf::from("/dev/shm");
    }
    fallback.to_path_buf()
}

/// A directory created for one renderer run (mode 0700 on unix, unique name), removed with
/// everything in it on drop.
#[derive(Debug)]
pub struct PrivateDir {
    path: PathBuf,
}

impl PrivateDir {
    /// Create `<base>/<prefix>-<pid>-<n>-<nanos>`. Fails rather than reusing an existing entry.
    pub fn create(base: &Path, prefix: &str) -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::fs::create_dir_all(base)?;
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let path = base.join(format!("{prefix}-{}-{n}-{nanos:08x}", std::process::id()));
        #[cfg_attr(not(unix), allow(unused_mut))] // `mode` is unix-only
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        // Not `recursive`: an existing entry (another user's, a symlink) is an error, never adopted.
        builder.create(&path)?;
        Ok(Self { path })
    }

    /// The directory.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `<dir>/<name>`.
    pub fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for PrivateDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// How control commands reach a serving renderer (`--control`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ControlAddr {
    /// One JSON object per line on stdin (`stdin`).
    #[default]
    Stdin,
    /// The same lines over a unix domain socket the renderer connects to (`unix:PATH`; unix only).
    Unix(PathBuf),
}

impl FromStr for ControlAddr {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        if s == "stdin" || s == "-" {
            return Ok(Self::Stdin);
        }
        match s.strip_prefix("unix:") {
            Some("") => Err("unix: needs a socket path".into()),
            Some(path) if path.len() > MAX_SOCKET_PATH => Err(format!(
                "socket path is {} bytes, the limit is {MAX_SOCKET_PATH}",
                path.len()
            )),
            Some(path) => Ok(Self::Unix(PathBuf::from(path))),
            None => Err(format!("expected `stdin` or `unix:PATH`, got {s:?}")),
        }
    }
}

impl fmt::Display for ControlAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stdin => f.write_str("stdin"),
            Self::Unix(p) => write!(f, "unix:{}", p.display()),
        }
    }
}

/// Open the renderer's end of the control channel.
pub fn connect_control(addr: &ControlAddr) -> io::Result<Box<dyn BufRead + Send>> {
    match addr {
        ControlAddr::Stdin => Ok(Box::new(io::BufReader::new(io::stdin()))),
        #[cfg(unix)]
        ControlAddr::Unix(path) => {
            let stream = std::os::unix::net::UnixStream::connect(path)?;
            Ok(Box::new(io::BufReader::new(stream)))
        }
        #[cfg(not(unix))]
        ControlAddr::Unix(path) => Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!(
                "unix control socket {} is not supported on this platform",
                path.display()
            ),
        )),
    }
}

/// The daemon's end of a unix control socket: bound, non-blocking, polled with
/// [`ControlListener::try_accept`] while the renderer starts. The socket file is removed on drop.
#[cfg(unix)]
#[derive(Debug)]
pub struct ControlListener {
    listener: std::os::unix::net::UnixListener,
    path: PathBuf,
}

#[cfg(unix)]
impl ControlListener {
    /// Bind `path` (which must not exist; put it in a [`PrivateDir`]).
    pub fn bind(path: &Path) -> io::Result<Self> {
        let listener = std::os::unix::net::UnixListener::bind(path)?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            path: path.to_path_buf(),
        })
    }

    /// The address to hand to the renderer.
    pub fn addr(&self) -> ControlAddr {
        ControlAddr::Unix(self.path.clone())
    }

    /// The renderer's connection if it has connected, as a blocking stream.
    pub fn try_accept(&self) -> io::Result<Option<std::os::unix::net::UnixStream>> {
        match self.listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false)?;
                Ok(Some(stream))
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(unix)]
impl Drop for ControlListener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Result of [`read_bounded_line`].
#[derive(Debug, PartialEq, Eq)]
pub enum BoundedLine {
    /// End of input (nothing read).
    Eof,
    /// One line (without the newline) is in the buffer.
    Line,
    /// The line was longer than the limit; it was consumed up to its newline and dropped.
    TooLong(usize),
}

/// Read one `\n`-terminated line into `buf` (cleared first) without ever holding more than
/// `max + 1` bytes of it: a peer that never sends a newline cannot make the reader allocate
/// without bound. A final line without newline counts as a line.
pub fn read_bounded_line<R: BufRead + ?Sized>(
    r: &mut R,
    buf: &mut Vec<u8>,
    max: usize,
) -> io::Result<BoundedLine> {
    buf.clear();
    let mut total = 0usize;
    let mut saw_any = false;
    loop {
        let chunk = match r.fill_buf() {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if chunk.is_empty() {
            return Ok(match (saw_any, total > max) {
                (false, _) => BoundedLine::Eof,
                (true, true) => BoundedLine::TooLong(total),
                (true, false) => BoundedLine::Line,
            });
        }
        saw_any = true;
        let (take, newline) = match chunk.iter().position(|&b| b == b'\n') {
            Some(i) => (i, true),
            None => (chunk.len(), false),
        };
        if total <= max {
            let room = max + 1 - total;
            buf.extend_from_slice(&chunk[..take.min(room)]);
        }
        total += take;
        r.consume(take + usize::from(newline));
        if newline {
            if total > max {
                buf.clear();
                return Ok(BoundedLine::TooLong(total));
            }
            return Ok(BoundedLine::Line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn shm_base_prefers_runtime_dir_then_dev_shm() {
        let fallback = Path::new("fallback-dir");
        let existing = std::env::temp_dir();
        assert_eq!(
            shm_base_from(Some(existing.clone().into()), true, fallback),
            existing
        );
        // unset, relative or missing XDG_RUNTIME_DIR falls through
        assert_eq!(shm_base_from(None, true, fallback), PathBuf::from("/dev/shm"));
        assert_eq!(
            shm_base_from(Some("relative".into()), true, fallback),
            PathBuf::from("/dev/shm")
        );
        let missing = existing.join("lightloom-no-such-runtime-dir-xyz");
        assert_eq!(shm_base_from(Some(missing.into()), false, fallback), fallback);
        assert_eq!(shm_base_from(None, false, fallback), fallback);
        if !cfg!(target_os = "linux") {
            assert_eq!(shm_base_dir(fallback), fallback);
        }
    }

    #[test]
    fn private_dirs_are_unique_and_removed() {
        let base = std::env::temp_dir().join(format!("lightloom-ipc-test-{}", std::process::id()));
        let a = PrivateDir::create(&base, "t").unwrap();
        let b = PrivateDir::create(&base, "t").unwrap();
        assert_ne!(a.path(), b.path());
        assert!(a.path().is_dir());
        std::fs::write(a.join(RING_FILE), b"x").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(a.path()).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        let (pa, pb) = (a.path().to_path_buf(), b.path().to_path_buf());
        drop(a);
        drop(b);
        assert!(!pa.exists() && !pb.exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn control_addresses_parse_and_print() {
        assert_eq!("stdin".parse::<ControlAddr>().unwrap(), ControlAddr::Stdin);
        assert_eq!("-".parse::<ControlAddr>().unwrap(), ControlAddr::Stdin);
        let unix: ControlAddr = "unix:/run/user/1000/hd/control.sock".parse().unwrap();
        assert_eq!(unix, ControlAddr::Unix("/run/user/1000/hd/control.sock".into()));
        assert_eq!(unix.to_string(), "unix:/run/user/1000/hd/control.sock");
        assert_eq!(ControlAddr::Stdin.to_string(), "stdin");
        assert!("unix:".parse::<ControlAddr>().is_err());
        assert!("tcp:127.0.0.1:1".parse::<ControlAddr>().is_err());
        assert!(format!("unix:/{}", "a".repeat(MAX_SOCKET_PATH))
            .parse::<ControlAddr>()
            .is_err());
        assert!(ControlAddr::Stdin.to_string().parse::<ControlAddr>().is_ok());
    }

    #[cfg(not(unix))]
    #[test]
    fn unix_control_is_refused_without_unix_sockets() {
        let err = connect_control(&ControlAddr::Unix("x.sock".into()))
            .err()
            .unwrap();
        assert_eq!(err.kind(), io::ErrorKind::Unsupported);
    }

    #[test]
    fn bounded_lines() {
        let mut buf = Vec::new();
        // a tiny BufReader capacity forces lines across many fill_buf calls
        let data = b"one\n\ntoolongline\nlast".to_vec();
        let mut r = io::BufReader::with_capacity(3, Cursor::new(data));
        assert_eq!(read_bounded_line(&mut r, &mut buf, 5).unwrap(), BoundedLine::Line);
        assert_eq!(buf, b"one");
        assert_eq!(read_bounded_line(&mut r, &mut buf, 5).unwrap(), BoundedLine::Line);
        assert!(buf.is_empty());
        assert_eq!(
            read_bounded_line(&mut r, &mut buf, 5).unwrap(),
            BoundedLine::TooLong(11)
        );
        assert!(buf.is_empty());
        assert_eq!(read_bounded_line(&mut r, &mut buf, 5).unwrap(), BoundedLine::Line);
        assert_eq!(buf, b"last");
        assert_eq!(read_bounded_line(&mut r, &mut buf, 5).unwrap(), BoundedLine::Eof);
        // exactly at the limit is fine; a newline-less overlong tail is reported too
        let mut r = Cursor::new(b"12345\n123456".to_vec());
        assert_eq!(read_bounded_line(&mut r, &mut buf, 5).unwrap(), BoundedLine::Line);
        assert_eq!(buf, b"12345");
        assert_eq!(
            read_bounded_line(&mut r, &mut buf, 5).unwrap(),
            BoundedLine::TooLong(6)
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_socket_round_trip() {
        use std::io::Write;
        let base = std::env::temp_dir();
        let dir = PrivateDir::create(&base, "lightloom-ipc-sock").unwrap();
        let listener = ControlListener::bind(&dir.join(CONTROL_SOCKET)).unwrap();
        assert!(listener.try_accept().unwrap().is_none(), "nobody connected yet");
        let addr = listener.addr();
        let reader = std::thread::spawn(move || {
            let mut r = connect_control(&addr).unwrap();
            let mut buf = Vec::new();
            let mut lines = Vec::new();
            while read_bounded_line(&mut r, &mut buf, 64).unwrap() != BoundedLine::Eof {
                lines.push(String::from_utf8(buf.clone()).unwrap());
            }
            lines
        });
        let mut stream = loop {
            if let Some(s) = listener.try_accept().unwrap() {
                break s;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        stream
            .write_all(b"{\"cmd\":\"fps\",\"value\":15}\n{\"cmd\":\"quit\"}\n")
            .unwrap();
        drop(stream);
        assert_eq!(
            reader.join().unwrap(),
            vec!["{\"cmd\":\"fps\",\"value\":15}", "{\"cmd\":\"quit\"}"]
        );
        let sock = dir.join(CONTROL_SOCKET);
        drop(listener);
        assert!(!sock.exists());
    }
}
