// SPDX-License-Identifier: GPL-3.0-or-later
//! Windows Dynamic Lighting: LampArray devices through WinRT
//! `Windows.Devices.Lights` ([`huedeck_hal::lamparray::LampArrayBackend`]).
//!
//! Discovery: a `DeviceWatcher` on `LampArray.GetDeviceSelector()` (the HID
//! top-level collection 0x59/0x01, so a LampArray's id is its HID interface
//! path) keeps the list of LampArray ids; its handlers only record ids and
//! bump [`LampArrayBackend::generation`] (they run on system threads and
//! must neither block nor panic). [`LampArrayBackend::enumerate`], called on
//! the engine thread, opens new ids with `LampArray.FromIdAsync` and caches
//! their description. Until the watcher has finished its first enumeration,
//! `enumerate` asks `DeviceInformation.FindAllAsync(selector)` instead.
//!
//! Control: Windows hands a LampArray to the foreground app, or to the
//! ambient lighting controller first in Settings > Personalization > Dynamic
//! Lighting > Background light control. Only a process with package identity
//! whose manifest declares the `com.microsoft.windows.lighting` app extension
//! can be an ambient controller (`packaging/msix-identity/`); the daemon,
//! never a foreground app, has no other way in. [`WinLampArray::has_identity`]
//! tells whether this process has identity; each handle follows
//! `LampArray.IsAvailable` through `AvailabilityChanged` (where the OS lacks
//! `ILampArray2`, the handle assumes availability exactly when the process
//! has identity). HueDeck never writes `HKCU\Software\Microsoft\Lighting`.
//!
//! Release: WinRT has no "give back" call; dropping the `LampArray` (and the
//! process ending) returns the device to the next controller or to its
//! autonomous mode.

use crate::probe::container_id;
use huedeck_hal::lamparray::{LampArrayBackend, LampArrayHandle, LampArrayInfo, LampArrayKind, LampInfo};
use huedeck_hal::{HalError, HalResult};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use windows::core::{IInspectable, HSTRING};
use windows::Devices::Enumeration::{DeviceInformation, DeviceInformationUpdate, DeviceWatcher};
use windows::Devices::Lights::LampArray;
use windows::Foundation::TypedEventHandler;
use windows::Win32::Foundation::APPMODEL_ERROR_NO_PACKAGE;
use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
use windows::UI::Color;

/// Whether this process runs with package identity (an identity package,
/// e.g. the sparse HueDeck package, is registered for its executable).
pub fn has_package_identity() -> bool {
    let mut len = 0u32;
    // SAFETY: a length query: valid out pointer, no buffer.
    unsafe { GetCurrentPackageFullName(&mut len, None) != APPMODEL_ERROR_NO_PACKAGE }
}

fn backend_error(what: &str, e: windows::core::Error) -> HalError {
    HalError::Backend(format!("{what}: {e}"))
}

#[derive(Default)]
struct Shared {
    /// LampArray ids the watcher reported, with their display names.
    ids: Mutex<BTreeMap<String, String>>,
    /// The watcher finished its first pass.
    enumerated: AtomicBool,
    generation: AtomicU64,
}

/// The WinRT LampArray backend.
pub struct WinLampArray {
    shared: Arc<Shared>,
    selector: HSTRING,
    watcher: Option<DeviceWatcher>,
    identity: bool,
    /// Opened LampArrays and their descriptions, by id.
    cache: Mutex<BTreeMap<String, (LampArray, LampArrayInfo)>>,
}

