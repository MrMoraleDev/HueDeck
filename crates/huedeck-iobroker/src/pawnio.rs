// SPDX-License-Identifier: GPL-3.0-or-later
//! PawnIO over raw IOCTLs (research m5-privileged-buses.md §1.2, §2).
//!
//! `\\?\GLOBALROOT\Device\PawnIO` is the one device path that works on
//! PawnIO 2.0, 2.1 and 2.2; only SYSTEM and elevated administrators can open
//! it. One module per handle: LOAD takes the signed blob (the driver checks
//! namazso's signature on the official edition), EXECUTE takes a 32-byte
//! NUL-padded `ioctl_*` function name followed by u64 input cells and fills
//! u64 output cells. `NtDeviceIoControlFile` returns the module's exact
//! NTSTATUS, which [`crate::piix4::map_status`] turns into a broker error.
//!
//! PawnIOLib.dll is not used (research §0.1): the driver's licence exception
//! covers programs talking to it solely through the IOCTL interface.

/// Device path (2.0, 2.1 and 2.2).
pub const PAWNIO_PATH: &str = r"\\?\GLOBALROOT\Device\PawnIO";
/// `CTL_CODE(0xA1B2, 0x821, METHOD_BUFFERED, FILE_ANY_ACCESS)`: load a signed module.
pub const IOCTL_PIO_LOAD_BINARY: u32 = 0xA1B2_2084;
/// `CTL_CODE(0xA1B2, 0x841, …)`: run a module function.
pub const IOCTL_PIO_EXECUTE_FN: u32 = 0xA1B2_2104;
/// `CTL_CODE(0xA1B2, 0x861, …)`: driver version, 2.1.0 and later (output exactly 4 bytes).
pub const IOCTL_PIO_VERSION: u32 = 0xA1B2_2184;
/// Oldest driver HueDeck runs on: 2.2.0 (`maj<<16 | min<<8 | patch`).
pub const MIN_VERSION: u32 = 0x0002_0200;
/// Version reported for a 2.0.x driver (it has no version IOCTL).
pub const VERSION_2_0: u32 = 0x0002_0000;
/// Length of the function-name field of an EXECUTE request.
pub const FN_NAME_LEN: usize = 32;
/// Longest function name (PawnIOLib 2.2.0 copies at most 30 characters plus NUL).
pub const FN_NAME_MAX: usize = 30;
/// Most cells HueDeck ever passes either way.
pub const MAX_CELLS: usize = 16;

/// An NTSTATUS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NtStatus(pub i32);

macro_rules! statuses {
    ($($name:ident = $value:literal),* $(,)?) => {
        impl NtStatus {
            $(
                #[doc = concat!("`STATUS_", stringify!($name), "`.")]
                pub const $name: NtStatus = NtStatus($value as u32 as i32);
            )*

            /// The `STATUS_*` name of a known status.
            pub fn name(self) -> &'static str {
                match self {
                    $(NtStatus::$name => stringify!($name),)*
                    _ => "UNKNOWN",
                }
            }
        }
    };
}

// Values from the Windows SDK ntstatus.h (research §1.6, §6.3).
statuses! {
    SUCCESS = 0x0000_0000,
    DEVICE_BUSY = 0x8000_0011,
    NOT_IMPLEMENTED = 0xC000_0002,
    INVALID_PARAMETER = 0xC000_000D,
    NO_SUCH_DEVICE = 0xC000_000E,
    ACCESS_DENIED = 0xC000_0022,
    BUFFER_TOO_SMALL = 0xC000_0023,
    OBJECT_NAME_NOT_FOUND = 0xC000_0034,
    OBJECT_PATH_NOT_FOUND = 0xC000_003A,
    DEVICE_NOT_READY = 0xC000_00A3,
    IO_TIMEOUT = 0xC000_00B5,
    NOT_SUPPORTED = 0xC000_00BB,
    IO_DEVICE_ERROR = 0xC000_0185,
    DEVICE_PROTOCOL_ERROR = 0xC000_0186,
    RETRY = 0xC000_022D,
    ALREADY_INITIALIZED = 0xC000_0510,
    INVALID_SIGNATURE = 0xC000_A000,
}

