// SPDX-License-Identifier: GPL-3.0-or-later
//! Loading `nvapi64.dll` and resolving functions through `nvapi_QueryInterface`.
//!
//! The DLL is loaded from System32 only (`LOAD_LIBRARY_SEARCH_SYSTEM32`), so a
//! planted `nvapi64.dll` next to the executable or in the working directory
//! is never picked up. Signatures follow NVIDIA's MIT headers
//! (`NVAPI_INTERFACE` is `__cdecl`, i.e. `extern "C"`).

use crate::info::NvI2cInfoV3;
use crate::NvapiError;
use std::ffi::c_void;
use windows::core::{s, w};
use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};

/// `void* nvapi_QueryInterface(NvU32 id)`.
pub type QueryInterfaceFn = unsafe extern "C" fn(id: u32) -> *mut c_void;
/// `NvAPI_Initialize()` / `NvAPI_Unload()`.
pub type InitializeFn = unsafe extern "C" fn() -> i32;
/// `NvAPI_EnumPhysicalGPUs(NvPhysicalGpuHandle[64], NvU32*)`.
pub type EnumPhysicalGpusFn = unsafe extern "C" fn(h: *mut NvPhysicalGpuHandle, count: *mut u32) -> i32;
/// `NvAPI_GPU_GetPCIIdentifiers(h, DeviceId*, SubSystemId*, RevisionId*, ExtDeviceId*)`.
pub type GetPciIdentifiersFn = unsafe extern "C" fn(
    h: NvPhysicalGpuHandle,
    dev: *mut u32,
    subsys: *mut u32,
    rev: *mut u32,
    ext: *mut u32,
) -> i32;
/// `NvAPI_GPU_GetFullName(h, char[64])`.
pub type GetFullNameFn = unsafe extern "C" fn(h: NvPhysicalGpuHandle, name: *mut u8) -> i32;
/// `NvAPI_GPU_GetBusId(h, NvU32*)`.
pub type GetBusIdFn = unsafe extern "C" fn(h: NvPhysicalGpuHandle, bus: *mut u32) -> i32;
/// `NvAPI_I2CReadEx` / `NvAPI_I2CWriteEx(h, NV_I2C_INFO_V3*, NvU32* unknown)` (OpenRGB's ids).
pub type I2cExFn =
    unsafe extern "C" fn(h: NvPhysicalGpuHandle, info: *mut NvI2cInfoV3, unknown: *mut u32) -> i32;

/// Most physical GPUs `NvAPI_EnumPhysicalGPUs` reports.
pub const NVAPI_MAX_PHYSICAL_GPUS: usize = 64;
/// `NvAPI_ShortString` length.
pub const NVAPI_SHORT_STRING_MAX: usize = 64;

/// `NvPhysicalGpuHandle`: an opaque pointer, valid only on the thread that enumerated it.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NvPhysicalGpuHandle(pub *mut c_void);

impl NvPhysicalGpuHandle {
    /// The null handle (array filler).
    pub const NULL: Self = Self(std::ptr::null_mut());
}

/// `nvapi64.dll`, loaded from System32 and freed on drop.
pub struct Library {
    module: HMODULE,
    query: QueryInterfaceFn,
}

impl Library {
    /// Load the DLL and find `nvapi_QueryInterface`. Calls no NvAPI function.
    pub fn load() -> Result<Self, NvapiError> {
        // SAFETY: a constant, NUL-terminated wide string; System32-only search
        // flag; no file handle. The returned module is freed in Drop.
        let module = unsafe { LoadLibraryExW(w!("nvapi64.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32) }
            .map_err(|_| NvapiError::NotAvailable)?;
        // SAFETY: `module` is a live module handle and the name is a constant
        // NUL-terminated ANSI string.
        let Some(proc) = (unsafe { GetProcAddress(module, s!("nvapi_QueryInterface")) }) else {
            // SAFETY: the module was loaded above and is not used afterwards.
            let _ = unsafe { FreeLibrary(module) };
            return Err(NvapiError::NotAvailable);
        };
        // SAFETY: `nvapi_QueryInterface` has the documented signature
        // `void* (__cdecl*)(NvU32)`; both are plain function pointers.
        let query =
            unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, QueryInterfaceFn>(proc) };
        Ok(Self { module, query })
    }

    /// Resolve interface `id` as the function pointer type `T`.
    ///
    /// # Safety
    ///
    /// `T` must be the `extern "C"` function pointer type of the NvAPI
    /// function that `id` names.
    pub unsafe fn query<T: Copy>(&self, id: u32) -> Option<T> {
        const { assert!(std::mem::size_of::<T>() == std::mem::size_of::<*mut c_void>()) };
        // SAFETY: `query` is `nvapi_QueryInterface` of the loaded module; any id is accepted
        // (unknown ids return NULL).
        let ptr = unsafe { (self.query)(id) };
        if ptr.is_null() {
            return None;
        }
        // SAFETY: non-null, pointer-sized, and the caller guarantees `T` matches the function.
        Some(unsafe { std::mem::transmute_copy::<*mut c_void, T>(&ptr) })
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: the module was loaded by `load` and every function pointer
        // resolved from it lives in the owner of this library, dropped first.
        let _ = unsafe { FreeLibrary(self.module) };
    }
}
