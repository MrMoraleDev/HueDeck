// SPDX-License-Identifier: GPL-3.0-or-later
//! Plugin export model.
//!
//! # The call-if-function rule
//!
//! A SignalRGB plugin is an ES module. After evaluation the host reads its
//! namespace and, for every *metadata* export in [`METADATA_EXPORTS`]:
//!
//! 1. if the export is a function, the host **calls it with no arguments**
//!    (with the `device`/`bus` globals already bound, so `Validate` and
//!    `ProductId` may call `device.productId()`), and uses the return value;
//! 2. otherwise the exported value is used as is;
//! 3. a missing export takes its documented default (`Type` → `"Hid"`,
//!    `DefaultScale` → `1.0`, …).
//!
//! `VendorId` and `ProductId` are required (unless the plugin exports
//! `HidUsages`, a HueDeck extension, see [`PluginMeta::hid_usages`]);
//! `ProductId()` must yield an int,
//! an int array, or an array of decimal numeric strings (what
//! `Object.keys(library)` produces for hex-literal keys), otherwise the module
//! is rejected with the exe's message *"ProductId() must be of type int, or
//! int[]"*. Helper modules without `ProductId` are skipped silently.
//!
//! *Lifecycle* exports in [`LIFECYCLE_EXPORTS`] are never called at scan
//! time; the host invokes them with their documented arguments
//! (`Validate(endpoint)`, `Initialize()`, `Render()`, `Shutdown(systemSuspending)`,
//! `Scan(bus)`, `new DiscoveryService()`), plus `on<Prop>Changed()` for every
//! controllable parameter and the built-in `onBrightnessChanged()`.
//!
//! Numeric fields of `ControllableParameters` are frequently strings in the
//! corpus (`"min": "0"`); the host coerces them (see [`coerce_number`]).

use huedeck_hal::smbus::{SmbusBusDescriptor, SmbusBusKind};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;

/// Exports read with the call-if-function rule, in the order the host reads them.
pub const METADATA_EXPORTS: &[&str] = &[
    "Name",
    "Publisher",
    "VendorId",
    "ProductId",
    "Size",
    "DefaultPosition",
    "DefaultScale",
    "LedNames",
    "LedPositions",
    "ImageUrl",
    "Image",
    "Documentation",
    "Version",
    "Type",
    "DeviceType",
    "ControllableParameters",
    "ConflictingProcesses",
    "SubdeviceController",
    "SupportsSubdevices",
    "LacksOnBoardLeds",
    "DefaultComponentBrand",
    "SupportsFanControl",
    "DeviceMessages",
    "UsesCorsairMutex",
    "UsesUSBSensorMutex",
    "AllowConflictBypass",
    "BrandGPUList",
    "HidUsages",
    "SmbusAddresses",
    "SmbusBuses",
    "SmbusDeniedRegisters",
    "SpdManufacturers",
    "SmbusBatchWrites",
    "SaveCapabilities",
    "ShutdownModes",
];

/// Exports the host calls during the device lifecycle (never at scan time).
pub const LIFECYCLE_EXPORTS: &[&str] = &[
    "Validate",
    "Initialize",
    "Render",
    "Shutdown",
    "Scan",
    "DiscoveryService",
];

/// Bare module specifiers the compat profile provides as host shims.
pub const HOST_MODULES: &[&str] = &[
    "udp",
    "tcp",
    "serial",
    "lcd",
    "permissions",
    "systeminfo",
    "DeviceDiscovery",
    "base64",
    "performance",
    "appInfo",
    "Errors",
];

/// Shims resolvable only under the `@huedeck/` prefix (HueDeck extensions
/// with no SignalRGB counterpart, see `prelude/modules/README.md`).
pub const HUEDECK_ONLY_MODULES: &[&str] = &["BinaryUtils", "createColorArray", "hexToRgb"];

/// `Type()` export, compared case-insensitively; `Hid` when absent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginType {
    /// HID reports (default).
    #[default]
    Hid,
    /// libusb/WinUSB bulk + control transfers only.
    RawUsb,
    /// HID plus bulk/control transfers.
    Hybrid,
    /// COM/tty port.
    Serial,
    /// SMBus/I2C (`Scan(bus)` driven).
    Smbus,
    /// Network service (`DiscoveryService`).
    Network,
}

impl FromStr for PluginType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "hid" => Ok(PluginType::Hid),
            "rawusb" => Ok(PluginType::RawUsb),
            "hybrid" => Ok(PluginType::Hybrid),
            "serial" => Ok(PluginType::Serial),
            "smbus" => Ok(PluginType::Smbus),
            "network" => Ok(PluginType::Network),
            other => Err(format!("unknown plugin Type() \"{other}\"")),
        }
    }
}

