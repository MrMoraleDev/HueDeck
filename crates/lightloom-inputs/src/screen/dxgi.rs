// SPDX-License-Identifier: GPL-3.0-or-later
//! DXGI Desktop Duplication of one display (docs/research/m6-inputs-network.md §2.3-2.4).
//!
//! * The D3D11 device is created on the adapter the output is connected to
//!   (not the default adapter), as `DuplicateOutput` requires.
//! * Legacy `IDXGIOutput1::DuplicateOutput`: the image is always
//!   `B8G8R8A8_UNORM`; on an HDR desktop Windows converts it to SDR. No
//!   process-wide DPI change is made.
//! * Per new image: `CopyResource` into a CPU-readable staging texture,
//!   `ReleaseFrame` right away, `Map`, hand the mapped rows to the caller
//!   (the CPU reduction in `sample.rs` reads ~256 k pixels of them), `Unmap`.
//! * Pointer-only updates (`LastPresentTime == 0`) are not new images.
//! * Any error but a timeout (`ACCESS_LOST` on a mode change or full-screen
//!   switch, `ACCESS_DENIED` on the secure desktop, …) drops the duplication;
//!   the capture thread reopens it with a backoff.

#![allow(unsafe_code)]

use super::sample::{Frame, Rotation};
use super::{display_matches, DisplayInfo, FrameSource};
use std::time::Duration;
use windows::core::Interface;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_MODE_ROTATION_ROTATE180, DXGI_MODE_ROTATION_ROTATE270,
    DXGI_MODE_ROTATION_ROTATE90, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput, IDXGIOutput1, IDXGIOutputDuplication,
    IDXGIResource, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO, DXGI_OUTPUT_DESC,
};

fn wide(s: &[u16]) -> String {
    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    String::from_utf16_lossy(&s[..end])
}

/// Every output attached to the desktop, with its adapter.
fn outputs() -> windows::core::Result<Vec<(IDXGIAdapter1, IDXGIOutput, DisplayInfo)>> {
    let mut found = Vec::new();
    // SAFETY: plain COM calls; every out-parameter is owned by the callee's
    // returned value.
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut i = 0;
        while let Ok(adapter) = factory.EnumAdapters1(i) {
            i += 1;
            let adapter_name = adapter
                .GetDesc1()
                .map(|d| wide(&d.Description))
                .unwrap_or_default();
            let mut j = 0;
            while let Ok(output) = adapter.EnumOutputs(j) {
                j += 1;
                let desc: DXGI_OUTPUT_DESC = output.GetDesc()?;
                if !desc.AttachedToDesktop.as_bool() {
                    continue;
                }
                let r = desc.DesktopCoordinates;
                found.push((
                    adapter.clone(),
                    output,
                    DisplayInfo {
                        name: wide(&desc.DeviceName),
                        width: (r.right - r.left).unsigned_abs(),
                        height: (r.bottom - r.top).unsigned_abs(),
                        primary: r.left == 0 && r.top == 0,
                        adapter: adapter_name.clone(),
                    },
                ));
            }
        }
    }
    Ok(found)
}

/// The displays that can be duplicated.
pub fn displays() -> Vec<DisplayInfo> {
    match outputs() {
        Ok(v) => v.into_iter().map(|(_, _, d)| d).collect(),
        Err(e) => {
            tracing::warn!("screen: cannot list displays: {e}");
            Vec::new()
        }
    }
}

/// A desktop duplication of one display.
pub struct Duplication {
    info: DisplayInfo,
    context: ID3D11DeviceContext,
    device: ID3D11Device,
    dupl: IDXGIOutputDuplication,
    rotation: Rotation,
    staging: Option<(ID3D11Texture2D, u32, u32)>,
}

// SAFETY: the D3D11 device is free-threaded and the immediate context and
// duplication are used only by the one thread that owns this value.
unsafe impl Send for Duplication {}

/// Releases the acquired frame when dropped.
struct Acquired<'a>(&'a IDXGIOutputDuplication);

impl Drop for Acquired<'_> {
    fn drop(&mut self) {
        // SAFETY: a frame was acquired on this duplication.
        let _ = unsafe { self.0.ReleaseFrame() };
    }
}

