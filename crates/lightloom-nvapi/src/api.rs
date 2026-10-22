// SPDX-License-Identifier: GPL-3.0-or-later
//! The NvAPI calls Lightloom makes: initialise, enumerate GPUs with their PCI
//! ids, names and bus numbers, and one I2C transfer. Lives on the hub's worker
//! thread only ([`crate::hub`]); GPU handles never leave it.

use crate::ffi::{
    EnumPhysicalGpusFn, GetBusIdFn, GetFullNameFn, GetPciIdentifiersFn, I2cExFn, InitializeFn, Library,
    NvPhysicalGpuHandle, NVAPI_MAX_PHYSICAL_GPUS, NVAPI_SHORT_STRING_MAX,
};
use crate::hub::{decode_pci, locate, GpuInfo, I2cTransport};
use crate::info::{fill_info, I2cPlan};
use crate::{ids, status, NvapiError};

/// One enumerated GPU.
struct NvGpu {
    handle: NvPhysicalGpuHandle,
    info: GpuInfo,
}

/// Loaded and initialised NvAPI. Dropping it calls `NvAPI_Unload`, then frees the DLL.
pub struct NvApi {
    unload: InitializeFn,
    init: InitializeFn,
    enum_gpus: EnumPhysicalGpusFn,
    full_name: GetFullNameFn,
    pci_ids: GetPciIdentifiersFn,
    bus_id: GetBusIdFn,
    read_ex: I2cExFn,
    write_ex: I2cExFn,
    gpus: Vec<NvGpu>,
    // Dropped last: every function pointer above points into it.
    _lib: Library,
}

fn check(status: i32) -> Result<(), NvapiError> {
    if status == 0 {
        Ok(())
    } else {
        Err(NvapiError::Status(status))
    }
}

impl NvApi {
    /// Load `nvapi64.dll`, resolve every function and call `NvAPI_Initialize`.
    pub fn load() -> Result<Self, NvapiError> {
        let lib = Library::load()?;
        macro_rules! resolve {
            ($id:expr, $ty:ty) => {
                // SAFETY: each id is paired with the signature NVIDIA's headers
                // (or OpenRGB, for the Ex functions) give it.
                unsafe { lib.query::<$ty>($id) }.ok_or(NvapiError::MissingFunction($id))?
            };
        }
        let init = resolve!(ids::INITIALIZE, InitializeFn);
        let unload = resolve!(ids::UNLOAD, InitializeFn);
        let enum_gpus = resolve!(ids::ENUM_PHYSICAL_GPUS, EnumPhysicalGpusFn);
        let full_name = resolve!(ids::GPU_GET_FULL_NAME, GetFullNameFn);
        let pci_ids = resolve!(ids::GPU_GET_PCI_IDENTIFIERS, GetPciIdentifiersFn);
        let bus_id = resolve!(ids::GPU_GET_BUS_ID, GetBusIdFn);
        let read_ex = resolve!(ids::I2C_READ_EX, I2cExFn);
        let write_ex = resolve!(ids::I2C_WRITE_EX, I2cExFn);
        // SAFETY: resolved above; takes no arguments. Paired with Unload in Drop.
        check(unsafe { init() })?;
        let mut api = Self {
            unload,
            init,
            enum_gpus,
            full_name,
            pci_ids,
            bus_id,
            read_ex,
            write_ex,
            gpus: Vec::new(),
            _lib: lib,
        };
        api.enumerate()?;
        Ok(api)
    }

