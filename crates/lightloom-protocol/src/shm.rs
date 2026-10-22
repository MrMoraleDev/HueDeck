// SPDX-License-Identifier: GPL-3.0-or-later
//! Shared-memory frame ring between `lightloom-renderer` and `lightloomd`.
//!
//! ```text
//! offset  size  field
//! 0       4     magic "HDFR"
//! 4       4     version (u32 LE)
//! 8       4     width   (u32 LE)
//! 12      4     height  (u32 LE)
//! 16      8     seq     (u64 LE)  — bumped once per published frame
//! 24      24    slot_generations[3] (u64 LE each) — seqlock stamps
//! 48      16    reserved (zero)
//! 64      …     slot 0, slot 1, slot 2 (each width*height*4 bytes BGRA)
//! ```
//!
//! Writer protocol per frame: pick `slot = seq % 3`, set
//! `slot_generations[slot]` to an odd value, copy pixels, set it to the next
//! even value, then store `seq + 1`. A reader copies the slot and re-checks the
//! generation is unchanged and even; otherwise it retries (torn read).

use serde::{Deserialize, Serialize};

/// Magic bytes at offset 0.
pub const FRAME_MAGIC: [u8; 4] = *b"HDFR";
/// Current header version.
pub const SHM_VERSION: u32 = 1;
/// Number of frame slots.
pub const SLOT_COUNT: usize = 3;
/// Canvas width every lightscript is rendered at.
pub const CANVAS_WIDTH: u32 = 320;
/// Canvas height every lightscript is rendered at.
pub const CANVAS_HEIGHT: u32 = 200;
/// BGRA8.
pub const BYTES_PER_PIXEL: usize = 4;
/// Fixed header size in bytes.
pub const HEADER_BYTES: usize = 64;
/// Bytes of one 320x200 BGRA slot.
pub const FRAME_SLOT_BYTES: usize = CANVAS_WIDTH as usize * CANVAS_HEIGHT as usize * BYTES_PER_PIXEL;

/// `engine.audio.freq` bin count.
pub const AUDIO_FREQ_BINS: usize = 200;
/// `engine.zone` grid columns (28x20 = 560 cells).
pub const ZONE_COLS: usize = 28;
/// `engine.zone` grid rows.
pub const ZONE_ROWS: usize = 20;
/// `engine.zone.imagedata` width.
pub const ZONE_IMAGE_WIDTH: usize = 160;
/// `engine.zone.imagedata` height.
pub const ZONE_IMAGE_HEIGHT: usize = 100;

/// Errors from [`FrameHeader::from_bytes`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ShmError {
    /// Buffer shorter than [`HEADER_BYTES`].
    #[error("shm header too short: {0} bytes")]
    TooShort(usize),
    /// Magic bytes mismatch.
    #[error("bad shm magic {0:?}")]
    BadMagic([u8; 4]),
    /// Version not understood by this build.
    #[error("unsupported shm version {0}")]
    UnsupportedVersion(u32),
}

/// The header at the start of the shared-memory region.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameHeader {
    /// [`FRAME_MAGIC`].
    pub magic: [u8; 4],
    /// [`SHM_VERSION`].
    pub version: u32,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Number of frames published so far; `seq % SLOT_COUNT` is the slot of the
    /// most recent one (when `seq > 0`).
    pub seq: u64,
    /// Seqlock stamps, one per slot (odd = being written).
    pub slot_generations: [u64; SLOT_COUNT],
}

