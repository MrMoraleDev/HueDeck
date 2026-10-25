// SPDX-License-Identifier: GPL-3.0-or-later
// Device reports: the prefilled GitHub issue behind the Devices tab's "Works" and
// "Report a problem" buttons (.github/ISSUE_TEMPLATE/device-report.yml). Nothing is
// sent by Lightloom: the dashboard builds an issues/new URL and the browser opens it;
// the user reads, edits and submits the form on GitHub, or does not.
//
// Everything that goes into the URL is redacted first: the API token, the user name in
// paths (C:\Users\<name> -> %USERPROFILE%), serial numbers (all but the last 4
// characters masked) and network addresses (IPv4, IPv6, MAC).

import type { DeviceView } from "./types";

export const ISSUE_NEW = "https://github.com/MrMoraleDev/Lightloom/issues/new";
export const TEMPLATE = "device-report.yml";
/** URL-encoded bytes the diagnostics may take (GitHub refuses URLs much over 8 KB). */
export const MAX_DIAGNOSTICS = 6000;
/** URL-encoded bytes the "What happens" text may take. */
export const MAX_DETAILS = 1500;
/** Log lines considered before the size cut (newest kept). */
const MAX_LOG_LINES = 60;

export type ReportResult = "works" | "problem";

/** The dropdown options of the form's `result` field. */
export const RESULT_LABELS: Record<ReportResult, string> = {
  works: "Works",
  problem: "Does not work",
};

export interface ReportInput {
  device: DeviceView;
  /** Lightloom's version (snapshot `version`). */
  version: string;
  /** OS and build (snapshot `os`). */
  os: string;
  /** The dashboard's log lines (oldest first); the device's are picked out. */
  logs: string[];
  /** The API token: never in a report. */
  token?: string;
}

export interface DeviceReport {
  title: string;
  /** Form field id -> value, in form order (all redacted). */
  fields: Record<string, string>;
  /** The issues/new URL with everything prefilled. */
  url: string;
  /** Log lines left out for size (oldest first). */
  droppedLogLines: number;
}

/** All but the last 4 characters as `*` (short values unchanged). */
export function maskSerial(serial: string): string {
  return serial.length <= 4 ? serial : "*".repeat(serial.length - 4) + serial.slice(-4);
}

/** The serial-like part of a device key: USB `vid:pid:<serial>`, network `net:<plugin>:<id>`. */
export function keySecret(key: string): string | null {
  const usb = /^[0-9a-f]{4}:[0-9a-f]{4}:(.+)$/i.exec(key);
  if (usb) return usb[1];
  const net = /^net:[^:]+:(.+)$/.exec(key);
  if (net) return net[1];
  return null;
}

/** A device key with its serial (or network id) masked. */
export function maskKey(key: string): string {
  const secret = keySecret(key);
  return secret ? key.slice(0, key.length - secret.length) + maskSerial(secret) : key;
}

const escapeRegExp = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