impl PluginType {
    /// The canonical SignalRGB spelling.
    pub fn as_compat_str(self) -> &'static str {
        match self {
            PluginType::Hid => "Hid",
            PluginType::RawUsb => "rawusb",
            PluginType::Hybrid => "hybrid",
            PluginType::Serial => "serial",
            PluginType::Smbus => "SMBUS",
            PluginType::Network => "network",
        }
    }
}

/// `DeviceType()` export (UI category). Unknown strings are preserved.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeviceType {
    /// keyboard
    Keyboard,
    /// mouse
    Mouse,
    /// lightingcontroller
    LightingController,
    /// other (default)
    #[default]
    Other,
    /// mousepad
    MousePad,
    /// headphones
    Headphones,
    /// gpu
    Gpu,
    /// aio
    Aio,
    /// lcd
    Lcd,
    /// motherboard
    Motherboard,
    /// speakers
    Speakers,
    /// ram
    Ram,
    /// microphone
    Microphone,
    /// dongle
    Dongle,
    /// case
    Case,
    /// Anything else, lowercased.
    Unknown(String),
}

impl FromStr for DeviceType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "keyboard" => DeviceType::Keyboard,
            "mouse" => DeviceType::Mouse,
            "lightingcontroller" => DeviceType::LightingController,
            "" | "other" => DeviceType::Other,
            "mousepad" => DeviceType::MousePad,
            "headphones" => DeviceType::Headphones,
            "gpu" => DeviceType::Gpu,
            "aio" => DeviceType::Aio,
            "lcd" => DeviceType::Lcd,
            "motherboard" => DeviceType::Motherboard,
            "speakers" => DeviceType::Speakers,
            "ram" => DeviceType::Ram,
            "microphone" => DeviceType::Microphone,
            "dongle" => DeviceType::Dongle,
            "case" => DeviceType::Case,
            other => DeviceType::Unknown(other.to_string()),
        })
    }
}

/// Control types of `ControllableParameters[].type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ParamType {
    /// `#RRGGBB` string.
    Color,
    /// One of `values`.
    Combobox,
    /// Number within `min..=max` by `step`.
    Number,
    /// true/false.
    Boolean,
    /// Hue `0..=360`.
    Hue,
    /// Free text (optional `filter` regex).
    Textfield,
    /// Key name.
    Keycode,
}

impl FromStr for ParamType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "color" => Ok(ParamType::Color),
            "combobox" | "list" => Ok(ParamType::Combobox),
            "number" => Ok(ParamType::Number),
            "boolean" => Ok(ParamType::Boolean),
            "hue" => Ok(ParamType::Hue),
            "textfield" => Ok(ParamType::Textfield),
            "keycode" => Ok(ParamType::Keycode),
            other => Err(format!("unknown ControllableParameters type \"{other}\"")),
        }
    }
}

/// A controllable-parameter value (also the type of its default).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ParamValue {
    /// Numeric (number, hue).
    Number(f64),
    /// Boolean.
    Bool(bool),
    /// String (color, combobox, textfield, keycode).
    Text(String),
}

/// One `ControllableParameters` entry (validated against `schemas/controllable-parameters.schema.json`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ControllableParameter {
    /// JS global name the value is injected as.
    pub property: String,
    /// UI group.
    pub group: Option<String>,
    /// UI label.
    pub label: String,
    /// Longer description.
    pub description: Option<String>,
    /// Hover text.
    pub tooltip: Option<String>,
    /// Control type.
    pub param_type: ParamType,
    /// Choices for comboboxes (strings; ints are stringified).
    pub values: Vec<String>,
    /// Minimum (numbers/hue).
    pub min: Option<f64>,
    /// Maximum.
    pub max: Option<f64>,
    /// Step.
    pub step: Option<f64>,
    /// Default value.
    pub default: Option<ParamValue>,
    /// Whether changes apply live (without a restart of the device).
    pub live: bool,
    /// Display order.
    pub order: Option<i64>,
    /// Regex filter for textfields.
    pub filter: Option<String>,
}

/// A `DeviceMessages()` entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceMessage {
    /// Property the message is attached to.
    pub property: String,
    /// Message text.
    pub message: String,
    /// Hover text.
    pub tooltip: Option<String>,
}

