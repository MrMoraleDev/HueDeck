// SPDX-License-Identifier: GPL-3.0-or-later
//
// Lightloom first-party plugin: any HID LampArray device. HID LampArray (HID Usage Tables, usage
// page 0x59 "Lighting and Illumination") is the open standard behind Windows Dynamic Lighting:
// the device describes its lamps (how many, where, how many colour levels) and takes colour
// updates in feature reports. Report ids, field sizes and offsets come from the device's own
// report descriptor (device.getHidReportFields(), a Lightloom host extension), so no
// device-specific code is needed. Protocol notes: docs/hardware/hid-lamparray.md.
//
// Lightloom offers this plugin for every device with a LampArray collection that no other plugin
// claims by vendor and product id (HidUsages(), a Lightloom extension).

/* global device */

const LIGHTING_PAGE = 0x59;
const USAGE = {
  LampArray: 0x01,
  AttributesReport: 0x02,
  LampCount: 0x03,
  BoundingBoxWidth: 0x04,
  BoundingBoxHeight: 0x05,
  LampArrayKind: 0x07,
  MinUpdateInterval: 0x08,
  AttributesRequestReport: 0x20,
  LampId: 0x21,
  AttributesResponseReport: 0x22,
  PositionX: 0x23,
  PositionY: 0x24,
  RedLevelCount: 0x28,
  GreenLevelCount: 0x29,
  BlueLevelCount: 0x2A,
  IntensityLevelCount: 0x2B,
  IsProgrammable: 0x2C,
  MultiUpdateReport: 0x50,
  RedChannel: 0x51,
  GreenChannel: 0x52,
  BlueChannel: 0x53,
  IntensityChannel: 0x54,
  UpdateFlags: 0x55,
  RangeUpdateReport: 0x60,
  LampIdStart: 0x61,
  LampIdEnd: 0x62,
  ControlReport: 0x70,
  AutonomousMode: 0x71,
};
const UPDATE_COMPLETE = 0x01;
const KINDS = ["", "keyboard", "mouse", "game controller", "peripheral", "scene", "notification",
  "chassis", "wearable", "furniture", "art"];

const MAX_LAMPS = 1024;
// Cells along the longer side of the device's bounding box.
const GRID_CELLS = 32;
// Render() resends unchanged colours only once this much time has passed.
const KEEPALIVE_MS = 1000;
// Frames closer together than this share of the device's minimum update interval are skipped
// (the rest absorbs frame-time jitter: a 33.3 ms device still gets every frame at 30 fps).
const INTERVAL_SHARE = 0.75;

export function Name() { return "HID LampArray"; }
export function Publisher() { return "Lightloom"; }
export function Version() { return "0.1.0"; }
export function Documentation() { return "hardware/hid-lamparray"; }
export function HidUsages() { return [[LIGHTING_PAGE, USAGE.LampArray]]; }
export function Type() { return "Hid"; }
export function DeviceType() { return "other"; }
// Placeholders: Initialize() sets the grid from the lamps the device reports.
export function Size() { return [1, 1]; }
export function DefaultPosition() { return [0, 0]; }
// GRID_CELLS cells of 4 canvas pixels: at most 128 of the canvas' 200 x 320 pixels.
export function DefaultScale() { return 4.0; }
export function LedNames() { return ["Lamp 0"]; }
export function LedPositions() { return [[0, 0]]; }
export function ImageUrl() { return ""; }

let reports = null;      // the six LampArray feature reports, from the report descriptor
let lamps = [];          // { id, position: [x, y] (µm), cell: [x, y], levels: [r, g, b, intensity], programmable }
let enabled = false;
let minIntervalMs = 0;   // the device's MinUpdateIntervalInMicroseconds, in milliseconds
let lastColours = null;
let lastSentAt = -Infinity;

export function Validate(endpoint) {
  return endpoint.usage_page === LIGHTING_PAGE && endpoint.usage === USAGE.LampArray;
}

