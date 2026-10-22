// SPDX-License-Identifier: GPL-3.0-or-later
//! HID report-descriptor parser.
//!
//! Written from the USB HID 1.11 specification (§6.2.2 "Report Descriptor",
//! Appendix E examples). It extracts what the device stack needs to present
//! identical `{usage_page, usage, collection}` endpoint tuples on every OS:
//!
//! * the flat list of short/long [`Item`]s,
//! * one [`TopLevelCollection`] per application collection, with its usage
//!   page/usage, 0-based index, the report ids it uses and the longest
//!   input/output/feature report **in bytes including the report-id byte**
//!   (the same convention as Windows `HIDP_CAPS.*ReportByteLength` and hidapi),
//! * every main item's [`ReportField`]: where its elements sit in the report,
//!   their size, usages, logical range and enclosing collection, which is what
//!   a driver needs to build or read a report by usage (HID LampArray).

#![forbid(unsafe_code)]

/// Errors from [`parse_items`] / [`parse`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DescError {
    /// Item header announced more data bytes than remain.
    #[error("truncated item at offset {offset}")]
    Truncated {
        /// Byte offset of the item header.
        offset: usize,
    },
    /// `End Collection` without a matching `Collection`.
    #[error("unbalanced End Collection at offset {offset}")]
    UnbalancedEnd {
        /// Byte offset.
        offset: usize,
    },
    /// Descriptor ended with open collections.
    #[error("{0} collection(s) left open at end of descriptor")]
    UnclosedCollections(usize),
}

/// Item type (bits 3:2 of the prefix byte).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    /// Input/Output/Feature/Collection/End Collection.
    Main,
    /// Usage Page, Logical Min/Max, Report Size/Id/Count, Push/Pop…
    Global,
    /// Usage, Usage Min/Max, Designator…, Delimiter.
    Local,
    /// Reserved type 3 (never emitted by [`parse_items`] except for long items).
    Reserved,
}

/// One descriptor item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item {
    /// Byte offset of the prefix byte.
    pub offset: usize,
    /// Item type.
    pub kind: ItemKind,
    /// Tag (bits 7:4 of the prefix; for long items the long-item tag byte).
    pub tag: u8,
    /// Payload size in bytes (0, 1, 2 or 4; up to 255 for long items).
    pub size: u8,
    /// Payload interpreted as unsigned little-endian (first 4 bytes for long items).
    pub data: u32,
    /// Payload sign-extended according to `size`.
    pub signed: i32,
}

/// Main-item tags.
pub mod tags {
    /// Input.
    pub const INPUT: u8 = 0x8;
    /// Output.
    pub const OUTPUT: u8 = 0x9;
    /// Collection.
    pub const COLLECTION: u8 = 0xA;
    /// Feature.
    pub const FEATURE: u8 = 0xB;
    /// End Collection.
    pub const END_COLLECTION: u8 = 0xC;
    /// Global: Usage Page.
    pub const USAGE_PAGE: u8 = 0x0;
    /// Main-item flag bit 0: Constant (padding), not Data.
    pub const FLAG_CONSTANT: u32 = 0x01;
    /// Main-item flag bit 1: Variable (one usage per element), not Array.
    pub const FLAG_VARIABLE: u32 = 0x02;
    /// Global: Logical Minimum.
    pub const LOGICAL_MIN: u8 = 0x1;
    /// Global: Logical Maximum.
    pub const LOGICAL_MAX: u8 = 0x2;
    /// Global: Report Size (bits).
    pub const REPORT_SIZE: u8 = 0x7;
    /// Global: Report ID.
    pub const REPORT_ID: u8 = 0x8;
    /// Global: Report Count.
    pub const REPORT_COUNT: u8 = 0x9;
    /// Global: Push.
    pub const PUSH: u8 = 0xA;
    /// Global: Pop.
    pub const POP: u8 = 0xB;
    /// Local: Usage.
    pub const USAGE: u8 = 0x0;
    /// Local: Usage Minimum.
    pub const USAGE_MIN: u8 = 0x1;
    /// Local: Usage Maximum.
    pub const USAGE_MAX: u8 = 0x2;
}

/// Collection types (data byte of a Collection item).
pub mod collection_types {
    /// Physical.
    pub const PHYSICAL: u8 = 0x00;
    /// Application (a top-level collection is always one of these).
    pub const APPLICATION: u8 = 0x01;
    /// Logical.
    pub const LOGICAL: u8 = 0x02;
}