const USER_PATH = /\b[A-Za-z]:[\\/]+Users[\\/]+[^\\/\s"'<>|:*?]+/gi;
const HOME_PATH = /(^|[\s"'(=])\/(?:home|Users)\/[^/\s"'<>|:*?]+/g;
const MAC = /\b[0-9a-f]{2}(?:([:-])[0-9a-f]{2})(?:\1[0-9a-f]{2}){4}\b/gi;
const IPV4 = /\b(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(?:\.(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}\b/g;
const IPV6_FULL = /\b(?:[0-9a-f]{1,4}:){7}[0-9a-f]{1,4}\b/gi;
// Compressed IPv6 (`fe80::1`, `::1`): at least one group around the `::`.
const IPV6_SHORT =
  /(?<![0-9a-z_:])(?:[0-9a-f]{1,4}(?::[0-9a-f]{1,4}){0,6}::(?:[0-9a-f]{1,4}(?::[0-9a-f]{1,4}){0,6})?|::[0-9a-f]{1,4}(?::[0-9a-f]{1,4}){0,6})(?![0-9a-z_:])/gi;
const BEARER = /\bBearer\s+[A-Za-z0-9._~+/=-]+/gi;
const TOKEN_PARAM = /\b(token|api[-_]?token)([=:]\s*)["']?[A-Za-z0-9._~+/=-]{8,}["']?/gi;

/** Remove what a public issue must not show. `serials` are masked wherever they appear. */
export function redact(text: string, secrets: { token?: string; serials?: string[] } = {}): string {
  let out = text;
  if (secrets.token && secrets.token.length >= 8) out = out.split(secrets.token).join("<api token>");
  out = out.replace(BEARER, "Bearer <api token>").replace(TOKEN_PARAM, "$1$2<api token>");
  out = out.replace(USER_PATH, "%USERPROFILE%").replace(HOME_PATH, "$1~");
  for (const s of secrets.serials ?? []) {
    if (s.length > 4) out = out.replace(new RegExp(escapeRegExp(s), "gi"), maskSerial(s));
  }
  out = out.replace(MAC, "<mac>").replace(IPV6_FULL, "<ip>").replace(IPV6_SHORT, "<ip>").replace(IPV4, "<ip>");
  return out;
}

const hex4 = (n: number) => n.toString(16).toUpperCase().padStart(4, "0");

/** The form's "USB VID:PID, SMBus address or network model" field. */
export function vidPidOf(d: DeviceView): string {
  if (d.key.startsWith("smbus:") || d.pluginType === "smbus") return `SMBus ${d.bus ?? ""}`.trim();
  if (d.key.startsWith("net:") || d.pluginType === "network") return `network device (${d.plugin})`;
  return `${hex4(d.vid)}:${hex4(d.pid)}`;
}

/** The form's "Plugin" field. */
export function pluginOf(d: DeviceView): string {
  return `${d.plugin}${d.pluginVersion ? ` ${d.pluginVersion}` : ""}${d.thirdParty ? " (third-party)" : ""}`;
}

/** Log lines that name the device (key, name or plugin), newest MAX_LOG_LINES. */
export function deviceLogLines(d: DeviceView, logs: string[]): string[] {
  const needles = [d.key, d.name, d.plugin].filter((n) => n && n.length >= 3).map((n) => n.toLowerCase());
  return logs.filter((l) => needles.some((n) => l.toLowerCase().includes(n))).slice(-MAX_LOG_LINES);
}

/** The diagnostics block, before redaction: facts first, then the log lines (oldest first). */
function facts(d: DeviceView): string[] {
  const lines = [
    `key: ${maskKey(d.key)}`,
    `name: ${d.name}`,
    `plugin: ${pluginOf(d)} · type ${d.pluginType || "?"} · device type ${d.deviceType ?? "?"} · ${d.tested ? "tested" : "untested"}`,
    `plugin file: ${d.pluginPath}`,
    `state: ${d.state} · ${d.enabled ? "enabled" : "disabled"} · ${d.connected ? "connected" : "not connected"}`,
    `driver: ${d.driverProblem ?? "ok"}`,
  ];
  if (d.bus) lines.push(`bus: ${d.bus}`);
  lines.push(`counters: ${d.frames} frames · ${d.renderErrors} render errors · ${d.ioErrors} I/O errors`);
  if (d.interfaces?.length) lines.push("interfaces:", ...d.interfaces.map((i) => `  ${i}`));
  if (d.endpoints.length) {
    lines.push("endpoints:");
    for (const e of d.endpoints) {
      lines.push(`  ${e.kind} "${e.name}"${e.channel ? ` on ${e.channel}` : ""} · ${e.size[0]}x${e.size[1]} · ${e.leds} LEDs`);
    }
  }
  if (d.channels.length) {
    lines.push("channels:");
    for (const c of d.channels) {
      const leds = c.components.reduce((n, x) => n + x.LedCount, 0);
      lines.push(`  ${c.name}: ${c.components.length} components, ${leds} LEDs${c.ledLimit ? ` of ${c.ledLimit}` : ""}`);
    }
  }
  const messages = d.messages ?? [];
  if (messages.length) {
    lines.push("messages:");
    for (const m of messages) {
      const level = (m.priority ?? 0) >= 2 ? "error" : m.priority === 1 ? "warning" : "info";
      lines.push(`  [${level}] ${m.title ? `${m.title}: ` : ""}${m.message}`);
    }
  }
  return lines;
}

const encodedLength = (s: string) => encodeURIComponent(s).length;

/** Cut `text` so that its URL encoding fits `max`, marking the cut. */
export function fitEncoded(text: string, max: number, mark = "\n… (cut for length)"): string {
  if (encodedLength(text) <= max) return text;
  // Never end on half a surrogate pair (encodeURIComponent would throw).
  const head = (n: number) => {
    const s = text.slice(0, n);
    return /[\uD800-\uDBFF]$/.test(s) ? s.slice(0, -1) : s;
  };
  let lo = 0;
  let hi = text.length;
  while (lo < hi) {
    const mid = Math.ceil((lo + hi) / 2);
    if (encodedLength(head(mid) + mark) <= max) lo = mid;
    else hi = mid - 1;
  }
  return head(lo) + mark;
}

/**
 * The redacted diagnostics, at most `max` URL-encoded bytes: the oldest log lines go
 * first, then (only if the facts alone are too long) the text is cut at the end.
 */
export function diagnosticsOf(input: ReportInput, max = MAX_DIAGNOSTICS): { text: string; dropped: number } {
  const { device: d } = input;
  const secret = keySecret(d.key);
  const scrub = (s: string) => redact(s, { token: input.token, serials: secret ? [secret] : [] });
  const head = scrub(facts(d).join("\n"));
  let logs = deviceLogLines(d, input.logs).map(scrub);
  let dropped = 0;
  const compose = () => {
    if (!logs.length) return head + (dropped ? `\nrecent log lines: ${dropped} left out for length` : "");
    const note = dropped ? ` (${dropped} older left out for length)` : "";
    return `${head}\nrecent log lines${note}:\n${logs.map((l) => `  ${l}`).join("\n")}`;
  };
  let text = compose();
  while (logs.length && encodedLength(text) > max) {
    logs = logs.slice(1);
    dropped++;
    text = compose();
  }
  return { text: fitEncoded(text, max), dropped };
}

/** The whole report: title, fields and the issues/new URL. */
export function buildReport(input: ReportInput, result: ReportResult, details = ""): DeviceReport {
  const d = input.device;
  const secret = keySecret(d.key);
  const scrub = (s: string) => redact(s, { token: input.token, serials: secret ? [secret] : [] });
  const { text: diagnostics, dropped } = diagnosticsOf(input);
  const vidpid = scrub(vidPidOf(d));
  const fields: Record<string, string> = {
    result: RESULT_LABELS[result],
    device: scrub(d.name),
    vidpid,
    plugin: scrub(pluginOf(d)),
    version: input.version,
    os: scrub(input.os),
  };
  const what = fitEncoded(scrub(details.trim()), MAX_DETAILS);
  if (what) fields.details = what;
  fields.diagnostics = diagnostics;
  const title = `[device] ${fields.device} (${vidpid}): ${result === "works" ? "works" : "problem"}`;
  const query = [`template=${encodeURIComponent(TEMPLATE)}`, `title=${encodeURIComponent(title)}`]
    .concat(Object.entries(fields).map(([k, v]) => `${k}=${encodeURIComponent(v)}`))
    .join("&");
  return { title, fields, url: `${ISSUE_NEW}?${query}`, droppedLogLines: dropped };
}
