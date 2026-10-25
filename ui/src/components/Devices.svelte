<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { api } from "../lib/api";
  import { daemon } from "../lib/store.svelte";
  import type { DeviceView, LibraryComponent, Snapshot } from "../lib/types";
  import ControlInput from "./ControlInput.svelte";
  import SaveDialog from "./SaveDialog.svelte";
  import BatteryBadge from "./BatteryBadge.svelte";
  import { groupParameters, groupTitle } from "../lib/params";

  let { snapshot }: { snapshot: Snapshot } = $props();
  let library = $state<LibraryComponent[]>([]);
  let libraryError = $state<string | null>(null);
  let open = $state<string | null>(null);
  let pick = $state<Record<string, string>>({});
  let strip = $state<Record<string, number>>({});
  let saving = $state(false);
  const MODE_NAMES: Record<string, string> = {
    restore: "Its own lighting",
    keep: "Keep the last colours",
    off: "Lights off",
  };

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

<div class="row toolbar">
  <button onclick={() => (saving = true)} disabled={!snapshot.devices.some((d) => d.connected)}>Save to devices…</button>
  <span class="muted">Stores the current lighting in each device's own memory, so it shows at boot without Lightloom.</span>
</div>
{#if saving}
  <SaveDialog {snapshot} onclose={() => (saving = false)} />
{/if}

<div class="list">
  {#each snapshot.devices as d (d.key)}
    <section class="panel device" id={`device-${d.key}`}>
      <div class="row head">
        {#if d.image}<img src={d.image} alt="" />{/if}
        <div class="title">
          <strong>{d.name}</strong>
          <span class="muted key">{d.bus ? (d.bus.startsWith("network") ? d.bus : `SMBus · ${d.bus}`) : d.key}</span>
        </div>
        <span class={`badge ${stateBadge(d)}`}>{d.state}</span>
        {#if d.thirdParty}<span class="badge" title="Driven by a plugin read from your SignalRGB installation"
            >third-party plugin</span
          >{/if}
        {#if d.battery}<BatteryBadge battery={d.battery} />{/if}
        {#if d.connected}
          <span class="muted stats"
            >{d.frames} frames · {d.renderErrors} render errors · {d.ioErrors} {d.bus ? (d.bus.startsWith("network") ? "network" : "bus") : "HID"} errors</span
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
      {#if d.driverProblem}
        <p class="driver-problem">
          <span class="badge bad">needs WinUSB driver</span>
          {d.driverProblem}
          {#if d.helpUrl}<a href={d.helpUrl} target="_blank" rel="noreferrer">Why, and what to check</a>{/if}
        </p>
      {/if}
      {#each d.messages ?? [] as m (m.id)}
        <p class={`message ${(m.priority ?? 0) >= 2 ? "bad" : m.priority === 1 ? "warn" : ""}`} title={m.tooltip ?? ""}>
          {#if m.title}<strong>{m.title}:</strong>{/if}
          {m.message}
        </p>
      {/each}
      {#if d.battery}
        <label class="check blink"
          ><input
            type="checkbox"
            checked={d.blinkLowBattery}
            onchange={(e) =>
              daemon.run(() => api.setDevice(d.key, { blinkLowBattery: (e.target as HTMLInputElement).checked }))}
          /> Pulse red while the battery is low (below {snapshot.settings.batteryLow} %)</label
        >
      {/if}

      {#if open === d.key}
        {#if d.parameters.length}
          <h3>Plugin settings</h3>
          {#each groupParameters(d.parameters) as g (g.name)}
            {#if g.name}<h4>{groupTitle(g.name)}</h4>{/if}
            {#each g.items as p (p.property)}
              <ControlInput
                kind={p.type}
                label={p.label || p.property}
                value={p.value}
                min={p.min}
                max={p.max}
                step={p.step}
                values={p.values}
                tooltip={p.tooltip ?? p.description}
                live={p.live !== false}
                filter={p.filter}
                onchange={(v) => daemon.run(() => api.setDevice(d.key, { parameters: { [p.property]: v } }))}
              />
            {/each}
          {/each}
        {/if}

        <h3>When Lightloom closes</h3>
        <label class="row"
          ><select
            value={d.shutdownMode ?? "default"}
            onchange={(e) =>
              daemon.run(() =>
                api.setDevice(d.key, { shutdownMode: (e.target as HTMLSelectElement).value as "default" }),
              )}
          >
            <option value="default">As in Settings ({MODE_NAMES[snapshot.settings.shutdownMode]})</option>
            {#each d.shutdownModes as m (m)}
              <option value={m}>{MODE_NAMES[m] ?? m}</option>
            {/each}
          </select></label
        >
        {#if d.shutdownModes.length < 3}
          <p class="muted">This device's plugin can only give the lights back to the device's own lighting.</p>
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
  .toolbar {
    margin-bottom: 12px;
  }
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
  .driver-problem {
    margin: 6px 0 0;
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .path {
    opacity: 0.6;
  }
  .message {
    margin: 6px 0 0;
    padding: 4px 8px;
    border-left: 3px solid var(--accent);
    font-size: 13px;
  }
  .message.warn {
    border-color: var(--warn);
  }
  .message.bad {
    border-color: var(--bad);
  }
  h4 {
    margin: 8px 0 2px;
    font-size: 13px;
  }
  .blink {
    font-size: 12px;
    margin: 6px 0 0;
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
