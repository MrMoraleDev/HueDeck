<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!-- Layout editor: the live canvas with every endpoint's footprint on top. Drag to
     move; the inspector sets scale, rotation (clockwise about the top-left
     corner, as the sampler applies it), mirroring, brightness and forced colour. -->
<script lang="ts">
  import { api } from "../lib/api";
  import { daemon } from "../lib/store.svelte";
  import type { DeviceView, Endpoint, Placement, Snapshot } from "../lib/types";
  import Preview from "./Preview.svelte";

  let { snapshot }: { snapshot: Snapshot } = $props();

  interface Item {
    ep: Endpoint;
    device: DeviceView;
  }
  const items = $derived<Item[]>(snapshot.devices.flatMap((d) => d.endpoints.map((ep) => ({ ep, device: d }))));

  let selected = $state<string | null>(null);
  let edits = $state<Record<string, Placement>>({});
  let snap = $state(true);
  let newName = $state("");
  let layoutBrightness = $state(100);
  // Follows the daemon's value, except while the slider is held.
  let sliding = false;
  $effect(() => {
    const b = snapshot.lighting.layoutBrightness;
    if (!sliding) layoutBrightness = b;
  });

  const GRID = 4;

  function placementOf(it: Item): Placement {
    return edits[it.ep.uid] ?? it.ep.placement;
  }

  function scaleOf(it: Item, p: Placement) {
    return p.scale ?? { x: it.ep.kind === "device" ? it.device.defaultScale || 1 : 1, y: it.ep.kind === "device" ? it.device.defaultScale || 1 : 1 };
  }

  const current = $derived(items.find((i) => i.ep.uid === selected) ?? null);

  async function commit(uid: string, p: Placement) {
    edits[uid] = p;
    await daemon.run(() => api.setPlacement(uid, p));
    delete edits[uid];
  }

  // Dragging, in canvas units.
  let drag: { uid: string; start: { x: number; y: number }; origin: { x: number; y: number }; svg: SVGSVGElement } | null =
    null;

  function toCanvas(svg: SVGSVGElement, e: PointerEvent) {
    const r = svg.getBoundingClientRect();
    return { x: ((e.clientX - r.left) / r.width) * 320, y: ((e.clientY - r.top) / r.height) * 200 };
  }

  function down(e: PointerEvent, it: Item) {
    const svg = (e.currentTarget as SVGGElement).ownerSVGElement!;
    selected = it.ep.uid;
    const p = placementOf(it);
    drag = { uid: it.ep.uid, start: toCanvas(svg, e), origin: { ...p.position }, svg };
    (e.currentTarget as SVGGElement).setPointerCapture(e.pointerId);
  }

  function move(e: PointerEvent, it: Item) {
    if (!drag || drag.uid !== it.ep.uid) return;
    const at = toCanvas(drag.svg, e);
    let x = drag.origin.x + at.x - drag.start.x;
    let y = drag.origin.y + at.y - drag.start.y;
    if (snap) {
      x = Math.round(x / GRID) * GRID;
      y = Math.round(y / GRID) * GRID;
    }
    edits[it.ep.uid] = { ...placementOf(it), position: { x, y } };
  }

  function up(it: Item) {
    if (!drag || drag.uid !== it.ep.uid) return;
    drag = null;
    const p = edits[it.ep.uid];
    if (p) void commit(it.ep.uid, p);
  }

  function update(change: Partial<Placement>) {
    if (!current) return;
    void commit(current.ep.uid, { ...placementOf(current), ...change });
  }

  function num(v: string, fallback: number) {
    const n = Number(v);
    return Number.isFinite(n) ? n : fallback;
  }
</script>

