// SPDX-License-Identifier: GPL-3.0-or-later
// Push events of `/v1/events` by kind, for the pages that follow one (the Macros page
// follows `macros`). The store owns one bus and emits every event it receives, plus
// `open` when the socket (re)connects, so a page can refetch what it may have missed.

export type Listener = (data: unknown) => void;

export class EventBus {
  private listeners = new Map<string, Set<Listener>>();

  /** Call `fn` with the data of every `kind` event; returns the unsubscribe function. */
  on(kind: string, fn: Listener): () => void {
    let set = this.listeners.get(kind);
    if (!set) {
      set = new Set();
      this.listeners.set(kind, set);
    }
    set.add(fn);
    return () => {
      set.delete(fn);
      if (set.size === 0) this.listeners.delete(kind);
    };
  }

  /** Hand `data` to the listeners of `kind`; one that throws does not stop the others. */
  emit(kind: string, data: unknown) {
    const set = this.listeners.get(kind);
    if (!set) return;
    for (const fn of [...set]) {
      try {
        fn(data);
      } catch (e) {
        console.error(`event listener for ${kind}`, e);
      }
    }
  }

  /** Whether anything listens for `kind`. */
  has(kind: string): boolean {
    return this.listeners.has(kind);
  }
}
