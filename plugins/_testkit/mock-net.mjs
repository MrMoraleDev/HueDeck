// SPDX-License-Identifier: GPL-3.0-or-later
//
// Mock of Lightloom's network host surface for first-party network plugin tests (Node 24,
// node:test). Conventions mirror the host (docs/host-api/network-services.md, Lightloom section):
//
//   * `@SignalRGB/udp`, `@SignalRGB/tcp` and `@SignalRGB/base64` imports resolve to this kit
//     (a module resolve hook is registered when this file is imported): udp/tcp forward to the
//     mock net installed with `installNetGlobals`, base64 is Lightloom's own prelude module.
//   * Sockets record every datagram/segment in `net.sent` as { proto, ip, port, bytes, text };
//     `send`/`write` return the byte count (or -1 when unbound/unconnected), never throw.
//   * Incoming data is fed with `net.deliver(socket, { data | buffer, address, port })`: the
//     socket's 'message' callbacks get { data, buffer, address, port, timestamp } like on the host.
//   * XMLHttpRequest answers from routes registered with `net.route(method, url, reply)`:
//     synchronous requests (open(..., false)) complete inside send(); asynchronous ones complete
//     at `net.flush()` (the host completes them between plugin calls). An unrouted request ends
//     with status 0, like an unreachable host.
//   * The `service` mock records announce/suppress/settings/broadcast calls.

import { registerHooks } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const MODULES = {
  "@SignalRGB/udp": pathToFileURL(join(here, "signalrgb", "udp.mjs")).href,
  "@SignalRGB/tcp": pathToFileURL(join(here, "signalrgb", "tcp.mjs")).href,
  "@SignalRGB/base64": pathToFileURL(join(here, "..", "..", "prelude", "base64.js")).href,
};

registerHooks({
  resolve(specifier, context, nextResolve) {
    const name = specifier.replace(/\.js$/, "");
    if (name in MODULES) {
      return { url: MODULES[name], shortCircuit: true };
    }
    return nextResolve(specifier, context);
  },
});

/** UTF-8 bytes of a string, or the bytes of an array-like (each & 0xFF). */
export function toBytes(data) {
  if (typeof data === "string") {
    return Array.from(new TextEncoder().encode(data));
  }
  if (data == null || typeof data.length !== "number") {
    return [];
  }
  return Array.from(data, (b) => Number(b) & 0xff);
}

function text(bytes) {
  return new TextDecoder("utf-8", { fatal: false }).decode(Uint8Array.from(bytes));
}

const STATES = {
  UnconnectedState: 0,
  HostLookupState: 1,
  ConnectingState: 2,
  ConnectedState: 3,
  BoundState: 4,
  ListeningState: 5,
  ClosingState: 6,
};

/**
 * @param {object} [options]
 * @param {Record<string, (req) => object>} [options.onSend]  `"<proto> <ip>:<port>"` -> called
 *   after each send to that peer with { socket, bytes, text }; may return a reply
 *   ({ data | buffer }) that is delivered to the sending socket at the next flush()
 * @param {Set<string>} [options.unreachableTcp]  `"<ip>:<port>"` peers whose TCP connect fails
 */