export function Initialize() {
  enabled = false;
  lastColours = null;
  const layout = typeof device.getHidReportFields === "function" ? device.getHidReportFields() : null;
  if (!layout) {
    device.log("LampArray: the host cannot read this collection's report descriptor; nothing to drive.");
    return;
  }
  reports = {
    attributes: featureReport(layout, USAGE.AttributesReport),
    request: featureReport(layout, USAGE.AttributesRequestReport),
    response: featureReport(layout, USAGE.AttributesResponseReport),
    multi: featureReport(layout, USAGE.MultiUpdateReport),
    range: featureReport(layout, USAGE.RangeUpdateReport),
    control: featureReport(layout, USAGE.ControlReport),
  };
  if (!reports.attributes || !reports.request || !reports.response || !(reports.multi || reports.range)) {
    device.log("LampArray: the report descriptor lacks the attribute or update reports; nothing to drive.");
    return;
  }

  const attributes = getFeature(reports.attributes);
  if (!attributes) {
    return;
  }
  const count = Math.min(read(reports.attributes, attributes, USAGE.LampCount), MAX_LAMPS);
  const box = [read(reports.attributes, attributes, USAGE.BoundingBoxWidth),
    read(reports.attributes, attributes, USAGE.BoundingBoxHeight)];
  lamps = [];
  for (let id = 0; id < count; id++) {
    const request = emptyReport(reports.request);
    write(reports.request, request, USAGE.LampId, id);
    if (!setFeature(reports.request, request)) {
      return;
    }
    const response = getFeature(reports.response);
    if (!response) {
      return;
    }
    const answered = read(reports.response, response, USAGE.LampId);
    if (answered !== id) {
      device.log(`LampArray: asked for lamp ${id}, the device described lamp ${answered}.`);
    }
    lamps.push({
      id,
      position: [read(reports.response, response, USAGE.PositionX), read(reports.response, response, USAGE.PositionY)],
      levels: [USAGE.RedLevelCount, USAGE.GreenLevelCount, USAGE.BlueLevelCount, USAGE.IntensityLevelCount]
        .map((usage) => read(reports.response, response, usage)),
      programmable: element(reports.response, USAGE.IsProgrammable) === undefined ||
        read(reports.response, response, USAGE.IsProgrammable) !== 0,
    });
  }
  placeLamps(box);

  const info = typeof device.getDeviceInfo === "function" ? device.getDeviceInfo() : null;
  if (info && info.product) {
    device.setName(String(info.product).trim());
  }
  const kind = KINDS[read(reports.attributes, attributes, USAGE.LampArrayKind)] || "vendor-defined";
  const interval = read(reports.attributes, attributes, USAGE.MinUpdateInterval);
  minIntervalMs = interval / 1000;
  device.log(`LampArray: ${lamps.length} lamps, ${kind}, ${(box[0] / 1000).toFixed(0)} x ${(box[1] / 1000).toFixed(0)} mm, ` +
    `updates at most every ${(interval / 1000).toFixed(1)} ms.`);
  setAutonomous(false);
  enabled = true;
}

export function Render() {
  if (!enabled) {
    return;
  }
  const elapsed = Date.now() - lastSentAt;
  if (elapsed < minIntervalMs * INTERVAL_SHARE) {
    return;
  }
  const colours = lamps.map((lamp) => (lamp.programmable ? device.color(lamp.cell[0], lamp.cell[1]) : null));
  if (lastColours !== null && sameColours(colours, lastColours) && elapsed < KEEPALIVE_MS) {
    return;
  }
  if (sendColours(colours)) {
    lastColours = colours;
    lastSentAt = Date.now();
  }
}

// Exit modes (Lightloom extension, docs/host-api/save-shutdown.md).
export function ShutdownModes() { return ["restore", "keep", "off"]; }

// "restore" (and SignalRGB's call without a mode, and suspend): the device takes its own lighting
// back (its autonomous mode). "keep" sends the last frame once more and "off" every programmable
// lamp black, each with the update reports of a frame (the last one flagged UpdateComplete), and
// AutonomousMode stays off, so the lamps keep those colours: the LampArray usage tables define no
// host timeout. Unverified on hardware: a device whose firmware takes its autonomous mode back on
// its own, or Windows Dynamic Lighting taking the device over, would replace them. Nothing is
// stored (LampArray has no persistent store).
export function Shutdown(SystemSuspending, mode) {
  if (!enabled) {
    return;
  }
  if (!SystemSuspending && (mode === "keep" || mode === "off")) {
    sendColours(lamps.map((lamp) => {
      if (!lamp.programmable) {
        return null;
      }
      return mode === "keep" ? device.color(lamp.cell[0], lamp.cell[1]) : [0, 0, 0];
    }));
    return;
  }
  setAutonomous(true);
}