impl NtStatus {
    /// `NT_SUCCESS`: success and informational codes (warnings such as
    /// `STATUS_DEVICE_BUSY` are failures).
    pub fn is_success(self) -> bool {
        self.0 >= 0
    }
}

impl std::fmt::Display for NtStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{:08X} STATUS_{}", self.0 as u32, self.name())
    }
}

/// An EXECUTE input buffer: the NUL-padded 32-byte `name`, then `input` as
/// little-endian u64 cells. Names must start with `ioctl_`, be ASCII without
/// NUL and at most [`FN_NAME_MAX`] long (the driver answers
/// `STATUS_INVALID_PARAMETER` otherwise; refused here first).
pub fn encode_execute(name: &str, input: &[u64]) -> Result<Vec<u8>, NtStatus> {
    if !name.starts_with("ioctl_")
        || name.len() > FN_NAME_MAX
        || !name.bytes().all(|b| b.is_ascii_graphic())
        || input.len() > MAX_CELLS
    {
        return Err(NtStatus::INVALID_PARAMETER);
    }
    let mut buf = vec![0u8; FN_NAME_LEN + input.len() * 8];
    buf[..name.len()].copy_from_slice(name.as_bytes());
    for (i, cell) in input.iter().enumerate() {
        let at = FN_NAME_LEN + i * 8;
        buf[at..at + 8].copy_from_slice(&cell.to_le_bytes());
    }
    Ok(buf)
}

/// The first `n` little-endian u64 cells of an output buffer (missing bytes read as 0).
pub fn decode_cells(buf: &[u8], n: usize) -> Vec<u64> {
    (0..n)
        .map(|i| {
            let mut cell = [0u8; 8];
            for (j, b) in cell.iter_mut().enumerate() {
                *b = buf.get(i * 8 + j).copied().unwrap_or(0);
            }
            u64::from_le_bytes(cell)
        })
        .collect()
}

/// The version IOCTL's 4-byte answer; anything else is refused (research §1.2).
pub fn decode_version(out: &[u8]) -> Result<u32, NtStatus> {
    let bytes: [u8; 4] = out.try_into().map_err(|_| NtStatus::BUFFER_TOO_SMALL)?;
    Ok(u32::from_le_bytes(bytes))
}

/// `2.2.0` style text of a driver version.
pub fn version_text(v: u32) -> String {
    format!("{}.{}.{}", v >> 16, (v >> 8) & 0xFF, v & 0xFF)
}

/// Something that runs PawnIO module functions: a loaded module, or a fake.
pub trait PawnExec {
    /// Run `ioctl_<…>` `function` with `input` cells, filling `output`
    /// (zero-initialised by the caller). The module's NTSTATUS on failure.
    fn execute(&mut self, function: &str, input: &[u64], output: &mut [u64]) -> Result<(), NtStatus>;
}

#[cfg(windows)]
pub use device::{code_integrity_options, PawnIoDevice, PawnIoModule};

