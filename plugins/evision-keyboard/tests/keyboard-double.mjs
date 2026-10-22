// SPDX-License-Identifier: GPL-3.0-or-later
//
// Test double of an EVision V2 keyboard, on top of plugins/_testkit/mock-device.mjs. The shared
// mock answers read() with one stored report per report id; these keyboards answer every request
// with its own input report, so this double queues, after each write, the answer `answer(sent)`
// gives (by default the request echoed with status byte 7 = 0, which is what OpenRGB's Query()
// accepts) and hands it to the next read().

import { createMockDevice } from "../../_testkit/mock-device.mjs";

/** The request echoed with status 0: how the firmware confirms a request. */
export function echo(sent) {
  const reply = sent.slice();
  reply[7] = 0;
  return reply;
}

/**
 * @param {object} [options]  createMockDevice options, plus
 * @param {(sent: number[]) => (number[] | number[][] | null | undefined)} [options.answer]
 *   the input report the keyboard sends after the output report `sent` (report id first), a
 *   list of input reports, or null/undefined for none. Default: echo.
 */
export function createKeyboardDevice(options = {}) {
  const device = createMockDevice(options);
  const answer = options.answer ?? echo;
  let queue = [];
  let lastReadSize = 0;

  const write = device.write;
  device.write = (data, length) => {
    const result = write(data, length);
    const sent = device.written[device.written.length - 1].bytes;
    const reply = answer(sent);
    if (reply == null) {
      queue = [];
    } else if (Array.isArray(reply[0])) {
      queue = reply.map((r) => r.slice());
    } else {
      queue = [reply.slice()];
    }
    return result;
  };

  const clearReadBuffer = device.clearReadBuffer;
  device.clearReadBuffer = () => {
    clearReadBuffer();
    queue = [];
  };

  device.read = (reportId, length, timeout) => {
    device.calls.push({ method: "read", args: [reportId, length, timeout] });
    const next = queue.shift();
    if (!next) {
      lastReadSize = 0;
      return [];
    }
    lastReadSize = Math.min(next.length, length);
    const out = new Array(length).fill(0);
    for (let i = 0; i < lastReadSize; i++) {
      out[i] = next[i] & 0xFF;
    }
    return out;
  };
  device.getLastReadSize = () => lastReadSize;

  return device;
}

/** Date.now() under the test's control (the plugin times its keepalive with it). */
export function fakeClock(start = 1_000_000) {
  const real = Date.now;
  let now = start;
  Date.now = () => now;
  return {
    set(t) { now = t; },
    advance(ms) { now += ms; },
    get now() { return now; },
    restore() { Date.now = real; },
  };
}