/// Which lifecycle exports a module declares (detected at scan time, never called there).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleExports {
    /// `Validate(endpoint)`.
    pub validate: bool,
    /// `Initialize()`.
    pub initialize: bool,
    /// `Render()`.
    pub render: bool,
    /// `Shutdown(systemSuspending)`.
    pub shutdown: bool,
    /// `Scan(bus)` (SMBus plugins).
    pub scan: bool,
    /// `DiscoveryService` (network plugins; a function or a class).
    pub discovery_service: bool,
    /// Every `on<Prop>Changed` export, including the built-in `onBrightnessChanged`, in module-namespace (sorted) order.
    pub change_hooks: Vec<String>,
    /// `SaveToDevice(request)` (HueDeck extension, [`crate::save`]).
    #[serde(default)]
    pub save_to_device: bool,
}

impl LifecycleExports {
    /// Names of the fixed lifecycle exports that are present, in [`LIFECYCLE_EXPORTS`] order.
    pub fn present(&self) -> Vec<&'static str> {
        let flags = [
            self.validate,
            self.initialize,
            self.render,
            self.shutdown,
            self.scan,
            self.discovery_service,
        ];
        LIFECYCLE_EXPORTS
            .iter()
            .zip(flags)
            .filter_map(|(name, on)| on.then_some(*name))
            .collect()
    }

    /// Whether the module can run as a device at all (`Initialize` + `Render`).
    pub fn is_device(&self) -> bool {
        self.initialize && self.render
    }
}

/// Raw shape of a `ProductId` export after the call-if-function step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProductIdExport {
    /// `ProductId() { return 0x7C91; }`
    Single(u32),
    /// `ProductId() { return [0x100E, 0x1010]; }`
    List(Vec<u32>),
    /// `ProductId() { return Object.keys(PIDLibrary); }` → decimal strings.
    Keys(Vec<String>),
}

impl ProductIdExport {
    /// Classify the JSON image of a `ProductId` export (`None` when the shape is not accepted).
    pub fn from_json(value: &serde_json::Value) -> Option<Self> {
        use serde_json::Value as J;
        match value {
            J::Number(n) => n.as_u64().and_then(|v| u32::try_from(v).ok()).map(Self::Single),
            J::String(s) => Some(Self::Keys(vec![s.clone()])),
            // `ProductId() { return PIDLibrary; }` → the object's keys.
            J::Object(map) => Some(Self::Keys(map.keys().cloned().collect())),
            J::Array(items) => {
                if items.iter().all(|v| v.is_number()) {
                    items
                        .iter()
                        .map(|v| v.as_u64().and_then(|v| u32::try_from(v).ok()))
                        .collect::<Option<Vec<_>>>()
                        .map(Self::List)
                } else {
                    items
                        .iter()
                        .map(|v| match v {
                            J::String(s) => Some(s.clone()),
                            J::Number(n) => Some(n.to_string()),
                            _ => None,
                        })
                        .collect::<Option<Vec<_>>>()
                        .map(Self::Keys)
                }
            }
            _ => None,
        }
    }

    /// Resolve to concrete 16-bit product ids, applying the exe's acceptance rule.
    pub fn product_ids(&self) -> Result<Vec<u16>, MetaError> {
        let to16 = |v: u32| {
            u16::try_from(v).map_err(|_| MetaError::InvalidProductId(format!("{v:#x} exceeds 16 bits")))
        };
        match self {
            ProductIdExport::Single(v) => Ok(vec![to16(*v)?]),
            ProductIdExport::List(vs) => {
                if vs.is_empty() {
                    return Err(MetaError::InvalidProductId("empty ProductId array".into()));
                }
                vs.iter().map(|v| to16(*v)).collect()
            }
            ProductIdExport::Keys(keys) => {
                if keys.is_empty() {
                    return Err(MetaError::InvalidProductId("empty ProductId key array".into()));
                }
                keys.iter()
                    .map(|k| {
                        let k = k.trim();
                        let v = if let Some(hex) = k.strip_prefix("0x").or_else(|| k.strip_prefix("0X")) {
                            u32::from_str_radix(hex, 16).ok()
                        } else {
                            k.parse::<u32>().ok()
                        };
                        v.ok_or_else(|| MetaError::InvalidProductId(format!("\"{k}\" is not numeric")))
                            .and_then(to16)
                    })
                    .collect()
            }
        }
    }
}

