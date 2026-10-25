// SPDX-License-Identifier: GPL-3.0-or-later
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  buildReport,
  diagnosticsOf,
  fitEncoded,
  ISSUE_NEW,
  maskKey,
  maskSerial,
  MAX_DETAILS,
  MAX_DIAGNOSTICS,
  redact,
  vidPidOf,
  type ReportInput,
} from "./report.ts";
import type { DeviceView } from "./types.ts";

const TOKEN = "3f9c1b2a7d4e5f60718293a4b5c6d7e8";

function device(over: Partial<DeviceView> = {}): DeviceView {
  return {
    key: "1462:7c91:A02019092700001",
    vid: 0x1462,
    pid: 0x7c91,
    name: "MSI MAG B550 TOMAHAWK",
    plugin: "MSI Mystic Light 185",
    pluginVersion: "0.2.0",
    pluginType: "hid",
    deviceType: "motherboard",
    pluginPath: "C:\\Users\\Tomahawk\\AppData\\Local\\Programs\\Lightloom\\plugins\\msi-mystic-light-7c91\\plugin.js",
    state: "running",
    enabled: true,
    connected: true,
    frames: 1200,
    renderErrors: 0,
    ioErrors: 2,
    defaultScale: 1,
    parameters: [],
    messages: [{ id: "m", title: "Write failed", message: "HID write returned -1", priority: 2 }],
    endpoints: [
      {
        uid: "u",
        kind: "device",
        name: "Mystic Light",
        size: [21, 1],
        leds: 6,
        placement: {
          position: { x: 0, y: 0 },
          rotation: 0,
          flipX: false,
          flipY: false,
          enabled: true,
          brightness: 100,
          lightingMode: "Canvas",
          forcedColor: "#000000",
        },
      },
    ],
    channels: [{ name: "JRAINBOW1", ledLimit: 120, components: [{ ComponentId: "c", DisplayName: "Strip", LedCount: 60 }] }],
    shutdownModes: ["restore"],
    blinkLowBattery: false,
    tested: true,
    interfaces: ["HID interface 0 · usage page 0x0001 · usage 0x0000 · collection 0"],
    ...over,
  };
}

function input(over: Partial<ReportInput> = {}): ReportInput {
  return { device: device(), version: "0.1.0", os: "Windows 11 Pro 25H2 (build 26200.6899)", logs: [], token: TOKEN, ...over };
}

/** The decoded query of an issues/new URL. */
function query(url: string): Record<string, string> {
  assert.ok(url.startsWith(`${ISSUE_NEW}?`));
  return Object.fromEntries(new URL(url).searchParams.entries());
}

test("serials keep only their last 4 characters", () => {
  assert.equal(maskSerial("A02019092700001"), "***********0001");
  assert.equal(maskSerial("1234"), "1234");
  assert.equal(maskSerial(""), "");
  assert.equal(maskKey("1462:7c91:A02019092700001"), "1462:7c91:***********0001");
  assert.equal(maskKey("net:wled:a0b765dc1234"), "net:wled:********1234");
  assert.equal(maskKey("smbus:10de:2504:1462:3903@pci01:68"), "smbus:10de:2504:1462:3903@pci01:68");
});

test("user paths, the token, network addresses and serials are redacted", () => {
  const text = [
    "C:\\Users\\Tomahawk\\AppData\\Local\\Lightloom\\plugins\\x.js",
    "c:/users/Some Body/plugins",
    "C:\\\\Users\\\\Tomahawk\\\\escaped",
    "/home/ciro/.local/share/lightloom",
    `Authorization: Bearer ${TOKEN}`,
    `token=${TOKEN} and bare ${TOKEN}`,
    "WLED at 192.168.1.50:80, mac A0:B7:65:DC:12:34, also a0-b7-65-dc-12-34",
    "v6 fe80::1c2b:3a4d and 2001:0db8:85a3:0000:0000:8a2e:0370:7334 and ::1",
    "opened 1462:7c91:A02019092700001 (serial a02019092700001)",
  ].join("\n");
  const out = redact(text, { token: TOKEN, serials: ["A02019092700001"] });
  assert.ok(!out.includes(TOKEN), out);
  assert.ok(!/Tomahawk|ciro/.test(out), out);
  assert.ok(!out.includes("Some Body"), out);
  assert.ok(out.includes("%USERPROFILE%\\AppData\\Local\\Lightloom\\plugins\\x.js"), out);
  assert.ok(out.includes("~/.local/share/lightloom"), out);
  assert.ok(!/192\.168|A0:B7|a0-b7|fe80|2001:0db8|::1/i.test(out), out);
  assert.ok(!/A02019092700001/i.test(out), out);
  assert.ok(out.includes("***********0001"), out);
});

test("what must stay readable is left alone", () => {
  const text = "MSI MAG B550 TOMAHAWK 1462:7C91 · usage page 0xff00 · build 26200.6899 · lightloom_api::model · v0.2.0";
  assert.equal(redact(text, { token: TOKEN }), text);
});