    fn enumerate(&mut self) -> Result<(), NvapiError> {
        let mut handles = [NvPhysicalGpuHandle::NULL; NVAPI_MAX_PHYSICAL_GPUS];
        let mut count = 0u32;
        // SAFETY: the array has the 64 entries the API fills at most; `count` is a valid out pointer.
        check(unsafe { (self.enum_gpus)(handles.as_mut_ptr(), &mut count) })?;
        let mut gpus = Vec::new();
        for &handle in handles.iter().take((count as usize).min(NVAPI_MAX_PHYSICAL_GPUS)) {
            let (mut dev, mut subsys, mut rev, mut ext) = (0u32, 0u32, 0u32, 0u32);
            // SAFETY: a handle from the enumeration above; four valid out pointers.
            check(unsafe { (self.pci_ids)(handle, &mut dev, &mut subsys, &mut rev, &mut ext) })?;
            let mut name = [0u8; NVAPI_SHORT_STRING_MAX];
            // SAFETY: a 64-byte buffer, the size of NvAPI_ShortString.
            let name = match unsafe { (self.full_name)(handle, name.as_mut_ptr()) } {
                0 => {
                    let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
                    String::from_utf8_lossy(&name[..end]).trim().to_string()
                }
                _ => "NVIDIA GPU".to_string(),
            };
            let mut bus = 0u32;
            // SAFETY: a valid handle and out pointer. A failure leaves bus 0.
            let _ = unsafe { (self.bus_id)(handle, &mut bus) };
            gpus.push(NvGpu {
                handle,
                info: GpuInfo {
                    name,
                    pci: decode_pci(dev, subsys),
                    bus,
                },
            });
        }
        self.gpus = gpus;
        Ok(())
    }

    /// `NvAPI_Unload` + `NvAPI_Initialize` + enumerate (after −4 / −10). The
    /// old handles are dropped first: when any step fails no GPU is left to
    /// address until a later re-initialisation succeeds.
    fn reinit(&mut self) -> Result<(), NvapiError> {
        self.gpus.clear();
        // SAFETY: resolved functions without arguments, called in pairs.
        unsafe { (self.unload)() };
        // SAFETY: as above.
        check(unsafe { (self.init)() })?;
        self.enumerate()
    }

    /// One transfer to card `gpu`, looked up among the GPUs enumerated now.
    fn transfer_once(
        &self,
        gpu: &GpuInfo,
        addr: u8,
        plan: &I2cPlan,
        out: &mut [u8],
    ) -> Result<(), NvapiError> {
        let infos: Vec<GpuInfo> = self.gpus.iter().map(|g| g.info.clone()).collect();
        let handle = self.gpus[locate(&infos, gpu)?].handle;
        let mut reg = [plan.reg.unwrap_or(0)];
        let mut data = plan.data.clone();
        let mut info = fill_info(plan, addr);
        if plan.reg.is_some() {
            info.reg_address = reg.as_mut_ptr();
        }
        info.data = data.as_mut_ptr();
        let mut unknown = 0u32;
        let f = if plan.write { self.write_ex } else { self.read_ex };
        // SAFETY: `info` describes `reg` (1 byte or none) and `data` (`size`
        // bytes); both buffers and `unknown` outlive the call.
        check(unsafe { f(handle, &mut info, &mut unknown) })?;
        if !plan.write {
            let n = out.len().min(data.len());
            out[..n].copy_from_slice(&data[..n]);
        }
        Ok(())
    }
}

impl I2cTransport for NvApi {
    fn gpus(&mut self) -> Result<Vec<GpuInfo>, NvapiError> {
        Ok(self.gpus.iter().map(|g| g.info.clone()).collect())
    }

    fn transfer(
        &mut self,
        gpu: &GpuInfo,
        addr: u8,
        plan: &I2cPlan,
        out: &mut [u8],
    ) -> Result<(), NvapiError> {
        if self.gpus.is_empty() {
            // A failed re-initialisation left no handle: try again first.
            self.reinit()?;
        }
        match self.transfer_once(gpu, addr, plan, out) {
            Err(NvapiError::Status(s)) if status::needs_reinit(s) => {
                // Research §4 step 6: driver resume or reset. Re-initialise and
                // retry once; the card is looked up again by bus and PCI ids.
                tracing::info!(status = s, "NvAPI asked for re-initialisation; retrying once");
                self.reinit()?;
                self.transfer_once(gpu, addr, plan, out)
            }
            other => other,
        }
    }
}

impl Drop for NvApi {
    fn drop(&mut self) {
        // SAFETY: paired with the Initialize in `load`; the library is still loaded.
        unsafe { (self.unload)() };
    }
}
