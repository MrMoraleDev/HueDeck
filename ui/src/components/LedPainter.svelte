<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!--
  The LED painter ("Customise LEDs"): a device's LEDs drawn at their cells, painted one by
  one (click or drag), by box selection, all at once, or by zone. Painting switches the
  endpoint to "Custom LEDs"; the toggle sends it back to the effect (the colours are kept).

  Changes are local first: one colour of the array changes, one rect re-renders. The edit
  queue sends them to the daemon at most every 40 ms (never two requests at a time), the
  device shows them from its next frame, and the daemon saves them after the burst.
-->
<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { api } from "../lib/api";
  import { daemon } from "../lib/store.svelte";
  import type { DeviceView, LedEndpointView, LedMapView } from "../lib/types";
  import {
    BLACK,
    EditQueue,
    UndoStack,
    capLabel,
    cellsBetween,
    looksLikeKeyboard,
    remember,
    slotsByCell,
    slotsInBox,
    type UndoStep,
  } from "../lib/painter";

  let { device }: { device: DeviceView } = $props();

  /** Endpoints with more LEDs than this are not drawn LED by LED (fill and zones still work). */
  const MAX_DRAWN = 6000;

  let map = $state.raw<LedMapView | null>(null);
  let loadError = $state<string | null>(null);
  let uid = $state("");
  /** The current endpoint's colours: per-slot signals, so painting one LED re-renders one rect. */
  let colors = $state<string[]>([]);
  let selected = $state<boolean[]>([]);
  let custom = $state(false);
  let zones = $state.raw<Record<string, number[]>>({});
  let color = $state("#ff0000");
  let tool = $state<"paint" | "select">("paint");
  let zoneName = $state("");
  let width = $state(800);
  let undoCount = $state(0);
  let box = $state<{ a: [number, number]; b: [number, number] } | null>(null);

  const undo = new UndoStack();
  let queue: EditQueue | null = null;
  let stroke: UndoStep | null = null;
  let last: [number, number] | null = null;
  let svg = $state<SVGSVGElement | null>(null);

  const endpoint = $derived<LedEndpointView | null>(map?.endpoints.find((e) => e.uid === uid) ?? null);
  const byCell = $derived(endpoint ? slotsByCell(endpoint.leds) : new Map<string, number[]>());
  const caps = $derived(!!endpoint && endpoint.kind === "device" && looksLikeKeyboard(map?.deviceType ?? "", endpoint.leds));
  const drawn = $derived(!!endpoint && endpoint.leds.length > 0 && endpoint.leds.length <= MAX_DRAWN);
  const cell = $derived(endpoint ? Math.max(6, Math.min(caps ? 46 : 32, Math.floor((width - 4) / Math.max(1, endpoint.size[0])))) : 0);
  const selectedCount = $derived(selected.reduce((n, s) => n + (s ? 1 : 0), 0));

  function fail(e: unknown) {
    daemon.error = e instanceof Error ? e.message : String(e);
  }

  async function load() {
    try {
      const m = await api.ledMap(device.key);
      map = m;
      loadError = null;
      const keep = m.endpoints.find((e) => e.uid === uid);
      open(keep ?? m.endpoints.find((e) => e.leds.length > 0) ?? m.endpoints[0] ?? null);
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e);
    }
  }

  function open(ep: LedEndpointView | null) {
    void queue?.flush();
    queue?.dispose();
    uid = ep?.uid ?? "";
    colors = ep ? [...ep.colors] : [];
    selected = ep ? new Array(ep.slots).fill(false) : [];
    custom = ep?.custom ?? false;
    zones = ep?.zones ?? {};
    undo.clear();
    undoCount = 0;
    queue = ep ? new EditQueue((edit) => api.setLeds(device.key, ep.uid, edit), 40, fail) : null;
  }

  onMount(() => void load());
  onDestroy(() => {
    const q = queue;
    void q?.flush().finally(() => q.dispose());
  });

  function ensureCustom() {
    if (!custom) {
      custom = true;
      queue?.setMode("custom");
    }
  }

  function setMode(on: boolean) {
    if (on === custom) return;
    custom = on;
    queue?.setMode(on ? "custom" : "effect");
  }

  /** Paint `slots` (recording their old colours in `step`); returns nothing. */
  function paint(slots: Iterable<number>, c: string, step: UndoStep) {
    const changes: [number, string][] = [];
    for (const s of slots) {
      if (colors[s] === c) continue;
      remember(step, colors, [s]);
      colors[s] = c;
      changes.push([s, c]);
    }
    if (changes.length) {
      queue?.set(changes);
      ensureCustom();
    }
  }

  function commit(step: UndoStep) {
    undo.push(step);
    undoCount = undo.length;
  }

  function cellOf(e: PointerEvent): [number, number] | null {
    if (!svg || !endpoint) return null;
    const r = svg.getBoundingClientRect();
    const [w, h] = endpoint.size;
    const x = Math.floor(((e.clientX - r.left) / r.width) * w);
    const y = Math.floor(((e.clientY - r.top) / r.height) * h);
    return x >= 0 && y >= 0 && x < w && y < h ? [x, y] : null;
  }

  function paintCells(cells: [number, number][]) {
    if (!stroke) return;
    const slots: number[] = [];
    for (const [x, y] of cells) slots.push(...(byCell.get(`${x},${y}`) ?? []));
    paint(slots, color, stroke);
  }

  function down(e: PointerEvent) {
    if (e.button !== 0) return;
    const c = cellOf(e);
    if (!c) return;
    svg?.setPointerCapture(e.pointerId);
    if (tool === "paint") {
      stroke = new Map();
      last = c;
      paintCells([c]);
    } else {
      box = { a: c, b: c };
    }
  }

  function move(e: PointerEvent) {
    const c = cellOf(e);
    if (!c) return;
    if (tool === "paint" && stroke && last) {
      if (c[0] === last[0] && c[1] === last[1]) return;
      paintCells(cellsBetween(last, c).slice(1));
      last = c;
    } else if (box && (box.b[0] !== c[0] || box.b[1] !== c[1])) {
      box = { a: box.a, b: c };
    }
  }

  function up(e: PointerEvent) {
    if (stroke) {
      commit(stroke);
      stroke = null;
      last = null;
    }
    if (box && endpoint) {
      const slots = slotsInBox(endpoint.leds, box.a, box.b);
      if (!e.shiftKey) selected.fill(false);
      for (const s of slots) selected[s] = !(e.ctrlKey && selected[s]);
      box = null;
    }
  }

  function targets(): number[] | null {
    if (selectedCount === 0) return null;
    const out: number[] = [];
    selected.forEach((s, i) => s && out.push(i));
    return out;
  }

  function fillWith(c: string) {
    const step: UndoStep = new Map();
    const slots = targets();
    if (slots) {
      paint(slots, c, step);
    } else {
      remember(step, colors, colors.keys());
      for (let i = 0; i < colors.length; i++) if (colors[i] !== c) colors[i] = c;
      queue?.fill(c);
      ensureCustom();
    }
    commit(step);
  }

  function undoLast() {
    const step = undo.pop();
    undoCount = undo.length;
    if (!step) return;
    for (const [s, c] of step) colors[s] = c;
    queue?.set(step);
  }

  function colorZone(name: string) {
    const step: UndoStep = new Map();
    paint(zones[name] ?? [], color, step);
    commit(step);
  }

  function selectZone(name: string) {
    selected.fill(false);
    for (const s of zones[name] ?? []) selected[s] = true;
  }

  async function saveZone() {
    const slots = targets();
    const name = zoneName.trim();
    if (!slots || !name || !endpoint) return;
    try {
      await queue?.flush();
      zones = (await api.saveLedZone(device.key, endpoint.uid, name, slots)).zones;
      zoneName = "";
    } catch (e) {
      fail(e);
    }
  }

  async function deleteZone(name: string) {
    if (!endpoint) return;
    try {
      zones = (await api.deleteLedZone(device.key, endpoint.uid, name)).zones;
    } catch (e) {
      fail(e);
    }
  }

  function keydown(e: KeyboardEvent) {
    const t = e.target as HTMLElement | null;
    if (t && (t.tagName === "INPUT" || t.tagName === "SELECT" || t.tagName === "TEXTAREA")) return;
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
      e.preventDefault();
      undoLast();
    }
  }

  /** Dark text on light key caps, light text on dark ones. */
  function ink(hex: string): string {
    const n = parseInt(hex.slice(1), 16);
    const l = 0.299 * ((n >> 16) & 255) + 0.587 * ((n >> 8) & 255) + 0.114 * (n & 255);
    return l > 140 ? "#111" : "#eee";
  }
