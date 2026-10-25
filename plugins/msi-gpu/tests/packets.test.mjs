// SPDX-License-Identifier: GPL-3.0-or-later
//
// MSI GPU plugin: Scan, Initialize, a frame, Shutdown (every exit mode) and SaveToDevice send
// exactly the register writes worked out by hand from OpenRGB's MSIGPUController
// (fixtures/expected.json), only to address 0x68; the flash-save register 0x3F only inside
// SaveToDevice({action: "save"}).

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { createMockDevice, loadPlugin } from "../../_testkit/mock-device.mjs";
import { createMockBus, installGlobals } from "../../_testkit/mock-bus.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginPath = join(here, "..", "plugin.js");
const expected = JSON.parse(readFileSync(join(here, "..", "fixtures", "expected.json"), "utf8"));
const pairs = (list) => list.map(([r, v]) => [Number(r), Number(v)]);
const startup = Object.fromEntries(Object.entries(expected.startup_registers).map(([r, v]) => [Number(r), Number(v)]));
const RTX3060 = { vendor: 0x10DE, device: 0x2504, subVendor: 0x1462, subDevice: 0x3903 };

async function setup({ color = [0, 0, 0], pci = RTX3060, kind = "nvidia", registers = startup, nack = [] } = {}) {
  const device = createMockDevice({ color });
  const mock = createMockBus({ kind, pci, name: "NVIDIA GeForce RTX 3060", address: 0x68, registers: { 0x68: { ...registers } }, nack });
  const restore = installGlobals(device, mock.fixed);
  const plugin = await loadPlugin(pluginPath);
  return { device, mock, plugin, restore };
}

const writes = (mock) => mock.writes().map((w) => [w.reg, w.data[0]]);

test("scan returns 0x68 only on the matching card", async () => {
  const { plugin, restore } = await setup();
  try {
    const bus = (opts) => createMockBus({ address: 0x68, registers: { 0x68: {} }, ...opts }).free;
    assert.deepEqual(plugin.Scan(bus({ kind: "nvidia", pci: RTX3060 })), [0x68]);
    assert.deepEqual(plugin.Scan(bus({ kind: "nvidia", pci: { ...RTX3060, subDevice: 0x3904 } })), []);
    assert.deepEqual(plugin.Scan(bus({ kind: "amd", pci: RTX3060 })), []);
    assert.deepEqual(plugin.SmbusAddresses(), [0x68]);
    assert.deepEqual(plugin.SmbusBuses(), ["nvidia"]);
    assert.deepEqual(plugin.SmbusDeniedRegisters(), [0x3F], "MSI_GPU_REG_SAVE is refused by the host too");
  } finally {
    restore();
  }
});

test("scan does no I/O", async () => {
  const { plugin, restore } = await setup();
  try {
    const mock = createMockBus({ kind: "nvidia", pci: RTX3060, registers: { 0x68: {} } });
    plugin.Scan(mock.free);
    assert.deepEqual(mock.ops, []);
  } finally {
    restore();
  }
});

test("initialize reads then enters static at full brightness", async () => {
  const { mock, plugin, device, restore } = await setup();
  try {
    plugin.Initialize();
    const reads = mock.ops.filter((o) => o.op === "ReadByte").map((o) => o.reg);
    assert.deepEqual(reads, expected.init_reads.map(Number));
    assert.deepEqual(writes(mock), pairs(expected.init_writes));
    assert.ok(device.calls.some((c) => c.method === "setName" && c.args[0] === "MSI GeForce RTX 3060 Gaming X Trio LHR"));
  } finally {
    restore();
  }
});

test("frame is 0x26 then R1 G1 B1", async () => {
  const { mock, plugin, restore } = await setup({ color: [0x12, 0x34, 0x56] });
  try {
    plugin.Initialize();
    mock.ops.length = 0;
    plugin.Render();
    assert.deepEqual(writes(mock), pairs(expected.frame_123456));
  } finally {
    restore();
  }
});

test("unchanged frame writes nothing", async () => {
  const { mock, plugin, device, restore } = await setup({ color: [1, 2, 3] });
  try {
    plugin.Initialize();
    plugin.Render();
    mock.ops.length = 0;
    plugin.Render();
    assert.deepEqual(mock.ops, []);
    device.setColor([1, 2, 4]);
    plugin.Render();
    assert.equal(mock.writes().length, 4);
  } finally {
    restore();
  }
});