impl std::fmt::Debug for WinLampArray {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WinLampArray")
            .field("watching", &self.watcher.is_some())
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl WinLampArray {
    /// Start watching for LampArrays. Fails where `Windows.Devices.Lights`
    /// is missing (Windows 10 before 1809).
    pub fn start() -> HalResult<Self> {
        let selector =
            LampArray::GetDeviceSelector().map_err(|e| backend_error("LampArray.GetDeviceSelector", e))?;
        let shared = Arc::new(Shared::default());
        let watcher = match Self::watch(&selector, &shared) {
            Ok(w) => Some(w),
            Err(e) => {
                tracing::warn!("LampArray: no DeviceWatcher ({e}); enumerating on every rescan");
                None
            }
        };
        Ok(Self {
            shared,
            selector,
            watcher,
            identity: has_package_identity(),
            cache: Mutex::new(BTreeMap::new()),
        })
    }

    fn watch(selector: &HSTRING, shared: &Arc<Shared>) -> windows::core::Result<DeviceWatcher> {
        let watcher = DeviceInformation::CreateWatcherAqsFilter(selector)?;
        // Handlers run on system threads: record and return, never panic.
        let s = Arc::clone(shared);
        watcher.Added(&TypedEventHandler::<DeviceWatcher, DeviceInformation>::new(
            move |_, info| {
                if let Some(info) = info.as_ref() {
                    let id = info.Id()?.to_string();
                    let name = info.Name().map(|n| n.to_string()).unwrap_or_default();
                    lock(&s.ids).insert(id, name);
                    s.generation.fetch_add(1, Ordering::Relaxed);
                }
                Ok(())
            },
        ))?;
        let s = Arc::clone(shared);
        watcher.Removed(&TypedEventHandler::<DeviceWatcher, DeviceInformationUpdate>::new(
            move |_, update| {
                if let Some(update) = update.as_ref() {
                    let id = update.Id()?.to_string();
                    lock(&s.ids).remove(&id);
                    s.generation.fetch_add(1, Ordering::Relaxed);
                }
                Ok(())
            },
        ))?;
        // Without an Updated handler the watcher stops reporting arrivals after its first pass.
        watcher.Updated(&TypedEventHandler::<DeviceWatcher, DeviceInformationUpdate>::new(
            |_, _| Ok(()),
        ))?;
        let s = Arc::clone(shared);
        watcher.EnumerationCompleted(&TypedEventHandler::<DeviceWatcher, IInspectable>::new(
            move |_, _| {
                s.enumerated.store(true, Ordering::Release);
                s.generation.fetch_add(1, Ordering::Relaxed);
                Ok(())
            },
        ))?;
        watcher.Start()?;
        Ok(watcher)
    }

    /// LampArray ids and names present now.
    fn ids(&self) -> HalResult<BTreeMap<String, String>> {
        if self.watcher.is_some() && self.shared.enumerated.load(Ordering::Acquire) {
            return Ok(lock(&self.shared.ids).clone());
        }
        let all = DeviceInformation::FindAllAsyncAqsFilter(&self.selector)
            .and_then(|op| op.join())
            .map_err(|e| backend_error("DeviceInformation.FindAllAsync", e))?;
        let mut out = BTreeMap::new();
        for info in &all {
            if let Ok(id) = info.Id() {
                out.insert(
                    id.to_string(),
                    info.Name().map(|n| n.to_string()).unwrap_or_default(),
                );
            }
        }
        Ok(out)
    }

    fn open_lamparray(id: &str) -> HalResult<LampArray> {
        LampArray::FromIdAsync(&HSTRING::from(id))
            .and_then(|op| op.join())
            // A null result (the device left meanwhile) is `Err(Error::empty())`.
            .map_err(|e| {
                if e.code().is_ok() {
                    HalError::NotFound(id.into())
                } else {
                    backend_error("LampArray.FromIdAsync", e)
                }
            })
    }
}

/// Describe an open LampArray.
fn describe(la: &LampArray, id: &str, name: &str) -> windows::core::Result<LampArrayInfo> {
    let n = la.LampCount()?.max(0);
    let bbox = la.BoundingBox()?;
    let um = |m: f32| (f64::from(m) * 1_000_000.0).round();
    let mut lamps = Vec::with_capacity(n as usize);
    for i in 0..n {
        let lamp = la.GetLampInfo(i)?;
        let p = lamp.Position()?;
        lamps.push(LampInfo {
            index: i as u32,
            position_um: [um(p.X) as i32, um(p.Y) as i32, um(p.Z) as i32],
            // `FixedColor` is `Err` (a null reference) for every lamp that takes colours.
            is_rgb: lamp.FixedColor().is_err(),
        });
    }
    // TimeSpan: 100 ns units.
    let min_update = la.MinUpdateInterval()?.Duration.max(0) / 10;
    Ok(LampArrayInfo {
        id: id.to_string(),
        name: if name.is_empty() {
            "LampArray".into()
        } else {
            name.to_string()
        },
        vid: la.HardwareVendorId()?,
        pid: la.HardwareProductId()?,
        container_id: container_id(id),
        kind: LampArrayKind::from_raw(la.LampArrayKind()?.0 as u32),
        bounding_box_um: [
            um(bbox.X).max(0.0) as u32,
            um(bbox.Y).max(0.0) as u32,
            um(bbox.Z).max(0.0) as u32,
        ],
        min_update_interval_us: u32::try_from(min_update).unwrap_or(u32::MAX),
        lamps,
    })
}

impl LampArrayBackend for WinLampArray {
    fn enumerate(&self) -> HalResult<Vec<LampArrayInfo>> {
        let ids = self.ids()?;
        let mut cache = lock(&self.cache);
        cache.retain(|id, _| ids.contains_key(id));
        let mut out = Vec::new();
        for (id, name) in &ids {
            if let Some((_, info)) = cache.get(id) {
                out.push(info.clone());
                continue;
            }
            let described = Self::open_lamparray(id).and_then(|la| {
                describe(&la, id, name)
                    .map(|i| (la, i))
                    .map_err(|e| backend_error("LampArray", e))
            });
            match described {
                Ok((la, info)) => {
                    out.push(info.clone());
                    cache.insert(id.clone(), (la, info));
                }
                // Left meanwhile, or not ready yet: the next rescan tries again.
                Err(e) => tracing::debug!("LampArray {id}: {e}"),
            }
        }
        Ok(out)
    }

    fn open(&self, id: &str) -> HalResult<Box<dyn LampArrayHandle>> {
        let cached = lock(&self.cache).get(id).cloned();
        let (la, info) = match cached {
            Some(c) => c,
            None => {
                let la = Self::open_lamparray(id)?;
                let info = describe(&la, id, "").map_err(|e| backend_error("LampArray", e))?;
                (la, info)
            }
        };
        Ok(Box::new(WinLampHandle::new(la, info, self.identity)))
    }

