// SPDX-License-Identifier: GPL-3.0-or-later
//! Loading `atiadlxx.dll` and resolving the `ADL2_*` functions Lightloom calls.
//!
//! The DLL is loaded from System32 only (`LOAD_LIBRARY_SEARCH_SYSTEM32`), so a
//! planted `atiadlxx.dll` next to the executable or in the working directory
//! is never picked up. Signatures follow AMD's MIT ADL SDK headers; on x64
//! there is one calling convention, so `extern "C"` and `extern "system"`
//! are the same.

use crate::info::{AdapterInfoX2, AdlI2c};
use crate::AdlError;
use std::ffi::c_void;
use windows::core::{s, w, PCSTR};
use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};

/// `ADL_CONTEXT_HANDLE`.
pub type AdlContext = *mut c_void;
/// `ADL_MAIN_MALLOC_CALLBACK`.
pub type MallocCallback = extern "system" fn(size: i32) -> *mut c_void;
/// `ADL2_Main_Control_Create(callback, iEnumConnectedAdapters, ADL_CONTEXT_HANDLE*)`.
pub type MainControlCreateFn =
    unsafe extern "C" fn(callback: MallocCallback, connected_only: i32, context: *mut AdlContext) -> i32;
/// `ADL2_Main_Control_Destroy(ADL_CONTEXT_HANDLE)`.
pub type MainControlDestroyFn = unsafe extern "C" fn(context: AdlContext) -> i32;
/// `ADL2_Adapter_AdapterInfoX4_Get(context, iAdapterIndex, int* numAdapters, AdapterInfoX2**)`.
pub type AdapterInfoX4GetFn = unsafe extern "C" fn(
    context: AdlContext,
    adapter_index: i32,
    count: *mut i32,
    info: *mut *mut AdapterInfoX2,
) -> i32;
/// `ADL2_Display_WriteAndReadI2C(context, iAdapterIndex, ADLI2C*)`.
pub type WriteAndReadI2cFn =
    unsafe extern "C" fn(context: AdlContext, adapter_index: i32, i2c: *mut AdlI2c) -> i32;

/// `atiadlxx.dll`, loaded from System32 with its functions resolved; freed on drop.
pub struct Library {
    module: HMODULE,
    pub create: MainControlCreateFn,
    pub destroy: MainControlDestroyFn,
    pub adapter_info: AdapterInfoX4GetFn,
    pub write_and_read_i2c: WriteAndReadI2cFn,
}

/// Resolve `name` from `module` as the function pointer type `T`.
///
/// # Safety
///
/// `module` must be a live module handle, and `T` the `extern "C"` function
/// pointer type of the export `name`.
unsafe fn resolve<T: Copy>(module: HMODULE, name: PCSTR, label: &'static str) -> Result<T, AdlError> {
    const { assert!(std::mem::size_of::<T>() == std::mem::size_of::<*mut c_void>()) };
    // SAFETY: the caller guarantees `module` is live; `name` is a constant NUL-terminated string.
    let proc = unsafe { GetProcAddress(module, name) }.ok_or(AdlError::MissingFunction(label))?;
    // SAFETY: non-null and pointer-sized; the caller guarantees `T` matches the export.
    Ok(unsafe { std::mem::transmute_copy::<unsafe extern "system" fn() -> isize, T>(&proc) })
}

impl Library {
    /// Load the DLL and resolve the four functions. Calls no ADL function.
    pub fn load() -> Result<Self, AdlError> {
        // SAFETY: a constant, NUL-terminated wide string; System32-only search
        // flag; no file handle. The returned module is freed in Drop or below.
        let module = unsafe { LoadLibraryExW(w!("atiadlxx.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32) }
            .map_err(|_| AdlError::NotAvailable)?;
        // SAFETY: `module` was just loaded; each name is paired with the type
        // AMD's headers give that function.
        let resolved = unsafe {
            (|| -> Result<_, AdlError> {
                Ok((
                    resolve::<MainControlCreateFn>(
                        module,
                        s!("ADL2_Main_Control_Create"),
                        "ADL2_Main_Control_Create",
                    )?,
                    resolve::<MainControlDestroyFn>(
                        module,
                        s!("ADL2_Main_Control_Destroy"),
                        "ADL2_Main_Control_Destroy",
                    )?,
                    resolve::<AdapterInfoX4GetFn>(
                        module,
                        s!("ADL2_Adapter_AdapterInfoX4_Get"),
                        "ADL2_Adapter_AdapterInfoX4_Get",
                    )?,
                    resolve::<WriteAndReadI2cFn>(
                        module,
                        s!("ADL2_Display_WriteAndReadI2C"),
                        "ADL2_Display_WriteAndReadI2C",
                    )?,
                ))
            })()
        };
        match resolved {
            Ok((create, destroy, adapter_info, write_and_read_i2c)) => Ok(Self {
                module,
                create,
                destroy,
                adapter_info,
                write_and_read_i2c,
            }),
            Err(e) => {
                // SAFETY: loaded above and not used afterwards.
                let _ = unsafe { FreeLibrary(module) };
                Err(e)
            }
        }
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: the module was loaded by `load`; its owner destroys the ADL
        // context before dropping the library.
        let _ = unsafe { FreeLibrary(self.module) };
    }
}
