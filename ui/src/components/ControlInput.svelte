<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!-- One effect control or plugin parameter: number, hue, boolean, color, combobox, text, keycode. -->
<script lang="ts">
  import { keyCombo, passesFilter } from "../lib/params";

  interface Props {
    kind: string;
    label: string;
    value: unknown;
    min?: number | null;
    max?: number | null;
    step?: number | null;
    values?: string[];
    tooltip?: string | null;
    /** Sliders: send while dragging (throttled), not only on release. */
    live?: boolean;
    /** Textfields: a regex the whole value must match before it is sent. */
    filter?: string | null;
    onchange: (value: unknown) => void;
  }
  let { kind, label, value, min, max, step, values = [], tooltip, live = false, filter, onchange }: Props = $props();

  const asNumber = (v: unknown, fallback = 0) => {
    const n = Number(v);
    return Number.isFinite(n) ? n : fallback;
  };
  const asBool = (v: unknown) => v === true || v === 1 || v === "true" || v === "1";
  const asColor = (v: unknown) => (typeof v === "string" && /^#[0-9a-f]{6}$/i.test(v) ? v : "#000000");

  let local = $state<unknown>(null);
  $effect(() => {
    local = value;
  });
  const lo = $derived(kind === "hue" ? 0 : (min ?? 0));
  const hi = $derived(kind === "hue" ? 360 : (max ?? 100));

  // Live sliders: at most one value per LIVE_EVERY ms while dragging; the release sends the last.
  const LIVE_EVERY = 100;
  let lastSent = 0;
  let sentValue: unknown = undefined;
  function slide(v: number) {
    local = v;
    const now = Date.now();
    if (live && now - lastSent >= LIVE_EVERY) {
      lastSent = now;
      sentValue = v;
      onchange(v);
    }
  }
  function release() {
    if (!live || sentValue !== local) {
      sentValue = local;
      onchange(local);
    }
  }

  let text = $state("");
  $effect(() => {
    text = String(value ?? "");
  });
  const textOk = $derived(passesFilter(filter, text));

  let recording = $state(false);
  function record(e: KeyboardEvent) {
    // Plain Tab still moves the focus.
    if (e.code === "Tab" && !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey) return;
    e.preventDefault();
    const combo = keyCombo(e);
    if (combo === null) return;
    local = combo;
    recording = false;
    onchange(combo);
  }
</script>

<label class="control" title={tooltip ?? ""}>
  <span class="label">{label}</span>
  {#if kind === "number" || kind === "hue"}
    <span class="row">
      <input
        type="range"
        min={lo}
        max={hi}
        step={step ?? 1}
        value={asNumber(local, lo)}
        oninput={(e) => slide(asNumber((e.target as HTMLInputElement).value))}
        onchange={release}
      />
      {#if kind === "hue"}
        <span class="swatch" style={`background:hsl(${asNumber(local)},100%,50%)`}></span>
      {/if}
      <span class="num">{asNumber(local, lo)}</span>
    </span>
  {:else if kind === "boolean"}
    <input type="checkbox" checked={asBool(local)} onchange={(e) => onchange((e.target as HTMLInputElement).checked)} />
  {:else if kind === "color"}
    <input type="color" value={asColor(local)} onchange={(e) => onchange((e.target as HTMLInputElement).value)} />
  {:else if kind === "combobox" || kind === "list"}
    <select value={String(local ?? "")} onchange={(e) => onchange((e.target as HTMLSelectElement).value)}>
      {#each values as v}
        <option value={v}>{v}</option>
      {/each}
    </select>
  {:else if kind === "keycode"}
    <span class="row">
      <button
        type="button"
        class="keycode"
        class:recording
        onclick={() => (recording = !recording)}
        onkeydown={(e) => recording && record(e)}
        onblur={() => (recording = false)}>{recording ? "Press a key…" : String(local ?? "") || "None"}</button
      >
    </span>
  {:else}
    <input
      type="text"
      class:invalid={!textOk}
      title={textOk ? (tooltip ?? "") : `Must match ${filter}`}
      bind:value={text}
      onchange={() => textOk && onchange(text)}
    />
  {/if}
</label>

<style>
  .control {
    display: grid;
    grid-template-columns: 150px 1fr;
    gap: 10px;
    align-items: center;
    padding: 4px 0;
  }
  .label {
    color: var(--muted);
  }
  .num {
    width: 3em;
    text-align: right;
  }
  .swatch {
    width: 16px;
    height: 16px;
    border-radius: 4px;
    display: inline-block;
  }
  .keycode {
    min-width: 140px;
    text-align: left;
  }
  .keycode.recording {
    outline: 2px solid var(--accent);
  }
  .invalid {
    outline: 2px solid var(--bad);
  }
  input[type="range"] {
    flex: 1;
    min-width: 120px;
  }
</style>