test("a failed write is retried, and a colour the card missed is sent again next frame", async () => {
  const { mock, plugin, restore } = await setup({ color: [0xFF, 0xFF, 0xFF] });
  try {
    plugin.Initialize();
    const real = mock.fixed.WriteByte.bind(mock.fixed);
    // Blue fails twice (the write and its retry), then the bus is fine again.
    let blueFailures = 2;
    mock.fixed.WriteByte = (reg, value) => (reg === 0x32 && blueFailures-- > 0 ? -1 : real(reg, value));
    mock.ops.length = 0;
    plugin.Render();
    assert.deepEqual(writes(mock), [[0x26, 0], [0x30, 0xFF], [0x31, 0xFF]], "blue never reached the card");
    mock.ops.length = 0;
    plugin.Render();
    assert.deepEqual(writes(mock), [[0x26, 0], [0x30, 0xFF], [0x31, 0xFF], [0x32, 0xFF]], "the whole colour again");
    mock.ops.length = 0;
    plugin.Render();
    assert.deepEqual(mock.ops, [], "then it is remembered");
  } finally {
    restore();
  }
});

test("an unchanged colour is sent again every 60 frames", async () => {
  const { mock, plugin, restore } = await setup({ color: [9, 8, 7] });
  try {
    plugin.Initialize();
    plugin.Render();
    mock.ops.length = 0;
    for (let i = 0; i < 59; i++) {
      plugin.Render();
    }
    assert.deepEqual(mock.ops, []);
    plugin.Render();
    assert.deepEqual(writes(mock), pairs(expected.frame_123456).map(([r], i) => [r, [0, 9, 8, 7][i]]));
  } finally {
    restore();
  }
});

test("shutdown restores the startup registers, mode last", async () => {
  const { mock, plugin, restore } = await setup({ color: [9, 9, 9] });
  try {
    plugin.Initialize();
    plugin.Render();
    mock.ops.length = 0;
    plugin.Shutdown(false);
    assert.deepEqual(writes(mock), pairs(expected.shutdown_restore));
    for (const [reg, value] of Object.entries(startup)) {
      assert.equal(mock.registers(0x68).get(Number(reg)), value, `register ${reg}`);
    }
  } finally {
    restore();
  }
});

test("a card whose state cannot be read gets the static shutdown colour", async () => {
  const { mock, plugin, device, restore } = await setup({ nack: [0x68] });
  try {
    plugin.Initialize();
    mock.ops.length = 0;
    plugin.Shutdown(false);
    // The card answers nothing, so every write is also tried a second time.
    assert.deepEqual(writes(mock), pairs(expected.shutdown_unreadable).flatMap((w) => [w, w]));
    assert.ok(device.logs.length >= 1, "the failed read is logged");
  } finally {
    restore();
  }
});

test("register 0x3F is never written outside SaveToDevice", async () => {
  const { mock, plugin, device, restore } = await setup();
  try {
    plugin.Initialize();
    for (let i = 0; i < 20; i++) {
      device.setColor([i * 10, 255 - i, i]);
      plugin.Render();
    }
    plugin.SaveToDevice({ action: "preview", effect: null, colors: [[1, 2, 3]] });
    plugin.SaveToDevice({ action: "preview", effect: "spectrum", colors: [[1, 2, 3]] });
    plugin.Shutdown(false);
    plugin.Shutdown(true);
    for (const mode of ["restore", "keep", "off", "bogus"]) {
      plugin.Shutdown(false, mode);
    }
    assert.ok(mock.writes().length > 0);
    assert.ok(mock.writes().every((w) => w.reg !== 0x3F));
  } finally {
    restore();
  }
});

test("only address 0x68 is touched", async () => {
  const { mock, plugin, restore } = await setup({ color: [5, 6, 7] });
  try {
    plugin.Initialize();
    plugin.Render();
    plugin.Shutdown(false);
    assert.ok(mock.ops.every((o) => o.addr === 0x68));
    assert.ok(mock.writes().every((w) => w.op === "WriteByte"), "byte writes only");
  } finally {
    restore();
  }
});

const save = (plugin, effect, colors = [[0x12, 0x34, 0x56]]) => plugin.SaveToDevice({ action: "save", effect, colors });

test("a startup read that fails once is retried", async () => {
  const { mock, plugin, device, restore } = await setup();
  try {
    const real = mock.fixed.ReadByte.bind(mock.fixed);
    let failures = 1;
    mock.fixed.ReadByte = (reg) => (reg === 0x31 && failures-- > 0 ? -1 : real(reg));
    plugin.Initialize();
    assert.ok(!device.logs.some((l) => /ReadByte/.test(String(l))), "no failure logged");
    mock.ops.length = 0;
    plugin.Shutdown(false, "restore");
    assert.deepEqual(writes(mock), pairs(expected.shutdown_restore));
  } finally {
    restore();
  }
});

