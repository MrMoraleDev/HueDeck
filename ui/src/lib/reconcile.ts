// SPDX-License-Identifier: GPL-3.0-or-later
// Fine-grained updates of live state: `reconcile(target, source)` writes into `target`
// only the leaves that differ from `source`, and keeps the identity of every object and
// array element that is still there. Run on the dashboard's `$state` snapshot, only the
// signals of values that really changed fire: a refetch that moves a frame counter
// re-renders that counter, not every list, select and slider bound to the snapshot.

type Obj = Record<string, unknown>;

/** Fields that identify an element of an array of objects, in order of preference. */
const ID_FIELDS = ["key", "uid", "id", "property", "plugin", "ComponentId", "name"] as const;

function isObject(v: unknown): v is Obj {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/** The identity field every element of both arrays has, unique in each; null if none. */
function idField(a: unknown[], b: unknown[]): string | null {
  if (a.length === 0 && b.length === 0) return null;
  for (const f of ID_FIELDS) {
    const ok = (list: unknown[]) => {
      const seen = new Set<unknown>();
      for (const x of list) {
        if (!isObject(x)) return false;
        const id = x[f];
        if ((typeof id !== "string" && typeof id !== "number") || seen.has(id)) return false;
        seen.add(id);
      }
      return true;
    };
    if (ok(a) && ok(b)) return f;
  }
  return null;
}

/** Make `target[k]` equal to `value`, reusing what `target[k]` already holds. */
function assign(target: Obj | unknown[], k: string | number, value: unknown) {
  const t = target as Obj & unknown[];
  const old = t[k as never] as unknown;
  if (isObject(old) && isObject(value)) {
    reconcile(old, value);
  } else if (Array.isArray(old) && Array.isArray(value)) {
    reconcileArray(target, k, old, value);
  } else if (!Object.is(old, value)) {
    (t as Obj)[k as string] = value;
  }
}

function reconcileArray(holder: Obj | unknown[], k: string | number, old: unknown[], next: unknown[]) {
  const field = idField(old, next);
  if (field) {
    const byId = new Map<unknown, Obj>();
    for (const o of old as Obj[]) byId.set(o[field], o);
    let same = old.length === next.length;
    const merged = (next as Obj[]).map((n, i) => {
      const o = byId.get(n[field]);
      if (o) {
        reconcile(o, n);
        if (old[i] !== o) same = false;
        return o;
      }
      same = false;
      return n;
    });
    // Same members in the same order: the array itself stays as it is.
    if (!same) (holder as Obj)[k as string] = merged;
    return;
  }
  // No identity: by index, then grow or shrink.
  const common = Math.min(old.length, next.length);
  for (let i = 0; i < common; i++) assign(old, i, next[i]);
  if (next.length > old.length) old.push(...next.slice(old.length));
  else if (next.length < old.length) old.length = next.length;
}

/**
 * Update `target` in place to deep-equal `source` (plain JSON data). Objects whose
 * identity field (`key`, `uid`, `id`, ...) matches keep their identity across
 * re-orderings; keys missing from `source` are removed. Returns `target`.
 */
export function reconcile<T extends object>(target: T, source: T): T {
  const t = target as Obj;
  const s = source as Obj;
  for (const k of Object.keys(s)) assign(t, k, s[k]);
  for (const k of Object.keys(t)) if (!(k in s)) delete t[k];
  return target;
}
