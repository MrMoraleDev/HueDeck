// SPDX-License-Identifier: GPL-3.0-or-later
//
// Mock of the SignalRGB-compatible SMBus `bus` objects for first-party SMBus plugin tests
// (Node 24, node:test). Conventions mirror the Lightloom compat host (crates/lightloom-plugin-host
// js/host.js `nativeBusMembers`, docs/host-api/smbus.md):
//   * `free` is the Scan(bus) argument (address first), `fixed` the global `bus` of a device
//     (address bound); both share the register map and the operation log.
//   * Transfers never throw: they return 0 / the value read, or a negative status (-1; NVIDIA
//     buses have no WriteQuick). Bad arguments throw RangeError / TypeError like the host.
//   * WriteBlock(reg, len, data) sends exactly `len` bytes: longer data is clipped, shorter
//     data zero-padded. The log records the count byte: { op: "WriteBlock", data: [len, ...] }.
//   * `indexed` devices model the ENE-style indexed register file of DRAM RGB controllers
//     (Crucial, ENE): WriteWord(0x00, swapped reg) sets the 16-bit pointer, ReadByte(0x81) /
//     WriteByte(0x01) / WriteBlock(0x03) go through it (the pointer does not advance), other
//     commands and Receive Byte read the `direct` map. Writing 0xF0 to 0x82F0 moves the device
//     to the 7-bit address (0x82EF) >> 1: the address remap of a module still at 0x27.
//   * `dryRun` plays Lightloom's --dry-run: IsDryRun() answers true, writes are logged with
//     `dry: true` and answer 0 but change nothing; reads still read the devices.
//   * `batchWrites` plays Lightloom's SmbusBatchWrites(): inside TransactionLock writes answer 0 at
//     once and are sent at the next read or TransactionUnlock(), which answers 0 or the status of
//     the first that failed (the ones after it are dropped). Without it TransactionUnlock()
//     answers nothing, as on SignalRGB.

const KINDS = ["system", "amd", "intel", "nvidia", "amdgpu", "nuvoton"];

/**
 * @param {object} [options]
 * @param {string} [options.kind]  "amd" | "intel" | "system" | "nvidia" | "amdgpu" | "nuvoton"
 * @param {{vendor?: number, device?: number, subVendor?: number, subDevice?: number}} [options.pci]
 * @param {string} [options.name]  bus Name()
 * @param {number} [options.address]  address the fixed bus is bound to
 * @param {Record<number|string, Record<number|string, number>>} [options.registers]
 *   devices present: address -> { register: value } (unset registers read 0xFF)
 * @param {Record<number|string, {direct?: Record<number|string, number>, memory?: Record<number|string, number>}>} [options.indexed]
 *   ENE-style indexed devices: address -> identification registers and 16-bit register file
 * @param {Iterable<number>} [options.failBlockAt]  addresses whose WriteBlock returns -1
 * @param {Iterable<number>} [options.nack]  addresses that answer nothing (every transfer -1)
 * @param {boolean} [options.lockFails]  TransactionLock() answers false (another program keeps the bus)
 * @param {boolean} [options.dryRun]  Lightloom --dry-run: writes logged, never applied
 * @param {boolean} [options.batchWrites]  Lightloom SmbusBatchWrites(): writes queued under the lock
 */
