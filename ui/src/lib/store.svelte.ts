// SPDX-License-Identifier: Apache-2.0 OR MIT
// Live daemon state: the snapshot (refetched on "state" events, and once a
// second while devices run), the log lines, and the connection status.

import { api, wsUrl } from "./api";
import type { Snapshot } from "./types";

class DaemonState {
  snapshot = $state<Snapshot | null>(null);
  connected = $state(false);
  error = $state<string | null>(null);
  logs = $state<string[]>([]);
  private socket: WebSocket | null = null;
  private pending = false;
  private again = false;

  async refresh() {
    if (this.pending) {
      this.again = true;
      return;
    }
    this.pending = true;
    try {
      this.snapshot = await api.state();
      this.error = null;
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
    } finally {
      this.pending = false;
      if (this.again) {
        this.again = false;
        void this.refresh();
      }
    }
  }

  connect() {
    void this.refresh();
    const ws = new WebSocket(wsUrl("/v1/events"));
    this.socket = ws;
    ws.onopen = () => (this.connected = true);
    ws.onmessage = (m) => {
      try {
        const e = JSON.parse(m.data as string) as { kind: string; data?: unknown };
        if (e.kind === "log" && typeof e.data === "string") {
          const time = new Date().toLocaleTimeString();
          this.logs = [...this.logs.slice(-499), `${time}  ${e.data}`];
        } else if (e.kind === "state") {
          void this.refresh();
        }
      } catch {
        // Not JSON: ignore.
      }
    };
    ws.onclose = () => {
      this.connected = false;
      this.socket = null;
      setTimeout(() => this.connect(), 2000);
    };
  }

  /** Run an API call, show its error, refresh after it. */
  async run<T>(call: () => Promise<T>): Promise<T | undefined> {
    try {
      const r = await call();
      this.error = null;
      return r;
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
      return undefined;
    } finally {
      void this.refresh();
    }
  }
}

export const daemon = new DaemonState();