#[cfg(windows)]
mod device {
    use super::*;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows::core::PCWSTR;
    use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SystemCodeIntegrityInformation};
    use windows::Wdk::System::IO::NtDeviceIoControlFile;
    use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::IO_STATUS_BLOCK;

    /// An open PawnIO handle without a module yet.
    #[derive(Debug)]
    pub struct PawnIoDevice(OwnedHandle);

    /// A handle with one loaded module. Dropping it closes the handle, which
    /// runs the module's `unload()`.
    #[derive(Debug)]
    pub struct PawnIoModule {
        handle: OwnedHandle,
        name: &'static str,
    }

    fn raw(h: &OwnedHandle) -> HANDLE {
        HANDLE(h.as_raw_handle())
    }

    /// One synchronous IOCTL; the driver's NTSTATUS on failure.
    fn ioctl(h: &OwnedHandle, code: u32, input: &[u8], output: &mut [u8]) -> Result<(), NtStatus> {
        let mut iosb = IO_STATUS_BLOCK::default();
        let in_len = u32::try_from(input.len()).map_err(|_| NtStatus::INVALID_PARAMETER)?;
        let out_len = u32::try_from(output.len()).map_err(|_| NtStatus::INVALID_PARAMETER)?;
        // SAFETY: `h` is an open synchronous handle (no FILE_FLAG_OVERLAPPED), so
        // the call completes before returning and no APC/event is needed; the
        // input and output buffers are valid for their stated lengths for the
        // whole call, and `iosb` outlives it.
        let status = unsafe {
            NtDeviceIoControlFile(
                raw(h),
                None,
                None,
                None,
                &mut iosb,
                code,
                (!input.is_empty()).then_some(input.as_ptr().cast()),
                in_len,
                (!output.is_empty()).then_some(output.as_mut_ptr().cast()),
                out_len,
            )
        };
        let status = NtStatus(status.0);
        if status.is_success() {
            Ok(())
        } else {
            Err(status)
        }
    }

    /// The Win32 error of a failed `CreateFileW` as an NTSTATUS (other errors
    /// keep their HRESULT value).
    fn open_status(e: &windows::core::Error) -> NtStatus {
        match e.code().0 as u32 {
            0x8007_0002 => NtStatus::OBJECT_NAME_NOT_FOUND,
            0x8007_0003 => NtStatus::OBJECT_PATH_NOT_FOUND,
            0x8007_0005 => NtStatus::ACCESS_DENIED,
            other => NtStatus(other as i32),
        }
    }

    impl PawnIoDevice {
        /// Open the driver (SYSTEM or an elevated administrator only).
        pub fn open() -> Result<Self, NtStatus> {
            let path: Vec<u16> = PAWNIO_PATH.encode_utf16().chain(Some(0)).collect();
            // SAFETY: NUL-terminated path that outlives the call; no security
            // attributes or template handle.
            let h = unsafe {
                CreateFileW(
                    PCWSTR(path.as_ptr()),
                    GENERIC_READ.0 | GENERIC_WRITE.0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    None,
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL,
                    None,
                )
            }
            .map_err(|e| open_status(&e))?;
            // SAFETY: CreateFileW returned a new handle that nothing else owns.
            Ok(Self(unsafe { OwnedHandle::from_raw_handle(h.0) }))
        }

        /// `IOCTL_PIO_VERSION`; a 2.0.x driver answers `STATUS_NOT_IMPLEMENTED`.
        pub fn driver_version(&self) -> Result<u32, NtStatus> {
            let mut out = [0u8; 4];
            ioctl(&self.0, IOCTL_PIO_VERSION, &[], &mut out)?;
            decode_version(&out)
        }

        /// Load `blob` (already checked against its pin by the caller) as the
        /// handle's one module.
        pub fn load(self, name: &'static str, blob: &[u8]) -> Result<PawnIoModule, NtStatus> {
            ioctl(&self.0, IOCTL_PIO_LOAD_BINARY, blob, &mut [])?;
            Ok(PawnIoModule { handle: self.0, name })
        }
    }

    impl PawnIoModule {
        /// The module name (`SmbusPIIX4`, `Echo`).
        pub fn name(&self) -> &'static str {
            self.name
        }
    }

    impl PawnExec for PawnIoModule {
        fn execute(&mut self, function: &str, input: &[u64], output: &mut [u64]) -> Result<(), NtStatus> {
            if output.len() > MAX_CELLS {
                return Err(NtStatus::INVALID_PARAMETER);
            }
            let request = encode_execute(function, input)?;
            let mut out = vec![0u8; output.len() * 8];
            ioctl(&self.handle, IOCTL_PIO_EXECUTE_FN, &request, &mut out)?;
            output.copy_from_slice(&decode_cells(&out, output.len()));
            Ok(())
        }
    }

    /// `SYSTEM_CODEINTEGRITY_INFORMATION` (winternl.h).
    #[repr(C)]
    struct CodeIntegrityInformation {
        length: u32,
        options: u32,
    }

    /// `CodeIntegrityOptions` (`NtQuerySystemInformation(103)`): bit 0x02 is TESTSIGN.
    pub fn code_integrity_options() -> Result<u32, NtStatus> {
        let mut info = CodeIntegrityInformation {
            length: std::mem::size_of::<CodeIntegrityInformation>() as u32,
            options: 0,
        };
        let mut returned = 0u32;
        // SAFETY: `info` is a properly sized and initialised
        // SYSTEM_CODEINTEGRITY_INFORMATION (Length set as the API requires)
        // that outlives the call; `returned` is a valid out pointer.
        let status = unsafe {
            NtQuerySystemInformation(
                SystemCodeIntegrityInformation,
                (&mut info as *mut CodeIntegrityInformation).cast(),
                info.length,
                &mut returned,
            )
        };
        let status = NtStatus(status.0);
        if status.is_success() {
            Ok(info.options)
        } else {
            Err(status)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_buffer_layout() {
        let buf = encode_execute("ioctl_smbus_xfer", &[0x39, 1, 0xA0, 2]).unwrap();
        assert_eq!(buf.len(), 32 + 4 * 8);
        assert_eq!(&buf[..16], b"ioctl_smbus_xfer");
        assert!(buf[16..32].iter().all(|b| *b == 0), "NUL padded");
        assert_eq!(&buf[32..40], &[0x39, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&buf[56..64], &[2, 0, 0, 0, 0, 0, 0, 0]);
        let cells = decode_cells(&buf[32..], 4);
        assert_eq!(cells, vec![0x39, 1, 0xA0, 2]);
        assert_eq!(decode_cells(&[1, 2], 2), vec![0x0201, 0], "short buffers read 0");
    }

    #[test]
    fn name_rules() {
        assert!(encode_execute("ioctl_identity", &[]).is_ok());
        assert_eq!(encode_execute("identity", &[]), Err(NtStatus::INVALID_PARAMETER));
        assert_eq!(
            encode_execute(&format!("ioctl_{}", "x".repeat(25)), &[]),
            Err(NtStatus::INVALID_PARAMETER),
            "31 characters: over PawnIOLib 2.2.0's 30"
        );
        assert!(encode_execute(&format!("ioctl_{}", "x".repeat(24)), &[]).is_ok());
        assert_eq!(
            encode_execute("ioctl_a\0b", &[]),
            Err(NtStatus::INVALID_PARAMETER)
        );
        assert_eq!(
            encode_execute("ioctl_x", &[0; 17]),
            Err(NtStatus::INVALID_PARAMETER)
        );
    }

    #[test]
    fn version_ioctl_needs_exactly_4_bytes() {
        assert_eq!(decode_version(&[0x00, 0x02, 0x02, 0x00]), Ok(MIN_VERSION));
        assert_eq!(decode_version(&[0; 3]), Err(NtStatus::BUFFER_TOO_SMALL));
        assert_eq!(decode_version(&[0; 8]), Err(NtStatus::BUFFER_TOO_SMALL));
        assert_eq!(version_text(MIN_VERSION), "2.2.0");
    }

    #[test]
    fn statuses_have_names_and_success_rule() {
        assert_eq!(NtStatus::NO_SUCH_DEVICE.0 as u32, 0xC000_000E);
        assert_eq!(NtStatus::IO_TIMEOUT.to_string(), "0xC00000B5 STATUS_IO_TIMEOUT");
        assert!(NtStatus::SUCCESS.is_success());
        assert!(!NtStatus::DEVICE_BUSY.is_success(), "warning class is a failure");
        assert!(!NtStatus::NOT_SUPPORTED.is_success());
        assert_eq!(NtStatus(0x1234).name(), "UNKNOWN");
    }
}
