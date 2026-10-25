// SPDX-License-Identifier: GPL-3.0-or-later
//! X11: grabs of one RandR monitor's area of the root window (`x11rb`, pure
//! Rust, no libxcb).
//!
//! * Displays are the RandR 1.5 monitors (`GetMonitors`): name (`DP-1`),
//!   size, primary flag.
//! * Each tick copies the monitor's area with `ShmGetImage` into a segment
//!   the server creates and passes as a file descriptor (MIT-SHM 1.2,
//!   `ShmCreateSegment`; works across IPC namespaces, so also in a Flatpak),
//!   mapped read-only here. Without MIT-SHM 1.2 (a remote display) it falls
//!   back to a plain `GetImage`.
//! * X11 has no "new frame" signal, so every tick is a new image (at most
//!   `fps` per second, ~14.7 MB copied by the server at 2560x1440).
//! * Every 2 s the monitor layout is checked; a change (mode switch,
//!   unplug) ends the grab and the capture thread reopens it.
//! * The root window must be a 24/32-bit TrueColor visual (every current
//!   desktop); 30-bit and paletted visuals are refused.

#![allow(unsafe_code)]

use super::pixels::{self, Order};
use crate::screen::sample::{Frame, Rotation};
use crate::screen::{display_matches, DisplayInfo, FrameSource};
use std::os::fd::AsRawFd;
use std::ptr::NonNull;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::shm::ConnectionExt as _;
use x11rb::protocol::xproto::{ConnectionExt as _, ImageFormat, ImageOrder, VisualClass};
use x11rb::rust_connection::RustConnection;

/// How often the monitor layout is checked.
const LAYOUT_CHECK: Duration = Duration::from_secs(2);

/// One RandR monitor.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Monitor {
    info: DisplayInfo,
    x: i16,
    y: i16,
}

fn monitors(conn: &RustConnection, root: u32) -> Result<Vec<Monitor>, String> {
    let reply = conn
        .randr_get_monitors(root, true)
        .map_err(|e| e.to_string())?
        .reply()
        .map_err(|e| format!("RandR GetMonitors: {e}"))?;
    Ok(reply
        .monitors
        .iter()
        .map(|m| {
            let name = conn
                .get_atom_name(m.name)
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|r| String::from_utf8_lossy(&r.name).into_owned())
                .unwrap_or_else(|| format!("monitor-{}", m.name));
            Monitor {
                info: DisplayInfo {
                    name,
                    width: u32::from(m.width),
                    height: u32::from(m.height),
                    primary: m.primary,
                    adapter: "X11".into(),
                },
                x: m.x,
                y: m.y,
            }
        })
        .collect())
}

fn connect() -> Result<(RustConnection, usize), String> {
    RustConnection::connect(None).map_err(|e| format!("connecting to the X server: {e}"))
}

/// The RandR monitors of the default X display.
pub fn displays() -> Result<Vec<DisplayInfo>, String> {
    let (conn, screen) = connect()?;
    let root = conn.setup().roots[screen].root;
    Ok(monitors(&conn, root)?.into_iter().map(|m| m.info).collect())
}

/// Pixel order of the root visual, when it is one this module reads.
fn root_order(conn: &RustConnection, screen: usize) -> Result<Order, String> {
    let setup = conn.setup();
    let s = &setup.roots[screen];
    let bpp = setup
        .pixmap_formats
        .iter()
        .find(|f| f.depth == s.root_depth)
        .map(|f| f.bits_per_pixel);
    let visual = s
        .allowed_depths
        .iter()
        .flat_map(|d| &d.visuals)
        .find(|v| v.visual_id == s.root_visual);
    let Some(v) = visual else {
        return Err("the root visual is not listed".into());
    };
    order(
        s.root_depth,
        bpp,
        v.class,
        (v.red_mask, v.green_mask, v.blue_mask),
        setup.image_byte_order,
    )
}

/// The memory order of a ZPixmap pixel, or why it cannot be read.
fn order(
    depth: u8,
    bits_per_pixel: Option<u8>,
    class: VisualClass,
    masks: (u32, u32, u32),
    byte_order: ImageOrder,
) -> Result<Order, String> {
    if !(depth == 24 || depth == 32) || bits_per_pixel != Some(32) || class != VisualClass::TRUE_COLOR {
        return Err(format!(
            "unsupported root visual (depth {depth}, {bits_per_pixel:?} bpp, {class:?})"
        ));
    }
    match (masks, byte_order) {
        ((0xff_0000, 0xff00, 0xff), ImageOrder::LSB_FIRST) => Ok(Order::Bgrx),
        ((0xff, 0xff00, 0xff_0000), ImageOrder::LSB_FIRST) => Ok(Order::Rgbx),
        _ => Err(format!(
            "unsupported root visual masks {masks:x?} ({byte_order:?})"
        )),
    }
}

