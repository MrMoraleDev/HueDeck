<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!--
  "Save to devices": stores the lighting in the devices' own memory so it shows at boot without
  HueDeck. Effects running on the canvas cannot be stored: each device gets either the colours of
  the current frame (as the device stores them, usually one per zone) or a built-in effect it
  offers. The preview comes from the plugins (nothing is written); the save needs a confirmation.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { daemon } from "../lib/store.svelte";
  import type { SaveResult, Snapshot } from "../lib/types";

  let { snapshot, onclose }: { snapshot: Snapshot; onclose: () => void } = $props();

  const EFFECT_NAMES: Record<string, string> = {
    spectrum: "Colour cycle (spectrum)",
    breathing: "Breathing",
    wave: "Wave",
    rainbow: "Rainbow",
  };

  /** Device key -> "" (current colours) or an effect name. */
  let choice = $state<Record<string, string>>({});
  /** Device key -> included in the save. */
  let chosen = $state<Record<string, boolean>>({});
  let previews = $state<Record<string, SaveResult>>({});
  let results = $state<SaveResult[] | null>(null);
  let busy = $state(false);

  const savable = $derived(snapshot.devices.filter((d) => d.connected && d.save));
  const others = $derived(snapshot.devices.filter((d) => d.connected && !d.save));
  const selected = $derived(savable.filter((d) => chosen[d.key] && previews[d.key]?.ok && !d.save?.waitSecs));

  function request(keys: string[]) {
    return Object.fromEntries(keys.map((k) => [k, { effect: choice[k] || null }]));
  }

  async function preview(keys: string[]) {
    if (keys.length === 0) return;
    const r = await daemon.run(() => api.savePreview(request(keys)));
    if (!r) return;
    for (const p of r.devices) previews[p.key] = p;
  }

  onMount(() => {
    // Preview every device that can save, with its current colours.
    const keys = savable.map((d) => d.key);
    for (const k of keys) chosen[k] = true;
    void preview(keys);
  });

  async function save() {
    const names = selected.map((d) => d.name).join(", ");
    const ok = window.confirm(
      `Store this lighting in the memory of: ${names}?\n\n` +
        "It is written to the devices' own flash or EEPROM and shows at boot without HueDeck. " +
        "Each device can be saved at most once a minute.",
    );
    if (!ok) return;
    busy = true;
    const r = await daemon.run(() => api.saveToDevices(request(selected.map((d) => d.key))));
    busy = false;
    if (r) results = r.devices;
  }
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="panel dialog" role="dialog" aria-modal="true" aria-labelledby="save-title">
    <h2 id="save-title">Save to devices</h2>
    <p class="muted">
      Stores the lighting in each device's own memory, so it shows when the computer starts, before (or without)
      HueDeck. Canvas effects cannot be stored: a device keeps the colours of the current frame, as it can hold them
      (often one colour per zone), or one of its built-in effects. Saving writes the device's flash: do it only when
      you mean to keep the result.
    </p>

    {#if results}
      <ul class="results">
        {#each results as r (r.key)}
          <li class:bad={!r.ok}>
            <strong>{r.name}</strong>:
            {r.ok ? (r.dryRun ? "would be stored (dry run: nothing was written)" : "stored") : r.error}
          </li>
        {/each}
      </ul>
      <div class="row end"><button class="primary" onclick={onclose}>Close</button></div>
    {:else}
      {#each savable as d (d.key)}
        {@const p = previews[d.key]}
        <div class="device">
          <label class="row">
            <input type="checkbox" bind:checked={chosen[d.key]} disabled={!p?.ok || !!d.save?.waitSecs} />
            <strong>{d.name}</strong>
          </label>
          <select
            value={choice[d.key] ?? ""}
            onchange={(e) => {
              choice[d.key] = (e.target as HTMLSelectElement).value;
              void preview([d.key]);
            }}
          >
            {#if d.save?.static}<option value="">Current colours</option>{/if}
            {#each d.save?.effects ?? [] as e (e)}
              <option value={e}>{EFFECT_NAMES[e] ?? e}</option>
            {/each}
          </select>
          {#if d.save?.waitSecs}
            <p class="muted">Saved less than a minute ago: wait {d.save.waitSecs} s.</p>
          {/if}
          {#if !p}
            <p class="muted">Asking the device…</p>
          {:else if !p.ok}
            <p class="muted">Cannot be saved: {p.error}</p>
          {:else}
            <div class="zones">
              {#each p.result?.zones ?? [] as z (z.name)}
                <span class="zone"><span class="swatch" style={`background:${z.color}`}></span>{z.name}</span>
              {:else}
                <span class="muted">{EFFECT_NAMES[p.result?.effect ?? ""] ?? p.result?.effect}</span>
              {/each}
            </div>
            {#if p.result?.note}<p class="muted note">{p.result.note}</p>{/if}
          {/if}
        </div>
      {:else}
        <p class="muted">None of the connected devices can store lighting in its own memory.</p>
      {/each}

      {#if others.length}
        <p class="muted">
          Cannot store lighting: {others.map((d) => d.name).join(", ")} (their plugins have no way to write the device's
          memory, or the device has none).
        </p>
      {/if}

      <div class="row end">
        <button onclick={onclose}>Cancel</button>
        <button class="primary" disabled={busy || selected.length === 0} onclick={save}>
          {busy ? "Saving…" : `Save to ${selected.length} device${selected.length === 1 ? "" : "s"}…`}
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.5);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: 40px 16px;
    overflow: auto;
    z-index: 10;
  }
  .dialog {
    width: min(640px, 100%);
  }
  .device {
    border-top: 1px solid var(--line);
    padding: 8px 0;
  }
  .zones {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 6px;
  }
  .zone {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
  }
  .swatch {
    width: 16px;
    height: 16px;
    border-radius: 3px;
    border: 1px solid var(--line);
  }
  .note {
    font-size: 12px;
    margin: 4px 0 0;
  }
  .end {
    justify-content: flex-end;
    margin-top: 12px;
  }
  .results .bad {
    color: var(--warn, #d08000);
  }
</style>
