// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Writer and reader for the shared-memory frame ring described in
//! [`lightloom_protocol::shm`] (64-byte header + 3 BGRA slots, per-slot seqlock).
//!
//! The region is a memory-mapped file ([`memmap2`]); `lightloom-renderer` maps it
//! read-write and the daemon maps the same file read-only. Every word that can
//! be touched by the other side while it reads is accessed atomically: header
//! words as `AtomicU64`, pixels as `AtomicU32` (one BGRA pixel per word). The
//! writer follows the protocol from the header docs:
//!
//! 1. `gen[slot]` := odd, `fence(Release)`
//! 2. pixels (relaxed stores)
//! 3. `gen[slot]` := next even (Release), then `seq` := `seq + 1` (Release)
//!
//! and the reader: load `seq` (Acquire) → slot, load `gen` (Acquire, must be
//! even), copy pixels (relaxed loads), `fence(Acquire)`, re-load `gen`; equal ⇒
//! the copy is a consistent frame, otherwise retry.
//!
//! Header words hold little-endian values (the `lightloom_protocol::shm` layout); they are
//! converted with `to_le`/`from_le`, which compiles to nothing on x86_64/aarch64.
//!
//! Generations count completed writes: a fresh ring starts at 0 and every publish adds 2
//! (odd while writing, next even when done), so the frame `seq` lives in slot
//! `(seq - 1) % 3` with generation [`expected_generation`]`(seq)`. The reader uses that to
//! notice a writer that lapped it (rewrote the slot for a newer frame) between its `seq` and
//! generation loads, so the `seq` it returns always labels the pixels it copied.

use lightloom_protocol::shm::{FrameHeader, ShmError, BYTES_PER_PIXEL, HEADER_BYTES, SLOT_COUNT};
use memmap2::{Mmap, MmapMut};
use std::fs::OpenOptions;
use std::path::Path;
use std::sync::atomic::{fence, AtomicU32, AtomicU64, Ordering};

/// Byte offset of `seq` in the header.
const SEQ_OFFSET: usize = 16;
/// Byte offset of `slot_generations[0]` in the header.
const GEN_OFFSET: usize = 24;
/// Reader retries before giving up with [`RingError::Contended`].
const READ_RETRIES: usize = 64;

