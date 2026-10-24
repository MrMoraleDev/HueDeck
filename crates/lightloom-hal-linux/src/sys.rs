// SPDX-License-Identifier: GPL-3.0-or-later
//! The only `unsafe` code of this crate: hidraw and i2c-dev ioctls, `poll`
//! and the netlink uevent socket, through `libc`. Each function is a thin wrapper
//! that checks its buffer sizes before the call.
//!
//! ioctl numbers follow the asm-generic encoding (`dir << 30 | size << 16 |
//! 'H' << 8 | nr`, READ = 2, WRITE = 1), which x86-64, arm64 and riscv64
//! use (docs/research/m7-linux-backend.md §1.1). The i2c-dev ioctls are
//! plain numbers (`linux/i2c-dev.h`, §3.3).

#![allow(unsafe_code)]

use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

const IOC_WRITE: u32 = 1;
const IOC_READ: u32 = 2;

const fn ioc(dir: u32, nr: u32, size: usize) -> u32 {
    (dir << 30) | ((size as u32 & 0x3fff) << 16) | (0x48 << 8) | nr
}

/// `HID_MAX_DESCRIPTOR_SIZE`.
pub const HID_MAX_DESCRIPTOR_SIZE: usize = 4096;
/// `HID_MAX_BUFFER_SIZE` (since 5.11; 8192 before).
pub const HID_MAX_BUFFER_SIZE: usize = 16384;

/// `struct hidraw_report_descriptor`.
#[repr(C)]
struct HidrawReportDescriptor {
    size: u32,
    value: [u8; HID_MAX_DESCRIPTOR_SIZE],
}

const HIDIOCGRDESCSIZE: u32 = ioc(IOC_READ, 0x01, std::mem::size_of::<libc::c_int>());
const HIDIOCGRDESC: u32 = ioc(IOC_READ, 0x02, std::mem::size_of::<HidrawReportDescriptor>());
const NR_SFEATURE: u32 = 0x06;
const NR_GFEATURE: u32 = 0x07;
const NR_GINPUT: u32 = 0x0A;

/// The report descriptor through `HIDIOCGRDESCSIZE` + `HIDIOCGRDESC`.
pub fn report_descriptor(file: &File) -> io::Result<Vec<u8>> {
    let fd = file.as_raw_fd();
    let mut size: libc::c_int = 0;
    // SAFETY: `size` is a valid `int` the kernel writes into.
    if unsafe { libc::ioctl(fd, HIDIOCGRDESCSIZE as _, &mut size as *mut libc::c_int) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let size = (size.max(0) as usize).min(HID_MAX_DESCRIPTOR_SIZE);
    let mut desc = Box::new(HidrawReportDescriptor {
        size: size as u32,
        value: [0; HID_MAX_DESCRIPTOR_SIZE],
    });
    // SAFETY: `desc` is a full `struct hidraw_report_descriptor` (size field
    // set as the kernel requires); the kernel writes at most `size` bytes.
    if unsafe { libc::ioctl(fd, HIDIOCGRDESC as _, &mut *desc as *mut HidrawReportDescriptor) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(desc.value[..size].to_vec())
}

fn report_ioctl(file: &File, nr: u32, buf: &mut [u8]) -> io::Result<usize> {
    if buf.is_empty() || buf.len() > HID_MAX_BUFFER_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "report length out of range",
        ));
    }
    let req = ioc(IOC_READ | IOC_WRITE, nr, buf.len());
    // SAFETY: the request encodes `buf.len()`, so the kernel reads/writes at
    // most that many bytes of `buf`.
    let n = unsafe { libc::ioctl(file.as_raw_fd(), req as _, buf.as_mut_ptr()) };
    if n < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(n as usize)
    }
}

/// `HIDIOCSFEATURE(len)`: SET_REPORT(Feature); `buf[0]` is the report id.
pub fn set_feature(file: &File, buf: &mut [u8]) -> io::Result<usize> {
    report_ioctl(file, NR_SFEATURE, buf)
}