/// Report direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ReportKind {
    /// Device → host.
    Input,
    /// Host → device.
    Output,
    /// Bidirectional control.
    Feature,
}

/// Accumulated size of one `(kind, report id)` report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReportInfo {
    /// Direction.
    pub kind: ReportKind,
    /// Report id (`0` when the collection uses none).
    pub id: u8,
    /// Payload bits (excluding the report id byte).
    pub bits: u32,
}

impl ReportInfo {
    /// Bytes on the wire including the report-id byte, Windows/hidapi style.
    pub fn byte_length(&self) -> usize {
        (self.bits as usize).div_ceil(8) + 1
    }
}

/// Usages `min..=max` on `page` (one usage when `min == max`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsageRange {
    /// Usage page.
    pub page: u16,
    /// First usage id.
    pub min: u16,
    /// Last usage id.
    pub max: u16,
}

impl UsageRange {
    fn len(&self) -> u32 {
        u32::from(self.max.saturating_sub(self.min)) + 1
    }
}

/// One Input/Output/Feature main item: `count` elements of `bit_size` bits
/// starting `bit_offset` bits into the report payload (the bytes after the
/// report-id byte, or the whole report when the collection uses no ids).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportField {
    /// Direction.
    pub kind: ReportKind,
    /// Report id (`0` when the collection uses none).
    pub report_id: u8,
    /// First bit of the first element, counted from the start of the payload.
    pub bit_offset: u32,
    /// Bits per element.
    pub bit_size: u32,
    /// Number of elements.
    pub count: u32,
    /// Main-item flags ([`tags::FLAG_CONSTANT`], [`tags::FLAG_VARIABLE`], ...).
    pub flags: u32,
    /// Logical minimum in force.
    pub logical_min: i32,
    /// Logical maximum in force.
    pub logical_max: i32,
    /// Usages in descriptor order. For a variable field element `i` takes
    /// the `i`-th usage, and the last one when there are fewer usages than
    /// elements (HID 1.11 §6.2.2.8); for an array field they are the
    /// selectable usages.
    pub usages: Vec<UsageRange>,
    /// `(usage page, usage)` of the innermost collection around the field.
    pub collection: (u16, u16),
}

impl ReportField {
    /// Whether the field is padding.
    pub fn is_constant(&self) -> bool {
        self.flags & tags::FLAG_CONSTANT != 0
    }

    /// Usage `(page, id)` of element `index` of a variable field.
    pub fn usage(&self, index: u32) -> Option<(u16, u16)> {
        let mut left = index;
        for r in &self.usages {
            if left < r.len() {
                return Some((r.page, r.min + left as u16));
            }
            left -= r.len();
        }
        self.usages.last().map(|r| (r.page, r.max))
    }
}

/// One application (top-level) collection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopLevelCollection {
    /// 0-based index among top-level collections of this descriptor.
    pub index: u16,
    /// Usage page in force when the collection opened.
    pub usage_page: u16,
    /// First local usage preceding the collection item (0 when none).
    pub usage: u16,
    /// Collection type byte (normally [`collection_types::APPLICATION`]).
    pub collection_type: u8,
    /// Report ids used inside this collection, ascending; empty when the
    /// collection uses unnumbered reports.
    pub report_ids: Vec<u8>,
    /// Longest input report in bytes incl. report-id byte (0 = none).
    pub input_len: usize,
    /// Longest output report in bytes incl. report-id byte (0 = none).
    pub output_len: usize,
    /// Longest feature report in bytes incl. report-id byte (0 = none).
    pub feature_len: usize,
    /// Every `(kind, id)` report seen, in first-appearance order.
    pub reports: Vec<ReportInfo>,
    /// Every main item inside the collection, in descriptor order.
    pub fields: Vec<ReportField>,
}

impl TopLevelCollection {
    /// Whether the collection uses report ids.
    pub fn uses_report_ids(&self) -> bool {
        !self.report_ids.is_empty()
    }
}

/// A parsed descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportDescriptor {
    /// Flat item list.
    pub items: Vec<Item>,
    /// Top-level collections in descriptor order.
    pub collections: Vec<TopLevelCollection>,
}