impl Duplication {
    /// Duplicate `display` (`None` or empty: the primary display).
    pub fn open(display: Option<&str>) -> Result<Self, String> {
        let all = outputs().map_err(|e| format!("enumerating displays: {e}"))?;
        let wanted = display.map(str::trim).filter(|d| !d.is_empty());
        let pick = match wanted {
            Some(name) => all.iter().find(|(_, _, d)| display_matches(name, &d.name)),
            None => all.iter().find(|(_, _, d)| d.primary).or(all.first()),
        };
        let Some((adapter, output, info)) = pick else {
            let names: Vec<_> = all.iter().map(|(_, _, d)| d.name.as_str()).collect();
            return Err(match wanted {
                Some(name) => format!("no display named {name:?} (displays: {})", names.join(", ")),
                None => "no display is attached to the desktop".into(),
            });
        };
        // SAFETY: COM calls with valid interface pointers; the out-parameters
        // are locals that receive owned references.
        unsafe {
            let mut device = None;
            let mut context = None;
            D3D11CreateDevice(
                adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .map_err(|e| format!("creating a D3D11 device on {}: {e}", info.adapter))?;
            let (device, context) = device
                .zip(context)
                .ok_or("D3D11CreateDevice returned no device")?;
            let output1: IDXGIOutput1 = output.cast().map_err(|e| format!("IDXGIOutput1: {e}"))?;
            let dupl = output1
                .DuplicateOutput(&device)
                .map_err(|e| format!("DuplicateOutput on {}: {e}", info.name))?;
            let rotation = match dupl.GetDesc().Rotation {
                DXGI_MODE_ROTATION_ROTATE90 => Rotation::Rotate90,
                DXGI_MODE_ROTATION_ROTATE180 => Rotation::Rotate180,
                DXGI_MODE_ROTATION_ROTATE270 => Rotation::Rotate270,
                _ => Rotation::Identity,
            };
            Ok(Self {
                info: info.clone(),
                context,
                device,
                dupl,
                rotation,
                staging: None,
            })
        }
    }

    /// A staging texture of `w` x `h` (re-created when the size changes).
    fn staging(&mut self, w: u32, h: u32) -> Result<ID3D11Texture2D, String> {
        if let Some((t, sw, sh)) = &self.staging {
            if (*sw, *sh) == (w, h) {
                return Ok(t.clone());
            }
        }
        let desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut tex = None;
        // SAFETY: `desc` is a valid staging description; `tex` receives an owned reference.
        unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut tex)) }
            .map_err(|e| format!("creating a {w}x{h} staging texture: {e}"))?;
        let tex = tex.ok_or("CreateTexture2D returned no texture")?;
        self.staging = Some((tex.clone(), w, h));
        Ok(tex)
    }
}

impl FrameSource for Duplication {
    fn describe(&self) -> String {
        format!(
            "{} ({}x{}{}) on {}",
            self.info.name,
            self.info.width,
            self.info.height,
            if self.info.primary { ", primary" } else { "" },
            self.info.adapter
        )
    }

    fn capture(&mut self, timeout: Duration, f: &mut dyn FnMut(&Frame<'_>)) -> Result<bool, String> {
        let ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;
        // SAFETY: valid out-pointers to locals.
        match unsafe { self.dupl.AcquireNextFrame(ms, &mut info, &mut resource) } {
            Ok(()) => {}
            Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(false),
            Err(e) => return Err(format!("AcquireNextFrame: {e}")),
        }
        let dupl = self.dupl.clone();
        let acquired = Acquired(&dupl);
        if info.LastPresentTime == 0 {
            return Ok(false);
        }
        let desktop: ID3D11Texture2D = resource
            .ok_or("AcquireNextFrame returned no image")?
            .cast()
            .map_err(|e| format!("desktop image: {e}"))?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: valid out-pointer.
        unsafe { desktop.GetDesc(&mut desc) };
        if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
            return Err(format!("unexpected desktop format {:?}", desc.Format));
        }
        let (w, h) = (desc.Width, desc.Height);
        let staging = self.staging(w, h)?;
        // SAFETY: both textures belong to this device and have the same size and format.
        unsafe { self.context.CopyResource(&staging, &desktop) };
        drop(acquired);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: a staging texture with CPU read access, subresource 0.
        unsafe {
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
        }
        .map_err(|e| format!("Map: {e}"))?;
        let pitch = mapped.RowPitch as usize;
        // SAFETY: a mapped 2D staging texture spans `RowPitch * Height` bytes
        // from `pData` until `Unmap` below; the slice does not outlive it.
        let bytes = unsafe { std::slice::from_raw_parts(mapped.pData as *const u8, pitch * h as usize) };
        let ok = match Frame::new(bytes, w, h, pitch, self.rotation) {
            Some(frame) => {
                f(&frame);
                true
            }
            None => false,
        };
        // SAFETY: mapped above.
        unsafe { self.context.Unmap(&staging, 0) };
        if ok {
            Ok(true)
        } else {
            Err(format!("bad capture dimensions {w}x{h} (pitch {pitch})"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::{reduce, ScreenSettings, ScreenSnapshot};
    use crate::ZoneSnapshot;
    use std::time::Instant;

    /// Manual check on a real desktop (never run by `cargo test`): duplicates
    /// the primary display for ~2 s and prints sizes, frame count and the
    /// reduction time. Nothing of the image is printed or kept.
    /// `cargo test -p lightloom-inputs dxgi -- --ignored --nocapture`
    #[test]
    #[ignore = "captures the real desktop"]
    fn duplicate_the_primary_display() {
        println!("displays: {:?}", displays());
        let mut d = Duplication::open(None).expect("open");
        println!("capturing {}", d.describe());
        let settings = ScreenSettings {
            fps: 30,
            zone: true,
            meters: Vec::new(),
        };
        let mut snap = ScreenSnapshot {
            zone: Some(ZoneSnapshot::default()),
            vision: Vec::new(),
        };
        let (mut frames, mut size, mut cpu) = (0, (0, 0), Duration::ZERO);
        let end = Instant::now() + Duration::from_secs(2);
        while Instant::now() < end {
            let got = d
                .capture(Duration::from_millis(100), &mut |f| {
                    let t = Instant::now();
                    reduce(f, &settings, &mut snap);
                    cpu += t.elapsed();
                    size = f.display_size();
                })
                .expect("capture");
            frames += usize::from(got);
        }
        println!(
            "{frames} new images of {size:?}; reduction {:?} per image",
            cpu / frames.max(1) as u32
        );
    }
}
