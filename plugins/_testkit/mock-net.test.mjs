// SPDX-License-Identifier: GPL-3.0-or-later
// The network testkit itself: module mapping, sockets, XHR routes, service settings.

import { test } from "node:test";
import assert from "node:assert/strict";
import { createMockNet, createMockService, installNetGlobals } from "./mock-net.mjs";

test("@SignalRGB/udp resolves to the installed mock net", async () => {
  const net = createMockNet();
  const restore = installNetGlobals({ net });
  try {
    const udp = (await import("@SignalRGB/udp")).default;
    const s = udp.createSocket();
    assert.equal(s.write([1, 2, 300], "192.168.1.9", 21324), 3);
    assert.deepEqual(net.sentTo("192.168.1.9", 21324)[0].bytes, [1, 2, 44]);
    const { encode } = await import("@SignalRGB/base64");
    assert.equal(encode([0xbb, 0x00]), "uwA=");
  } finally {
    restore();
  }
});

test("sync and async XHR answer from routes; unrouted is status 0", () => {
  const net = createMockNet();
  net.route("GET", "http://10.0.0.2/json/info", { status: 200, body: { leds: { count: 30 } } });
  const sync = new net.XMLHttpRequest();
  sync.open("GET", "http://10.0.0.2/json/info", false);
  sync.send();
  assert.equal(sync.status, 200);
  assert.equal(JSON.parse(sync.responseText).leds.count, 30);
  const lost = new net.XMLHttpRequest();
  let done = false;
  lost.onreadystatechange = () => { done = lost.readyState === 4; };
  lost.open("GET", "http://10.0.0.3/", true);
  lost.send();
  assert.equal(done, false);
  net.flush();
  assert.equal(done, true);
  assert.equal(lost.status, 0);
});

test("service settings reject path segments", () => {
  const service = createMockService({ settings: { bridge: { key: "abc" } } });
  assert.equal(service.getSetting("bridge", "key"), "abc");
  assert.throws(() => service.getSetting("a/b", "key"));
  service.saveSetting("bridge", "key", undefined);
  assert.equal(service.getSetting("bridge", "key"), undefined);
});