/// A shared-memory segment created by the server and mapped here.
struct Segment {
    seg: u32,
    ptr: NonNull<u8>,
    len: usize,
}

impl Segment {
    fn create(conn: &RustConnection, len: usize) -> Result<Self, String> {
        let version = conn
            .shm_query_version()
            .map_err(|e| e.to_string())?
            .reply()
            .map_err(|e| e.to_string())?;
        if (version.major_version, version.minor_version) < (1, 2) {
            return Err(format!(
                "MIT-SHM {}.{} has no CreateSegment",
                version.major_version, version.minor_version
            ));
        }
        let seg = conn.generate_id().map_err(|e| e.to_string())?;
        let size = u32::try_from(len).map_err(|_| "image too large")?;
        let reply = conn
            .shm_create_segment(seg, size, false)
            .map_err(|e| e.to_string())?
            .reply()
            .map_err(|e| format!("ShmCreateSegment: {e}"))?;
        // The server writes the images (`read_only = false` is its access).
        // SAFETY: maps `len` bytes of the segment fd read-only and shared; the
        // mapping is owned by this value and unmapped in `Drop`. The fd may be
        // closed afterwards: the mapping keeps the segment alive.
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ,
                libc::MAP_SHARED,
                reply.shm_fd.as_raw_fd(),
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            let _ = conn.shm_detach(seg);
            return Err(format!(
                "mmap of the X segment: {}",
                std::io::Error::last_os_error()
            ));
        }
        let ptr = NonNull::new(ptr.cast::<u8>()).ok_or("mmap returned null")?;
        Ok(Self { seg, ptr, len })
    }

    fn bytes(&self) -> &[u8] {
        // SAFETY: a live read-only mapping of `len` bytes; the server writes
        // it only during `ShmGetImage`, whose reply was awaited before this
        // is read.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl Drop for Segment {
    fn drop(&mut self) {
        // SAFETY: the mapping created in `create`, unmapped once.
        unsafe { libc::munmap(self.ptr.as_ptr().cast(), self.len) };
    }
}

/// Grabs of one monitor.
pub struct Grab {
    conn: RustConnection,
    root: u32,
    monitor: Monitor,
    order: Order,
    shm: Option<Segment>,
    packed: Vec<u8>,
    checked: Instant,
}

// SAFETY: `Segment` holds a raw pointer to a private mapping that only the
// thread owning this value reads; nothing about it is tied to a thread.
unsafe impl Send for Grab {}

impl Grab {
    /// Grab `display` (a RandR monitor name; `None`/empty: the primary monitor).
    pub fn open(display: Option<&str>) -> Result<Self, String> {
        let (conn, screen) = connect()?;
        let root = conn.setup().roots[screen].root;
        let order = root_order(&conn, screen)?;
        let all = monitors(&conn, root)?;
        let wanted = display.map(str::trim).filter(|d| !d.is_empty());
        let pick = match wanted {
            Some(name) => all.iter().find(|m| display_matches(name, &m.info.name)),
            None => all.iter().find(|m| m.info.primary).or(all.first()),
        };
        let Some(monitor) = pick.cloned() else {
            let names: Vec<_> = all.iter().map(|m| m.info.name.as_str()).collect();
            return Err(match wanted {
                Some(name) => format!("no display named {name:?} (displays: {})", names.join(", ")),
                None => "the X server reports no monitor".into(),
            });
        };
        let len = monitor.info.width as usize * monitor.info.height as usize * 4;
        let shm = match Segment::create(&conn, len) {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::info!("screen: no MIT-SHM ({e}); using GetImage");
                None
            }
        };
        Ok(Self {
            conn,
            root,
            monitor,
            order,
            shm,
            packed: Vec::new(),
            checked: Instant::now(),
        })
    }
}

impl Drop for Grab {
    fn drop(&mut self) {
        if let Some(s) = &self.shm {
            let _ = self.conn.shm_detach(s.seg);
            let _ = self.conn.flush();
        }
    }
}

impl FrameSource for Grab {
    fn describe(&self) -> String {
        let i = &self.monitor.info;
        format!(
            "{} ({}x{}{}) on X11{}",
            i.name,
            i.width,
            i.height,
            if i.primary { ", primary" } else { "" },
            if self.shm.is_some() { " (MIT-SHM)" } else { "" }
        )
    }