test("without a startup state, restore after a save shows what was saved, not the shutdown colour", async () => {
  const { mock, plugin, restore } = await setup({ color: [0x00, 0x82, 0xba] });
  try {
    const real = mock.fixed.ReadByte.bind(mock.fixed);
    mock.fixed.ReadByte = (reg) => (reg === 0x31 ? -1 : real(reg));
    plugin.Initialize();
    plugin.Render();
    save(plugin, null);
    mock.ops.length = 0;
    plugin.Shutdown(false, "restore");
    assert.deepEqual(writes(mock), [[0x36, 100], [0x26, 0], [0x30, 0x12], [0x31, 0x34], [0x32, 0x56], [0x22, 0x13]]);
  } finally {
    restore();
  }
});

test("save static: DeviceUpdateMode, DeviceUpdateLEDs, then 0x3F <- 1, then Direct again", async () => {
  const { mock, plugin, restore } = await setup({ color: [0x12, 0x34, 0x56] });
  try {
    plugin.Initialize();
    plugin.Render();
    mock.ops.length = 0;
    const result = save(plugin, null);
    assert.deepEqual(writes(mock), [...pairs(expected.save_static_123456), ...pairs(expected.save_resume)]);
    assert.deepEqual(result.zones, [{ name: "GPU", color: "#123456" }]);
    assert.equal(result.effect, "static");
    // The next frame goes out whole, and the restore puts back what was stored.
    mock.ops.length = 0;
    plugin.Render();
    assert.deepEqual(writes(mock), pairs(expected.frame_123456));
    mock.ops.length = 0;
    plugin.Shutdown(false, "restore");
    assert.deepEqual(writes(mock), pairs(expected.shutdown_after_save));
  } finally {
    restore();
  }
});

test("save the card's own effects", async () => {
  const { mock, plugin, restore } = await setup();
  try {
    plugin.Initialize();
    mock.ops.length = 0;
    const spectrum = save(plugin, "spectrum");
    assert.deepEqual(writes(mock), [...pairs(expected.save_spectrum), ...pairs(expected.save_resume)]);
    assert.deepEqual(spectrum.zones, []);
    mock.ops.length = 0;
    save(plugin, "breathing");
    assert.deepEqual(writes(mock), [...pairs(expected.save_breathing_123456), ...pairs(expected.save_resume)]);
    mock.ops.length = 0;
    assert.ok(save(plugin, "wave").unsupported);
    assert.deepEqual(mock.ops, [], "nothing for an effect the card does not have");
    assert.deepEqual(plugin.SaveCapabilities(), { static: true, effects: ["spectrum", "breathing"] });
  } finally {
    restore();
  }
});

test("a preview describes the save and writes nothing", async () => {
  const { mock, plugin, restore } = await setup({ color: [0xAA, 0xBB, 0xCC] });
  try {
    plugin.Initialize();
    mock.ops.length = 0;
    const result = plugin.SaveToDevice({ action: "preview", effect: null, colors: [[0xAA, 0xBB, 0xCC]] });
    assert.deepEqual(result.zones, [{ name: "GPU", color: "#aabbcc" }]);
    assert.deepEqual(mock.ops, []);
  } finally {
    restore();
  }
});

test("exit modes: keep shows the last colour, off static black, neither stored", async () => {
  const { mock, plugin, restore } = await setup({ color: [0x12, 0x34, 0x56] });
  try {
    assert.deepEqual(plugin.ShutdownModes(), ["restore", "keep", "off"]);
    plugin.Initialize();
    plugin.Render();
    mock.ops.length = 0;
    plugin.Shutdown(false, "keep");
    assert.deepEqual(writes(mock), pairs(expected.shutdown_keep_123456));
    mock.ops.length = 0;
    plugin.Shutdown(false, "off");
    assert.deepEqual(writes(mock), pairs(expected.shutdown_off));
    mock.ops.length = 0;
    plugin.Shutdown(false, "restore");
    assert.deepEqual(writes(mock), pairs(expected.shutdown_restore));
  } finally {
    restore();
  }
});