impl ReportDescriptor {
    /// Collection by 0-based index.
    pub fn collection(&self, index: u16) -> Option<&TopLevelCollection> {
        self.collections.iter().find(|c| c.index == index)
    }

    /// First collection with the given usage page / usage.
    pub fn find(&self, usage_page: u16, usage: u16) -> Option<&TopLevelCollection> {
        self.collections
            .iter()
            .find(|c| c.usage_page == usage_page && c.usage == usage)
    }
}

/// Split a descriptor into items (short and long).
pub fn parse_items(bytes: &[u8]) -> Result<Vec<Item>, DescError> {
    let mut items = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let prefix = bytes[i];
        let offset = i;
        if prefix == 0xFE {
            // Long item: [0xFE][bDataSize][bLongItemTag][data…]
            if i + 2 >= bytes.len() {
                return Err(DescError::Truncated { offset });
            }
            let size = bytes[i + 1] as usize;
            let tag = bytes[i + 2];
            let start = i + 3;
            let end = start + size;
            if end > bytes.len() {
                return Err(DescError::Truncated { offset });
            }
            let data = le_u32(&bytes[start..end.min(start + 4)]);
            items.push(Item {
                offset,
                kind: ItemKind::Reserved,
                tag,
                size: size as u8,
                data,
                signed: data as i32,
            });
            i = end;
            continue;
        }
        let size = match prefix & 0x03 {
            0 => 0usize,
            1 => 1,
            2 => 2,
            _ => 4,
        };
        let kind = match (prefix >> 2) & 0x03 {
            0 => ItemKind::Main,
            1 => ItemKind::Global,
            2 => ItemKind::Local,
            _ => ItemKind::Reserved,
        };
        let tag = prefix >> 4;
        let start = i + 1;
        let end = start + size;
        if end > bytes.len() {
            return Err(DescError::Truncated { offset });
        }
        let payload = &bytes[start..end];
        let data = le_u32(payload);
        let signed = match size {
            1 => payload[0] as i8 as i32,
            2 => i16::from_le_bytes([payload[0], payload[1]]) as i32,
            4 => data as i32,
            _ => 0,
        };
        items.push(Item {
            offset,
            kind,
            tag,
            size: size as u8,
            data,
            signed,
        });
        i = end;
    }
    Ok(items)
}

fn le_u32(payload: &[u8]) -> u32 {
    let mut v = 0u32;
    for (n, b) in payload.iter().take(4).enumerate() {
        v |= (*b as u32) << (8 * n);
    }
    v
}

#[derive(Clone, Copy, Default)]
struct GlobalState {
    usage_page: u16,
    logical_min: i32,
    logical_max: i32,
    report_size: u32,
    report_count: u32,
    report_id: u8,
}

/// A local usage as declared: extended usages carry their page, 16-bit ones
/// take the Usage Page in force at the next main item.
#[derive(Clone, Copy)]
struct DeclaredUsage {
    page: Option<u16>,
    min: u16,
    max: u16,
}

#[derive(Default)]
struct LocalState {
    usages: Vec<DeclaredUsage>,
    /// A Usage Minimum waiting for its Usage Maximum.
    usage_min: Option<(Option<u16>, u16)>,
}

impl LocalState {
    fn resolved(&self, page: u16) -> Vec<UsageRange> {
        self.usages
            .iter()
            .map(|u| UsageRange {
                page: u.page.unwrap_or(page),
                min: u.min,
                max: u.max.max(u.min),
            })
            .collect()
    }
}

/// `(page override, usage id)` of a local usage item.
fn declared(item: &Item) -> (Option<u16>, u16) {
    if item.size == 4 {
        (Some((item.data >> 16) as u16), item.data as u16)
    } else {
        (None, item.data as u16)
    }
}