    fn generation(&self) -> u64 {
        self.shared.generation.load(Ordering::Relaxed)
    }

    fn has_identity(&self) -> bool {
        self.identity
    }

    fn container_of(&self, interface_path: &str) -> Option<String> {
        container_id(interface_path)
    }
}

impl Drop for WinLampArray {
    fn drop(&mut self) {
        if let Some(w) = self.watcher.take() {
            let _ = w.Stop();
        }
    }
}

/// The lamp indices that take colours and the colours for them, from one
/// RGBA entry per lamp of `info` (fixed-colour lamps and missing entries are
/// left out).
pub fn rgb_indices_and_colors(info: &LampArrayInfo, rgba: &[[u8; 4]]) -> (Vec<i32>, Vec<Color>) {
    info.lamps
        .iter()
        .zip(rgba)
        .filter(|(l, _)| l.is_rgb)
        .map(|(l, c)| {
            (
                l.index as i32,
                Color {
                    A: c[3],
                    R: c[0],
                    G: c[1],
                    B: c[2],
                },
            )
        })
        .unzip()
}

struct WinLampHandle {
    la: LampArray,
    info: LampArrayInfo,
    available: Arc<AtomicBool>,
    token: Option<i64>,
}

impl WinLampHandle {
    fn new(la: LampArray, info: LampArrayInfo, identity: bool) -> Self {
        // `IsAvailable` lives on ILampArray2: absent on older Windows, where only
        // an ambient controller (identity) or the foreground app gets the device.
        let available = Arc::new(AtomicBool::new(la.IsAvailable().unwrap_or(identity)));
        let flag = Arc::clone(&available);
        let token = la
            .AvailabilityChanged(&TypedEventHandler::<LampArray, IInspectable>::new(
                move |sender, _| {
                    if let Some(s) = sender.as_ref() {
                        flag.store(s.IsAvailable().unwrap_or(identity), Ordering::Relaxed);
                    }
                    Ok(())
                },
            ))
            .map_err(|e| tracing::debug!("LampArray {}: no AvailabilityChanged: {e}", info.id))
            .ok();
        Self {
            la,
            info,
            available,
            token,
        }
    }
}

impl LampArrayHandle for WinLampHandle {
    fn info(&self) -> &LampArrayInfo {
        &self.info
    }

    fn available(&self) -> bool {
        self.available.load(Ordering::Relaxed)
    }

    fn set_colors(&mut self, rgba: &[[u8; 4]]) -> HalResult<()> {
        let (indices, colors) = rgb_indices_and_colors(&self.info, rgba);
        if indices.is_empty() {
            return Ok(());
        }
        self.la.SetColorsForIndices(&colors, &indices).map_err(|e| {
            if self.la.IsConnected().is_ok_and(|c| !c) {
                HalError::Disconnected
            } else {
                backend_error("LampArray.SetColorsForIndices", e)
            }
        })
    }

    fn release(&mut self) -> HalResult<()> {
        // Nothing to call: dropping the LampArray hands the device back.
        if let Some(t) = self.token.take() {
            let _ = self.la.RemoveAvailabilityChanged(t);
        }
        Ok(())
    }
}

impl Drop for WinLampHandle {
    fn drop(&mut self) {
        if let Some(t) = self.token.take() {
            let _ = self.la.RemoveAvailabilityChanged(t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_colour_lamps_are_left_out() {
        let info = LampArrayInfo {
            id: "x".into(),
            name: "x".into(),
            vid: 1,
            pid: 2,
            container_id: None,
            kind: LampArrayKind::Mouse,
            bounding_box_um: [1, 1, 1],
            min_update_interval_us: 0,
            lamps: (0..3)
                .map(|i| LampInfo {
                    index: i,
                    position_um: [0; 3],
                    is_rgb: i != 1,
                })
                .collect(),
        };
        let (idx, colors) = rgb_indices_and_colors(&info, &[[1, 2, 3, 255], [4, 5, 6, 255], [7, 8, 9, 128]]);
        assert_eq!(idx, vec![0, 2]);
        assert_eq!(
            (colors[1].R, colors[1].G, colors[1].B, colors[1].A),
            (7, 8, 9, 128)
        );
        // Fewer colours than lamps: the rest are not sent.
        assert_eq!(rgb_indices_and_colors(&info, &[[0; 4]]).0, vec![0]);
    }

    /// Read-only: lists the LampArrays Windows reports (opens no HID handle,
    /// sets no colour). `cargo test -p huedeck-hal-windows -- --ignored lamparray`.
    #[test]
    #[ignore = "reads this machine's LampArrays"]
    fn lamparray_enumerate_this_machine() {
        let b = WinLampArray::start().unwrap();
        for info in b.enumerate().unwrap() {
            println!(
                "{} {:04x}:{:04x} {:?} lamps={} container={:?} identity={}",
                info.id,
                info.vid,
                info.pid,
                info.kind,
                info.lamps.len(),
                info.container_id,
                b.has_identity()
            );
        }
    }
}