/// Validation errors of a plugin's metadata.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MetaError {
    /// `ProductId()` shape rejected (exe: "ProductId() must be of type int, or int[]").
    #[error("ProductId() must be of type int, or int[]: {0}")]
    InvalidProductId(String),
    /// `Size()` below 1x1 (exe: "Invalid Size set: Width and Height must be at least 1").
    #[error("Invalid Size set: Width and Height must be at least 1")]
    InvalidSize,
    /// `DefaultScale()` outside 0.5..=75.0.
    #[error("Invalid Scale set: Value must be between 0.5 and 75.0")]
    InvalidScale,
    /// `LedNames()` and `LedPositions()` differ in length.
    #[error("LedNames ({names}) and LedPositions ({positions}) must have equal length")]
    LedCountMismatch {
        /// Length of `LedNames()`.
        names: usize,
        /// Length of `LedPositions()`.
        positions: usize,
    },
    /// An LED position lies outside `Size()`.
    #[error("LED {index} at [{x}, {y}] is outside Size")]
    LedOutOfBounds {
        /// LED index.
        index: usize,
        /// X.
        x: u32,
        /// Y.
        y: u32,
    },
}

/// Everything the host knows about a plugin after reading its exports.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PluginMeta {
    /// Source file.
    pub source_path: PathBuf,
    /// `Name()`.
    pub name: String,
    /// `Publisher()`.
    pub publisher: Option<String>,
    /// `VendorId()`.
    pub vendor_id: u16,
    /// Resolved `ProductId()`; one registry entry per element.
    pub product_ids: Vec<u16>,
    /// `Type()`.
    pub plugin_type: PluginType,
    /// `DeviceType()`.
    pub device_type: DeviceType,
    /// `Size()`.
    pub size: [u32; 2],
    /// `DefaultPosition()`.
    pub default_position: [f32; 2],
    /// `DefaultScale()`.
    pub default_scale: f32,
    /// `LedNames()`.
    pub led_names: Vec<String>,
    /// `LedPositions()`.
    pub led_positions: Vec<[u32; 2]>,
    /// `ImageUrl()`.
    pub image_url: Option<String>,
    /// `Image()` (base64 PNG).
    pub image_base64: Option<String>,
    /// `Documentation()`.
    pub documentation: Option<String>,
    /// `Version()`.
    pub version: Option<String>,
    /// `ControllableParameters()`.
    pub controllable_parameters: Vec<ControllableParameter>,
    /// `ConflictingProcesses()`.
    pub conflicting_processes: Vec<String>,
    /// `SubdeviceController()` (or deprecated `SupportsSubdevices()`).
    pub subdevice_controller: bool,
    /// `LacksOnBoardLeds()`.
    pub lacks_on_board_leds: bool,
    /// `DefaultComponentBrand()`.
    pub default_component_brand: Option<String>,
    /// `SupportsFanControl()`.
    pub supports_fan_control: bool,
    /// `DeviceMessages()`.
    pub device_messages: Vec<DeviceMessage>,
    /// `UsesCorsairMutex()`.
    pub uses_corsair_mutex: bool,
    /// `UsesUSBSensorMutex()`.
    pub uses_usb_sensor_mutex: bool,
    /// `AllowConflictBypass()`.
    pub allow_conflict_bypass: bool,
    /// Deprecated `SupportsSubdevices()` was exported (folded into `subdevice_controller` as well).
    #[serde(default)]
    pub supports_subdevices: bool,
    /// `BrandGPUList()` entries, kept verbatim (plugin-defined identifier objects).
    #[serde(default)]
    pub brand_gpu_list: Vec<serde_json::Value>,
    /// `HidUsages()` (HueDeck extension): `[usage page, usage]` of HID
    /// top-level collections the plugin drives on any device, e.g.
    /// `[[0x59, 0x01]]` for HID LampArray. Used for devices no plugin claims
    /// by VID:PID.
    #[serde(default)]
    pub hid_usages: Vec<[u16; 2]>,
    /// `SmbusAddresses()` (HueDeck extension): the 7-bit addresses an SMBus
    /// plugin's `Scan(bus)` may touch and its devices may live at. HueDeck
    /// runs only SMBus plugins that declare it (docs/host-api/smbus.md).
    #[serde(default)]
    pub smbus_addresses: Vec<u8>,
    /// `SmbusBuses()` (HueDeck extension): the bus kinds `Scan(bus)` is
    /// offered (`"amd"`, `"intel"`, `"nvidia"`, ...); empty means any.
    #[serde(default)]
    pub smbus_buses: Vec<SmbusBusKind>,
    /// `SmbusDeniedRegisters()` (HueDeck extension): registers the host never
    /// lets the plugin write, at any address (save-to-flash and the like).
    #[serde(default)]
    pub smbus_denied_registers: Vec<u8>,
    /// `SpdManufacturers()` (HueDeck extension): the JEP106 module
    /// manufacturers a RAM plugin drives. On chipset buses a RAM plugin
    /// scans only when the SPD shows one of them ([`crate::spd`]).
    #[serde(default)]
    pub spd_manufacturers: Vec<crate::spd::SpdManufacturer>,
    /// `SmbusBatchWrites()` returned `true` (HueDeck extension): inside a
    /// `TransactionLock` the host queues the plugin's writes and sends them
    /// as one batch (crate::smbus); `TransactionUnlock()` reports a failure.
    #[serde(default)]
    pub smbus_batch_writes: bool,
    /// `SaveCapabilities()` (HueDeck extension): what `SaveToDevice(request)` can
    /// store in the device's own memory ([`crate::save`]).
    #[serde(default)]
    pub save_caps: Option<crate::save::SaveCaps>,
    /// `ShutdownModes()` (HueDeck extension): the [`crate::save::ShutdownMode`]s
    /// the plugin's `Shutdown(SystemSuspending, mode)` carries out itself.
    #[serde(default)]
    pub shutdown_modes: Vec<crate::save::ShutdownMode>,
    /// Lifecycle exports the module declares.
    #[serde(default)]
    pub lifecycle: LifecycleExports,
}

