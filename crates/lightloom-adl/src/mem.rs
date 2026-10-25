// SPDX-License-Identifier: GPL-3.0-or-later
//! The memory callback ADL allocates its results with.
//!
//! `ADL2_Main_Control_Create` takes an `ADL_MAIN_MALLOC_CALLBACK`
//! (`void* __stdcall (int)`); functions that return arrays, such as
//! `ADL2_Adapter_AdapterInfoX4_Get`, allocate them through it and leave them to
//! the caller. [`adl_alloc`] puts the allocation size in a 16-byte header in
//! front of the block so [`adl_free`] can hand it back to Rust's allocator.

use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ffi::c_void;

/// Bytes in front of every block: the total size, padded to the alignment.
const HEADER: usize = 16;
/// Alignment of every block (enough for any ADL structure).
const ALIGN: usize = 16;

/// `ADL_MAIN_MALLOC_CALLBACK`: `size` zeroed bytes, or null when `size` is not
/// positive or memory runs out. Never unwinds.
pub extern "system" fn adl_alloc(size: i32) -> *mut c_void {
    let Ok(size) = usize::try_from(size) else {
        return std::ptr::null_mut();
    };
    if size == 0 {
        return std::ptr::null_mut();
    }
    let Some(total) = size.checked_add(HEADER) else {
        return std::ptr::null_mut();
    };
    let Ok(layout) = Layout::from_size_align(total, ALIGN) else {
        return std::ptr::null_mut();
    };
    // SAFETY: `layout` has a non-zero size.
    let base = unsafe { alloc_zeroed(layout) };
    if base.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: `base` is valid for `total >= HEADER` bytes and 16-byte aligned,
    // so a `usize` fits at its start; the caller's block starts after the header.
    unsafe {
        base.cast::<usize>().write(total);
        base.add(HEADER).cast()
    }
}

/// Free a block from [`adl_alloc`]. Null is ignored.
///
/// # Safety
///
/// `block` must be null or a pointer [`adl_alloc`] returned and not yet freed.
pub unsafe fn adl_free(block: *mut c_void) {
    if block.is_null() {
        return;
    }
    // SAFETY: the caller guarantees `block` came from `adl_alloc`, which
    // returned `base + HEADER` after writing the total size at `base`.
    unsafe {
        let base = block.cast::<u8>().sub(HEADER);
        let total = base.cast::<usize>().read();
        dealloc(base, Layout::from_size_align_unchecked(total, ALIGN));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_are_zeroed_aligned_and_freed() {
        let p = adl_alloc(1580 * 3);
        assert!(!p.is_null());
        assert_eq!(p as usize % ALIGN, 0);
        // SAFETY: a fresh block of 4740 bytes.
        let bytes = unsafe { std::slice::from_raw_parts_mut(p.cast::<u8>(), 1580 * 3) };
        assert!(bytes.iter().all(|&b| b == 0));
        bytes[4739] = 7;
        // SAFETY: from adl_alloc, freed once.
        unsafe { adl_free(p) };
        // SAFETY: null is ignored.
        unsafe { adl_free(std::ptr::null_mut()) };
    }

    #[test]
    fn nonsense_sizes_give_null() {
        assert!(adl_alloc(0).is_null());
        assert!(adl_alloc(-4).is_null());
    }
}