export function createMockNet(options = {}) {
  const routes = [];
  const pendingXhr = [];
  const pendingMessages = [];
  const net = {
    sockets: [],
    sent: [],
    http: [],
    route(method, url, reply) {
      routes.push({ method: method.toUpperCase(), url, reply });
    },
    /** Datagrams/segments sent to `ip` (and `port`, when given). */
    sentTo(ip, port) {
      return net.sent.filter((s) => s.ip === ip && (port === undefined || s.port === port));
    },
    /** Feed a message to a socket's 'message' callbacks now. */
    deliver(socket, msg) {
      const buffer = msg.buffer ? toBytes(msg.buffer) : toBytes(msg.data ?? "");
      const value = { data: text(buffer), buffer, address: msg.address ?? socket.peer?.ip ?? "0.0.0.0", port: msg.port ?? socket.peer?.port ?? 0, timestamp: Date.now() };
      for (const cb of socket.callbacks.message ?? []) cb(value);
    },
    /** Complete pending asynchronous requests and deliver queued replies. */
    flush() {
      while (pendingXhr.length > 0 || pendingMessages.length > 0) {
        const xhrs = pendingXhr.splice(0);
        for (const x of xhrs) x.__complete();
        const msgs = pendingMessages.splice(0);
        for (const [socket, msg] of msgs) net.deliver(socket, msg);
      }
    },
    udp: null,
    tcp: null,
    XMLHttpRequest: null,
  };

  function record(socket, proto, ip, port, data) {
    const bytes = toBytes(data);
    const entry = { proto, ip, port, bytes, text: text(bytes), socket };
    net.sent.push(entry);
    const hook = options.onSend?.[`${proto} ${ip}:${port}`];
    const reply = hook?.({ socket, bytes, text: entry.text });
    if (reply) pendingMessages.push([socket, { address: ip, port, ...reply }]);
    return bytes.length;
  }

  function makeSocket(proto) {
    const socket = {
      ...STATES,
      proto,
      state: STATES.UnconnectedState,
      callbacks: {},
      peer: null,
      local: null,
      listening: null,
      closed: false,
      on(event, cb) {
        (socket.callbacks[event] ??= []).push(cb);
      },
      bind(port = 0) {
        socket.local = { address: "0.0.0.0", port: Number(port) || 50000 + net.sockets.indexOf(socket) };
        socket.state = STATES.BoundState;
        for (const cb of socket.callbacks.listening ?? []) cb();
        return true;
      },
      connect(ip, port) {
        if (proto === "tcp" && options.unreachableTcp?.has(`${ip}:${port}`)) {
          for (const cb of socket.callbacks.error ?? []) cb(0, "Connection refused");
          return false;
        }
        socket.peer = { ip: String(ip), port: Number(port) };
        if (!socket.local) socket.local = { address: "192.168.1.2", port: 50000 + net.sockets.indexOf(socket) };
        socket.state = STATES.ConnectedState;
        for (const cb of socket.callbacks.connection ?? []) cb();
        return true;
      },
      send(data) {
        if (!socket.peer || socket.closed) return -1;
        return record(socket, proto, socket.peer.ip, socket.peer.port, data);
      },
      write(data, ip, port) {
        if (socket.closed) return -1;
        if (!socket.local) socket.local = { address: "0.0.0.0", port: 50000 + net.sockets.indexOf(socket) };
        return record(socket, proto, String(ip), Number(port), data);
      },
      // Lightloom extension (TCP): listen on the LAN address facing `peerIp`; only it may connect.
      listen(port, peerIp) {
        socket.listening = { address: "192.168.1.2", port: Number(port) || 55000 + net.sockets.indexOf(socket), peer: String(peerIp) };
        socket.state = STATES.ListeningState;
        return { address: socket.listening.address, port: socket.listening.port };
      },
      disconnect() {
        socket.peer = null;
        socket.state = STATES.UnconnectedState;
      },
      close() {
        socket.closed = true;
        socket.state = STATES.UnconnectedState;
        for (const cb of socket.callbacks.close ?? []) cb();
      },
      address() {
        return { address: socket.local?.address ?? "0.0.0.0", port: socket.local?.port ?? 0, protocol: "IPv4" };
      },
      remoteAddress() {
        return { address: socket.peer?.ip ?? "0.0.0.0", port: socket.peer?.port ?? 0, protocol: "IPv4" };
      },
    };
    net.sockets.push(socket);
    return socket;
  }

  /** A device connecting to a listening TCP socket: returns the accepted socket. */
  net.incoming = (listener, ip = listener.listening?.peer, port = 40000) => {
    const client = makeSocket("tcp");
    client.peer = { ip, port };
    client.state = STATES.ConnectedState;
    for (const cb of listener.callbacks.connection ?? []) cb(client);
    return client;
  };

  net.udp = {
    createSocket: () => makeSocket("udp"),
    // Feature global after device.addFeature("udp"): send(ip, port, bytes, endianness).
    send: (ip, port, data) => record(null, "udp", String(ip), Number(port), data),
  };
  net.tcp = { createSocket: () => makeSocket("tcp") };

  function lookup(method, url) {
    return routes.findLast((r) => r.method === method && (r.url instanceof RegExp ? r.url.test(url) : r.url === url));
  }

  class MockXMLHttpRequest {
    constructor() {
      this.readyState = 0;
      this.status = 0;
      this.statusText = "";
      this.responseText = "";
      this.response = "";
      this.responseType = "";
      this.timeout = 0;
      this.onreadystatechange = null;
      this.onload = null;
      this.onerror = null;
      this.ontimeout = null;
      this.headers = {};
      this.responseHeaders = {};
    }
    open(method, url, async = true) {
      this.method = String(method).toUpperCase();
      this.url = String(url);
      this.async = async !== false;
      this.readyState = 1;
    }
    setRequestHeader(name, value) {
      this.headers[String(name)] = String(value);
    }
    getResponseHeader(name) {
      return this.responseHeaders[String(name).toLowerCase()] ?? null;
    }
    getAllResponseHeaders() {
      return Object.entries(this.responseHeaders).map(([k, v]) => `${k}: ${v}`).join("\r\n");
    }
    abort() {
      this.aborted = true;
    }
    send(body = null) {
      const req = { method: this.method, url: this.url, headers: { ...this.headers }, body: body == null ? null : String(body), async: this.async };
      net.http.push(req);
      this.__complete = () => {
        if (this.aborted) return;
        const route = lookup(req.method, req.url);
        let reply = route ? (typeof route.reply === "function" ? route.reply(req) : route.reply) : null;
        this.readyState = 4;
        if (!reply) {
          this.status = 0;
          this.responseText = "";
          this.response = "";
          this.onreadystatechange?.();
          this.onerror?.();
          return;
        }
        const payload = typeof reply.body === "string" ? reply.body : reply.body === undefined ? "" : JSON.stringify(reply.body);
        this.status = reply.status ?? 200;
        this.statusText = reply.statusText ?? "";
        this.responseHeaders = Object.fromEntries(Object.entries(reply.headers ?? {}).map(([k, v]) => [k.toLowerCase(), String(v)]));
        this.responseText = payload;
        this.response = this.responseType === "json" ? safeJson(payload) : payload;
        this.onreadystatechange?.();
        this.onload?.();
      };
      if (this.async) {
        pendingXhr.push(this);
      } else {
        this.__complete();
      }
    }
  }
  MockXMLHttpRequest.UNSENT = 0;
  MockXMLHttpRequest.OPENED = 1;
  MockXMLHttpRequest.HEADERS_RECEIVED = 2;
  MockXMLHttpRequest.LOADING = 3;
  MockXMLHttpRequest.DONE = 4;
  net.XMLHttpRequest = MockXMLHttpRequest;
  return net;
}

