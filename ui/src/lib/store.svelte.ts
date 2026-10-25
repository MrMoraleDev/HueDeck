// SPDX-License-Identifier: GPL-3.0-or-later
// Live daemon state: the snapshot (refetched on "state" events, and once a
// second while devices run), the log lines, and the connection status.
//
// The snapshot is reconciled into the existing state, not replaced: a refetch that
// only moves frame counters fires only the counters' signals, so lists, selects and
// sliders bound to the rest of the snapshot are left alone (no re-render, no reset).

import { api, wsUrl } from "./api";
import { EventBus } from "./events";
import { reconcile } from "./reconcile";
import type { DashboardAction, Snapshot } from "./types";

/** Log lines kept for the Settings panel and device reports. */
const LOG_LINES = 500;
/** Log lines are rendered at most this often (they come in bursts). */
const LOG_FLUSH_MS = 250;

/** A message for the banner; a failed fetch means the daemon is not running. */
function describe(e: unknown): string {
  if (e instanceof TypeError) return "Lightloom is not running (or not reachable on this address). Start it and this page reconnects.";
  return e instanceof Error ? e.message : String(e);
}

class DaemonState {
  snapshot = $state<Snapshot | null>(null);
  connected = $state(false);
  error = $state<string | null>(null);
  /** The last LOG_LINES log lines; replaced (never mutated) on each flush. */
  logs = $state.raw<string[]>([]);
  /** The last toast an action URL sent (`app/toast`). */
  toast = $state<{ title: string; message: string } | null>(null);
  /** Called for `view` and `raiseWindow` actions (the app switches tabs). */
  onAction: ((a: DashboardAction) => void) | null = null;
  /** Every push event by kind (pages follow theirs, e.g. `macros`), and `open` on (re)connect. */
  readonly events = new EventBus();
  private socket: WebSocket | null = null;
  private pending = false;
  private again = false;
  private incoming: string[] = [];
  private flushTimer: ReturnType<typeof setTimeout> | null = null;
  private toastTimer: ReturnType<typeof setTimeout> | null = null;

  async refresh() {
    if (this.pending) {
      this.again = true;
      return;
    }
    this.pending = true;
    try {
      const next = await api.state();
      if (this.snapshot) reconcile(this.snapshot, next);
      else this.snapshot = next;
      if (this.error !== null) this.error = null;
    } catch (e) {
      this.error = describe(e);
    } finally {
      this.pending = false;
      if (this.again) {
        this.again = false;
        void this.refresh();
      }
    }
  }

  private log(line: string) {
    this.incoming.push(`${new Date().toLocaleTimeString()}  ${line}`);
    if (this.flushTimer !== null) return;
    this.flushTimer = setTimeout(() => {
      this.flushTimer = null;
      const lines = this.incoming;
      this.incoming = [];
      this.logs = [...this.logs, ...lines].slice(-LOG_LINES);
    }, LOG_FLUSH_MS);
  }

  connect() {
    void this.refresh();
    const ws = new WebSocket(wsUrl("/v1/events"));
    this.socket = ws;
    ws.onopen = () => {
      this.connected = true;
      this.events.emit("open", null);
    };
    ws.onmessage = (m) => {
      try {
        const e = JSON.parse(m.data as string) as { kind: string; data?: unknown };
        this.events.emit(e.kind, e.data);
        if (e.kind === "log" && typeof e.data === "string") {
          this.log(e.data);
        } else if (e.kind === "state") {
          void this.refresh();
        } else if (e.kind === "action" && e.data && typeof e.data === "object") {
          const a = e.data as DashboardAction;
          if (a.action === "toast") {
            this.toast = { title: a.title, message: a.message };
            if (this.toastTimer !== null) clearTimeout(this.toastTimer);
            this.toastTimer = setTimeout(() => (this.toast = null), 8000);
          } else {
            this.onAction?.(a);
          }
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
      if (this.error !== null) this.error = null;
      return r;
    } catch (e) {
      this.error = describe(e);
      return undefined;
    } finally {
      void this.refresh();
    }
  }
}

export const daemon = new DaemonState();