// ---------------------------------------------------------------------------------------------
// Lamps
// ---------------------------------------------------------------------------------------------
// One grid cell per GRID_CELLS-th of the longer side of the bounding box (micrometres); lamps
// that share a cell share its colour.
function placeLamps(box) {
  let width = Math.max(box[0], 1);
  let height = Math.max(box[1], 1);
  for (const lamp of lamps) {
    width = Math.max(width, lamp.position[0] + 1);
    height = Math.max(height, lamp.position[1] + 1);
  }
  const cell = Math.max(width, height) / GRID_CELLS;
  const size = [Math.max(1, Math.ceil(width / cell)), Math.max(1, Math.ceil(height / cell))];
  for (const lamp of lamps) {
    lamp.cell = [Math.min(size[0] - 1, Math.floor(lamp.position[0] / cell)),
      Math.min(size[1] - 1, Math.floor(lamp.position[1] / cell))];
  }
  device.setSize(size);
  device.setControllableLeds(lamps.map((lamp) => `Lamp ${lamp.id}`), lamps.map((lamp) => lamp.cell));
}

// One multi-update report per as many lamps as it holds (the last one marked complete), or one
// range update per lamp on devices without the multi-update report.
function sendColours(colours) {
  const updates = [];
  lamps.forEach((lamp, index) => {
    if (colours[index]) {
      updates.push({ lamp, rgb: colours[index] });
    }
  });
  if (updates.length === 0) {
    return true;
  }
  const report = reports.multi;
  if (report) {
    const ids = elements(report, USAGE.LampId);
    const channels = [USAGE.RedChannel, USAGE.GreenChannel, USAGE.BlueChannel, USAGE.IntensityChannel]
      .map((usage) => elements(report, usage));
    const perReport = Math.min(ids.length, ...channels.slice(0, 3).map((list) => list.length));
    if (perReport === 0) {
      return false;
    }
    for (let start = 0; start < updates.length; start += perReport) {
      const chunk = updates.slice(start, start + perReport);
      const bytes = emptyReport(report);
      write(report, bytes, USAGE.LampCount, chunk.length);
      write(report, bytes, USAGE.UpdateFlags, start + perReport >= updates.length ? UPDATE_COMPLETE : 0);
      chunk.forEach((update, k) => {
        writeElement(bytes, ids[k], update.lamp.id);
        channelValues(update, channels[3][k]).forEach((value, c) => {
          if (channels[c][k]) {
            writeElement(bytes, channels[c][k], value);
          }
        });
      });
      if (!setFeature(report, bytes)) {
        return false;
      }
    }
    return true;
  }
  const range = reports.range;
  for (let i = 0; i < updates.length; i++) {
    const bytes = emptyReport(range);
    write(range, bytes, USAGE.UpdateFlags, i === updates.length - 1 ? UPDATE_COMPLETE : 0);
    write(range, bytes, USAGE.LampIdStart, updates[i].lamp.id);
    write(range, bytes, USAGE.LampIdEnd, updates[i].lamp.id);
    const values = channelValues(updates[i], element(range, USAGE.IntensityChannel));
    [USAGE.RedChannel, USAGE.GreenChannel, USAGE.BlueChannel, USAGE.IntensityChannel]
      .forEach((usage, c) => write(range, bytes, usage, values[c]));
    if (!setFeature(range, bytes)) {
      return false;
    }
  }
  return true;
}

// Red, green, blue scaled to the lamp's level counts; intensity at its highest level (the lamp's
// level count, else the intensity field's largest value).
function channelValues(update, intensity) {
  const [r, g, b] = update.rgb;
  const levels = update.lamp.levels;
  const scale = (value, count) => (count >= 2 ? Math.round(((value & 0xFF) * (count - 1)) / 255) : value & 0xFF);
  const top = levels[3] >= 2 ? levels[3] - 1 : (intensity ? largest(intensity) : 0xFF);
  return [scale(r, levels[0]), scale(g, levels[1]), scale(b, levels[2]), top];
}

// An element's largest value: its logical maximum, or all bits set when the descriptor encodes
// the maximum as a negative number (0x25 0xFF for 255).
function largest(e) {
  return e.logicalMax > 0 ? e.logicalMax : 2 ** e.bitSize - 1;
}

function setAutonomous(on) {
  if (!reports || !reports.control) {
    return;
  }
  const bytes = emptyReport(reports.control);
  write(reports.control, bytes, USAGE.AutonomousMode, on ? 1 : 0);
  setFeature(reports.control, bytes);
}

