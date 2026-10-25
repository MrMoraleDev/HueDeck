// SPDX-License-Identifier: GPL-3.0-or-later
// A periodic image fetch for small live previews (the LCD face preview): one request at a
// time, the next one `every` ms after the last answer, only between start() and stop(). Each
// image becomes an object URL; the previous one is revoked once the new one is shown, so the
// <img> swaps pictures without a blank frame.

export interface PreviewDeps {
  /** Fetch the next image; `null` when there is none (the LCD stopped). */
  load: () => Promise<Blob | null>;
  /** Show this object URL. */
  show: (url: string | null) => void;
  /** Milliseconds between the end of one fetch and the next. */
  every: number;
  createUrl?: (b: Blob) => string;
  revokeUrl?: (url: string) => void;
  schedule?: (fn: () => void, ms: number) => unknown;
  cancel?: (handle: unknown) => void;
}

export class PreviewLoop {
  private running = false;
  private busy = false;
  private timer: unknown = null;
  private url: string | null = null;
  private readonly createUrl: (b: Blob) => string;
  private readonly revokeUrl: (url: string) => void;
  private readonly schedule: (fn: () => void, ms: number) => unknown;
  private readonly cancel: (handle: unknown) => void;
  private readonly deps: PreviewDeps;

  constructor(deps: PreviewDeps) {
    this.deps = deps;
    this.createUrl = deps.createUrl ?? ((b) => URL.createObjectURL(b));
    this.revokeUrl = deps.revokeUrl ?? ((u) => URL.revokeObjectURL(u));
    this.schedule = deps.schedule ?? ((fn, ms) => setTimeout(fn, ms));
    this.cancel = deps.cancel ?? ((h) => clearTimeout(h as ReturnType<typeof setTimeout>));
  }

  get active(): boolean {
    return this.running;
  }

  /** Start fetching (again); a no-op while running. */
  start() {
    if (this.running) return;
    this.running = true;
    void this.tick();
  }

  /** Stop fetching; the picture shown stays. */
  stop() {
    this.running = false;
    if (this.timer !== null) {
      this.cancel(this.timer);
      this.timer = null;
    }
  }

  /** Stop and release the picture. */
  dispose() {
    this.stop();
    if (this.url) this.revokeUrl(this.url);
    this.url = null;
  }

  private async tick() {
    this.timer = null;
    if (!this.running || this.busy) return;
    this.busy = true;
    try {
      const blob = await this.deps.load();
      if (this.running) {
        const old = this.url;
        this.url = blob ? this.createUrl(blob) : null;
        this.deps.show(this.url);
        if (old) this.revokeUrl(old);
      }
    } catch {
      // Not reachable right now: try again at the next tick.
    } finally {
      this.busy = false;
      if (this.running) this.timer = this.schedule(() => void this.tick(), this.deps.every);
    }
  }
}
