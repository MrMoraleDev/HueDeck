// SPDX-License-Identifier: GPL-3.0-or-later
// Pickers of the Macros page for option types that name something Lightloom has:
// `layoutname` (the saved layouts) and `profilename` (the saved profiles), both read from
// the snapshot (GET /v1/state).

export interface NamedChoice {
  name: string;
  /** The saved option names something that is gone (it stays selectable until changed). */
  missing: boolean;
}

interface Named {
  layouts?: string[];
  profiles?: { name: string }[];
}

/** The option types with a name picker. */
export const NAME_PICKERS = ["layoutname", "profilename"] as const;

export function hasNamePicker(type: string): boolean {
  return (NAME_PICKERS as readonly string[]).includes(type);
}

/**
 * The names a `layoutname` / `profilename` option can take, in the snapshot's order; the
 * option's current value first when the snapshot no longer has it. `null` for other types.
 */
export function nameChoices(type: string, value: unknown, snapshot: Named | null | undefined): NamedChoice[] | null {
  let names: string[];
  if (type === "layoutname") names = snapshot?.layouts ?? [];
  else if (type === "profilename") names = (snapshot?.profiles ?? []).map((p) => p.name);
  else return null;
  const out = names.map((name) => ({ name, missing: false }));
  const current = typeof value === "string" ? value : "";
  if (current && !names.includes(current)) out.unshift({ name: current, missing: true });
  return out;
}