// Hardware modes (Lightloom extension): OpenRGB's mode switch, live, never the save register.
test("hardware modes are the card's own effects, without Direct and Off", async () => {
  const { plugin, restore } = await setup();
  try {
    const modes = plugin.HardwareModes();
    assert.equal(modes.length, 19);
    assert.ok(!modes.some((m) => m.id === "direct" || m.id === "off"));
    assert.deepEqual(modes.find((m) => m.id === "rainbow"), { id: "rainbow", name: "Rainbow", colors: 0, speed: true, brightness: true });
    assert.deepEqual(modes.find((m) => m.id === "static"), { id: "static", name: "Static", colors: 1, speed: false, brightness: true });
    assert.equal(modes.find((m) => m.id === "fadein").colors, 2);
  } finally {
    restore();
  }
});

test("SetHardwareMode: brightness, speed, mode, then the colours (DeviceUpdateMode + DeviceUpdateLEDs)", async () => {
  const { mock, plugin, restore } = await setup();
  try {
    plugin.Initialize();
    mock.ops.length = 0;
    plugin.SetHardwareMode({ id: "breathing", colors: [[0x12, 0x34, 0x56]], speed: 100, brightness: 60 });
    // Brightness 60 % = level 3 of 5 (x20); speed 100 % = MSI_GPU_SPEED_MAX = 0x01; MSI_GPU_MODE_BREATHING.
    assert.deepEqual(writes(mock), [[0x36, 60], [0x38, 0x01], [0x22, 0x04], [0x26, 0], [0x30, 0x12], [0x31, 0x34], [0x32, 0x56]]);
    mock.ops.length = 0;
    plugin.SetHardwareMode({ id: "rainbow", colors: [], speed: 0, brightness: 100 });
    assert.deepEqual(writes(mock), [[0x36, 100], [0x38, 0x04], [0x22, 0x08]], "no colour for Rainbow");
    mock.ops.length = 0;
    plugin.SetHardwareMode({ id: "fadein", colors: [[1, 2, 3], [4, 5, 6]], speed: 50, brightness: 100 });
    assert.deepEqual(writes(mock), [[0x36, 100], [0x38, 0x02], [0x22, 0x14], [0x26, 0], [0x27, 1], [0x28, 2], [0x29, 3], [0x2A, 4], [0x2B, 5], [0x2C, 6]]);
    mock.ops.length = 0;
    plugin.SetHardwareMode({ id: "static", colors: [[9, 9, 9]], brightness: 0 });
    assert.deepEqual(writes(mock), [[0x36, 0], [0x22, 0x13], [0x26, 0], [0x30, 9], [0x31, 9], [0x32, 9]], "Static has no speed");
    assert.throws(() => plugin.SetHardwareMode({ id: "disco" }), /disco/);
  } finally {
    restore();
  }
});

test("in a hardware mode Render sends nothing; null goes back to Direct and the next frame goes out", async () => {
  const { mock, plugin, restore } = await setup({ color: [0x12, 0x34, 0x56] });
  try {
    plugin.Initialize();
    plugin.SetHardwareMode({ id: "rainbow", colors: [], speed: 50, brightness: 100 });
    mock.ops.length = 0;
    plugin.Render();
    assert.deepEqual(mock.ops, []);
    plugin.SetHardwareMode(null);
    assert.deepEqual(writes(mock), pairs(expected.save_resume), "Direct: full brightness, static");
    mock.ops.length = 0;
    plugin.Render();
    assert.deepEqual(writes(mock), pairs(expected.frame_123456));
  } finally {
    restore();
  }
});

test("keep leaves the hardware mode; restore and off still work; 0x3F never written", async () => {
  const { mock, plugin, restore } = await setup();
  try {
    plugin.Initialize();
    for (const m of plugin.HardwareModes()) {
      plugin.SetHardwareMode({ id: m.id, colors: [[1, 2, 3], [4, 5, 6]].slice(0, m.colors), speed: 30, brightness: 80 });
    }
    mock.ops.length = 0;
    plugin.Shutdown(false, "keep");
    assert.deepEqual(mock.ops, [], "keep: the hardware mode stays");
    plugin.Shutdown(false, "off");
    assert.deepEqual(writes(mock), pairs(expected.shutdown_off));
    plugin.SetHardwareMode({ id: "wave", colors: [[7, 7, 7]], speed: 10, brightness: 10 });
    mock.ops.length = 0;
    plugin.Shutdown(false, "restore");
    assert.deepEqual(writes(mock), pairs(expected.shutdown_restore));
    assert.ok(mock.writes().every((w) => w.reg !== 0x3F), "never the save register");
  } finally {
    restore();
  }
});
