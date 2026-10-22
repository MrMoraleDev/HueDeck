<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!-- One effect control or plugin parameter: number, hue, boolean, color, combobox, text. -->
<script lang="ts">
  interface Props {
    kind: string;
    label: string;
    value: unknown;
    min?: number | null;
    max?: number | null;
    step?: number | null;
    values?: string[];
    tooltip?: string | null;
    onchange: (value: unknown) => void;
  }
  let { kind, label, value, min, max, step, values = [], tooltip, onchange }: Props = $props();

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
        oninput={(e) => (local = asNumber((e.target as HTMLInputElement).value))}
        onchange={() => onchange(local)}
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
  {:else}
    <input type="text" value={String(local ?? "")} onchange={(e) => onchange((e.target as HTMLInputElement).value)} />
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
  input[type="range"] {
    flex: 1;
    min-width: 120px;
  }
</style>