function sameColours(a, b) {
  for (let i = 0; i < a.length; i++) {
    const x = a[i];
    const y = b[i];
    if ((x === null) !== (y === null) || (x && (x[0] !== y[0] || x[1] !== y[1] || x[2] !== y[2]))) {
      return false;
    }
  }
  return true;
}

// ---------------------------------------------------------------------------------------------
// Reports from the report descriptor
// ---------------------------------------------------------------------------------------------
// The feature report inside the logical collection `collectionUsage`: its id, its length
// (report id included) and every element { usage, bitOffset, bitSize, logicalMax }.
function featureReport(layout, collectionUsage) {
  const fields = (layout.fields || []).filter((f) => f.kind === "feature" &&
    f.collection[0] === LIGHTING_PAGE && f.collection[1] === collectionUsage);
  if (fields.length === 0) {
    return null;
  }
  const id = fields[0].reportId;
  const elementsOf = [];
  let bits = 0;
  for (const field of fields) {
    if (field.reportId !== id) {
      continue;
    }
    const usages = elementUsages(field);
    for (let i = 0; i < field.count; i++) {
      const usage = usages[i];
      if (usage && usage[0] === LIGHTING_PAGE) {
        elementsOf.push({ usage: usage[1], bitOffset: field.bitOffset + i * field.bitSize,
          bitSize: field.bitSize, logicalMax: field.logicalMax });
      }
    }
    bits = Math.max(bits, field.bitOffset + field.count * field.bitSize);
  }
  const info = (layout.reports || []).find((r) => r.kind === "feature" && r.reportId === id);
  return { id, length: info ? info.bytes : 1 + Math.ceil(bits / 8), elements: elementsOf };
}

// The usage of each element of a variable field: the declared usages in order, the last one
// repeated for the remaining elements (HID 1.11, 6.2.2.8).
function elementUsages(field) {
  const out = [];
  for (const [page, min, max] of field.usages || []) {
    for (let usage = min; usage <= max && out.length < field.count; usage++) {
      out.push([page, usage]);
    }
  }
  const last = out[out.length - 1];
  while (last && out.length < field.count) {
    out.push(last);
  }
  return out;
}

function elements(report, usage) {
  return report.elements.filter((e) => e.usage === usage);
}

function element(report, usage) {
  return report.elements.find((e) => e.usage === usage);
}

function read(report, bytes, usage) {
  const e = element(report, usage);
  return e ? readElement(bytes, e) : 0;
}

function write(report, bytes, usage, value) {
  const e = element(report, usage);
  if (e) {
    writeElement(bytes, e, value);
  }
}

// Fields are little-endian, least significant bit first, after the report-id byte.
function readElement(bytes, e) {
  let value = 0;
  for (let i = 0; i < e.bitSize && i < 53; i++) {
    const bit = e.bitOffset + i;
    if (((bytes[1 + (bit >> 3)] ?? 0) >> (bit & 7)) & 1) {
      value += 2 ** i;
    }
  }
  return value;
}

function writeElement(bytes, e, value) {
  let v = Math.max(0, Math.floor(Number(value) || 0));
  for (let i = 0; i < e.bitSize; i++) {
    const bit = e.bitOffset + i;
    const index = 1 + (bit >> 3);
    if (v % 2 === 1) {
      bytes[index] |= 1 << (bit & 7);
    } else {
      bytes[index] &= ~(1 << (bit & 7)) & 0xFF;
    }
    v = Math.floor(v / 2);
  }
}

function emptyReport(report) {
  const bytes = new Array(report.length).fill(0);
  bytes[0] = report.id;
  return bytes;
}

// GET_REPORT(Feature): the bytes, report id first, or null (logged) when the device did not answer.
function getFeature(report) {
  const bytes = device.get_report([report.id], report.length);
  if (!bytes || bytes.length < 2 || (bytes[0] & 0xFF) !== report.id) {
    device.log(`LampArray: GET feature report ${report.id} failed.`);
    return null;
  }
  return Array.from(bytes, (b) => b & 0xFF);
}

function setFeature(report, bytes) {
  if (device.send_report(bytes, report.length) < 0) {
    device.log(`LampArray: SET feature report ${report.id} failed.`);
    return false;
  }
  return true;
}