/// Errors from the ring writer/reader.
#[derive(Debug, thiserror::Error)]
pub enum RingError {
    /// Mapping shorter than the header + 3 slots.
    #[error("shm region too small: need {need} bytes, have {have}")]
    TooSmall {
        /// Bytes required.
        need: usize,
        /// Bytes mapped.
        have: usize,
    },
    /// Mapping base is not 8-byte aligned (mmap bases always are; anything else is a bug).
    #[error("shm region is not 8-byte aligned")]
    Misaligned,
    /// Zero width or height.
    #[error("invalid canvas size {0}x{1}")]
    InvalidSize(u32, u32),
    /// A frame whose dimensions differ from the ring's.
    #[error("frame is {got_w}x{got_h}, ring expects {want_w}x{want_h}")]
    SizeMismatch {
        /// Frame width.
        got_w: u32,
        /// Frame height.
        got_h: u32,
        /// Ring width.
        want_w: u32,
        /// Ring height.
        want_h: u32,
    },
    /// Pixel buffer length does not match `width * height * 4`.
    #[error("pixel buffer is {got} bytes, expected {want}")]
    BufferLength {
        /// Bytes supplied.
        got: usize,
        /// Bytes expected.
        want: usize,
    },
    /// Header validation failed.
    #[error(transparent)]
    Header(#[from] ShmError),
    /// File / mapping I/O.
    #[error("shm i/o: {0}")]
    Io(#[from] std::io::Error),
    /// The reader saw a write in progress on every retry.
    #[error("shm reader gave up after {READ_RETRIES} torn reads")]
    Contended,
}

fn check_region(base: *const u8, len: usize, header: &FrameHeader) -> Result<(), RingError> {
    if header.width == 0 || header.height == 0 {
        return Err(RingError::InvalidSize(header.width, header.height));
    }
    if len < header.total_bytes() {
        return Err(RingError::TooSmall {
            need: header.total_bytes(),
            have: len,
        });
    }
    if !(base as usize).is_multiple_of(8) {
        return Err(RingError::Misaligned);
    }
    Ok(())
}

/// # Safety
/// `base` must point to a live mapping of at least `HEADER_BYTES` bytes that is 8-byte aligned,
/// and every concurrent access to the header words must be atomic.
unsafe fn header_word<'a>(base: *mut u8, offset: usize) -> &'a AtomicU64 {
    debug_assert!(offset + 8 <= HEADER_BYTES && offset.is_multiple_of(8));
    // SAFETY: caller guarantees the mapping is live, large enough and 8-aligned; offset is a
    // multiple of 8 inside the header, so the pointer is valid and aligned for AtomicU64.
    unsafe { AtomicU64::from_ptr(base.add(offset).cast::<u64>()) }
}

/// # Safety
/// `base` must point to a live, 8-byte aligned mapping covering `header.total_bytes()`, and every
/// concurrent access to the slot must be atomic.
unsafe fn slot_words<'a>(base: *mut u8, header: &FrameHeader, slot: usize) -> &'a [AtomicU32] {
    debug_assert!(slot < SLOT_COUNT);
    let words = header.slot_bytes() / BYTES_PER_PIXEL;
    // SAFETY: slot_offset = 64 + slot * w*h*4 is a multiple of 4 inside the mapping (checked by
    // check_region), the base is 8-aligned, so the range is valid and aligned for AtomicU32;
    // AtomicU32 has the same layout as u32 and permits shared mutation.
    unsafe { std::slice::from_raw_parts(base.add(header.slot_offset(slot)).cast::<AtomicU32>(), words) }
}

/// Load a little-endian header word.
fn load_le(word: &AtomicU64, order: Ordering) -> u64 {
    u64::from_le(word.load(order))
}

/// Store a header word as little-endian.
fn store_le(word: &AtomicU64, value: u64, order: Ordering) {
    word.store(value.to_le(), order);
}

/// Generation stamp slot `(seq - 1) % 3` carries once frame `seq` (1-based) is complete:
/// 2 per write of that slot so far.
pub fn expected_generation(seq: u64) -> u64 {
    debug_assert!(seq > 0);
    2 * ((seq - 1) / SLOT_COUNT as u64 + 1)
}

/// Snapshot of the header taken with atomic loads (safe against a concurrent writer).
///
/// # Safety
/// Same as [`header_word`].
unsafe fn load_header_bytes(base: *mut u8) -> [u8; HEADER_BYTES] {
    let mut out = [0u8; HEADER_BYTES];
    for (i, chunk) in out.as_chunks_mut::<8>().0.iter_mut().enumerate() {
        // SAFETY: forwarded from the caller; i * 8 < HEADER_BYTES.
        let word = unsafe { header_word(base, i * 8) }.load(Ordering::Acquire);
        chunk.copy_from_slice(&word.to_ne_bytes());
    }
    out
}

/// Writes frames into the ring. One writer per region.
pub struct RingWriter {
    /// Keeps the mapping alive (and flushes it); pixels/header are only touched through `base`.
    map: MmapMut,
    base: *mut u8,
    header: FrameHeader,
}

// SAFETY: `base` points into `map`, which the writer owns and which moves with it; all accesses
// go through `&mut self` (publish) or atomics, so sending the writer to another thread is sound.
unsafe impl Send for RingWriter {}

impl RingWriter {
    /// Create (or truncate) `path`, size it for a `width`x`height` ring and map it read-write.
    pub fn create_file(path: &Path, width: u32, height: u32) -> Result<Self, RingError> {
        let header = FrameHeader::new(width, height);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        file.set_len(header.total_bytes() as u64)?;
        // SAFETY: the file was just created/truncated and sized by us. Other processes are
        // expected to map it read-only and follow the seqlock protocol; if one truncated it
        // while mapped, accesses would fault — the renderer owns the file, as documented.
        let map = unsafe { MmapMut::map_mut(&file)? };
        Self::from_map(map, width, height)
    }

    /// Anonymous (process-private) ring, for tests and benchmarks.
    pub fn anonymous(width: u32, height: u32) -> Result<Self, RingError> {
        let header = FrameHeader::new(width, height);
        let map = MmapMut::map_anon(header.total_bytes())?;
        Self::from_map(map, width, height)
    }

    /// Initialise an existing mapping: header with `seq = 0`, all generations 0, slots zeroed.
    pub fn from_map(mut map: MmapMut, width: u32, height: u32) -> Result<Self, RingError> {
        let header = FrameHeader::new(width, height);
        let base = map.as_mut_ptr();
        check_region(base, map.len(), &header)?;
        // Nobody may read before the header is valid: zero the slots first, then the header
        // words (magic last-but-not-least is irrelevant — a reader validates magic/version).
        // SAFETY: base/len were validated; the slot range lies inside the mapping.
        unsafe {
            std::ptr::write_bytes(base.add(HEADER_BYTES), 0, header.total_bytes() - HEADER_BYTES);
        }
        let bytes = header.to_bytes();
        for (i, chunk) in bytes.as_chunks::<8>().0.iter().enumerate() {
            let mut word = [0u8; 8];
            word.copy_from_slice(chunk);
            // SAFETY: header lies inside the validated, aligned mapping.
            unsafe { header_word(base, i * 8) }.store(u64::from_ne_bytes(word), Ordering::Release);
        }
        Ok(Self { map, base, header })
    }

    /// Ring geometry (the `seq`/generation fields are those at creation time).
    pub fn geometry(&self) -> FrameHeader {
        self.header
    }

    /// Frames published so far.
    pub fn seq(&self) -> u64 {
        // SAFETY: base is the validated mapping owned by self.
        load_le(unsafe { header_word(self.base, SEQ_OFFSET) }, Ordering::Acquire)
    }

    /// Current header as another process would see it.
    pub fn header(&self) -> Result<FrameHeader, RingError> {
        // SAFETY: base is the validated mapping owned by self.
        let bytes = unsafe { load_header_bytes(self.base) };
        Ok(FrameHeader::from_bytes(&bytes)?)
    }

    /// Publish one BGRA frame (`width * height * 4` bytes, tightly packed, top-left origin).
    /// Returns the new `seq`.
    pub fn publish(&mut self, bgra: &[u8], width: u32, height: u32) -> Result<u64, RingError> {
        if width != self.header.width || height != self.header.height {
            return Err(RingError::SizeMismatch {
                got_w: width,
                got_h: height,
                want_w: self.header.width,
                want_h: self.header.height,
            });
        }
        let want = self.header.slot_bytes();
        if bgra.len() != want {
            return Err(RingError::BufferLength {
                got: bgra.len(),
                want,
            });
        }
        // SAFETY: base is the validated mapping owned by self; offsets are inside the header.
        let seq_word = unsafe { header_word(self.base, SEQ_OFFSET) };
        let seq = load_le(seq_word, Ordering::Relaxed);
        let slot = (seq % SLOT_COUNT as u64) as usize;
        // SAFETY: as above; slot < SLOT_COUNT so the generation word is inside the header.
        let gen_word = unsafe { header_word(self.base, GEN_OFFSET + slot * 8) };
        // The single writer always leaves an even stamp; `| 1` makes it odd (= +1).
        let writing = load_le(gen_word, Ordering::Relaxed) | 1;
        store_le(gen_word, writing, Ordering::Relaxed);
        fence(Ordering::Release);
        // SAFETY: base is the validated mapping; slot < SLOT_COUNT.
        let dst = unsafe { slot_words(self.base, &self.header, slot) };
        for (word, px) in dst.iter().zip(bgra.as_chunks::<BYTES_PER_PIXEL>().0) {
            word.store(u32::from_ne_bytes(*px), Ordering::Relaxed);
        }
        store_le(gen_word, writing.wrapping_add(1), Ordering::Release);
        store_le(seq_word, seq + 1, Ordering::Release);
        Ok(seq + 1)
    }

    /// Flush dirty pages of a file-backed ring to disk (for post-run inspection).
    pub fn flush(&self) -> Result<(), RingError> {
        self.map.flush()?;
        Ok(())
    }
}

/// Reads the latest consistent frame from a ring written by another thread or process.
pub struct RingReader {
    _map: Mmap,
    base: *mut u8,
    header: FrameHeader,
}

// SAFETY: `base` points into `_map`, owned by the reader; it is only accessed through atomics.
unsafe impl Send for RingReader {}

impl RingReader {
    /// Map `path` read-only and validate its header.
    pub fn open_file(path: &Path) -> Result<Self, RingError> {
        let file = OpenOptions::new().read(true).open(path)?;
        // SAFETY: read-only mapping; concurrent modification by the writer is the point of the
        // seqlock and all accesses below are atomic. Truncation by the owner would fault.
        let map = unsafe { Mmap::map(&file)? };
        Self::from_map(map)
    }

    /// Validate an existing read-only mapping.
    pub fn from_map(map: Mmap) -> Result<Self, RingError> {
        let base = map.as_ptr().cast_mut();
        if map.len() < HEADER_BYTES {
            return Err(ShmError::TooShort(map.len()).into());
        }
        if !(base as usize).is_multiple_of(8) {
            return Err(RingError::Misaligned);
        }
        // SAFETY: mapping is live, aligned and at least one header long.
        let header = FrameHeader::from_bytes(&unsafe { load_header_bytes(base) })?;
        check_region(base, map.len(), &header)?;
        Ok(Self {
            _map: map,
            base,
            header,
        })
    }

    /// Geometry from the header (width/height never change after creation).
    pub fn geometry(&self) -> FrameHeader {
        self.header
    }

    /// Current header (live `seq` and generations).
    pub fn header(&self) -> Result<FrameHeader, RingError> {
        // SAFETY: validated mapping owned by self.
        Ok(FrameHeader::from_bytes(&unsafe { load_header_bytes(self.base) })?)
    }

    /// Copy the most recent frame into `out` (`slot_bytes` long). `Ok(None)` before the first
    /// frame; otherwise the `seq` of the frame copied.
    pub fn read_latest(&self, out: &mut [u8]) -> Result<Option<u64>, RingError> {
        let want = self.header.slot_bytes();
        if out.len() != want {
            return Err(RingError::BufferLength { got: out.len(), want });
        }
        // SAFETY: validated mapping owned by self; offsets inside the header.
        let seq_word = unsafe { header_word(self.base, SEQ_OFFSET) };
        for _ in 0..READ_RETRIES {
            let seq = load_le(seq_word, Ordering::Acquire);
            if seq == 0 {
                return Ok(None);
            }
            let slot = ((seq - 1) % SLOT_COUNT as u64) as usize;
            // SAFETY: as above, slot < SLOT_COUNT.
            let gen_word = unsafe { header_word(self.base, GEN_OFFSET + slot * 8) };
            let before = load_le(gen_word, Ordering::Acquire);
            // Odd: being written. Newer than `seq`'s stamp: the writer lapped us and the slot
            // now holds frame seq + 3k; re-read `seq` so the label matches the pixels.
            if FrameHeader::is_writing(before) || before != expected_generation(seq) {
                std::hint::spin_loop();
                continue;
            }
            // SAFETY: validated mapping; slot < SLOT_COUNT.
            let src = unsafe { slot_words(self.base, &self.header, slot) };
            for (px, word) in out.as_chunks_mut::<BYTES_PER_PIXEL>().0.iter_mut().zip(src) {
                px.copy_from_slice(&word.load(Ordering::Relaxed).to_ne_bytes());
            }
            fence(Ordering::Acquire);
            if load_le(gen_word, Ordering::Relaxed) == before {
                return Ok(Some(seq));
            }
            std::hint::spin_loop();
        }
        Err(RingError::Contended)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generations_count_writes_and_header_is_little_endian() {
        assert_eq!(expected_generation(1), 2);
        assert_eq!(expected_generation(3), 2);
        assert_eq!(expected_generation(4), 4);
        assert_eq!(expected_generation(120), 80);
        let (w, h) = (4u32, 2u32);
        let mut ring = RingWriter::anonymous(w, h).unwrap();
        let frame = vec![7u8; (w * h * 4) as usize];
        for seq in 1..=10u64 {
            assert_eq!(ring.publish(&frame, w, h).unwrap(), seq);
            // header() decodes the raw bytes as little-endian (protocol layout)
            let header = ring.header().unwrap();
            assert_eq!(header.seq, seq);
            let slot = header.latest_slot().unwrap();
            assert_eq!(header.slot_generations[slot], expected_generation(seq));
        }
    }

    /// Read-only ring whose slots are filled with `slot_fill[i]`, header `seq`/`gens` as given.
    fn crafted(seq: u64, gens: [u64; SLOT_COUNT], slot_fill: [u8; SLOT_COUNT]) -> RingReader {
        let (w, h) = (4u32, 2u32);
        let mut header = FrameHeader::new(w, h);
        header.seq = seq;
        header.slot_generations = gens;
        let mut map = MmapMut::map_anon(header.total_bytes()).unwrap();
        map[..HEADER_BYTES].copy_from_slice(&header.to_bytes());
        for (slot, fill) in slot_fill.iter().enumerate() {
            let at = header.slot_offset(slot);
            map[at..at + header.slot_bytes()].fill(*fill);
        }
        RingReader::from_map(map.make_read_only().unwrap()).unwrap()
    }

    #[test]
    fn reader_labels_match_pixels_when_the_writer_laps() {
        let mut out = vec![0u8; 4 * 2 * 4];
        // seq 4 complete: slot 0 holds frame 4 (gen 4), slots 1/2 frames 2/3 (gen 2).
        let ring = crafted(4, [4, 2, 2], [4, 2, 3]);
        assert_eq!(ring.read_latest(&mut out).unwrap(), Some(4));
        assert!(out.iter().all(|&b| b == 4));
        // `seq` still says 1 but slot 0 was already rewritten with frame 4 (gen 4): a reader that
        // trusted `seq` would hand out frame 4's pixels labelled as frame 1. It must not.
        let lapped = crafted(1, [4, 2, 2], [4, 2, 3]);
        assert!(matches!(lapped.read_latest(&mut out), Err(RingError::Contended)));
        // mid-write slot (odd stamp) is never copied either
        let writing = crafted(4, [5, 2, 2], [4, 2, 3]);
        assert!(matches!(writing.read_latest(&mut out), Err(RingError::Contended)));
    }
}
