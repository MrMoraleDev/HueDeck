// SPDX-License-Identifier: Apache-2.0 OR MIT
import { test } from "node:test";
import assert from "node:assert/strict";
import BinaryUtils, {
  BinaryUtils as Named,
  ReadInt16BigEndian,
  WriteInt32LittleEndian,
} from "../BinaryUtils.js";

test("default and named exports are the same class and expose the 8 corpus members", () => {
  assert.equal(BinaryUtils, Named);
  for (const m of [
    "WriteInt16LittleEndian",
    "WriteInt16BigEndian",
    "WriteInt32LittleEndian",
    "WriteInt32BigEndian",
    "ReadInt16LittleEndian",
    "ReadInt16BigEndian",
    "ReadInt32LittleEndian",
    "ReadInt32BigEndian",
  ]) {
    assert.equal(typeof BinaryUtils[m], "function", m);
  }
  assert.equal(ReadInt16BigEndian, BinaryUtils.ReadInt16BigEndian);
  assert.equal(WriteInt32LittleEndian, BinaryUtils.WriteInt32LittleEndian);
});

test("WriteInt16LittleEndian / BigEndian", () => {
  assert.deepEqual(BinaryUtils.WriteInt16LittleEndian(0x1234), [0x34, 0x12]);
  assert.deepEqual(BinaryUtils.WriteInt16BigEndian(0x1234), [0x12, 0x34]);
  assert.deepEqual(BinaryUtils.WriteInt16LittleEndian(0), [0, 0]);
  assert.deepEqual(BinaryUtils.WriteInt16LittleEndian(0xffff), [0xff, 0xff]);
  // overflow truncates to 16 bits
  assert.deepEqual(BinaryUtils.WriteInt16LittleEndian(0x1abcd), [0xcd, 0xab]);
  // negative numbers pack as two's complement
  assert.deepEqual(BinaryUtils.WriteInt16LittleEndian(-1), [0xff, 0xff]);
  assert.deepEqual(BinaryUtils.WriteInt16LittleEndian(-2), [0xfe, 0xff]);
  assert.deepEqual(BinaryUtils.WriteInt16BigEndian(-32768), [0x80, 0x00]);
  // non-integers are truncated toward zero; NaN/undefined become 0
  assert.deepEqual(BinaryUtils.WriteInt16LittleEndian(258.9), [0x02, 0x01]);
  assert.deepEqual(BinaryUtils.WriteInt16LittleEndian(undefined), [0, 0]);
  assert.deepEqual(BinaryUtils.WriteInt16LittleEndian(NaN), [0, 0]);
});

test("WriteInt32LittleEndian / BigEndian", () => {
  assert.deepEqual(BinaryUtils.WriteInt32LittleEndian(0x12345678), [0x78, 0x56, 0x34, 0x12]);
  assert.deepEqual(BinaryUtils.WriteInt32BigEndian(0x12345678), [0x12, 0x34, 0x56, 0x78]);
  assert.deepEqual(BinaryUtils.WriteInt32LittleEndian(-1), [0xff, 0xff, 0xff, 0xff]);
  assert.deepEqual(BinaryUtils.WriteInt32LittleEndian(-2147483648), [0, 0, 0, 0x80]);
  assert.deepEqual(BinaryUtils.WriteInt32BigEndian(-2147483648), [0x80, 0, 0, 0]);
  assert.deepEqual(BinaryUtils.WriteInt32LittleEndian(0xffffffff), [0xff, 0xff, 0xff, 0xff]);
  assert.deepEqual(BinaryUtils.WriteInt32LittleEndian(0x80000000), [0, 0, 0, 0x80]);
  // the corpus idiom: header + payload
  assert.deepEqual([0x01, ...BinaryUtils.WriteInt32LittleEndian(3)].concat([9, 9, 9]), [1, 3, 0, 0, 0, 9, 9, 9]);
});

