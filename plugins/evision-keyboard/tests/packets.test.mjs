// SPDX-License-Identifier: GPL-3.0-or-later
//
// EVision V2 keyboards: the capabilities request, colour frames, the keepalive and the end
// request are byte for byte what fixtures/packets.json works out from OpenRGB's C++ code
// (EVisionV2KeyboardController::Query, ::Write, ::SetLedsDirect, ::RefreshLedDirect,
// ::EndLedsDirect), for both layouts, two key-map sizes and two colour patterns; plus how the
// plugin treats answers OpenRGB would refuse.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { installGlobals, loadPlugin, hexToBytes, bytesToHex } from "../../_testkit/mock-device.mjs";
import { createKeyboardDevice, echo, fakeClock } from "./keyboard-double.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const pluginDir = join(here, "..");
const fixture = JSON.parse(readFileSync(join(pluginDir, "fixtures", "packets.json"), "utf8"));
const hex = (h) => bytesToHex(hexToBytes(h));

const MODELS = [
  { file: "plugin.js", key: "evision", vid: 0x320F, pid: 0x5041 },
  { file: "endorfy-omnis.js", key: "endorfy", vid: 0x3299, pid: 0x0012 },
];

// The colour functions fixtures/packets.json names (device.color(x, y) of the LED's matrix cell).
const COLOURS = {
  solid: () => [0xFF, 0x80, 0x01],
  gradient: (x, y) => [(x * 12 + y) & 0xFF, (y * 40 + x * 3) & 0xFF, (255 - x * 11 - y * 7) & 0xFF],
};

const capsQuery = hexToBytes(fixture.layouts.evision.capabilitiesQuery);

/** The firmware: the capabilities for `mapSize` key slots, every other request echoed. */
function firmware(mapSize = 126, capsReply = hexToBytes(fixture.layouts.evision.capabilitiesReply[String(mapSize)])) {
  return (sent) => (bytesToHex(sent) === bytesToHex(capsQuery) ? capsReply : echo(sent));
}

async function setup(model, { mapSize = 126, color = COLOURS.gradient, answer } = {}) {
  const device = createKeyboardDevice({
    productId: model.pid,
    vendorId: model.vid,
    color,
    answer: answer ?? firmware(mapSize),
  });
  const restore = installGlobals(device);
  const plugin = await loadPlugin(join(pluginDir, model.file));
  return { device, plugin, restore };
}

const written = (device) => device.written.map((w) => bytesToHex(w.bytes));

for (const model of MODELS) {
  const expected = fixture.layouts[model.key];

  test(`${model.file}: Initialize asks for the capabilities only`, async () => {
    const { device, plugin, restore } = await setup(model);
    try {
      plugin.Initialize();
      assert.deepEqual(written(device), [hex(expected.capabilitiesQuery)]);
      assert.equal(hex(expected.capabilitiesQuery).slice(0, 23), "04 0A 00 03 07 00 00 00", "Query(0x03, 0, -, 7)");
      assert.ok(device.written.every((w) => w.length === 64));
      assert.ok(device.logs.some((l) => /126 key slots/.test(l.message)), "the slot count is logged");
    } finally {
      restore();
    }
  });

  for (const mapSize of ["126", "128"]) {
    for (const colour of Object.keys(COLOURS)) {
      test(`${model.file}: a ${colour} frame with ${mapSize} key slots is byte for byte the C++ derivation`, async () => {
        const clock = fakeClock();
        const { device, plugin, restore } = await setup(model, { mapSize: Number(mapSize), color: COLOURS[colour] });
        try {
          plugin.Initialize();
          device.written.length = 0;
          plugin.Render();
          assert.deepEqual(written(device), expected.frames[mapSize][colour].map(hex));
          assert.ok(device.written.every((w) => w.length === 64 && w.reportId === 0x04));
        } finally {
          restore();
          clock.restore();
        }
      });
    }
  }

  test(`${model.file}: unchanged frames are skipped and refreshed after 200 ms, as OpenRGB's keepalive does`, async () => {
    const clock = fakeClock(5000);
    let colour = COLOURS.gradient;
    const { device, plugin, restore } = await setup(model, { color: (x, y) => colour(x, y) });
    try {
      plugin.Initialize();
      device.written.length = 0;
      plugin.Render();
      assert.equal(device.written.length, 7, "the first frame is sent whole");
      device.written.length = 0;
      for (const t of [5033, 5150, 5200]) {
        clock.set(t);
        plugin.Render();
      }
      assert.deepEqual(written(device), [], "nothing while the colours stay and 200 ms have not passed");
      clock.set(5201);
      plugin.Render();
      assert.deepEqual(written(device), [hex(expected.keepaliveAfterGradient)], "one keepalive after more than 200 ms");
      device.written.length = 0;
      clock.set(5401);
      plugin.Render();
      assert.deepEqual(written(device), [], "the keepalive restarts the 200 ms");
      clock.set(5402);
      plugin.Render();
      assert.deepEqual(written(device), [hex(expected.keepaliveAfterGradient)]);
      device.written.length = 0;
      colour = COLOURS.solid;
      clock.set(5410);
      plugin.Render();
      assert.deepEqual(written(device), expected.frames["126"].solid.map(hex), "a change is sent whole at once");
    } finally {
      restore();
      clock.restore();
    }
  });

  for (const suspending of [false, true]) {
    test(`${model.file}: Shutdown(${suspending}, every exit mode) hands the keyboard back with "end dynamic colours"`, async () => {
      const clock = fakeClock();
      const { device, plugin, restore } = await setup(model);
      try {
        // The keyboard cannot hold colours without the host: only "restore" is offered.
        assert.deepEqual(plugin.ShutdownModes(), ["restore"]);
        for (const mode of [undefined, "restore", "keep", "off"]) {
          plugin.Initialize();
          plugin.Render();
          device.written.length = 0;
          plugin.Shutdown(suspending, mode);
          assert.deepEqual(written(device), [hex(expected.end)], String(mode));
        }
        assert.equal(hex(expected.end).slice(0, 23), "04 13 00 13 00 00 00 00", "Query(0x13)");
      } finally {
        restore();
        clock.restore();
      }
    });
  }
}

