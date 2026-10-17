// SPDX-License-Identifier: Apache-2.0 OR MIT
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
//!   (the same convention as Windows `HIDP_CAPS.*ReportByteLength` and hidapi).

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
    report_size: u32,
    report_count: u32,
    report_id: u8,
}

#[derive(Default)]
struct LocalState {
    usages: Vec<(u16, u16)>, // (page override or 0, usage)
}

/// Parse a descriptor and compute its top-level collections.
pub fn parse(bytes: &[u8]) -> Result<ReportDescriptor, DescError> {
    let items = parse_items(bytes)?;
    let mut collections: Vec<TopLevelCollection> = Vec::new();
    let mut global = GlobalState::default();
    let mut stack: Vec<GlobalState> = Vec::new();
    let mut local = LocalState::default();
    let mut depth = 0usize;
    let mut current: Option<usize> = None; // index into `collections`

    for item in &items {
        match item.kind {
            ItemKind::Global => match item.tag {
                tags::USAGE_PAGE => global.usage_page = item.data as u16,
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
                    if item.size == 4 {
                        local.usages.push(((item.data >> 16) as u16, item.data as u16));
                    } else {
                        local.usages.push((0, item.data as u16));
                    }
                }
                tags::USAGE_MIN => {
                    // Record the minimum as the representative usage of a range.
                    if item.size == 4 {
                        local.usages.push(((item.data >> 16) as u16, item.data as u16));
                    } else {
                        local.usages.push((0, item.data as u16));
                    }
                }
                _ => {}
            },
            ItemKind::Main => {
                match item.tag {
                    tags::COLLECTION => {
                        if depth == 0 {
                            let (page_override, usage) = local.usages.first().copied().unwrap_or((0, 0));
                            let usage_page = if page_override != 0 {
                                page_override
                            } else {
                                global.usage_page
                            };
                            collections.push(TopLevelCollection {
                                index: collections.len() as u16,
                                usage_page,
                                usage,
                                collection_type: item.data as u8,
                                report_ids: Vec::new(),
                                input_len: 0,
                                output_len: 0,
                                feature_len: 0,
                                reports: Vec::new(),
                            });
                            current = Some(collections.len() - 1);
                        }
                        depth += 1;
                    }
                    tags::END_COLLECTION => {
                        if depth == 0 {
                            return Err(DescError::UnbalancedEnd { offset: item.offset });
                        }
                        depth -= 1;
                        if depth == 0 {
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
                            match tlc.reports.iter_mut().find(|r| r.kind == kind && r.id == id) {
                                Some(r) => r.bits = r.bits.saturating_add(bits),
                                None => tlc.reports.push(ReportInfo { kind, id, bits }),
                            }
                        }
                    }
                    _ => {}
                }
                local = LocalState::default();
            }
            ItemKind::Reserved => {}
        }
    }

    if depth != 0 {
        return Err(DescError::UnclosedCollections(depth));
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