</script>

<svelte:window onkeydown={keydown} />

<div class="painter" bind:clientWidth={width}>
  {#if loadError}
    <p class="muted">Cannot read the device's LEDs: {loadError}</p>
  {:else if !map}
    <p class="muted">Reading the device's LEDs…</p>
  {:else}
    <div class="row bar">
      {#if map.endpoints.length > 1}
        <select value={uid} onchange={(e) => open(map?.endpoints.find((x) => x.uid === (e.target as HTMLSelectElement).value) ?? null)}>
          {#each map.endpoints as ep (ep.uid)}
            <option value={ep.uid}>{ep.channel ? `${ep.channel} · ` : ""}{ep.name} ({ep.leds.length} LEDs)</option>
          {/each}
        </select>
      {/if}
      <div class="seg" role="radiogroup" aria-label="What this part shows">
        <button class:on={!custom} onclick={() => setMode(false)} role="radio" aria-checked={!custom}>Follow effect</button>
        <button class:on={custom} onclick={() => setMode(true)} role="radio" aria-checked={custom}>Custom LEDs</button>
      </div>
      <button class="small" onclick={() => void load()} title="Read the LEDs again (after the plugin changed them)">Reload</button>
    </div>

    {#if endpoint}
      <div class="row bar">
        <label class="row"><input type="color" bind:value={color} /> Colour</label>
        <div class="seg" role="radiogroup" aria-label="Tool">
          <button class:on={tool === "paint"} onclick={() => (tool = "paint")} role="radio" aria-checked={tool === "paint"}
            >Paint</button
          >
          <button
            class:on={tool === "select"}
            onclick={() => (tool = "select")}
            role="radio"
            aria-checked={tool === "select"}
            title="Drag a box; Shift adds to the selection, Ctrl removes from it">Select</button
          >
        </div>
        <button onclick={() => fillWith(color)}>{selectedCount ? `Fill selection (${selectedCount})` : "Fill all"}</button>
        <button onclick={() => fillWith(BLACK)}>{selectedCount ? "Clear selection" : "Clear"}</button>
        <button disabled={undoCount === 0} onclick={undoLast} title="Ctrl+Z">Undo</button>
        {#if selectedCount}<button class="small" onclick={() => selected.fill(false)}>Select none</button>{/if}
      </div>

      {#if drawn}
        <svg
          bind:this={svg}
          class="grid"
          class:caps
          viewBox={`0 0 ${endpoint.size[0]} ${endpoint.size[1]}`}
          width={cell * endpoint.size[0]}
          height={cell * endpoint.size[1]}
          role="application"
          aria-label="LEDs"
          onpointerdown={down}
          onpointermove={move}
          onpointerup={up}
          onpointercancel={up}
        >
          {#each endpoint.leds as led (led.slot)}
            <rect
              x={led.x + 0.06}
              y={led.y + 0.06}
              width="0.88"
              height="0.88"
              rx={caps ? 0.14 : 0.44}
              fill={colors[led.slot]}
              class:sel={selected[led.slot]}
              ><title>{led.name || `LED ${led.slot + 1}`}</title></rect
            >
            {#if caps && led.name && cell >= 18}
              <text x={led.x + 0.5} y={led.y + 0.56} fill={ink(colors[led.slot] ?? BLACK)}>{capLabel(led.name)}</text>
            {/if}
          {/each}
          {#if box}
            <rect
              class="box"
              x={Math.min(box.a[0], box.b[0])}
              y={Math.min(box.a[1], box.b[1])}
              width={Math.abs(box.a[0] - box.b[0]) + 1}
              height={Math.abs(box.a[1] - box.b[1]) + 1}
            />
          {/if}
        </svg>
      {:else if endpoint.leds.length === 0}
        <p class="muted">This part lists no LEDs to paint.</p>
      {:else}
        <p class="muted">{endpoint.leds.length} LEDs: too many to draw one by one. Fill, Clear and zones still work.</p>
      {/if}

      <div class="zones">
        <strong>Zones</strong>
        {#each Object.entries(zones) as [name, slots] (name)}
          <div class="row zone">
            <span>{name}</span>
            <span class="muted">{slots.length} LEDs</span>
            <button class="small" onclick={() => selectZone(name)}>Select</button>
            <button class="small" onclick={() => colorZone(name)} title="Paint the zone with the colour">Colour</button>
            <button class="small" onclick={() => void deleteZone(name)}>Delete</button>
          </div>
        {:else}
          <p class="muted">None yet: select LEDs, name them, save.</p>
        {/each}
        <div class="row">
          <input placeholder="Zone name" maxlength="64" bind:value={zoneName} />
          <button disabled={!selectedCount || !zoneName.trim()} onclick={() => void saveZone()}>Save selection as zone</button>
        </div>
      </div>
      <p class="muted hint">
        Painting switches this part to Custom LEDs; "Follow effect" gives it back to the effect and keeps your colours.
        Brightness still applies.
      </p>
    {/if}
  {/if}
</div>

<style>
  .painter {
    border-top: 1px solid var(--line);
    margin-top: 8px;
    padding-top: 8px;
  }
  .bar {
    margin-bottom: 8px;
    gap: 8px;
    flex-wrap: wrap;
  }
  .seg {
    display: inline-flex;
  }
  .seg button {
    border-radius: 0;
  }
  .seg button:first-child {
    border-radius: 6px 0 0 6px;
  }
  .seg button:last-child {
    border-radius: 0 6px 6px 0;
  }
  .seg button.on {
    background: var(--accent);
    border-color: var(--accent);
    color: #08111d;
  }
  .small {
    padding: 2px 8px;
    font-size: 12px;
  }
  .grid {
    display: block;
    max-width: 100%;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 6px;
    touch-action: none;
    user-select: none;
    cursor: crosshair;
  }
  .grid rect {
    stroke: rgba(255, 255, 255, 0.18);
    stroke-width: 0.04;
  }
  .grid rect.sel {
    stroke: var(--accent);
    stroke-width: 0.12;
  }
  .grid rect.box {
    fill: rgba(79, 163, 255, 0.15);
    stroke: var(--accent);
    stroke-width: 0.06;
    stroke-dasharray: 0.2 0.1;
    pointer-events: none;
  }
  .grid text {
    font-size: 0.3px;
    text-anchor: middle;
    dominant-baseline: middle;
    pointer-events: none;
  }
  .zones {
    margin-top: 10px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .zone span:first-child {
    min-width: 120px;
  }
  .hint {
    font-size: 12px;
  }
</style>
