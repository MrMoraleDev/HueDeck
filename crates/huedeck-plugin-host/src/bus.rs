// SPDX-License-Identifier: GPL-3.0-or-later
//! Natives behind the compat `bus` object of SMBus plugins.
//!
//! [`install_bus`] adds `bus*` functions to `globalThis.__huedeckNative`; the
//! JS host (`js/host.js`, `nativeBusMembers`) validates arguments the way
//! SignalRGB does, binds the address of a fixed bus and calls these. Every
//! operation goes through a [`GuardedBus`] (crate::smbus), so the policy holds
//! whatever the JS side does.
//!
//! Return convention: a byte or word (`>= 0`) or a negative status; block
//! reads return `[status, bytes…]` as one array, which the host splits.

use crate::device::{lock_device, panic_text, SharedDevice};
use crate::runtime::Deadline;
use crate::smbus::GuardedBus;
use huedeck_hal::smbus::SmbusBusKind;
use rquickjs::{Ctx, Function, Object};
use std::sync::{Arc, Mutex, MutexGuard};

/// A guard shared between the session and the natives.
pub type SharedGuard = Arc<Mutex<GuardedBus>>;

/// Lock a [`SharedGuard`], recovering from a poisoned lock.
pub fn lock_guard(g: &SharedGuard) -> MutexGuard<'_, GuardedBus> {
    g.lock().unwrap_or_else(|e| e.into_inner())
}

/// `bus.IsXBus()` spelling of a bus kind (`busInfo().kind`).
pub fn kind_name(kind: SmbusBusKind) -> &'static str {
    match kind {
        SmbusBusKind::System => "system",
        SmbusBusKind::Amd => "amd",
        SmbusBusKind::Intel => "intel",
        SmbusBusKind::Nvidia => "nvidia",
        SmbusBusKind::AmdGpu => "amdgpu",
        SmbusBusKind::Nuvoton => "nuvoton",
    }
}

fn status(r: Result<(), i32>) -> i32 {
    r.map_or_else(|s| s, |()| 0)
}

fn value<T: Into<i32>>(r: Result<T, i32>) -> i32 {
    r.map_or_else(|s| s, Into::into)
}

fn block(r: Result<Vec<u8>, i32>) -> Vec<i32> {
    match r {
        Ok(bytes) => std::iter::once(0)
            .chain(bytes.into_iter().map(i32::from))
            .collect(),
        Err(s) => vec![s],
    }
}

fn bytes(values: &[i32]) -> Vec<u8> {
    values.iter().take(256).map(|&v| v as u8).collect()
}

/// Add the `bus*` natives (and `log` / `pause` when `natives` lacks them, the
/// scan mode) to `natives`. A panic marks `device` fatal when there is one.
pub(crate) fn install_bus<'js>(
    ctx: &Ctx<'js>,
    natives: &Object<'js>,
    guard: &SharedGuard,
    deadline: &Deadline,
    device: Option<&SharedDevice>,
) -> rquickjs::Result<()> {
    // As in device.rs: blocking natives suspend the JavaScript budget, answer
    // `else` at once past the deadline, and turn a panic into a JS InternalError.
    macro_rules! native {
        ($name:literal else $fallback:expr, |$g:ident $(, $arg:ident : $ty:ty)*| $body:expr) => {{
            let shared = Arc::clone(guard);
            let deadline = deadline.clone();
            let device = device.cloned();
            let f = Function::new(ctx.clone(), move |cx: Ctx<'_>, $($arg: $ty),*| -> rquickjs::Result<_> {
                if deadline.expired() {
                    return Ok($fallback);
                }
                let _blocked = deadline.suspend();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut guard = lock_guard(&shared);
                    let $g: &mut GuardedBus = &mut guard;
                    $body
                }));
                result.map_err(|panic| {
                    let msg = format!("host panic in bus.{}: {}", $name, panic_text(panic.as_ref()));
                    if let Some(d) = &device {
                        lock_device(d).set_fatal(msg.clone());
                    }
                    rquickjs::Exception::throw_internal(&cx, &msg)
                })
            })?;
            natives.set($name, f)?;
        }};
    }
    let addr = |a: i32| u8::try_from(a).unwrap_or(0xFF);
    let reg = |r: i32| (r & 0xFF) as u8;
    let len = |n: i32| usize::try_from(n).unwrap_or(0);

    native!("busInfo" else String::new(), |g| {
        let d = g.descriptor();
        serde_json::json!({
            "name": d.name,
            "kind": kind_name(d.kind),
            "vendor": d.pci.vendor,
            "device": d.pci.device,
            "subVendor": d.pci.sub_vendor,
            "subDevice": d.pci.sub_device,
            "address": g.address(),
        })
        .to_string()
    });
    native!("busQuick" else -1, |g, a: i32| status(g.write_quick(addr(a))));
    native!("busReadByte" else -1, |g, a: i32, r: i32| value(g.read_byte(addr(a), reg(r))));
    native!("busWriteByte" else -1, |g, a: i32, r: i32, v: i32| status(g.write_byte(
        addr(a),
        reg(r),
        v as u8
    )));
    native!("busReadWord" else -1, |g, a: i32, r: i32| value(g.read_word(addr(a), reg(r))));
    native!("busWriteWord" else -1, |g, a: i32, r: i32, w: i32| status(g.write_word(
        addr(a),
        reg(r),
        w as u16
    )));
    native!("busWriteBlock" else -1, |g, a: i32, r: i32, data: Vec<i32>| status(g.write_block(
        addr(a),
        reg(r),
        &bytes(&data)
    )));
    native!("busReadBlock" else vec![-1], |g, a: i32, r: i32, n: i32| block(g.read_block(
        addr(a),
        reg(r),
        len(n)
    )));
    native!("busWriteNoReg" else -1, |g, a: i32, data: Vec<i32>| status(g.write_block_no_reg(
        addr(a),
        &bytes(&data)
    )));
    native!("busReadNoReg" else vec![-1], |g, a: i32, n: i32| block(g.read_block_no_reg(addr(a), len(n))));
    native!("busReadByteNoReg" else -1, |g, a: i32| value(g.read_byte_no_reg(addr(a))));
    native!("busLock" else false, |g| g.lock());
    native!("busUnlock" else (), |g| g.unlock());
    native!("busHasLock" else false, |g| g.has_lock());

    if !natives.contains_key("log")? {
        let bus_name = lock_guard(guard).descriptor().name.clone();
        natives.set(
            "log",
            Function::new(ctx.clone(), move |text: String| {
                let text: String = text.chars().take(4096).collect();
                tracing::info!(target: "huedeck::plugin", bus = %bus_name, "{text}");
            })?,
        )?;
    }
    if !natives.contains_key("pause")? {
        let d = deadline.clone();
        natives.set(
            "pause",
            Function::new(ctx.clone(), move |ms: f64| {
                if d.expired() || !ms.is_finite() || ms <= 0.0 {
                    return;
                }
                let _blocked = d.suspend();
                let ms = ms.min(crate::device::MAX_PAUSE.as_secs_f64() * 1000.0);
                std::thread::sleep(std::time::Duration::from_micros((ms * 1000.0) as u64));
            })?,
        )?;
    }
    Ok(())
}