impl PluginMeta {
    /// Minimal meta for tests and fixtures.
    pub fn minimal(
        source_path: impl Into<PathBuf>,
        name: &str,
        vendor_id: u16,
        product_ids: Vec<u16>,
    ) -> Self {
        Self {
            source_path: source_path.into(),
            name: name.to_string(),
            publisher: None,
            vendor_id,
            product_ids,
            plugin_type: PluginType::Hid,
            device_type: DeviceType::Other,
            size: [1, 1],
            default_position: [0.0, 0.0],
            default_scale: 1.0,
            led_names: Vec::new(),
            led_positions: Vec::new(),
            image_url: None,
            image_base64: None,
            documentation: None,
            version: None,
            controllable_parameters: Vec::new(),
            conflicting_processes: Vec::new(),
            subdevice_controller: false,
            lacks_on_board_leds: false,
            default_component_brand: None,
            supports_fan_control: false,
            device_messages: Vec::new(),
            uses_corsair_mutex: false,
            uses_usb_sensor_mutex: false,
            allow_conflict_bypass: false,
            supports_subdevices: false,
            brand_gpu_list: Vec::new(),
            hid_usages: Vec::new(),
            smbus_addresses: Vec::new(),
            smbus_buses: Vec::new(),
            smbus_denied_registers: Vec::new(),
            spd_manufacturers: Vec::new(),
            smbus_batch_writes: false,
            save_caps: None,
            shutdown_modes: Vec::new(),
            lifecycle: LifecycleExports::default(),
        }
    }

    /// The PCI 4-tuples of `BrandGPUList()` (entries without the four ids are skipped).
    pub fn gpu_ids(&self) -> Vec<GpuIdentifier> {
        self.brand_gpu_list
            .iter()
            .filter_map(GpuIdentifier::from_json)
            .collect()
    }

    /// Whether `Scan(bus)` is offered the bus `d`: a GPU bus to a plugin
    /// whose `BrandGPUList()` names that exact card, a chipset (or SuperIO)
    /// bus to a plugin without a GPU list; both only for kinds its
    /// `SmbusBuses()` allows.
    pub fn smbus_offered(&self, d: &SmbusBusDescriptor) -> bool {
        if !self.smbus_buses.is_empty() && !self.smbus_buses.contains(&d.kind) {
            return false;
        }
        let gpus = self.gpu_ids();
        match d.kind {
            SmbusBusKind::Nvidia | SmbusBusKind::AmdGpu => gpus.iter().any(|g| g.matches(d)),
            SmbusBusKind::System | SmbusBusKind::Amd | SmbusBusKind::Intel | SmbusBusKind::Nuvoton => {
                gpus.is_empty()
            }
        }
    }

    /// Whether the SPD check gates this plugin on chipset buses: a RAM
    /// plugin (`DeviceType()` "ram"), or one that declares `SpdManufacturers()`.
    pub fn spd_gated(&self) -> bool {
        self.device_type == DeviceType::Ram || !self.spd_manufacturers.is_empty()
    }

    /// The dry-run "meta-complete" rule: `Name` + `VendorId` + `ProductId`, or a
    /// named SMBus / network plugin (those are keyed by bus address or service, not VID:PID).
    pub fn is_complete(&self) -> bool {
        if self.name.is_empty() {
            return false;
        }
        match self.plugin_type {
            PluginType::Smbus | PluginType::Network => true,
            _ => (self.vendor_id != 0 && !self.product_ids.is_empty()) || !self.hid_usages.is_empty(),
        }
    }

