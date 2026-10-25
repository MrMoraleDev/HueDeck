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

test("the dtls feature: connect, establish, send, close; https requests keep tlsName", () => {
  const net = createMockNet();
  const device = {};
  const restore = installNetGlobals({ net, device });
  try {
    device.addFeature("dtls");
    const events = [];
    globalThis.dtls.onConnectionEstablished(() => events.push("open"));
    globalThis.dtls.onConnectionError((m) => events.push(`error ${m}`));
    globalThis.dtls.onConnectionClosed(() => events.push("closed"));
    assert.equal(globalThis.dtls.createConnection("192.168.1.20", 2100, "id", "00ff"), true);
    assert.equal(globalThis.dtls.send([1]), -1, "not before the handshake");
    net.dtls.establish();
    assert.equal(globalThis.dtls.hasEncryptedConnection(), true);
    assert.equal(globalThis.dtls.send([1, 2, 300]), 3);
    assert.deepEqual(net.dtls.sent, [[1, 2, 44]]);
    globalThis.dtls.CloseConnection();
    net.dtls.fail("x");
    assert.deepEqual(events, ["open", "closed", "error x"]);
    assert.deepEqual(net.dtls.connections, [{ host: "192.168.1.20", port: 2100, identity: "id", key: "00ff" }]);
    net.route("GET", "https://192.168.1.20/auth/v1", { status: 200, headers: { "hue-application-id": "abc" }, tlsFingerprint: "f".repeat(64) });
    const x = new net.XMLHttpRequest();
    x.open("GET", "https://192.168.1.20/auth/v1", false);
    x.tlsName = "001788fffe1a2b3c";
    x.send();
    assert.equal(x.getResponseHeader("hue-application-id"), "abc");
    assert.equal(x.tlsFingerprint, "f".repeat(64));
    assert.equal(net.http[0].tlsName, "001788fffe1a2b3c");
  } finally {
    restore();
  }
  assert.equal(globalThis.dtls, undefined, "restored");
});