export function createMockBus(options = {}) {
  const kind = options.kind ?? "amd";
  if (!KINDS.includes(kind)) throw new Error(`unknown bus kind ${kind}`);
  const pci = { vendor: 0, device: 0, subVendor: 0, subDevice: 0, ...(options.pci ?? {}) };
  const name = options.name ?? "Mock Bus";
  const address = options.address ?? 0;
  const registers = new Map();
  for (const [addr, regs] of Object.entries(options.registers ?? {})) {
    registers.set(Number(addr), new Map(Object.entries(regs).map(([r, v]) => [Number(r), Number(v) & 0xFF])));
  }
  const toMap = (o) => new Map(Object.entries(o ?? {}).map(([r, v]) => [Number(r), Number(v) & 0xFF]));
  const indexed = new Map();
  for (const [addr, dev] of Object.entries(options.indexed ?? {})) {
    indexed.set(Number(addr), { pointer: 0, direct: toMap(dev.direct), memory: toMap(dev.memory) });
  }
  const failBlock = new Set(options.failBlockAt ?? []);
  const lockFails = options.lockFails === true;
  const nack = new Set(options.nack ?? []);
  const dryRun = options.dryRun === true;
  const batchWrites = options.batchWrites === true;
  const queue = [];
  let queueStatus;
  const ops = [];
  const logs = [];
  const pauses = [];
  let locked = false;

  const present = (a) => (registers.has(a) || indexed.has(a)) && !nack.has(a);
  const regsOf = (a) => registers.get(a);
  const ix = (a) => indexed.get(a);
  // An indexed device's register write; the remap command moves the device.
  const ixWrite = (a, value) => {
    const d = ix(a);
    d.memory.set(d.pointer, value);
    if (d.pointer === 0x82F0 && value === 0xF0) {
      const to = (d.memory.get(0x82EF) ?? 0) >> 1;
      indexed.delete(a);
      indexed.set(to, d);
    }
  };
  const num = (method, what, v, max) => {
    if (typeof v !== "number" || !Number.isFinite(v) || v < 0 || v > max) {
      throw new RangeError(`${method}(): ${what} must be a number between 0 and ${max}`);
    }
    return Math.trunc(v);
  };
  const block = (method, len, data) => {
    if (!Array.isArray(data)) throw new TypeError(`${method}(): Data must be an array.`);
    const n = num(method, "Length", len, 32);
    return Array.from({ length: n }, (_, i) => (i < data.length ? Number(data[i]) & 0xFF : 0));
  };
  const record = (op, addr, reg, data) => ops.push({ op, addr, reg, data, locked });
  // Send the queued writes; the status of the first that failed (the rest dropped), or 0.
  const flush = () => {
    let status = 0;
    while (queue.length) {
      const send = queue.shift();
      const r = send();
      if (r < 0) {
        status = r;
        queue.length = 0;
      }
    }
    if (status < 0 && queueStatus === undefined) queueStatus = status;
    return status;
  };
  // A write as the host makes it: logged only in a dry run, queued under the lock with batchWrites.
  const write = (fn, op, addr, reg, data) => {
    if (dryRun) {
      ops.push({ op, addr, reg, data, locked, dry: true });
      return 0;
    }
    if (batchWrites && locked) {
      queue.push(fn);
      return 0;
    }
    return fn();
  };
  // A read after the queued writes; not sent when one of them failed.
  const read = (fn) => (flush() < 0 ? -1 : fn());

  const raw = {
    WriteQuick(a) {
      record("WriteQuick", a, undefined, []);
      if (kind === "nvidia") return -1;
      return present(a) ? 0 : -1;
    },
    ReadByte(a, r) {
      record("ReadByte", a, r, []);
      if (!present(a)) return -1;
      if (ix(a)) return r === 0x81 ? ix(a).memory.get(ix(a).pointer) ?? 0xFF : ix(a).direct.get(r) ?? 0xFF;
      return regsOf(a).get(r) ?? 0xFF;
    },
    WriteByte(a, r, v) {
      record("WriteByte", a, r, [v]);
      if (!present(a)) return -1;
      if (ix(a)) {
        if (r === 0x01) ixWrite(a, v);
        else ix(a).direct.set(r, v);
        return 0;
      }
      regsOf(a).set(r, v);
      return 0;
    },
    ReadWord(a, r) {
      record("ReadWord", a, r, []);
      if (!present(a) || ix(a)) return -1;
      return (regsOf(a).get(r) ?? 0xFF) | ((regsOf(a).get((r + 1) & 0xFF) ?? 0xFF) << 8);
    },
    WriteWord(a, r, w) {
      record("WriteWord", a, r, [w & 0xFF, (w >> 8) & 0xFF]);
      if (!present(a)) return -1;
      if (ix(a)) {
        if (r !== 0x00) return -1;
        // Byte-swapped on the wire: the low byte sent first is the pointer's high byte.
        ix(a).pointer = ((w & 0xFF) << 8) | ((w >> 8) & 0xFF);
        return 0;
      }
      regsOf(a).set(r, w & 0xFF);
      regsOf(a).set((r + 1) & 0xFF, (w >> 8) & 0xFF);
      return 0;
    },
    WriteBlock(a, r, data) {
      record("WriteBlock", a, r, [data.length, ...data]);
      if (!present(a) || failBlock.has(a)) return -1;
      if (ix(a)) {
        if (r !== 0x03) return -1;
        data.forEach((b, i) => ix(a).memory.set(ix(a).pointer + i, b));
        return 0;
      }
      data.forEach((b, i) => regsOf(a).set((r + i) & 0xFF, b));
      return 0;
    },
    ReadBlockBytes(a, r, len) {
      record("ReadBlockBytes", a, r, [len]);
      if (!present(a) || ix(a)) return [-1, []];
      return [0, Array.from({ length: len }, (_, i) => regsOf(a).get((r + i) & 0xFF) ?? 0xFF)];
    },
    ReadByteWithoutRegister(a) {
      record("ReadByteWithoutRegister", a, undefined, []);
      if (kind === "nuvoton") return -1;
      if (!present(a)) return -1;
      return ix(a) ? ix(a).direct.get(0) ?? 0xFF : regsOf(a).get(0) ?? 0xFF;
    },
  };
  // The transfers as the host hands them over (dry run, queued writes).
  const io = {
    WriteQuick: (a) => write(() => raw.WriteQuick(a), "WriteQuick", a, undefined, []),
    ReadByte: (a, r) => read(() => raw.ReadByte(a, r)),
    WriteByte: (a, r, v) => write(() => raw.WriteByte(a, r, v), "WriteByte", a, r, [v]),
    ReadWord: (a, r) => read(() => raw.ReadWord(a, r)),
    WriteWord: (a, r, w) => write(() => raw.WriteWord(a, r, w), "WriteWord", a, r, [w & 0xFF, (w >> 8) & 0xFF]),
    WriteBlock: (a, r, data) => write(() => raw.WriteBlock(a, r, data), "WriteBlock", a, r, [data.length, ...data]),
    ReadBlockBytes: (a, r, len) => (flush() < 0 ? [-1, []] : raw.ReadBlockBytes(a, r, len)),
    ReadByteWithoutRegister: (a) => read(() => raw.ReadByteWithoutRegister(a)),
  };

  const identity = (fixed) => ({
    Vendor: () => pci.vendor,
    SubVendor: () => pci.subVendor,
    Product: () => pci.device,
    SubDevice: () => pci.subDevice,
    Name: () => name,
    Type: () => (fixed ? "Fixed" : "Free"),
    GetAddress: () => address,
    IsSystemBus: () => kind === "system" || kind === "amd" || kind === "intel",
    IsAMDBus: () => kind === "amd",
    IsIntelBus: () => kind === "intel",
    IsNvidiaBus: () => kind === "nvidia",
    IsAMDGpuBus: () => kind === "amdgpu",
    IsNuvotonBus: () => kind === "nuvoton",
    IsDryRun: () => dryRun,
    log: (msg) => logs.push(String(msg)),
    pause: (ms) => pauses.push(ms),
    // Lightloom's TransactionLock() answers whether the bus is held (host.js).
    TransactionLock: () => {
      if (lockFails) return false;
      locked = true;
      return true;
    },
    TransactionUnlock: () => {
      if (!batchWrites) {
        locked = false;
        return undefined;
      }
      flush();
      locked = false;
      const status = queueStatus ?? 0;
      queueStatus = undefined;
      return status;
    },
    HasTransactionLock: () => locked,
    toString: () => (fixed ? `I2CBusWrapperFixedAddress(${name})` : `I2CBusWrapper(${name})`),
  });

  const free = {
    ...identity(false),
    WriteQuick: (a) => io.WriteQuick(num("WriteQuick", "Address", a, 255)),
    ReadByte: (a, r) => io.ReadByte(num("ReadByte", "Address", a, 255), num("ReadByte", "Register", r, 255)),
    WriteByte: (a, r, v) =>
      io.WriteByte(num("WriteByte", "Address", a, 255), num("WriteByte", "Register", r, 255), num("WriteByte", "Byte", v, 255)),
    ReadWord: (a, r) => io.ReadWord(num("ReadWord", "Address", a, 255), num("ReadWord", "Register", r, 255)),
    WriteWord: (a, r, w) =>
      io.WriteWord(num("WriteWord", "Address", a, 255), num("WriteWord", "Register", r, 255), num("WriteWord", "Word", w, 65535)),
    WriteBlock: (a, r, len, data) =>
      io.WriteBlock(num("WriteBlock", "Address", a, 255), num("WriteBlock", "Register", r, 255), block("WriteBlock", len, data)),
    ReadByteWithoutRegister: (a) => io.ReadByteWithoutRegister(num("ReadByteWithoutRegister", "Address", a, 255)),
  };

  const fixed = {
    ...identity(true),
    ReadByte: (r) => io.ReadByte(address, num("ReadByte", "Register", r, 255)),
    WriteByte: (r, v) => io.WriteByte(address, num("WriteByte", "Register", r, 255), num("WriteByte", "Byte", v, 255)),
    ReadWord: (r) => io.ReadWord(address, num("ReadWord", "Register", r, 255)),
    WriteWord: (r, w) => io.WriteWord(address, num("WriteWord", "Register", r, 255), num("WriteWord", "Word", w, 65535)),
    WriteBlock: (r, len, data) => io.WriteBlock(address, num("WriteBlock", "Register", r, 255), block("WriteBlock", len, data)),
    ReadBlockBytes: (r, len) => io.ReadBlockBytes(address, num("ReadBlockBytes", "Register", r, 255), num("ReadBlockBytes", "Length", len, 32)),
    ReadByteWithoutRegister: () => io.ReadByteWithoutRegister(address),
  };

  return {
    free,
    fixed,
    ops,
    logs,
    pauses,
    /** Current register map of the device at `addr` (a Map), or undefined. */
    registers: (addr) => registers.get(addr),
    /** Register file of the indexed device now at `addr` (a Map keyed by 16-bit register), or undefined. */
    indexedRegisters: (addr) => indexed.get(addr)?.memory,
    /** Addresses an indexed device answers at now. */
    indexedAddresses: () => [...indexed.keys()].sort((x, y) => x - y),
    /** Writes only: [{ op, addr, reg, data }]. */
    writes: () => ops.filter((o) => o.op.startsWith("Write")),
    isLocked: () => locked,
  };
}

/** Install `device` and the fixed `bus` (plus parameter globals); returns a function that restores them. */
export function installGlobals(device, bus, params = {}) {
  const previous = new Map();
  const set = (name, value) => {
    previous.set(name, Object.getOwnPropertyDescriptor(globalThis, name));
    globalThis[name] = value;
  };
  set("device", device);
  set("bus", bus);
  for (const [name, value] of Object.entries(params)) {
    set(name, value);
  }
  return () => {
    for (const [name, descriptor] of previous) {
      if (descriptor) {
        Object.defineProperty(globalThis, name, descriptor);
      } else {
        delete globalThis[name];
      }
    }
  };
}

/** `"0x2A"` / 42 -> 42 (fixtures write registers as hex strings). */
export function num(v) {
  return Number(v);
}