    /// `(vid, pid)` registry keys, one per product id.
    pub fn keys(&self) -> impl Iterator<Item = (u16, u16)> + '_ {
        self.product_ids.iter().map(move |pid| (self.vendor_id, *pid))
    }

    /// Validate the geometry exports the way the exe does.
    pub fn validate(&self) -> Result<(), MetaError> {
        if self.size[0] == 0 || self.size[1] == 0 {
            return Err(MetaError::InvalidSize);
        }
        if !(0.5..=75.0).contains(&self.default_scale) {
            return Err(MetaError::InvalidScale);
        }
        if self.led_names.len() != self.led_positions.len() {
            return Err(MetaError::LedCountMismatch {
                names: self.led_names.len(),
                positions: self.led_positions.len(),
            });
        }
        for (index, p) in self.led_positions.iter().enumerate() {
            if p[0] >= self.size[0] || p[1] >= self.size[1] {
                return Err(MetaError::LedOutOfBounds {
                    index,
                    x: p[0],
                    y: p[1],
                });
            }
        }
        Ok(())
    }
}

/// One `BrandGPUList()` entry (`{Vendor, SubVendor, Device, SubDevice, Address, Name}`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuIdentifier {
    /// PCI vendor.
    pub vendor: u16,
    /// PCI device.
    pub device: u16,
    /// PCI subsystem vendor (board maker).
    pub sub_vendor: u16,
    /// PCI subsystem device (board).
    pub sub_device: u16,
    /// Controller address on the GPU's bus.
    pub address: Option<u8>,
    /// Card name.
    pub name: String,
}

impl GpuIdentifier {
    /// Read one `BrandGPUList()` entry.
    pub fn from_json(v: &serde_json::Value) -> Option<Self> {
        let o = v.as_object()?;
        let id = |k: &str| o.get(k)?.as_u64().and_then(|n| u16::try_from(n).ok());
        Some(Self {
            vendor: id("Vendor")?,
            device: id("Device")?,
            sub_vendor: id("SubVendor")?,
            sub_device: id("SubDevice")?,
            address: o
                .get("Address")
                .and_then(|a| a.as_u64())
                .and_then(|a| u8::try_from(a).ok()),
            name: o
                .get("Name")
                .and_then(|n| n.as_str())
                .unwrap_or_default()
                .to_string(),
        })
    }

    /// Whether the bus belongs to this card (all four PCI ids equal).
    pub fn matches(&self, d: &SmbusBusDescriptor) -> bool {
        let p = d.pci;
        (p.vendor, p.device, p.sub_vendor, p.sub_device)
            == (self.vendor, self.device, self.sub_vendor, self.sub_device)
    }
}

/// `SmbusBuses()` spelling of a bus kind (`"any"`, no restriction, is `None` here).
pub fn smbus_kind(name: &str) -> Option<SmbusBusKind> {
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "system" => SmbusBusKind::System,
        "amd" => SmbusBusKind::Amd,
        "intel" => SmbusBusKind::Intel,
        "nvidia" => SmbusBusKind::Nvidia,
        "amdgpu" => SmbusBusKind::AmdGpu,
        "nuvoton" => SmbusBusKind::Nuvoton,
        _ => return None,
    })
}

/// Coerce the string-or-number fields of `ControllableParameters` (`"min": "0"`).
pub fn coerce_number(raw: &str) -> Option<f64> {
    let s = raw.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u64::from_str_radix(hex, 16).ok().map(|v| v as f64);
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// A parameter value as its control type expects it: the numeric strings of
/// the corpus become numbers for `number` / `hue` (`"800"` -> 800), `"true"` /
/// `"false"` become booleans for `boolean` (docs/host-api/hid-usb.md: numeric
/// fields are frequently strings). Anything else is returned unchanged.
pub fn coerce_param_value(param_type: ParamType, value: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value as J;
    match (param_type, value) {
        (ParamType::Number | ParamType::Hue, J::String(s)) => coerce_number(s)
            .and_then(serde_json::Number::from_f64)
            .map_or_else(|| value.clone(), J::Number),
        (ParamType::Boolean, J::String(s)) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => J::Bool(true),
            "false" | "0" | "" => J::Bool(false),
            _ => value.clone(),
        },
        (ParamType::Boolean, J::Number(n)) => J::Bool(n.as_f64().is_some_and(|f| f != 0.0)),
        _ => value.clone(),
    }
}