test("the board name survives even when it equals the user name", () => {
  const r = buildReport(input(), "works");
  assert.equal(r.fields.device, "MSI MAG B550 TOMAHAWK");
  assert.ok(!r.fields.diagnostics.includes("\\Users\\Tomahawk"));
});

test("vidpid per kind of device", () => {
  assert.equal(vidPidOf(device()), "1462:7C91");
  assert.equal(
    vidPidOf(device({ key: "smbus:10de:2504:1462:3903@pci01:68", pluginType: "smbus", bus: "NVIDIA GeForce RTX 3080 · 0x68" })),
    "SMBus NVIDIA GeForce RTX 3080 · 0x68",
  );
  assert.equal(vidPidOf(device({ key: "net:wled:aabbccddeeff", pluginType: "network", plugin: "WLED" })), "network device (WLED)");
});

test("the URL carries every field, encoded, and decodes back", () => {
  const r = buildReport(input(), "problem", "Only half the strip lights & the rest flickers #2 ?");
  const q = query(r.url);
  assert.equal(q.template, "device-report.yml");
  assert.equal(q.result, "Does not work");
  assert.equal(q.device, "MSI MAG B550 TOMAHAWK");
  assert.equal(q.vidpid, "1462:7C91");
  assert.equal(q.plugin, "MSI Mystic Light 185 0.2.0");
  assert.equal(q.version, "0.1.0");
  assert.equal(q.os, "Windows 11 Pro 25H2 (build 26200.6899)");
  assert.equal(q.details, "Only half the strip lights & the rest flickers #2 ?");
  assert.equal(q.title, "[device] MSI MAG B550 TOMAHAWK (1462:7C91): problem");
  assert.equal(q.diagnostics, r.fields.diagnostics);
  // Nothing that needs encoding is left raw in the query.
  assert.ok(!/[ #&"<>\\]/.test(r.url.slice(ISSUE_NEW.length + 1).replace(/&(?=[a-z]+=)/g, "")), r.url);
  assert.ok(!r.url.includes(TOKEN));
});

test("works has no details field and says Works", () => {
  const q = query(buildReport(input(), "works").url);
  assert.equal(q.result, "Works");
  assert.equal(q.details, undefined);
  assert.ok(q.title.endsWith(": works"));
});

test("diagnostics carry the device facts, masked", () => {
  const { text } = diagnosticsOf(input({ logs: ["10:00:00  session 1462:7c91:A02019092700001: write failed", "10:00:01  unrelated line"] }));
  assert.ok(text.includes("key: 1462:7c91:***********0001"));
  assert.ok(text.includes("type hid"));
  assert.ok(text.includes("tested"));
  assert.ok(text.includes("HID interface 0"));
  assert.ok(text.includes("counters: 1200 frames · 0 render errors · 2 I/O errors"));
  assert.ok(text.includes("[error] Write failed: HID write returned -1"));
  assert.ok(text.includes("JRAINBOW1: 1 components, 60 LEDs of 120"));
  assert.ok(text.includes("session 1462:7c91:***********0001: write failed"));
  assert.ok(!text.includes("unrelated line"));
  assert.ok(!text.includes("A02019092700001"));
});

test("long logs lose their oldest lines first and the whole fits the limit", () => {
  const logs = Array.from({ length: 200 }, (_, i) => `10:00:${String(i).padStart(3, "0")}  MSI Mystic Light 185: frame ${i} ${"x".repeat(120)}`);
  const { text, dropped } = diagnosticsOf(input({ logs }));
  assert.ok(encodeURIComponent(text).length <= MAX_DIAGNOSTICS);
  assert.ok(dropped > 0);
  assert.ok(text.includes(`${dropped} older left out for length`));
  assert.ok(text.includes("frame 199"), "the newest line stays");
  assert.ok(!text.includes("frame 140 "), "old lines go");
  assert.ok(text.startsWith("key: "), "the facts stay");
});

test("facts alone over the limit are cut at the end, never mid-character", () => {
  const huge = device({ interfaces: Array.from({ length: 400 }, (_, i) => `HID interface ${i} · ünïcödé 🎛️ ${i}`) });
  const { text } = diagnosticsOf(input({ device: huge }));
  assert.ok(encodeURIComponent(text).length <= MAX_DIAGNOSTICS);
  assert.ok(text.endsWith("(cut for length)"));
  assert.doesNotThrow(() => encodeURIComponent(fitEncoded("🎛️".repeat(5000), 100)));
});

test("details are redacted and capped; the whole URL stays under 8 KB", () => {
  const details = `my IP is 10.0.0.7 and my files are in C:\\Users\\Tomahawk\\Desktop ${"é".repeat(5000)}`;
  const logs = Array.from({ length: 200 }, (_, i) => `MSI MAG B550 TOMAHAWK line ${i} ${"y".repeat(100)}`);
  const r = buildReport(input({ logs }), "problem", details);
  assert.ok(encodeURIComponent(r.fields.details).length <= MAX_DETAILS);
  assert.ok(!r.fields.details.includes("10.0.0.7"));
  assert.ok(!r.fields.details.includes("Tomahawk"));
  assert.ok(r.url.length < 8192, `${r.url.length}`);
});