impl FrameHeader {
    /// Header for an empty ring of the given size.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            magic: FRAME_MAGIC,
            version: SHM_VERSION,
            width,
            height,
            seq: 0,
            slot_generations: [0; SLOT_COUNT],
        }
    }

    /// Bytes of one slot for this header's dimensions.
    pub fn slot_bytes(&self) -> usize {
        self.width as usize * self.height as usize * BYTES_PER_PIXEL
    }

    /// Byte offset of `slot` within the region.
    pub fn slot_offset(&self, slot: usize) -> usize {
        HEADER_BYTES + slot * self.slot_bytes()
    }

    /// Total region size.
    pub fn total_bytes(&self) -> usize {
        HEADER_BYTES + SLOT_COUNT * self.slot_bytes()
    }

    /// Slot the writer will use for the next frame.
    pub fn next_slot(&self) -> usize {
        (self.seq % SLOT_COUNT as u64) as usize
    }

    /// Slot holding the latest published frame, `None` before the first frame.
    pub fn latest_slot(&self) -> Option<usize> {
        if self.seq == 0 {
            None
        } else {
            Some(((self.seq - 1) % SLOT_COUNT as u64) as usize)
        }
    }

    /// Whether a generation stamp says the slot is mid-write.
    pub fn is_writing(generation: u64) -> bool {
        generation & 1 == 1
    }

    /// Fixed little-endian encoding.
    pub fn to_bytes(&self) -> [u8; HEADER_BYTES] {
        let mut out = [0u8; HEADER_BYTES];
        out[0..4].copy_from_slice(&self.magic);
        out[4..8].copy_from_slice(&self.version.to_le_bytes());
        out[8..12].copy_from_slice(&self.width.to_le_bytes());
        out[12..16].copy_from_slice(&self.height.to_le_bytes());
        out[16..24].copy_from_slice(&self.seq.to_le_bytes());
        for (i, g) in self.slot_generations.iter().enumerate() {
            let at = 24 + i * 8;
            out[at..at + 8].copy_from_slice(&g.to_le_bytes());
        }
        out
    }

    /// Decode and validate the fixed layout.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ShmError> {
        if bytes.len() < HEADER_BYTES {
            return Err(ShmError::TooShort(bytes.len()));
        }
        let mut magic = [0u8; 4];
        magic.copy_from_slice(&bytes[0..4]);
        if magic != FRAME_MAGIC {
            return Err(ShmError::BadMagic(magic));
        }
        let u32_at = |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
        let u64_at = |at: usize| {
            let mut b = [0u8; 8];
            b.copy_from_slice(&bytes[at..at + 8]);
            u64::from_le_bytes(b)
        };
        let version = u32_at(4);
        if version != SHM_VERSION {
            return Err(ShmError::UnsupportedVersion(version));
        }
        let mut slot_generations = [0u64; SLOT_COUNT];
        for (i, g) in slot_generations.iter_mut().enumerate() {
            *g = u64_at(24 + i * 8);
        }
        Ok(Self {
            magic,
            version,
            width: u32_at(8),
            height: u32_at(12),
            seq: u64_at(16),
            slot_generations,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants() {
        assert_eq!(FRAME_SLOT_BYTES, 256_000);
        let h = FrameHeader::new(CANVAS_WIDTH, CANVAS_HEIGHT);
        assert_eq!(h.total_bytes(), HEADER_BYTES + 3 * 256_000);
        assert_eq!(h.slot_offset(2), HEADER_BYTES + 2 * 256_000);
    }

    #[test]
    fn byte_layout_round_trip() {
        let mut h = FrameHeader::new(320, 200);
        h.seq = 7;
        h.slot_generations = [2, 4, 5];
        let bytes = h.to_bytes();
        assert_eq!(&bytes[0..4], b"HDFR");
        assert_eq!(FrameHeader::from_bytes(&bytes).unwrap(), h);
        assert_eq!(h.latest_slot(), Some(0));
        assert_eq!(h.next_slot(), 1);
        assert!(FrameHeader::is_writing(h.slot_generations[2]));
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(FrameHeader::from_bytes(&[0; 10]), Err(ShmError::TooShort(10)));
        let mut bytes = FrameHeader::new(320, 200).to_bytes();
        bytes[0] = b'X';
        assert!(matches!(
            FrameHeader::from_bytes(&bytes),
            Err(ShmError::BadMagic(_))
        ));
        let mut bytes = FrameHeader::new(320, 200).to_bytes();
        bytes[4] = 9;
        assert_eq!(
            FrameHeader::from_bytes(&bytes),
            Err(ShmError::UnsupportedVersion(9))
        );
    }

    #[test]
    fn postcard_round_trip() {
        let h = FrameHeader::new(320, 200);
        let enc = crate::encode(&h).unwrap();
        let dec: FrameHeader = crate::decode(&enc).unwrap();
        assert_eq!(dec, h);
    }
}