function safeJson(text) {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

/**
 * The `service` global of a network plugin's discovery context.
 * @param {object} [options]
 * @param {Record<string, Record<string, string>>} [options.settings]  initial settings: id -> key -> value
 */
export function createMockService(options = {}) {
  const settings = new Map();
  for (const [id, keys] of Object.entries(options.settings ?? {})) {
    for (const [key, value] of Object.entries(keys)) settings.set(`${id}/${key}`, String(value));
  }
  const checkKey = (what, s) => {
    if (String(s).includes("/") || String(s).includes("..")) throw new Error(`${what} cannot contain a '/' or '..'`);
  };
  const service = {
    logs: [],
    controllers: [],
    announced: [],
    suppressed: [],
    broadcasts: [],
    settings,
    log(message) {
      service.logs.push(typeof message === "string" ? message : JSON.stringify(message));
    },
    addController(obj) {
      if (!obj || typeof obj !== "object") throw new Error("addController(): instance must be an object!");
      if (obj.id === undefined) throw new Error("addController(): instance must have an 'id' property!");
      if (!service.controllers.some((c) => c.id === obj.id)) service.controllers.push({ id: obj.id, obj });
    },
    removeController(obj) {
      const id = obj?.id;
      service.controllers = service.controllers.filter((c) => c.id !== id);
    },
    updateController(obj) {
      service.updated = (service.updated ?? 0) + 1;
      return obj;
    },
    getController(id) {
      return service.controllers.find((c) => c.id === id)?.obj;
    },
    hasController(id) {
      return service.controllers.some((c) => c.id === id);
    },
    announceController(obj) {
      // The host deep-copies the controller's data (functions dropped) into the device context.
      service.announced.push(JSON.parse(JSON.stringify(obj.obj ?? obj)));
    },
    suppressController(obj) {
      service.suppressed.push(obj?.id);
    },
    getSetting(id, key) {
      checkKey("getSetting(): id", id);
      checkKey("getSetting(): key", key);
      return settings.get(`${id}/${key}`);
    },
    saveSetting(id, key, value) {
      checkKey("saveSetting(): id", id);
      checkKey("saveSetting(): key", key);
      if (value === undefined) settings.delete(`${id}/${key}`);
      else settings.set(`${id}/${key}`, typeof value === "string" ? value : JSON.stringify(value));
    },
    removeSetting(id, key) {
      settings.delete(`${id}/${key}`);
    },
    broadcast(message) {
      service.broadcasts.push(typeof message === "string" ? message : toBytes(message));
    },
    resolve(hostname, cb) {
      const ip = options.hosts?.[hostname];
      if (ip) cb({ protocol: "IPV4", ip });
    },
  };
  return service;
}

/**
 * Install the network globals a plugin context has: `device` (with addFeature: "udp" installs
 * the `udp` feature global, "base64" the base64 one), `service`, `controller`,
 * `XMLHttpRequest`, the mock net behind the @SignalRGB modules, and parameter globals.
 * Returns a function that restores the previous globals.
 */
export function installNetGlobals({ net, device, service, controller, params = {} }) {
  const previous = new Map();
  const set = (name, value) => {
    if (!previous.has(name)) previous.set(name, Object.getOwnPropertyDescriptor(globalThis, name));
    globalThis[name] = value;
  };
  set("__lightloomMockNet", net);
  set("XMLHttpRequest", net.XMLHttpRequest);
  if (device) {
    device.features = device.features ?? [];
    device.addFeature = (name) => {
      device.features.push(name);
      if (name === "udp") set("udp", { send: net.udp.send });
    };
    set("device", device);
  }
  if (service) set("service", service);
  if (controller !== undefined) set("controller", controller);
  for (const [name, value] of Object.entries(params)) set(name, value);
  return () => {
    for (const [name, descriptor] of previous) {
      if (descriptor) Object.defineProperty(globalThis, name, descriptor);
      else delete globalThis[name];
    }
  };
}

/**
 * Construct the plugin's DiscoveryService the way the host does (`new DiscoveryService()`, bound
 * to the global `discovery`) and call its Initialize(). Globals must be installed first.
 */
export function startService(plugin) {
  const discovery = new plugin.DiscoveryService();
  globalThis.discovery = discovery;
  discovery.Initialize?.();
  return discovery;
}
