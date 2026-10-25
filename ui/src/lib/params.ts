// SPDX-License-Identifier: GPL-3.0-or-later
// Plugin parameters in the dashboard: ordering, grouping, key names.

import type { Parameter } from "./types";

export interface ParameterGroup {
  /** The plugin's `group` ("" for none). */
  name: string;
  items: Parameter[];
}

/**
 * Parameters by `order` (lower first; those without one keep their place after
 * the ordered ones), then in groups in the order a group first appears.
 */
export function groupParameters(list: Parameter[]): ParameterGroup[] {
  const sorted = list
    .map((p, i) => ({ p, i }))
    .sort((a, b) => {
      const oa = typeof a.p.order === "number" ? a.p.order : Infinity;
      const ob = typeof b.p.order === "number" ? b.p.order : Infinity;
      return oa === ob ? a.i - b.i : oa - ob;
    })
    .map(({ p }) => p);
  const groups: ParameterGroup[] = [];
  for (const p of sorted) {
    const name = p.group ?? "";
    let g = groups.find((x) => x.name === name);
    if (!g) {
      g = { name, items: [] };
      groups.push(g);
    }
    g.items.push(p);
  }
  return groups;
}

/** A group key as a heading ("lighting" -> "Lighting"). */
export function groupTitle(name: string): string {
  return name ? name.charAt(0).toUpperCase() + name.slice(1) : "";
}

const NAMED: Record<string, string> = {
  Space: "Space",
  Backspace: "Back Space",
  Enter: "Enter",
  NumpadEnter: "Num Enter",
  Escape: "Escape",
  Tab: "Tab",
  CapsLock: "Caps Lock",
  Delete: "Delete",
  Insert: "Insert",
  Home: "Home",
  End: "End",
  PageUp: "Page Up",
  PageDown: "Page Down",
  ArrowLeft: "Left Arrow",
  ArrowRight: "Right Arrow",
  ArrowUp: "Up Arrow",
  ArrowDown: "Down Arrow",
  ShiftLeft: "Left Shift",
  ShiftRight: "Right Shift",
  ControlLeft: "Left Ctrl",
  ControlRight: "Right Ctrl",
  AltLeft: "Left Alt",
  AltRight: "Right Alt",
  MetaLeft: "Left Win",
  MetaRight: "Right Win",
  ContextMenu: "Menu",
  PrintScreen: "Print Screen",
  ScrollLock: "Scroll Lock",
  Pause: "Pause",
  NumLock: "Num Lock",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  Comma: ",",
  Period: ".",
  Slash: "/",
};

const MODIFIERS = new Set(["ShiftLeft", "ShiftRight", "ControlLeft", "ControlRight", "AltLeft", "AltRight", "MetaLeft", "MetaRight"]);

/**
 * A `keycode` value from a key press, in the macro vocabulary
 * (docs/host-api/macros-system.md): "F1", "A", "Left Win", "Ctrl + Shift + K".
 * A modifier pressed alone is its side-specific name. `null` for keys without a name.
 */
export function keyCombo(e: KeyboardEvent): string | null {
  const code = e.code;
  let key: string | undefined = NAMED[code];
  if (!key) {
    if (/^Key[A-Z]$/.test(code)) key = code.slice(3);
    else if (/^Digit[0-9]$/.test(code)) key = code.slice(5);
    else if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) key = code;
    else if (/^Numpad[0-9]$/.test(code)) key = `Num ${code.slice(6)}`;
    else if (code.startsWith("Numpad")) key = `Num ${code.slice(6)}`;
    else return null;
  }
  if (MODIFIERS.has(code)) return key;
  const mods = [e.ctrlKey && "Ctrl", e.shiftKey && "Shift", e.altKey && "Alt", e.metaKey && "Win"].filter(Boolean);
  return [...mods, key].join(" + ");
}

/** Whether `value` passes a textfield `filter` (a regex the whole value must match; a bad regex passes). */
export function passesFilter(filter: string | null | undefined, value: string): boolean {
  if (!filter) return true;
  try {
    return new RegExp(`^(?:${filter})$`, "u").test(value);
  } catch {
    try {
      return new RegExp(`^(?:${filter})$`).test(value);
    } catch {
      return true;
    }
  }
}
