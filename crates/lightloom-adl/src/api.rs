// SPDX-License-Identifier: GPL-3.0-or-later
//! The ADL calls Lightloom makes: create a context, list the adapters, and one
//! I2C transfer. Lives on the hub's worker thread only ([`crate::hub`]); the
//! context and the adapter indexes never leave it.

use crate::ffi::{AdlContext, Library};
use crate::hub::{locate, GpuInfo, I2cTransport};
use crate::info::{fill_i2c, select_gpus, AdapterInfoX2, I2cPlan};
use crate::mem::{adl_alloc, adl_free};
use crate::{status, AdlError};

/// One enumerated AMD GPU.
struct AdlGpu {
    /// The ADL adapter index transfers go to.
    index: i32,
    info: GpuInfo,
}

/// Loaded ADL with a live context. Dropping it destroys the context, then frees the DLL.
pub struct Adl {
    lib: Library,
    context: AdlContext,
    gpus: Vec<AdlGpu>,
}

fn check(code: i32) -> Result<(), AdlError> {
    if status::is_ok(code) {
        Ok(())
    } else {
        Err(AdlError::Status(code))
    }
}

impl Adl {
    /// Load `atiadlxx.dll`, create a context (connected adapters only, as
    /// OpenRGB does) and enumerate the AMD GPUs.
    pub fn load() -> Result<Self, AdlError> {
        let lib = Library::load()?;
        let context = Self::create(&lib)?;
        let mut adl = Self {
            lib,
            context,
            gpus: Vec::new(),
        };
        adl.enumerate()?;
        Ok(adl)
    }

    fn create(lib: &Library) -> Result<AdlContext, AdlError> {
        let mut context: AdlContext = std::ptr::null_mut();
        // SAFETY: `adl_alloc` has the callback signature; `context` is a valid
        // out pointer. Paired with `destroy` in `reinit` and Drop.
        check(unsafe { (lib.create)(adl_alloc, 1, &mut context) })?;
        if context.is_null() {
            return Err(AdlError::Status(status::ADL_ERR_NOT_INIT));
        }
        Ok(context)
    }

    fn enumerate(&mut self) -> Result<(), AdlError> {
        let mut count = 0i32;
        let mut list: *mut AdapterInfoX2 = std::ptr::null_mut();
        // SAFETY: a live context; index -1 asks for every adapter; two valid
        // out pointers. ADL allocates `list` through `adl_alloc`.
        let code = unsafe { (self.lib.adapter_info)(self.context, -1, &mut count, &mut list) };
        let records = if list.is_null() || count <= 0 {
            Vec::new()
        } else {
            // SAFETY: ADL filled `count` consecutive records at `list`.
            let slice = unsafe { std::slice::from_raw_parts(list, count as usize) };
            slice.iter().map(AdapterInfoX2::record).collect()
        };
        // SAFETY: `list` is null or the block ADL allocated with `adl_alloc`;
        // nothing refers to it any more (the records own their strings).
        unsafe { adl_free(list.cast()) };
        check(code)?;
        self.gpus = select_gpus(&records)
            .into_iter()
            .map(|(index, name, pci, bus)| AdlGpu {
                index,
                info: GpuInfo { name, pci, bus },
            })
            .collect();
        Ok(())
    }

    /// A new context and a new enumeration (after a lost context or a stale
    /// adapter index). The old adapters are dropped first: when any step fails
    /// no GPU is left to address until a later re-initialisation succeeds.
    fn reinit(&mut self) -> Result<(), AdlError> {
        self.gpus.clear();
        // SAFETY: the context was created by `create` and is not used again.
        unsafe { (self.lib.destroy)(self.context) };
        self.context = std::ptr::null_mut();
        self.context = Self::create(&self.lib)?;
        self.enumerate()
    }

    /// One transfer to card `gpu`, looked up among the GPUs enumerated now.
    fn transfer_once(&self, gpu: &GpuInfo, addr: u8, plan: &I2cPlan, out: &mut [u8]) -> Result<(), AdlError> {
        let infos: Vec<GpuInfo> = self.gpus.iter().map(|g| g.info.clone()).collect();
        let index = self.gpus[locate(&infos, gpu)?].index;
        // One spare byte so a zero-length write (Quick) still passes a valid pointer.
        let mut data = plan.data.clone();
        data.push(0);
        let mut i2c = fill_i2c(plan, addr);
        i2c.data = data.as_mut_ptr();
        // SAFETY: a live context and an adapter index from the current
        // enumeration; `i2c` describes `data` (`data_size` bytes, at most its
        // length), which outlives the call.
        check(unsafe { (self.lib.write_and_read_i2c)(self.context, index, &mut i2c) })?;
        if !plan.write {
            let n = out.len().min(plan.data.len());
            out[..n].copy_from_slice(&data[..n]);
        }
        Ok(())
    }
}

impl I2cTransport for Adl {
    fn gpus(&mut self) -> Result<Vec<GpuInfo>, AdlError> {
        Ok(self.gpus.iter().map(|g| g.info.clone()).collect())
    }

    fn transfer(&mut self, gpu: &GpuInfo, addr: u8, plan: &I2cPlan, out: &mut [u8]) -> Result<(), AdlError> {
        if self.context.is_null() || self.gpus.is_empty() {
            // A failed re-initialisation left nothing to address: try again first.
            self.reinit()?;
        }
        match self.transfer_once(gpu, addr, plan, out) {
            Err(AdlError::Status(s)) if status::needs_reinit(s) => {
                // Driver reset or update: new context, re-enumerate and retry
                // once; the card is looked up again by bus and PCI ids.
                tracing::info!(status = s, "ADL asked for re-initialisation; retrying once");
                self.reinit()?;
                self.transfer_once(gpu, addr, plan, out)
            }
            other => other,
        }
    }
}

impl Drop for Adl {
    fn drop(&mut self) {
        if !self.context.is_null() {
            // SAFETY: created by `create`; the library is still loaded (dropped after this).
            unsafe { (self.lib.destroy)(self.context) };
        }
    }
}