/// Parse a descriptor and compute its top-level collections.
pub fn parse(bytes: &[u8]) -> Result<ReportDescriptor, DescError> {
    let items = parse_items(bytes)?;
    let mut collections: Vec<TopLevelCollection> = Vec::new();
    let mut global = GlobalState::default();
    let mut stack: Vec<GlobalState> = Vec::new();
    let mut local = LocalState::default();
    // (usage page, usage) of every open collection, outermost first.
    let mut open: Vec<(u16, u16)> = Vec::new();
    let mut current: Option<usize> = None; // index into `collections`

    for item in &items {
        match item.kind {
            ItemKind::Global => match item.tag {
                tags::USAGE_PAGE => global.usage_page = item.data as u16,
                tags::LOGICAL_MIN => global.logical_min = item.signed,
                tags::LOGICAL_MAX => global.logical_max = item.signed,
                tags::REPORT_SIZE => global.report_size = item.data,
                tags::REPORT_COUNT => global.report_count = item.data,
                tags::REPORT_ID => global.report_id = item.data as u8,
                tags::PUSH => stack.push(global),
                tags::POP => {
                    if let Some(g) = stack.pop() {
                        global = g;
                    }
                }
                _ => {}
            },
            ItemKind::Local => match item.tag {
                tags::USAGE => {
                    let (page, usage) = declared(item);
                    local.usages.push(DeclaredUsage {
                        page,
                        min: usage,
                        max: usage,
                    });
                }
                tags::USAGE_MIN => local.usage_min = Some(declared(item)),
                tags::USAGE_MAX => {
                    let (max_page, max) = declared(item);
                    let (page, min) = local.usage_min.take().unwrap_or((max_page, max));
                    local.usages.push(DeclaredUsage { page, min, max });
                }
                _ => {}
            },
            ItemKind::Main => {
                // A Usage Minimum without a maximum stands for itself.
                if let Some((page, min)) = local.usage_min.take() {
                    local.usages.push(DeclaredUsage { page, min, max: min });
                }
                match item.tag {
                    tags::COLLECTION => {
                        let usage = local
                            .resolved(global.usage_page)
                            .first()
                            .map_or((global.usage_page, 0), |u| (u.page, u.min));
                        if open.is_empty() {
                            collections.push(TopLevelCollection {
                                index: collections.len() as u16,
                                usage_page: usage.0,
                                usage: usage.1,
                                collection_type: item.data as u8,
                                report_ids: Vec::new(),
                                input_len: 0,
                                output_len: 0,
                                feature_len: 0,
                                reports: Vec::new(),
                                fields: Vec::new(),
                            });
                            current = Some(collections.len() - 1);
                        }
                        open.push(usage);
                    }
                    tags::END_COLLECTION => {
                        if open.pop().is_none() {
                            return Err(DescError::UnbalancedEnd { offset: item.offset });
                        }
                        if open.is_empty() {
                            current = None;
                        }
                    }
                    tags::INPUT | tags::OUTPUT | tags::FEATURE => {
                        let kind = match item.tag {
                            tags::INPUT => ReportKind::Input,
                            tags::OUTPUT => ReportKind::Output,
                            _ => ReportKind::Feature,
                        };
                        if let Some(idx) = current {
                            let tlc = &mut collections[idx];
                            let bits = global.report_size.saturating_mul(global.report_count);
                            let id = global.report_id;
                            if id != 0 && !tlc.report_ids.contains(&id) {
                                tlc.report_ids.push(id);
                                tlc.report_ids.sort_unstable();
                            }
                            let report = match tlc.reports.iter_mut().find(|r| r.kind == kind && r.id == id) {
                                Some(r) => r,
                                None => {
                                    tlc.reports.push(ReportInfo { kind, id, bits: 0 });
                                    tlc.reports.last_mut().expect("just pushed")
                                }
                            };
                            let bit_offset = report.bits;
                            report.bits = report.bits.saturating_add(bits);
                            tlc.fields.push(ReportField {
                                kind,
                                report_id: id,
                                bit_offset,
                                bit_size: global.report_size,
                                count: global.report_count,
                                flags: item.data,
                                logical_min: global.logical_min,
                                logical_max: global.logical_max,
                                usages: local.resolved(global.usage_page),
                                collection: open.last().copied().unwrap_or((0, 0)),
                            });
                        }
                    }
                    _ => {}
                }
                local = LocalState::default();
            }
            ItemKind::Reserved => {}
        }
    }

    if !open.is_empty() {
        return Err(DescError::UnclosedCollections(open.len()));
    }

    for tlc in &mut collections {
        for r in &tlc.reports {
            let len = r.byte_length();
            let slot = match r.kind {
                ReportKind::Input => &mut tlc.input_len,
                ReportKind::Output => &mut tlc.output_len,
                ReportKind::Feature => &mut tlc.feature_len,
            };
            *slot = (*slot).max(len);
        }
    }

    Ok(ReportDescriptor { items, collections })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Boot keyboard, written from HID 1.11 Appendix E.6.
    const KEYBOARD: &[u8] = &[
        0x05, 0x01, // Usage Page (Generic Desktop)
        0x09, 0x06, // Usage (Keyboard)
        0xA1, 0x01, // Collection (Application)
        0x05, 0x07, //   Usage Page (Key Codes)
        0x19, 0xE0, //   Usage Minimum (224)
        0x29, 0xE7, //   Usage Maximum (231)
        0x15, 0x00, //   Logical Minimum (0)
        0x25, 0x01, //   Logical Maximum (1)
        0x75, 0x01, //   Report Size (1)
        0x95, 0x08, //   Report Count (8)
        0x81, 0x02, //   Input (Data, Variable, Absolute) — modifier byte
        0x95, 0x01, //   Report Count (1)
        0x75, 0x08, //   Report Size (8)
        0x81, 0x01, //   Input (Constant) — reserved byte
        0x95, 0x05, //   Report Count (5)
        0x75, 0x01, //   Report Size (1)
        0x05, 0x08, //   Usage Page (LEDs)
        0x19, 0x01, //   Usage Minimum (1)
        0x29, 0x05, //   Usage Maximum (5)
        0x91, 0x02, //   Output (Data, Variable, Absolute) — LED report
        0x95, 0x01, //   Report Count (1)
        0x75, 0x03, //   Report Size (3)
        0x91, 0x01, //   Output (Constant) — LED padding
        0x95, 0x06, //   Report Count (6)
        0x75, 0x08, //   Report Size (8)
        0x15, 0x00, //   Logical Minimum (0)
        0x25, 0x65, //   Logical Maximum (101)
        0x05, 0x07, //   Usage Page (Key Codes)
        0x19, 0x00, //   Usage Minimum (0)
        0x29, 0x65, //   Usage Maximum (101)
        0x81, 0x00, //   Input (Data, Array) — key array
        0xC0, // End Collection
    ];

    /// Boot mouse with nested physical collection, written from HID 1.11 Appendix E.10.
    const MOUSE: &[u8] = &[
        0x05, 0x01, // Usage Page (Generic Desktop)
        0x09, 0x02, // Usage (Mouse)
        0xA1, 0x01, // Collection (Application)
        0x09, 0x01, //   Usage (Pointer)
        0xA1, 0x00, //   Collection (Physical)
        0x05, 0x09, //     Usage Page (Buttons)
        0x19, 0x01, //     Usage Minimum (1)
        0x29, 0x03, //     Usage Maximum (3)
        0x15, 0x00, //     Logical Minimum (0)
        0x25, 0x01, //     Logical Maximum (1)
        0x95, 0x03, //     Report Count (3)
        0x75, 0x01, //     Report Size (1)
        0x81, 0x02, //     Input (Data, Variable, Absolute) — buttons
        0x95, 0x01, //     Report Count (1)
        0x75, 0x05, //     Report Size (5)
        0x81, 0x01, //     Input (Constant) — padding
        0x05, 0x01, //     Usage Page (Generic Desktop)
        0x09, 0x30, //     Usage (X)
        0x09, 0x31, //     Usage (Y)
        0x15, 0x81, //     Logical Minimum (-127)
        0x25, 0x7F, //     Logical Maximum (127)
        0x75, 0x08, //     Report Size (8)
        0x95, 0x02, //     Report Count (2)
        0x81, 0x06, //     Input (Data, Variable, Relative) — X, Y
        0xC0, //   End Collection
        0xC0, // End Collection
    ];

    /// Vendor-defined page 0xFF00 with numbered 64-byte in/out reports and a
    /// 32-byte feature report, in the shape RGB controllers commonly use.
    const VENDOR: &[u8] = &[
        0x06, 0x00, 0xFF, // Usage Page (Vendor 0xFF00)
        0x09, 0x01, // Usage (1)
        0xA1, 0x01, // Collection (Application)
        0x85, 0x02, //   Report ID (2)
        0x09, 0x02, //   Usage (2)
        0x15, 0x00, //   Logical Minimum (0)
        0x26, 0xFF, 0x00, // Logical Maximum (255)
        0x75, 0x08, //   Report Size (8)
        0x95, 0x40, //   Report Count (64)
        0x81, 0x02, //   Input (Data, Variable, Absolute)
        0x09, 0x03, //   Usage (3)
        0x91, 0x02, //   Output (Data, Variable, Absolute)
        0x85, 0x03, //   Report ID (3)
        0x09, 0x04, //   Usage (4)
        0x95, 0x20, //   Report Count (32)
        0xB1, 0x02, //   Feature (Data, Variable, Absolute)
        0xC0, // End Collection
    ];

    /// Consumer-control collection with a report id, to append after another TLC.
    const CONSUMER: &[u8] = &[
        0x05, 0x0C, // Usage Page (Consumer)
        0x09, 0x01, // Usage (Consumer Control)
        0xA1, 0x01, // Collection (Application)
        0x85, 0x05, //   Report ID (5)
        0x19, 0x00, //   Usage Minimum (0)
        0x2A, 0x3C, 0x02, // Usage Maximum (0x23C)
        0x15, 0x00, //   Logical Minimum (0)
        0x26, 0x3C, 0x02, // Logical Maximum (0x23C)
        0x95, 0x01, //   Report Count (1)
        0x75, 0x10, //   Report Size (16)
        0x81, 0x00, //   Input (Data, Array)
        0xC0, // End Collection
    ];

    /// HID LampArray (usage page 0x59, HID Usage Tables "Lighting and
    /// Illumination"): the attributes report and a multi-update report for 4
    /// lamps, written for this test from the usage table.
    const LAMP_ARRAY: &[u8] = &[
        0x05, 0x59, // Usage Page (Lighting and Illumination)
        0x09, 0x01, // Usage (LampArray)
        0xA1, 0x01, // Collection (Application)
        0x85, 0x01, //   Report ID (1)
        0x09, 0x02, //   Usage (LampArrayAttributesReport)
        0xA1, 0x02, //   Collection (Logical)
        0x09, 0x03, //     Usage (LampCount)
        0x15, 0x00, //     Logical Minimum (0)
        0x27, 0xFF, 0xFF, 0x00, 0x00, // Logical Maximum (65535)
        0x75, 0x10, //     Report Size (16)
        0x95, 0x01, //     Report Count (1)
        0xB1, 0x03, //     Feature (Constant, Variable)
        0x09, 0x04, //     Usage (BoundingBoxWidthInMicrometers)
        0x09, 0x05, //     Usage (BoundingBoxHeightInMicrometers)
        0x09, 0x06, //     Usage (BoundingBoxDepthInMicrometers)
        0x09, 0x07, //     Usage (LampArrayKind)
        0x09, 0x08, //     Usage (MinUpdateIntervalInMicroseconds)
        0x27, 0xFF, 0xFF, 0xFF, 0x7F, // Logical Maximum (2^31 - 1)
        0x75, 0x20, //     Report Size (32)
        0x95, 0x05, //     Report Count (5)
        0xB1, 0x03, //     Feature (Constant, Variable)
        0xC0, //   End Collection
        0x85, 0x04, //   Report ID (4)
        0x09, 0x50, //   Usage (LampMultiUpdateReport)
        0xA1, 0x02, //   Collection (Logical)
        0x09, 0x03, //     Usage (LampCount)
        0x09, 0x55, //     Usage (LampUpdateFlags)
        0x25, 0x08, //     Logical Maximum (8)
        0x75, 0x08, //     Report Size (8)
        0x95, 0x02, //     Report Count (2)
        0xB1, 0x02, //     Feature (Data, Variable)
        0x09, 0x21, 0x09, 0x21, 0x09, 0x21, 0x09, 0x21, // Usage (LampId) x4
        0x27, 0xFF, 0xFF, 0x00, 0x00, // Logical Maximum (65535)
        0x75, 0x10, //     Report Size (16)
        0x95, 0x04, //     Report Count (4)
        0xB1, 0x02, //     Feature (Data, Variable)
        0x09, 0x51, 0x09, 0x52, 0x09, 0x53, 0x09, 0x54, // Red, Green, Blue, Intensity (lamp 0)
        0x09, 0x51, 0x09, 0x52, 0x09, 0x53, 0x09, 0x54, // lamp 1
        0x09, 0x51, 0x09, 0x52, 0x09, 0x53, 0x09, 0x54, // lamp 2
        0x09, 0x51, 0x09, 0x52, 0x09, 0x53, 0x09, 0x54, // lamp 3
        0x26, 0xFF, 0x00, // Logical Maximum (255)
        0x75, 0x08, //     Report Size (8)
        0x95, 0x10, //     Report Count (16)
        0xB1, 0x02, //     Feature (Data, Variable)
        0xC0, //   End Collection
        0xC0, // End Collection
    ];

    #[test]
    fn lamp_array_fields() {
        let d = parse(LAMP_ARRAY).unwrap();
        let c = d.find(0x59, 0x01).unwrap();
        assert_eq!(c.report_ids, vec![1, 4]);
        // Report 1: 16 + 5 * 32 bits = 22 bytes + id; report 4: 2 * 8 + 4 * 16 + 16 * 8 bits.
        let lengths: Vec<(u8, usize)> = c.reports.iter().map(|r| (r.id, r.byte_length())).collect();
        assert_eq!(lengths, vec![(1, 23), (4, 27)]);
        assert_eq!(c.feature_len, 27);
        assert_eq!(c.fields.len(), 5);

        let count = &c.fields[0];
        assert_eq!(
            (
                count.kind,
                count.report_id,
                count.bit_offset,
                count.bit_size,
                count.count
            ),
            (ReportKind::Feature, 1, 0, 16, 1)
        );
        assert!(count.is_constant());
        assert_eq!(count.collection, (0x59, 0x02));
        assert_eq!(count.usage(0), Some((0x59, 0x03)));
        assert_eq!((count.logical_min, count.logical_max), (0, 65535));

        let attributes = &c.fields[1];
        assert_eq!(
            (attributes.bit_offset, attributes.bit_size, attributes.count),
            (16, 32, 5)
        );
        assert_eq!(attributes.usage(0), Some((0x59, 0x04)));
        assert_eq!(attributes.usage(4), Some((0x59, 0x08)));
        assert_eq!(attributes.logical_max, i32::MAX);

        // Report 4 starts again at bit 0.
        let header = &c.fields[2];
        assert_eq!((header.report_id, header.bit_offset, header.count), (4, 0, 2));
        assert_eq!(header.collection, (0x59, 0x50));
        assert_eq!(header.usage(1), Some((0x59, 0x55)));
        let ids = &c.fields[3];
        assert_eq!((ids.bit_offset, ids.bit_size, ids.count), (16, 16, 4));
        assert_eq!(ids.usage(3), Some((0x59, 0x21)));
        let channels = &c.fields[4];
        assert_eq!(
            (channels.bit_offset, channels.bit_size, channels.count),
            (80, 8, 16)
        );
        assert_eq!(channels.usage(5), Some((0x59, 0x52)), "lamp 1, green");
        assert_eq!(channels.usage(15), Some((0x59, 0x54)), "lamp 3, intensity");
        assert!(!channels.is_constant());
    }

    #[test]
    fn usage_ranges_and_extended_usages() {
        let d = parse(KEYBOARD).unwrap();
        let fields = &d.collections[0].fields;
        let modifiers = &fields[0];
        assert_eq!(
            modifiers.usages,
            vec![UsageRange {
                page: 0x07,
                min: 0xE0,
                max: 0xE7
            }]
        );
        assert_eq!(modifiers.usage(3), Some((0x07, 0xE3)));
        assert!(fields[1].is_constant() && fields[1].usages.is_empty());
        let leds = fields.iter().find(|f| f.kind == ReportKind::Output).unwrap();
        assert_eq!((leds.bit_offset, leds.count), (0, 5));
        assert_eq!(leds.usage(4), Some((0x08, 0x05)));
        let keys = fields.last().unwrap();
        assert_eq!(keys.flags & tags::FLAG_VARIABLE, 0, "array field");
        assert_eq!((keys.bit_offset, keys.count, keys.logical_max), (16, 6, 0x65));

        // An extended (4-byte) usage carries its own page.
        let extended = [
            0x05, 0x01, // Usage Page (Generic Desktop)
            0x09, 0x00, // Usage (0)
            0xA1, 0x01, // Collection (Application)
            0x0B, 0x01, 0x00, 0x00, 0xFF, // Usage (0xFF00:0x0001)
            0x75, 0x08, 0x95, 0x01, // Report Size (8), Report Count (1)
            0x91, 0x02, //   Output (Data, Variable)
            0xC0,
        ];
        let d = parse(&extended).unwrap();
        assert_eq!(d.collections[0].fields[0].usage(0), Some((0xFF00, 0x0001)));
    }

    #[test]
    fn keyboard() {
        let d = parse(KEYBOARD).unwrap();
        assert_eq!(d.collections.len(), 1);
        let c = &d.collections[0];
        assert_eq!((c.index, c.usage_page, c.usage), (0, 0x01, 0x06));
        assert_eq!(c.collection_type, collection_types::APPLICATION);
        assert!(!c.uses_report_ids());
        assert_eq!(c.input_len, 9); // 8 modifier bits + 8 reserved + 6*8 keys = 64 bits → 8 + id byte
        assert_eq!(c.output_len, 2); // 5 LED bits + 3 padding = 8 bits → 1 + id byte
        assert_eq!(c.feature_len, 0);
    }

    #[test]
    fn mouse_with_nested_physical_collection() {
        let d = parse(MOUSE).unwrap();
        assert_eq!(
            d.collections.len(),
            1,
            "nested physical collection must not count as a TLC"
        );
        let c = &d.collections[0];
        assert_eq!((c.usage_page, c.usage), (0x01, 0x02));
        assert_eq!(c.input_len, 4); // 3 + 5 + 16 bits = 24 → 3 + id byte
        assert_eq!(c.output_len, 0);
    }

    #[test]
    fn vendor_defined_with_report_ids() {
        let d = parse(VENDOR).unwrap();
        let c = &d.collections[0];
        assert_eq!((c.usage_page, c.usage), (0xFF00, 0x01));
        assert_eq!(c.report_ids, vec![2, 3]);
        assert_eq!(c.input_len, 65);
        assert_eq!(c.output_len, 65);
        assert_eq!(c.feature_len, 33);
        let feature = c.reports.iter().find(|r| r.kind == ReportKind::Feature).unwrap();
        assert_eq!((feature.id, feature.bits), (3, 256));
        assert_eq!(d.find(0xFF00, 0x01).map(|c| c.index), Some(0));
    }

    #[test]
    fn two_top_level_collections_get_sequential_indices() {
        let mut bytes = KEYBOARD.to_vec();
        bytes.extend_from_slice(CONSUMER);
        let d = parse(&bytes).unwrap();
        assert_eq!(d.collections.len(), 2);
        assert_eq!(
            (
                d.collections[0].index,
                d.collections[0].usage_page,
                d.collections[0].usage
            ),
            (0, 0x01, 0x06)
        );
        assert_eq!(
            (
                d.collections[1].index,
                d.collections[1].usage_page,
                d.collections[1].usage
            ),
            (1, 0x0C, 0x01)
        );
        assert_eq!(d.collections[1].report_ids, vec![5]);
        assert_eq!(d.collections[1].input_len, 3);
        assert_eq!(d.collection(1).unwrap().usage, 0x01);
    }

    #[test]
    fn items_decode_sizes_and_sign() {
        let items = parse_items(MOUSE).unwrap();
        let logical_min = items
            .iter()
            .find(|i| i.kind == ItemKind::Global && i.tag == tags::LOGICAL_MIN && i.signed < 0)
            .unwrap();
        assert_eq!(logical_min.signed, -127);
        let page = parse_items(VENDOR).unwrap()[0];
        assert_eq!(
            (page.kind, page.tag, page.size, page.data),
            (ItemKind::Global, tags::USAGE_PAGE, 2, 0xFF00)
        );
    }

    #[test]
    fn errors() {
        assert_eq!(
            parse_items(&[0x06, 0x00]),
            Err(DescError::Truncated { offset: 0 })
        );
        assert_eq!(parse(&[0xC0]), Err(DescError::UnbalancedEnd { offset: 0 }));
        assert_eq!(
            parse(&[0x05, 0x01, 0x09, 0x06, 0xA1, 0x01]),
            Err(DescError::UnclosedCollections(1))
        );
        // Long item is skipped, not fatal.
        let items = parse_items(&[0xFE, 0x02, 0x10, 0xAA, 0xBB]).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(
            (items[0].kind, items[0].tag, items[0].size),
            (ItemKind::Reserved, 0x10, 2)
        );
    }
}
