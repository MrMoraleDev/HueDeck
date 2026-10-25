// SPDX-License-Identifier: GPL-3.0-or-later
//
// The SMBus mock behaves like the Lightloom host's `bus` objects: free and fixed shapes, negative
// statuses instead of exceptions, argument errors as exceptions, WriteBlock clip/pad with the
// count byte recorded.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockBus } from "./mock-bus.mjs";

test("free and fixed buses share registers and the log", () => {
  const mock = createMockBus({ kind: "amd", address: 0x39, registers: { 0x39: { 0xA1: 0x01 } } });
  assert.equal(mock.free.WriteQuick(0x39), 0);
  assert.equal(mock.free.WriteQuick(0x3A), -1);
  assert.equal(mock.free.ReadByte(0x39, 0xA1), 0x01);
  assert.equal(mock.fixed.WriteByte(0x10, 0x55), 0);
  assert.equal(mock.free.ReadByte(0x39, 0x10), 0x55);
  assert.equal(mock.fixed.ReadByte(0x11), 0xFF, "unset registers read 0xFF");
  assert.equal(mock.fixed.WriteQuick, undefined, "no Quick on the fixed bus");
  assert.equal(mock.free.Type(), "Free");
  assert.ok(String(mock.fixed).startsWith("I2CBusWrapperFixedAddress"));
  assert.equal(mock.ops.length, 6);
});

test("WriteBlock clips and pads to len and records the count byte", () => {
  const mock = createMockBus({ kind: "amd", address: 0x39, registers: { 0x39: {} } });
  assert.equal(mock.fixed.WriteBlock(0x03, 2, [1, 2, 3]), 0);
  assert.equal(mock.fixed.WriteBlock(0x03, 4, [9]), 0);
  assert.deepEqual(mock.writes().map((w) => w.data), [[2, 1, 2], [4, 9, 0, 0, 0]]);
  assert.throws(() => mock.fixed.WriteBlock(0x03, 33, []), RangeError);
  assert.throws(() => mock.fixed.WriteBlock(0x03, 1, "x"), TypeError);
  assert.throws(() => mock.fixed.WriteByte(0x100, 0), RangeError);
});

test("WriteBlockWithoutRegister: first byte is the register, 140 bytes on NVIDIA buses only", () => {
  const gpu = createMockBus({ kind: "nvidia", address: 0x68, registers: { 0x68: {} } });
  const frame = [0x49, ...Array.from({ length: 136 }, (_, i) => i)];
  assert.equal(gpu.fixed.WriteBlockWithoutRegister(frame.length, frame), 0);
  assert.deepEqual(gpu.writes().map((w) => [w.op, w.reg, w.data.length]), [["WriteBlockWithoutRegister", 0x49, 137]]);
  assert.equal(gpu.registers(0x68).get(0x4B), 1, "data lands from the register on");
  assert.throws(() => gpu.fixed.WriteBlockWithoutRegister(141, []), RangeError);
  const ram = createMockBus({ kind: "amd", address: 0x39, registers: { 0x39: {} } });
  assert.throws(() => ram.fixed.WriteBlockWithoutRegister(33, []), RangeError);
  assert.equal(ram.free.WriteBlockWithoutRegister(0x39, 2, [1, 2]), 0);
});

test("failures are statuses, not exceptions", () => {
  const nv = createMockBus({ kind: "nvidia", address: 0x68, registers: { 0x68: {} }, nack: [0x69] });
  assert.equal(nv.free.WriteQuick(0x68), -1, "NVIDIA buses have no WriteQuick");
  assert.equal(nv.free.ReadByte(0x69, 0), -1);
  const blocked = createMockBus({ kind: "amd", address: 0x39, registers: { 0x39: {} }, failBlockAt: [0x39] });
  assert.equal(blocked.fixed.WriteBlock(0x03, 1, [1]), -1);
  assert.equal(blocked.fixed.WriteByte(0x01, 1), 0);
  assert.equal(nv.free.IsNvidiaBus(), true);
  assert.equal(nv.free.IsSystemBus(), false);
});

test("indexed devices go through the register pointer and move on remap", () => {
  const mock = createMockBus({ kind: "amd", address: 0x27, indexed: { 0x27: { direct: { 0xA0: 0 }, memory: { 0x1025: 0x4D } } } });
  assert.equal(mock.fixed.WriteWord(0x00, 0x2510), 0, "pointer 0x1025, byte-swapped");
  assert.equal(mock.fixed.ReadByte(0x81), 0x4D);
  assert.equal(mock.fixed.ReadByte(0xA0), 0);
  assert.equal(mock.fixed.WriteWord(0x00, 0x0083), 0);
  assert.equal(mock.fixed.WriteBlock(0x03, 2, [7, 8]), 0);
  assert.equal(mock.indexedRegisters(0x27).get(0x8301), 8);
  mock.free.WriteWord(0x27, 0x00, 0xEF82);
  mock.free.WriteByte(0x27, 0x01, 0x39 << 1);
  mock.free.WriteWord(0x27, 0x00, 0xF082);
  mock.free.WriteByte(0x27, 0x01, 0xF0);
  assert.deepEqual(mock.indexedAddresses(), [0x39]);
  assert.equal(mock.free.ReadByteWithoutRegister(0x27), -1);
  assert.equal(mock.free.ReadByteWithoutRegister(0x39), 0xFF);
});
