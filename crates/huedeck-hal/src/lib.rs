// SPDX-License-Identifier: GPL-3.0-or-later
//! HueDeck hardware abstraction layer.
//!
//! This crate is the design centre of the HueDeck device stack: every OS
//! backend (`huedeck-hal-windows`, `huedeck-hal-linux`, the privileged
//! `huedeck-iobroker` client) implements the traits defined here, and every
//! consumer (plugin host, engine, CLI) talks to hardware only through them.
//! Nothing in this crate touches an OS API; it is pure Rust and `#![forbid(unsafe_code)]`.
//!
//! The vocabulary mirrors the SignalRGB plugin contract documented in
//! `docs/host-api/*.md` so that a plugin's `Validate(endpoint)` sees the
//! same `{interface, usage, usage_page, collection}` tuple on every OS.
//!
//! Enable the `mock` feature (always on under `cargo test`) for scripted
//! in-memory backends and the `mock::TransactionRecorder` used by golden tests.

#![forbid(unsafe_code)]

pub mod audio;
pub mod capture;
pub mod error;
pub mod hid;
pub mod hotplug;
pub mod key;
pub mod lamparray;
pub mod sensors;
pub mod serial;
pub mod smbios;
pub mod smbus;
pub mod trace;
pub mod usb;

#[cfg(any(test, feature = "mock"))]
pub mod mock;

pub use error::{HalError, HalResult};
pub use hid::{HidBackend, HidEndpointInfo, HidEndpointTuple, HidHandle, ReportLengths, ReportSpec};
pub use hotplug::{DeviceClass, DeviceEvent, HotplugSource};
pub use key::{DeviceKey, EndpointUid};
pub use serial::{SerialBackend, SerialConfig, SerialHandle, SerialPortInfo};
pub use smbios::SystemInfo;
pub use smbus::{PciIds, SmbusBatch, SmbusBus, SmbusBusDescriptor, SmbusBusKind, SmbusOp, SmbusReply};
pub use usb::{UsbBackend, UsbDeviceInfo, UsbHandle};