<div class="bar panel row">
  <span class="muted">Layout</span>
  <select value={snapshot.lighting.layout} onchange={(e) => daemon.run(() => api.applyLayout((e.target as HTMLSelectElement).value))}>
    {#each snapshot.layouts as l (l)}
      <option value={l}>{l}</option>
    {/each}
  </select>
  <input type="text" placeholder="Save as…" bind:value={newName} />
  <button
    disabled={!newName.trim()}
    onclick={async () => {
      await daemon.run(() => api.saveLayoutAs(newName.trim()));
      newName = "";
    }}>Save as</button
  >
  <button class="danger" disabled={snapshot.layouts.length < 2} onclick={() => {
    const other = snapshot.layouts.find((l) => l !== snapshot.lighting.layout);
    const doomed = snapshot.lighting.layout;
    if (other) void daemon.run(async () => { await api.applyLayout(other); return api.deleteLayout(doomed); });
  }}>Delete this layout</button>
  <label class="row">
    <span class="muted">Layout brightness</span>
    <input
      type="range"
      min="0"
      max="100"
      bind:value={layoutBrightness}
      oninput={() => (sliding = true)}
      onblur={() => (sliding = false)}
      onchange={() => {
        sliding = false;
        void daemon.run(() => api.setLayoutBrightness(layoutBrightness));
      }}
    />
    <span class="num">{layoutBrightness}</span>
  </label>
  <label class="row"><input type="checkbox" bind:checked={snap} /> <span class="muted">Snap to {GRID} px</span></label>
</div>

<div class="editor">
  <div class="stage">
    <Preview fill tappable={false} />
    <svg viewBox="0 0 320 200" preserveAspectRatio="none" class="overlay" role="presentation">
      {#each items as it (it.ep.uid)}
        {@const p = placementOf(it)}
        {@const sc = scaleOf(it, p)}
        <g
          transform={`translate(${p.position.x} ${p.position.y}) rotate(${p.rotation})`}
          class="ep"
          class:sel={selected === it.ep.uid}
          class:off={!p.enabled}
          class:child={it.ep.kind !== "device"}
          role="button"
          tabindex="0"
          aria-label={it.ep.name}
          onpointerdown={(e) => down(e, it)}
          onpointermove={(e) => move(e, it)}
          onpointerup={() => up(it)}
          onkeydown={(e) => e.key === "Enter" && (selected = it.ep.uid)}
        >
          <rect width={Math.max(1, it.ep.size[0] * sc.x)} height={Math.max(1, it.ep.size[1] * sc.y)} />
          <text x="1" y="-1.5">{it.ep.name}</text>
        </g>
      {/each}
    </svg>
  </div>

  <aside class="panel inspector">
    {#if current}
      {@const p = placementOf(current)}
      {@const sc = scaleOf(current, p)}
      <h2>{current.ep.name}</h2>
      <p class="muted">
        {current.ep.kind}{current.ep.channel ? ` on ${current.ep.channel}` : ""} · {current.ep.size[0]}×{current.ep.size[1]} cells ·
        {current.ep.leds} LEDs
      </p>
      <div class="fields">
        <label>X <input type="number" value={p.position.x} onchange={(e) => update({ position: { x: num((e.target as HTMLInputElement).value, p.position.x), y: p.position.y } })} /></label>
        <label>Y <input type="number" value={p.position.y} onchange={(e) => update({ position: { x: p.position.x, y: num((e.target as HTMLInputElement).value, p.position.y) } })} /></label>
        <label>Scale <input type="number" step="0.5" min="0.1" value={sc.x} onchange={(e) => { const v = Math.max(0.1, num((e.target as HTMLInputElement).value, sc.x)); update({ scale: { x: v, y: v } }); }} /></label>
        <label>Rotation
          <input type="range" min="0" max="359" value={p.rotation} onchange={(e) => update({ rotation: num((e.target as HTMLInputElement).value, 0) })} />
          <span>{Math.round(p.rotation)}°</span>
        </label>
        <label><input type="checkbox" checked={p.flipX} onchange={(e) => update({ flipX: (e.target as HTMLInputElement).checked })} /> Mirror horizontally</label>
        <label><input type="checkbox" checked={p.flipY} onchange={(e) => update({ flipY: (e.target as HTMLInputElement).checked })} /> Mirror vertically</label>
        <label><input type="checkbox" checked={p.enabled} onchange={(e) => update({ enabled: (e.target as HTMLInputElement).checked })} /> Lit</label>
        <label>Brightness
          <input type="range" min="0" max="100" value={p.brightness} onchange={(e) => update({ brightness: num((e.target as HTMLInputElement).value, 100) })} />
          <span>{p.brightness}</span>
        </label>
        <label>Colour
          <select value={p.lightingMode} onchange={(e) => update({ lightingMode: (e.target as HTMLSelectElement).value as "Canvas" | "Forced" })}>
            <option value="Canvas">From the effect</option>
            <option value="Forced">Fixed colour</option>
          </select>
          {#if p.lightingMode === "Forced"}
            <input type="color" value={p.forcedColor} onchange={(e) => update({ forcedColor: (e.target as HTMLInputElement).value })} />
          {/if}
        </label>
      </div>
    {:else}
      <h2>Placement</h2>
      <p class="muted">Select a device, subdevice or component on the canvas. Drag to move it.</p>
      {#if items.length === 0}
        <p class="muted">No device is running yet.</p>
      {/if}
    {/if}
  </aside>
</div>

<style>
  .bar {
    margin-bottom: 12px;
  }
  .num {
    width: 2.5em;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .editor {
    display: flex;
    gap: 16px;
    align-items: flex-start;
    flex-wrap: wrap;
  }
  .stage {
    position: relative;
    flex: 1 1 600px;
    max-width: 960px;
    aspect-ratio: 320 / 200;
  }
  .overlay {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: visible;
  }
  .ep {
    cursor: grab;
  }
  .ep rect {
    fill: rgba(79, 163, 255, 0.12);
    stroke: rgba(255, 255, 255, 0.85);
    stroke-width: 0.6;
    vector-effect: non-scaling-stroke;
  }
  .ep.child rect {
    stroke: rgba(240, 178, 74, 0.9);
  }
  .ep.sel rect {
    stroke: var(--accent);
    stroke-width: 2;
    fill: rgba(79, 163, 255, 0.25);
  }
  .ep.off rect {
    stroke-dasharray: 3 2;
    fill: rgba(0, 0, 0, 0.35);
  }
  .ep text {
    font-size: 5px;
    fill: #fff;
    paint-order: stroke;
    stroke: #000;
    stroke-width: 1px;
    pointer-events: none;
  }
  .inspector {
    width: 320px;
  }
  .fields {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .fields label {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .fields input[type="number"] {
    width: 90px;
  }
</style>