/// If `export_name` is an `on<Prop>Changed` hook, return `<Prop>`.
pub fn change_hook_property(export_name: &str) -> Option<&str> {
    let inner = export_name.strip_prefix("on")?.strip_suffix("Changed")?;
    (!inner.is_empty()).then_some(inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_type_is_case_insensitive_with_hid_default() {
        assert_eq!("HID".parse::<PluginType>().unwrap(), PluginType::Hid);
        assert_eq!("hid".parse::<PluginType>().unwrap(), PluginType::Hid);
        assert_eq!("".parse::<PluginType>().unwrap(), PluginType::Hid);
        assert_eq!("SMBUS".parse::<PluginType>().unwrap(), PluginType::Smbus);
        assert_eq!("smbus".parse::<PluginType>().unwrap(), PluginType::Smbus);
        assert_eq!("RawUSB".parse::<PluginType>().unwrap(), PluginType::RawUsb);
        assert!("bluetooth".parse::<PluginType>().is_err());
        assert_eq!(PluginType::Smbus.as_compat_str(), "SMBUS");
    }

    #[test]
    fn device_type_keeps_unknowns() {
        assert_eq!("Keyboard".parse::<DeviceType>().unwrap(), DeviceType::Keyboard);
        assert_eq!(
            "lightingcontroller".parse::<DeviceType>().unwrap(),
            DeviceType::LightingController
        );
        assert_eq!(
            "toaster".parse::<DeviceType>().unwrap(),
            DeviceType::Unknown("toaster".into())
        );
    }

    #[test]
    fn product_id_shapes() {
        assert_eq!(
            ProductIdExport::Single(0x7C91).product_ids().unwrap(),
            vec![0x7C91]
        );
        assert_eq!(
            ProductIdExport::List(vec![0x100E, 0x1010]).product_ids().unwrap(),
            vec![0x100E, 0x1010]
        );
        // Object.keys({0x0DA0: ..., 0x0DA1: ...}) → ["3488", "3489"]
        let keys = ProductIdExport::Keys(vec!["3488".into(), "3489".into()]);
        assert_eq!(keys.product_ids().unwrap(), vec![0x0DA0, 0x0DA1]);
        assert!(ProductIdExport::Keys(vec!["abc".into()]).product_ids().is_err());
        assert!(ProductIdExport::List(vec![]).product_ids().is_err());
        assert!(ProductIdExport::Single(0x1_0000).product_ids().is_err());
    }

    #[test]
    fn product_id_from_json_shapes() {
        use serde_json::json;
        assert_eq!(
            ProductIdExport::from_json(&json!(0x7C91)),
            Some(ProductIdExport::Single(0x7C91))
        );
        assert_eq!(
            ProductIdExport::from_json(&json!([1, 2])),
            Some(ProductIdExport::List(vec![1, 2]))
        );
        assert_eq!(
            ProductIdExport::from_json(&json!(["3488", "0x0DA1"]))
                .unwrap()
                .product_ids()
                .unwrap(),
            vec![0x0DA0, 0x0DA1]
        );
        assert_eq!(
            ProductIdExport::from_json(&json!({"3488": 1}))
                .unwrap()
                .product_ids()
                .unwrap(),
            vec![0x0DA0]
        );
        assert!(ProductIdExport::from_json(&json!({"a": 1}))
            .unwrap()
            .product_ids()
            .is_err());
        assert_eq!(ProductIdExport::from_json(&json!(true)), None);
    }

    #[test]
    fn lifecycle_and_completeness() {
        let mut l = LifecycleExports {
            initialize: true,
            render: true,
            ..Default::default()
        };
        assert_eq!(l.present(), vec!["Initialize", "Render"]);
        assert!(l.is_device());
        l.scan = true;
        assert_eq!(l.present(), vec!["Initialize", "Render", "Scan"]);

        let mut m = PluginMeta::minimal("x.js", "X", 0x1462, vec![0x7C91]);
        assert!(m.is_complete());
        m.product_ids.clear();
        assert!(!m.is_complete());
        m.plugin_type = PluginType::Smbus;
        assert!(m.is_complete());
        m.name.clear();
        assert!(!m.is_complete());
    }

    #[test]
    fn meta_validation() {
        let mut m = PluginMeta::minimal("x.js", "X", 0x1462, vec![0x7C91]);
        assert!(m.validate().is_ok());
        m.led_names = vec!["A".into()];
        assert!(matches!(
            m.validate(),
            Err(MetaError::LedCountMismatch {
                names: 1,
                positions: 0
            })
        ));
        m.led_positions = vec![[1, 0]];
        assert!(matches!(
            m.validate(),
            Err(MetaError::LedOutOfBounds { index: 0, .. })
        ));
        m.size = [2, 1];
        assert!(m.validate().is_ok());
        m.default_scale = 0.1;
        assert_eq!(m.validate(), Err(MetaError::InvalidScale));
        assert_eq!(m.keys().collect::<Vec<_>>(), vec![(0x1462, 0x7C91)]);
    }

    fn bus(kind: SmbusBusKind, pci: [u16; 4]) -> SmbusBusDescriptor {
        SmbusBusDescriptor {
            name: "bus".into(),
            kind,
            pci: huedeck_hal::PciIds {
                vendor: pci[0],
                device: pci[1],
                sub_vendor: pci[2],
                sub_device: pci[3],
            },
            instance: String::new(),
            max_block_len: 32,
        }
    }

    fn msi_gpu() -> PluginMeta {
        let mut m = PluginMeta::minimal("gpu.js", "MSI GPU", 0, vec![]);
        m.plugin_type = PluginType::Smbus;
        m.brand_gpu_list = vec![serde_json::json!({
            "Vendor": 0x10DE, "Device": 0x2504, "SubVendor": 0x1462, "SubDevice": 0x3903,
            "Address": 0x68, "Name": "MSI GeForce RTX 3060 Gaming X Trio LHR"
        })];
        m.smbus_addresses = vec![0x68];
        m.smbus_buses = vec![SmbusBusKind::Nvidia];
        m
    }

    #[test]
    fn brand_gpu_list_parses_pci_tuples() {
        let ids = msi_gpu().gpu_ids();
        assert_eq!(ids.len(), 1);
        assert_eq!(
            (
                ids[0].vendor,
                ids[0].device,
                ids[0].sub_vendor,
                ids[0].sub_device,
                ids[0].address
            ),
            (0x10DE, 0x2504, 0x1462, 0x3903, Some(0x68))
        );
        let mut m = msi_gpu();
        m.brand_gpu_list
            .push(serde_json::json!({"Vendor": 1, "Name": "half"}));
        assert_eq!(m.gpu_ids().len(), 1, "incomplete entries are skipped");
        assert_eq!(smbus_kind("NVIDIA"), Some(SmbusBusKind::Nvidia));
        assert_eq!(smbus_kind("any"), None);
    }

    #[test]
    fn gpu_plugins_are_offered_only_their_cards() {
        let m = msi_gpu();
        assert!(m.smbus_offered(&bus(SmbusBusKind::Nvidia, [0x10DE, 0x2504, 0x1462, 0x3903])));
        assert!(!m.smbus_offered(&bus(SmbusBusKind::Nvidia, [0x10DE, 0x2504, 0x1462, 0x3904])));
        assert!(!m.smbus_offered(&bus(SmbusBusKind::Amd, [0x1022, 0x790B, 0x1022, 0x790B])));
    }

    #[test]
    fn ram_plugins_are_offered_chipset_buses_only() {
        let mut m = PluginMeta::minimal("ram.js", "RAM", 0, vec![]);
        m.plugin_type = PluginType::Smbus;
        m.smbus_addresses = vec![0x39];
        m.smbus_buses = vec![SmbusBusKind::Amd, SmbusBusKind::Intel];
        let piix4 = bus(SmbusBusKind::Amd, [0x1022, 0x790B, 0x1022, 0x790B]);
        assert!(m.smbus_offered(&piix4));
        assert!(!m.smbus_offered(&bus(SmbusBusKind::Nvidia, [0x10DE, 0x2504, 0x1462, 0x3903])));
        assert!(!m.smbus_offered(&bus(SmbusBusKind::Nuvoton, [0; 4])));
        m.smbus_buses.clear();
        assert!(
            m.smbus_offered(&bus(SmbusBusKind::Nuvoton, [0; 4])),
            "no SmbusBuses(): any chipset kind"
        );
        assert!(!m.smbus_offered(&bus(SmbusBusKind::Nvidia, [0x10DE, 0x2504, 0x1462, 0x3903])));
    }

    #[test]
    fn number_coercion_and_hooks() {
        assert_eq!(coerce_number("0"), Some(0.0));
        assert_eq!(coerce_number(" 360 "), Some(360.0));
        assert_eq!(coerce_number("0x10"), Some(16.0));
        assert_eq!(coerce_number("NaN"), None);
        assert_eq!(coerce_number("abc"), None);
        assert_eq!(change_hook_property("onforcedColorChanged"), Some("forcedColor"));
        assert_eq!(change_hook_property("onBrightnessChanged"), Some("Brightness"));
        assert_eq!(change_hook_property("onChanged"), None);
        assert_eq!(change_hook_property("Render"), None);
        assert_eq!("list".parse::<ParamType>().unwrap(), ParamType::Combobox);
    }
}