test("the keepalive is OpenRGB's on the EVision V2 layout and keeps F5 lit on the Endorfy layout", () => {
  const evision = fixture.layouts.evision;
  const endorfy = fixture.layouts.endorfy;
  assert.equal(evision.slot6, null, "cell 6 is blank on the EVision V2 layout");
  assert.equal(evision.keepaliveAfterGradient, evision.keepaliveOpenRGB);
  assert.equal(hex(evision.keepaliveOpenRGB).slice(0, 26), "04 25 00 12 01 12 00 00 00", "Query(0x12, 18, -, 1)");
  assert.deepEqual(endorfy.slot6, ["F5"], "cell 6 is F5 on the Endorfy layout");
  // F5 sits at matrix cell (6, 0): red = 6 * 12 + 0 = 0x48 in the gradient.
  const keep = hexToBytes(endorfy.keepaliveAfterGradient);
  assert.deepEqual(keep.slice(3, 9), [0x12, 0x01, 0x12, 0x00, 0x00, 0x48]);
  assert.deepEqual(keep.slice(1, 3), [0x6D, 0x00], "checksum 0x12 + 0x01 + 0x12 + 0x48");
});

test("a silent keyboard gets no colours, but still the end request", async () => {
  const { device, plugin, restore } = await setup(MODELS[0], { answer: () => null });
  try {
    plugin.Initialize();
    assert.deepEqual(written(device), [hex(fixture.layouts.evision.capabilitiesQuery)]);
    assert.ok(device.logs.some((l) => /no answer/.test(l.message)));
    plugin.Render();
    plugin.Render();
    assert.equal(device.written.length, 1, "no frames without the key map size");
    plugin.Shutdown(false);
    assert.deepEqual(written(device).slice(1), [hex(fixture.layouts.evision.end)]);
  } finally {
    restore();
  }
});

test("a refused request stops the rest of the frame, and the next frame is sent whole", async () => {
  const clock = fakeClock();
  let refuse = true;
  const answer = (sent) => {
    const reply = firmware()(sent);
    // Refuse the third chunk (offset 112 = 0x70) once, with status 1.
    if (refuse && sent[3] === 0x12 && sent[5] === 0x70 && sent[6] === 0x00) {
      refuse = false;
      reply[7] = 0x01;
    }
    return reply;
  };
  const { device, plugin, restore } = await setup(MODELS[0], { answer });
  try {
    const frame = fixture.layouts.evision.frames["126"].gradient.map(hex);
    plugin.Initialize();
    device.written.length = 0;
    plugin.Render();
    assert.deepEqual(written(device), frame.slice(0, 3), "OpenRGB's Write() stops at the refused chunk");
    device.written.length = 0;
    clock.advance(33);
    plugin.Render();
    assert.deepEqual(written(device), frame, "an unconfirmed frame is not taken as sent");
  } finally {
    restore();
    clock.restore();
  }
});

test("answers that do not echo the request are not taken", async () => {
  const good = hexToBytes(fixture.layouts.evision.capabilitiesReply["126"]);
  const variants = {
    "checksum low byte": (r) => { r[1] ^= 0x01; },
    "checksum high byte": (r) => { r[2] ^= 0x01; },
    "command": (r) => { r[3] = 0x05; },
    "offset": (r) => { r[5] = 0x01; },
    "size above 56": (r) => { r[4] = 57; },
    "status": (r) => { r[7] = 0x02; },
    "report id": (r) => { r[0] = 0x05; },
  };
  for (const [name, spoil] of Object.entries(variants)) {
    const bad = good.slice();
    spoil(bad);
    const { device, plugin, restore } = await setup(MODELS[0], { answer: firmware(126, bad) });
    try {
      plugin.Initialize();
      device.written.length = 0;
      plugin.Render();
      assert.equal(device.written.length, 0, `a wrong ${name} leaves the plugin without colours`);
    } finally {
      restore();
    }
  }
  // A short input report is not an answer either.
  const { device, plugin, restore } = await setup(MODELS[0], { answer: firmware(126, good.slice(0, 63)) });
  try {
    plugin.Initialize();
    device.written.length = 0;
    plugin.Render();
    assert.equal(device.written.length, 0);
  } finally {
    restore();
  }
});