test("ReadInt16LittleEndian / BigEndian", () => {
  assert.equal(BinaryUtils.ReadInt16LittleEndian([0x34, 0x12]), 0x1234);
  assert.equal(BinaryUtils.ReadInt16BigEndian([0x12, 0x34]), 0x1234);
  // unsigned result: never sign-extended
  assert.equal(BinaryUtils.ReadInt16LittleEndian([0xff, 0xff]), 0xffff);
  assert.equal(BinaryUtils.ReadInt16BigEndian([0x80, 0x00]), 0x8000);
  // round-trip with the writers
  assert.equal(BinaryUtils.ReadInt16LittleEndian(BinaryUtils.WriteInt16LittleEndian(-2)), 0xfffe);
  assert.equal(BinaryUtils.ReadInt16BigEndian(BinaryUtils.WriteInt16BigEndian(4660)), 4660);
  // values above 255 are masked per byte
  assert.equal(BinaryUtils.ReadInt16LittleEndian([0x1ff, 0x100]), 0x00ff);
  // negative byte values are masked too
  assert.equal(BinaryUtils.ReadInt16LittleEndian([-1, 0]), 0xff);
  // extra bytes are ignored
  assert.equal(BinaryUtils.ReadInt16LittleEndian([0x34, 0x12, 0xaa, 0xbb]), 0x1234);
  assert.equal(BinaryUtils.ReadInt16BigEndian([0x12, 0x34, 0xaa]), 0x1234);
  // typed arrays and other indexables work
  assert.equal(BinaryUtils.ReadInt16LittleEndian(Uint8Array.from([0x34, 0x12])), 0x1234);
  assert.equal(BinaryUtils.ReadInt16BigEndian(Buffer.from([0x12, 0x34])), 0x1234);
});

test("ReadInt16 odd lengths and empty input mirror the inline corpus helper", () => {
  assert.equal(BinaryUtils.ReadInt16LittleEndian([0x7b]), 0x7b);
  assert.equal(BinaryUtils.ReadInt16BigEndian([0x7b]), 0x7b); // reverse of a 1-element slice is a no-op
  assert.equal(BinaryUtils.ReadInt16LittleEndian([]), 0);
  assert.equal(BinaryUtils.ReadInt16BigEndian([]), 0);
  assert.equal(BinaryUtils.ReadInt16LittleEndian(undefined), 0);
  assert.equal(BinaryUtils.ReadInt16BigEndian(null), 0);
  const input = [0x12];
  BinaryUtils.ReadInt16BigEndian(input);
  assert.deepEqual(input, [0x12], "input is not mutated");
});

test("ReadInt32LittleEndian / BigEndian", () => {
  assert.equal(BinaryUtils.ReadInt32LittleEndian([0x78, 0x56, 0x34, 0x12]), 0x12345678);
  assert.equal(BinaryUtils.ReadInt32BigEndian([0x12, 0x34, 0x56, 0x78]), 0x12345678);
  // signed int32 (corpus arithmetic produced a signed result)
  assert.equal(BinaryUtils.ReadInt32LittleEndian([0xff, 0xff, 0xff, 0xff]), -1);
  assert.equal(BinaryUtils.ReadInt32LittleEndian([0, 0, 0, 0x80]), -2147483648);
  assert.equal(BinaryUtils.ReadInt32BigEndian([0x80, 0, 0, 0]), -2147483648);
  assert.equal(BinaryUtils.ReadInt32LittleEndian(BinaryUtils.WriteInt32LittleEndian(-123456)), -123456);
  assert.equal(BinaryUtils.ReadInt32BigEndian(BinaryUtils.WriteInt32BigEndian(2147483647)), 2147483647);
  // extra bytes ignored, bytes masked
  assert.equal(BinaryUtils.ReadInt32LittleEndian([1, 0, 0, 0, 0xff]), 1);
  assert.equal(BinaryUtils.ReadInt32LittleEndian([0x101, 0, 0, 0]), 1);
});

test("ReadInt32 short input: 3-byte slices (corpus data.slice(4, 7)) and shorter", () => {
  assert.equal(BinaryUtils.ReadInt32LittleEndian([0x78, 0x56, 0x34]), 0x345678);
  assert.equal(BinaryUtils.ReadInt32LittleEndian([0x78]), 0x78);
  assert.equal(BinaryUtils.ReadInt32LittleEndian([]), 0);
  assert.equal(BinaryUtils.ReadInt32LittleEndian(null), 0);
  // big endian pads with trailing zeros before reversing (inline helper quirk)
  assert.equal(BinaryUtils.ReadInt32BigEndian([0x12]), 0x12000000);
  assert.equal(BinaryUtils.ReadInt32BigEndian([0x12, 0x34, 0x56]), 0x12345600);
  assert.equal(BinaryUtils.ReadInt32BigEndian([]), 0);
  const input = [0x12];
  BinaryUtils.ReadInt32BigEndian(input);
  assert.deepEqual(input, [0x12], "input is not mutated or padded in place");
});

test("static members work when detached (plugins alias them)", () => {
  const { ReadInt16BigEndian: be, WriteInt16LittleEndian: le } = BinaryUtils;
  assert.equal(be([0x01, 0x02]), 0x0102);
  assert.deepEqual(le(0x0102), [0x02, 0x01]);
});