    fn capture(&mut self, _: Duration, f: &mut dyn FnMut(&Frame<'_>)) -> Result<bool, String> {
        if self.checked.elapsed() >= LAYOUT_CHECK {
            self.checked = Instant::now();
            let now = monitors(&self.conn, self.root)?;
            if !now.contains(&self.monitor) {
                return Err("the monitor layout changed".into());
            }
        }
        let m = &self.monitor;
        let (w, h) = (m.info.width as u16, m.info.height as u16);
        let image;
        let bytes: &[u8] = match &self.shm {
            Some(s) => {
                self.conn
                    .shm_get_image(
                        self.root,
                        m.x,
                        m.y,
                        w,
                        h,
                        !0,
                        ImageFormat::Z_PIXMAP.into(),
                        s.seg,
                        0,
                    )
                    .map_err(|e| e.to_string())?
                    .reply()
                    .map_err(|e| format!("ShmGetImage: {e}"))?;
                s.bytes()
            }
            None => {
                image = self
                    .conn
                    .get_image(ImageFormat::Z_PIXMAP, self.root, m.x, m.y, w, h, !0)
                    .map_err(|e| e.to_string())?
                    .reply()
                    .map_err(|e| format!("GetImage: {e}"))?;
                &image.data
            }
        };
        let (w, h) = (usize::from(w), usize::from(h));
        let (data, stride) = if self.order == Order::Bgrx {
            (bytes, w * 4)
        } else if pixels::to_bgra(bytes, w, h, w * 4, self.order, &mut self.packed) {
            (&self.packed[..], w * 4)
        } else {
            return Err("short X image".into());
        };
        match Frame::new(data, w as u32, h as u32, stride, Rotation::Identity) {
            Some(frame) => {
                f(&frame);
                Ok(true)
            }
            None => Err(format!("bad X image of {} bytes for {w}x{h}", data.len())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::{reduce, ScreenSettings, ScreenSnapshot};
    use crate::ZoneSnapshot;

    #[test]
    fn reads_only_true_colour_32_bit_visuals() {
        let rgb = (0xff_0000, 0xff00, 0xff);
        let lsb = ImageOrder::LSB_FIRST;
        assert_eq!(
            order(24, Some(32), VisualClass::TRUE_COLOR, rgb, lsb),
            Ok(Order::Bgrx)
        );
        assert_eq!(
            order(32, Some(32), VisualClass::TRUE_COLOR, rgb, lsb),
            Ok(Order::Bgrx)
        );
        assert_eq!(
            order(
                24,
                Some(32),
                VisualClass::TRUE_COLOR,
                (0xff, 0xff00, 0xff_0000),
                lsb
            ),
            Ok(Order::Rgbx)
        );
        assert!(order(
            30,
            Some(32),
            VisualClass::TRUE_COLOR,
            (0x3ff0_0000, 0xffc00, 0x3ff),
            lsb
        )
        .is_err());
        assert!(order(16, Some(16), VisualClass::TRUE_COLOR, (0xf800, 0x7e0, 0x1f), lsb).is_err());
        assert!(order(24, Some(24), VisualClass::TRUE_COLOR, rgb, lsb).is_err());
        assert!(order(8, Some(8), VisualClass::PSEUDO_COLOR, (0, 0, 0), lsb).is_err());
        assert!(order(24, Some(32), VisualClass::TRUE_COLOR, rgb, ImageOrder::MSB_FIRST).is_err());
    }

    /// Manual check on a real X display (never run by `cargo test`): grabs
    /// the primary monitor for ~2 s and prints the monitors, frame count and
    /// reduction time. Nothing of the image is printed or kept.
    /// `cargo test -p lightloom-inputs x11 -- --ignored --nocapture`
    #[test]
    #[ignore = "captures the real X display"]
    fn grab_the_primary_monitor() {
        println!("displays: {:?}", displays());
        let mut g = Grab::open(None).expect("open");
        println!("capturing {}", g.describe());
        let settings = ScreenSettings {
            fps: 30,
            zone: true,
            meters: Vec::new(),
        };
        let mut snap = ScreenSnapshot {
            zone: Some(ZoneSnapshot::default()),
            vision: Vec::new(),
        };
        let (mut frames, mut cpu) = (0u32, Duration::ZERO);
        let end = Instant::now() + Duration::from_secs(2);
        while Instant::now() < end {
            let t = Instant::now();
            frames += u32::from(
                g.capture(Duration::from_millis(33), &mut |f| {
                    reduce(f, &settings, &mut snap)
                })
                .expect("capture"),
            );
            cpu += t.elapsed();
        }
        println!("{frames} grabs; grab + reduction {:?} each", cpu / frames.max(1));
        assert!(frames > 0);
    }
}
