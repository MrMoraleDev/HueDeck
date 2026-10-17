// SPDX-License-Identifier: Apache-2.0 OR MIT
//
// BinaryUtils: little/big-endian 16/32-bit packing helpers in the exact shape
// SignalRGB-era plugins expect.
//
// Corpus survey (2026-10-17, 1006 plugin .js files: plugin_cdn/main 341,
// Signal-x64 462, addons 203). No plugin imports BinaryUtils from the host:
// 27 files carry their own inline `class BinaryUtils` (one is commented
// "Inlined from signal-plugins-internal/CodeSnippets/BinaryUtils.js"), all 27
// exposing the same eight static members. Call-site counts across the corpus:
//   ReadInt16BigEndian     68   e.g. ReadInt16BigEndian([pkt[12], pkt[13]])
//   ReadInt16LittleEndian  21   e.g. ReadInt16LittleEndian(data.slice(3, 5))
//   WriteInt32LittleEndian 19   e.g. [..., ...WriteInt32LittleEndian(len)]
//   ReadInt32LittleEndian  14   e.g. ReadInt32LittleEndian(data.slice(4, 7))  <- 3 bytes
//   WriteInt16LittleEndian 11
//   WriteInt16BigEndian     6
//   ReadInt32BigEndian      0 call sites (declared in every inline class)
//   WriteInt32BigEndian     0 call sites (declared in every inline class)
//
// Semantics mirrored from those call shapes (re-expressed, not copied):
//   * Write* return plain Arrays of bytes (plugins .concat() them). Values
//     are truncated to 32 bits with two's complement, so negative numbers
//     pack as expected (WriteInt16LittleEndian(-1) -> [0xff, 0xff]).
//   * Read* accept any indexable (Array, Uint8Array, Buffer). Missing elements
//     count as 0, so 3-byte slices read fine. Every byte is masked to 0..255.
//   * ReadInt16* return an unsigned 0..65535 (the corpus never sign-extends
//     16-bit reads). ReadInt32* return a signed int32, which is what the
//     corpus arithmetic `(b3 << 24) & 0xff000000` produced.
//   * ReadInt16BigEndian reverses the first two elements and reads LE, so a
//     one-element input yields that byte unchanged. ReadInt32BigEndian pads a
//     short input with trailing zeros to 4 bytes before reversing (so a
//     one-element input is the most-significant byte). Both quirks are kept
//     on purpose for byte-identical behaviour with the inline helper; the
//     input array is never mutated here.
//
// This module is the shim the Lightloom SDK and first-party plugins import
// (`import BinaryUtils from "./BinaryUtils.js"` or the host specifier
// "@lightloom/BinaryUtils"). It must not be injected as a global: imported
// plugins define their own class of the same name in module scope.

function byteAt(array, index) {
  if (array == null) {
    return 0;
  }
  const v = array[index];
  return v == null ? 0 : v & 0xff;
}

function lengthOf(array) {
  if (array == null) {
    return 0;
  }
  const n = array.length;
  return typeof n === "number" && n > 0 ? n : 0;
}

export class BinaryUtils {
  /** 16-bit value -> [low, high]. */
  static WriteInt16LittleEndian(value) {
    const v = value | 0;
    return [v & 0xff, (v >> 8) & 0xff];
  }

  /** 16-bit value -> [high, low]. */
  static WriteInt16BigEndian(value) {
    const v = value | 0;
    return [(v >> 8) & 0xff, v & 0xff];
  }

  /** 32-bit value -> [b0, b1, b2, b3] least significant first. */
  static WriteInt32LittleEndian(value) {
    const v = value | 0;
    return [v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff, (v >>> 24) & 0xff];
  }

  /** 32-bit value -> [b3, b2, b1, b0] most significant first. */
  static WriteInt32BigEndian(value) {
    const v = value | 0;
    return [(v >>> 24) & 0xff, (v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff];
  }

  /** [low, high] -> unsigned 0..65535. Missing bytes read as 0. */
  static ReadInt16LittleEndian(array) {
    return byteAt(array, 0) | (byteAt(array, 1) << 8);
  }

  /** [high, low] -> unsigned 0..65535 (first two elements reversed, then LE). */
  static ReadInt16BigEndian(array) {
    if (lengthOf(array) >= 2) {
      return byteAt(array, 1) | (byteAt(array, 0) << 8);
    }
    // One element: reversing a 1-element slice is a no-op, so it is read as
    // the low byte (byte-identical with the inline corpus helper).
    return byteAt(array, 0);
  }

  /** [b0, b1, b2, b3] -> signed int32. Missing bytes read as 0. */
  static ReadInt32LittleEndian(array) {
    return (byteAt(array, 0) | (byteAt(array, 1) << 8) | (byteAt(array, 2) << 16) | (byteAt(array, 3) << 24)) | 0;
  }

  /** [b3, b2, b1, b0] -> signed int32. Short input is zero-padded at the end first. */
  static ReadInt32BigEndian(array) {
    const padded = [byteAt(array, 0), byteAt(array, 1), byteAt(array, 2), byteAt(array, 3)];
    return BinaryUtils.ReadInt32LittleEndian(padded.reverse());
  }
}

export const WriteInt16LittleEndian = BinaryUtils.WriteInt16LittleEndian;
export const WriteInt16BigEndian = BinaryUtils.WriteInt16BigEndian;
export const WriteInt32LittleEndian = BinaryUtils.WriteInt32LittleEndian;
export const WriteInt32BigEndian = BinaryUtils.WriteInt32BigEndian;
export const ReadInt16LittleEndian = BinaryUtils.ReadInt16LittleEndian;
export const ReadInt16BigEndian = BinaryUtils.ReadInt16BigEndian;
export const ReadInt32LittleEndian = BinaryUtils.ReadInt32LittleEndian;
export const ReadInt32BigEndian = BinaryUtils.ReadInt32BigEndian;

export default BinaryUtils;