/// `HIDIOCGFEATURE(len)`: GET_REPORT(Feature); `buf[0]` is the report id.
pub fn get_feature(file: &File, buf: &mut [u8]) -> io::Result<usize> {
    report_ioctl(file, NR_GFEATURE, buf)
}

/// `HIDIOCGINPUT(len)` (Linux 5.11+): GET_REPORT(Input).
pub fn get_input(file: &File, buf: &mut [u8]) -> io::Result<usize> {
    report_ioctl(file, NR_GINPUT, buf)
}

/// What [`poll_in`] saw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ready {
    /// Data to read.
    Readable,
    /// Nothing within the timeout.
    Timeout,
    /// The device went away (or was revoked).
    Hangup,
}

/// Wait up to `timeout_ms` (`-1` forever) for `fd` to become readable.
pub fn poll_in(fd: &impl AsRawFd, timeout_ms: i32) -> io::Result<Ready> {
    let mut pfd = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    loop {
        // SAFETY: one valid `pollfd`.
        let r = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
        if r < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }
        if r == 0 {
            return Ok(Ready::Timeout);
        }
        if pfd.revents & libc::POLLIN != 0 {
            return Ok(Ready::Readable);
        }
        if pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Ok(Ready::Hangup);
        }
        return Ok(Ready::Timeout);
    }
}

/// `NETLINK_KOBJECT_UEVENT`.
const NETLINK_KOBJECT_UEVENT: libc::c_int = 15;

