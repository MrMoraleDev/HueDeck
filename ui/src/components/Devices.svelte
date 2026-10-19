<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { api } from "../lib/api";
  import { daemon } from "../lib/store.svelte";
  import type { DeviceView, LibraryComponent, Snapshot } from "../lib/types";
  import ControlInput from "./ControlInput.svelte";

  let { snapshot }: { snapshot: Snapshot } = $props();
  let library = $state<LibraryComponent[]>([]);
  let libraryError = $state<string | null>(null);
  let open = $state<string | null>(null);
  let pick = $state<Record<string, string>>({});
  let strip = $state<Record<string, number>>({});

  async function loadLibrary() {
    try {
      library = await api.components();
      libraryError = null;
    } catch (e) {
      libraryError = e instanceof Error ? e.message : String(e);
    }
  }

  function stateBadge(d: DeviceView) {
    if (d.state === "running") return "good";
    if (d.state === "absent" || d.state === "disabled") return "";
    return "warn";
  }

  function keep(d: DeviceView, channel: string): unknown[] {
    const ch = d.channels.find((c) => c.name === channel);
    return (ch?.components ?? []).map((c) => ({ ComponentId: c.ComponentId, ProductName: c.ProductName ?? c.DisplayName }));
  }

  async function setChain(d: DeviceView, channel: string, chain: unknown[]) {
    await daemon.run(() => api.setChannel(d.key, channel, chain));
  }
</script>

<div class="list">
  {#each snapshot.devices as d (d.key)}
    <section class="panel device">
      <div class="row head">
        {#if d.image}<img src={d.image} alt="" />{/if}
        <div class="title">
          <strong>{d.name}</strong>
          <span class="muted key">{d.bus ? `SMBus · ${d.bus}` : d.key}</span>
        </div>
        <span class={`badge ${stateBadge(d)}`}>{d.state}</span>
        {#if d.connected}
          <span class="muted stats"
            >{d.frames} frames · {d.renderErrors} render errors · {d.hidErrors} {d.bus ? "bus" : "HID"} errors</span
          >
        {/if}
        <label class="row"
          ><input
            type="checkbox"
            checked={d.enabled}
            onchange={(e) => daemon.run(() => api.setDevice(d.key, { enabled: (e.target as HTMLInputElement).checked }))}
          /> Drive this device</label
        >
        {#if d.connected}
          <button
            onclick={() => {
              open = open === d.key ? null : d.key;
              if (open && library.length === 0) void loadLibrary();
            }}>{open === d.key ? "Close" : "Settings"}</button
          >
        {/if}
      </div>
      <p class="muted plugin">Plugin: {d.plugin} <span class="path">({d.pluginPath})</span></p>

      {#if open === d.key}
        {#if d.parameters.length}
          <h3>Plugin settings</h3>
          {#each d.parameters as p (p.property)}
            <ControlInput
              kind={p.type}
              label={p.label || p.property}
              value={p.value}
              min={p.min}
              max={p.max}
              step={p.step}
              values={p.values}
              tooltip={p.tooltip ?? p.description}
              onchange={(v) => daemon.run(() => api.setDevice(d.key, { parameters: { [p.property]: v } }))}
            />
          {/each}
        {/if}

        {#if d.channels.length}
          <h3>Channels and components</h3>
          {#if libraryError}<p class="muted">Component library unavailable: {libraryError}</p>{/if}
          {#each d.channels as ch (ch.name)}
            {@const total = ch.components.reduce((n, c) => n + c.LedCount, 0)}
            <div class="channel">
              <div class="row">
                <strong>{ch.name}</strong>
                <span class="muted">{total} LEDs{ch.ledLimit ? ` of ${ch.ledLimit}` : ""}</span>
              </div>
              <ol>
                {#each ch.components as c, i (c.ComponentId)}
                  <li class="row">
                    <span>{c.DisplayName}</span>
                    <span class="muted">{c.LedCount} LEDs</span>
                    <button
                      class="x"
                      title="Remove"
                      onclick={() => {
                        const chain = keep(d, ch.name);
                        chain.splice(i, 1);
                        void setChain(d, ch.name, chain);
                      }}>×</button
                    >
                  </li>
                {:else}
                  <li class="muted">Nothing assigned.</li>
                {/each}
              </ol>
              <div class="row">
                <select bind:value={pick[`${d.key}/${ch.name}`]}>
                  <option value="">Add a component…</option>
                  {#each library as c (c.productName)}
                    <option value={c.productName}>{c.brand ? `${c.brand} — ` : ""}{c.displayName || c.productName} ({c.ledCount} LEDs)</option>
                  {/each}
                </select>
                <button
                  disabled={!pick[`${d.key}/${ch.name}`]}
                  onclick={() => setChain(d, ch.name, [...keep(d, ch.name), { ProductName: pick[`${d.key}/${ch.name}`] }])}
                  >Add</button
                >
                <span class="muted">or a strip of</span>
                <input type="number" min="1" max="1000" style="width:80px" bind:value={strip[`${d.key}/${ch.name}`]} />
                <button
                  disabled={!(strip[`${d.key}/${ch.name}`] > 0)}
                  onclick={() => setChain(d, ch.name, [...keep(d, ch.name), `Default Strip - ${strip[`${d.key}/${ch.name}`]}`])}
                  >Add strip</button
                >
              </div>
            </div>
          {/each}
        {/if}
      {/if}
    </section>
  {:else}
    <p class="muted">No supported device found. Settings → use your SignalRGB plugins, or add plugins to your Lightloom plugin folder.</p>
  {/each}
</div>

<style>
  .list {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .head img {
    width: 40px;
    height: 40px;
    object-fit: contain;
  }
  .title {
    display: flex;
    flex-direction: column;
    min-width: 220px;
  }
  .key {
    font-size: 12px;
  }
  .stats {
    flex: 1;
  }
  .plugin {
    margin: 6px 0 0;
    font-size: 12px;
  }
  .path {
    opacity: 0.6;
  }
  .channel {
    border-top: 1px solid var(--line);
    padding: 8px 0;
  }
  ol {
    margin: 6px 0;
    padding-left: 20px;
  }
  .x {
    padding: 2px 8px;
  }
</style>
