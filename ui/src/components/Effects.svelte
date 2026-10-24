<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { api } from "../lib/api";
  import { daemon } from "../lib/store.svelte";
  import type { EffectView, Snapshot } from "../lib/types";
  import ControlInput from "./ControlInput.svelte";
  import Preview from "./Preview.svelte";

  let { snapshot }: { snapshot: Snapshot } = $props();
  let filter = $state("");
  let presetName = $state("");

  const current = $derived(snapshot.effects.find((e) => e.id === snapshot.lighting.effect) ?? null);
  const shown = $derived(
    snapshot.effects.filter(
      (e) =>
        !filter ||
        e.title.toLowerCase().includes(filter.toLowerCase()) ||
        e.description.toLowerCase().includes(filter.toLowerCase()),
    ),
  );

  function valueOf(e: EffectView, property: string, fallback: unknown) {
    return property in e.values ? e.values[property] : fallback;
  }
</script>

<div class="grid">
  <section class="panel list">
    <div class="row head">
      <h2>Effects</h2>
      <input type="text" placeholder="Filter" bind:value={filter} />
      <button onclick={() => daemon.run(() => api.state())} title="Reload">↻</button>
    </div>
    {#each shown as e (e.id)}
      <button class="effect" class:active={e.id === snapshot.lighting.effect} onclick={() => daemon.run(() => api.applyEffect(e.id))}>
        <span class="title">{e.title}</span>
        <span class="badge" title={e.thirdParty ? "Third-party effect, read from your SignalRGB installation" : undefined}
          >{e.source === "signalrgb" ? "SignalRGB · third-party" : e.source === "user" ? "yours" : "Lightloom"}</span
        >
        {#if e.description}<span class="desc muted">{e.description}</span>{/if}
      </button>
    {:else}
      <p class="muted">No effect matches.</p>
    {/each}
  </section>

  <section class="side">
    <div class="panel">
      <Preview width={480} />
    </div>
    {#if current}
      <div class="panel">
        <h2>{current.title}</h2>
        {#if current.publisher}<p class="muted">by {current.publisher}</p>{/if}
        {#each current.controls as c (c.property)}
          <ControlInput
            kind={c.type}
            label={c.label || c.property}
            value={valueOf(current, c.property, c.default)}
            min={c.min}
            max={c.max}
            step={c.step}
            values={c.values}
            tooltip={c.tooltip}
            onchange={(v) => daemon.run(() => api.setEffectValues(current.id, { [c.property]: v }))}
          />
        {:else}
          <p class="muted">This effect has no controls.</p>
        {/each}
        <h3>Presets</h3>
        <div class="row">
          {#each current.presets as p}
            <span class="preset">
              <button onclick={() => daemon.run(() => api.applyPreset(current.id, p))}>{p}</button>
              <button class="x" title="Delete" onclick={() => daemon.run(() => api.deletePreset(current.id, p))}>×</button>
            </span>
          {/each}
        </div>
        <div class="row">
          <input type="text" placeholder="Preset name" bind:value={presetName} />
          <button
            disabled={!presetName.trim()}
            onclick={async () => {
              await daemon.run(() => api.savePreset(current.id, presetName.trim()));
              presetName = "";
            }}>Save preset</button
          >
        </div>
      </div>
    {/if}
  </section>
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: minmax(280px, 1fr) minmax(320px, 520px);
    gap: 16px;
    align-items: start;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .head h2 {
    flex: 1;
    margin: 0;
  }
  .effect {
    display: grid;
    grid-template-columns: 1fr auto;
    text-align: left;
    gap: 2px 8px;
  }
  .effect.active {
    border-color: var(--accent);
    background: #1d2b3d;
  }
  .title {
    font-weight: 600;
  }
  .desc {
    grid-column: 1 / -1;
    font-size: 12px;
  }
  .side {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .preset {
    display: inline-flex;
  }
  .preset .x {
    padding: 6px 8px;
  }
</style>