/// A non-blocking `NETLINK_KOBJECT_UEVENT` socket bound to multicast `groups`
/// (`1` kernel, `2` udev).
pub fn uevent_socket(groups: u32) -> io::Result<OwnedFd> {
    // SAFETY: plain socket(2) call; the result is checked before use.
    let fd = unsafe {
        libc::socket(
            libc::AF_NETLINK,
            libc::SOCK_RAW | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
            NETLINK_KOBJECT_UEVENT,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `fd` is a fresh descriptor this function owns.
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    // SAFETY: zeroed `sockaddr_nl` is a valid value; fields set below.
    let mut addr: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
    addr.nl_family = libc::AF_NETLINK as libc::sa_family_t;
    addr.nl_groups = groups;
    // SAFETY: `addr` is a `sockaddr_nl` of the length passed.
    let r = unsafe {
        libc::bind(
            owned.as_raw_fd(),
            &addr as *const libc::sockaddr_nl as *const libc::sockaddr,
            std::mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t,
        )
    };
    if r < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(owned)
}

/// Receive one datagram (non-blocking: `WouldBlock` when none is queued).
pub fn recv(fd: &OwnedFd, buf: &mut [u8]) -> io::Result<usize> {
    // SAFETY: `buf` is valid for `buf.len()` bytes.
    let n = unsafe { libc::recv(fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len(), 0) };
    if n < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(n as usize)
    }
}

/// `I2C_SLAVE`: the address of the following `I2C_SMBUS` transfers.
const I2C_SLAVE: libc::c_ulong = 0x0703;
/// `I2C_FUNCS`: the adapter's functionality mask (`unsigned long`).
const I2C_FUNCS: libc::c_ulong = 0x0705;
/// `I2C_RDWR`: combined I2C messages.
const I2C_RDWR: libc::c_ulong = 0x0707;
/// `I2C_SMBUS`: one SMBus transfer.
const I2C_SMBUS: libc::c_ulong = 0x0720;
/// `I2C_M_RD`.
const I2C_M_RD: u16 = 0x0001;

/// `struct i2c_smbus_ioctl_data`.
#[repr(C)]
struct I2cSmbusIoctlData {
    read_write: u8,
    command: u8,
    size: u32,
    data: *mut crate::i2c::SmbusData,
}

/// `struct i2c_msg`.
#[repr(C)]
struct I2cMsg {
    addr: u16,
    flags: u16,
    len: u16,
    buf: *mut u8,
}

/// `struct i2c_rdwr_ioctl_data`.
#[repr(C)]
struct I2cRdwrIoctlData {
    msgs: *mut I2cMsg,
    nmsgs: u32,
}

fn check(r: libc::c_int) -> io::Result<()> {
    if r < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// `I2C_FUNCS`.
pub fn i2c_funcs(file: &File) -> io::Result<u32> {
    let mut funcs: libc::c_ulong = 0;
    // SAFETY: the kernel writes one `unsigned long` into `funcs`.
    check(unsafe { libc::ioctl(file.as_raw_fd(), I2C_FUNCS as _, &mut funcs as *mut libc::c_ulong) })?;
    Ok(funcs as u32)
}

/// `I2C_SLAVE` (never `I2C_SLAVE_FORCE`): `EBUSY` when a kernel driver owns `addr`.
pub fn i2c_slave(file: &File, addr: u8) -> io::Result<()> {
    // SAFETY: the argument is passed by value.
    check(unsafe { libc::ioctl(file.as_raw_fd(), I2C_SLAVE as _, libc::c_ulong::from(addr)) })
}

/// `I2C_SMBUS`: `read_write` 1 read / 0 write, `size` an `I2C_SMBUS_*` size.
pub fn i2c_smbus(
    file: &File,
    read_write: u8,
    command: u8,
    size: u32,
    data: &mut crate::i2c::SmbusData,
) -> io::Result<()> {
    let mut args = I2cSmbusIoctlData {
        read_write,
        command,
        size,
        data,
    };
    // SAFETY: `args.data` points at a full `union i2c_smbus_data` (34 bytes,
    // 2-aligned), the most the kernel copies in either direction.
    check(unsafe { libc::ioctl(file.as_raw_fd(), I2C_SMBUS as _, &mut args as *mut I2cSmbusIoctlData) })
}

/// `I2C_RDWR` at `addr`: `write` then (repeated start) `read`; either may be
/// empty, not both. Each is at most 8192 bytes (callers send at most 32).
pub fn i2c_rdwr(file: &File, addr: u8, write: &[u8], read: &mut [u8]) -> io::Result<()> {
    if (write.is_empty() && read.is_empty()) || write.len() > 8192 || read.len() > 8192 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "I2C transfer length out of range",
        ));
    }
    let mut msgs = Vec::with_capacity(2);
    if !write.is_empty() {
        msgs.push(I2cMsg {
            addr: u16::from(addr),
            flags: 0,
            len: write.len() as u16,
            // The kernel only reads from a message without I2C_M_RD.
            buf: write.as_ptr() as *mut u8,
        });
    }
    if !read.is_empty() {
        msgs.push(I2cMsg {
            addr: u16::from(addr),
            flags: I2C_M_RD,
            len: read.len() as u16,
            buf: read.as_mut_ptr(),
        });
    }
    let mut args = I2cRdwrIoctlData {
        msgs: msgs.as_mut_ptr(),
        nmsgs: msgs.len() as u32,
    };
    // SAFETY: every message's `buf` is valid for its `len` bytes (written
    // only for `I2C_M_RD`), and `msgs` outlives the call.
    check(unsafe { libc::ioctl(file.as_raw_fd(), I2C_RDWR as _, &mut args as *mut I2cRdwrIoctlData) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ioctl_numbers_match_the_uapi_header() {
        assert_eq!(HIDIOCGRDESCSIZE, 0x8004_4801);
        assert_eq!(HIDIOCGRDESC, 0x9004_4802);
        assert_eq!(ioc(IOC_READ | IOC_WRITE, NR_SFEATURE, 0), 0xC000_4806);
        assert_eq!(ioc(IOC_READ | IOC_WRITE, NR_GFEATURE, 65), 0xC041_4807);
        assert_eq!(ioc(IOC_READ | IOC_WRITE, NR_GINPUT, 0), 0xC000_480A);
    }
}