test("capabilities: like OpenRGB, the plugin gives up only when both magic bytes differ", async () => {
  const good = hexToBytes(fixture.layouts.evision.capabilitiesReply["126"]);
  for (const [magic, sends] of [[[0xAA, 0x55], true], [[0xAA, 0x00], true], [[0x00, 0x55], true], [[0x00, 0x00], false]]) {
    const caps = good.slice();
    caps[8] = magic[0];
    caps[9] = magic[1];
    const clock = fakeClock();
    const { device, plugin, restore } = await setup(MODELS[0], { answer: firmware(126, caps) });
    try {
      plugin.Initialize();
      device.written.length = 0;
      plugin.Render();
      assert.equal(device.written.length, sends ? 7 : 0, `magic ${bytesToHex(magic)}`);
    } finally {
      restore();
      clock.restore();
    }
  }
});

test("a capabilities answer in two parts is read on, like OpenRGB's Read() loop", async () => {
  const parts = fixture.capabilitiesPartial;
  const answer = (sent) => {
    if (bytesToHex(sent) === bytesToHex(capsQuery)) return hexToBytes(parts.firstReply);
    if (bytesToHex(sent) === hex(parts.secondQuery)) return hexToBytes(parts.secondReply);
    return echo(sent);
  };
  const clock = fakeClock();
  const { device, plugin, restore } = await setup(MODELS[0], { answer });
  try {
    plugin.Initialize();
    assert.deepEqual(written(device), [bytesToHex(capsQuery), hex(parts.secondQuery)]);
    device.written.length = 0;
    plugin.Render();
    assert.deepEqual(written(device), fixture.layouts.evision.frames["126"].gradient.map(hex));
  } finally {
    restore();
    clock.restore();
  }
});

test("input reports with another report id are skipped while waiting for the answer", async () => {
  const other = [0x01, 0x00, 0x00, 0x04, ...new Array(60).fill(0)];
  const clock = fakeClock();
  const { device, plugin, restore } = await setup(MODELS[1], { answer: (sent) => [other, firmware()(sent)] });
  try {
    plugin.Initialize();
    device.written.length = 0;
    plugin.Render();
    assert.deepEqual(written(device), fixture.layouts.endorfy.frames["126"].gradient.map(hex));
  } finally {
    restore();
    clock.restore();
  }
});

test("keys whose slot is beyond the keyboard's key map stay dark (OpenRGB would overrun its buffer)", async () => {
  const caps = hexToBytes(fixture.layouts.evision.capabilitiesReply["126"]);
  caps[13] = 100; // map size: data byte 5
  const clock = fakeClock();
  const { device, plugin, restore } = await setup(MODELS[0], { answer: firmware(100, caps), color: COLOURS.solid });
  try {
    plugin.Initialize();
    assert.ok(device.logs.some((l) => /left dark: .*Num Enter/.test(l.message)));
    device.written.length = 0;
    plugin.Render();
    const sizes = device.written.map((w) => w.bytes[4]);
    const offsets = device.written.map((w) => w.bytes[5] | (w.bytes[6] << 8));
    assert.deepEqual(sizes, [56, 56, 56, 56, 56, 20], "3 * 100 bytes");
    assert.deepEqual(offsets, [0, 56, 112, 168, 224, 280]);
  } finally {
    restore();
    clock.restore();
  }
});

test("a keyboard that stops answering is retried once a second after 3 unconfirmed frames", async () => {
  const clock = fakeClock();
  let silent = false;
  const answer = (sent) => (silent && sent[3] === 0x12 ? null : firmware()(sent));
  const { device, plugin, restore } = await setup(MODELS[0], { answer });
  try {
    const frame = fixture.layouts.evision.frames["126"].gradient.map(hex);
    plugin.Initialize();
    silent = true;
    device.written.length = 0;
    for (let i = 0; i < 3; i++) {
      plugin.Render();
      clock.advance(33);
    }
    assert.equal(device.written.length, 3, "each of the first 3 frames tries its first chunk");
    assert.ok(device.logs.some((l) => /not confirmed/.test(l.message)));
    device.written.length = 0;
    for (let i = 0; i < 20; i++) {
      plugin.Render();
      clock.advance(33);
    }
    assert.equal(device.written.length, 0, "no frames inside the back-off second");
    clock.advance(1000);
    silent = false;
    plugin.Render();
    assert.deepEqual(written(device), frame, "the whole frame once the second has passed");
    assert.ok(device.logs.some((l) => /confirms colours again/.test(l.message)));
    device.written.length = 0;
    clock.advance(33);
    plugin.Render();
    assert.equal(device.written.length, 0, "back to skipping unchanged frames");
  } finally {
    restore();
    clock.restore();
  }
});
