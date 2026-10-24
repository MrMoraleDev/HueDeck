<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { api } from "../lib/api";
  import { daemon } from "../lib/store.svelte";
  import type { Snapshot } from "../lib/types";

  let { snapshot }: { snapshot: Snapshot } = $props();
  let plugin = $state("");
  let ip = $state("");

  const chosen = $derived(plugin || snapshot.network[0]?.plugin || "");

  async function add(e: SubmitEvent) {
    e.preventDefault();
    if (!chosen || !ip.trim()) return;
    const r = await daemon.run(() => api.networkAdd(chosen, ip.trim()));
    if (r) ip = "";
  }
</script>

<section class="panel network">
  <div class="row head">
    <h2>Network lights</h2>
    <label class="row"
      ><input
        type="checkbox"
        checked={snapshot.settings.network}
        onchange={(e) => daemon.run(() => api.setSettings({ network: (e.target as HTMLInputElement).checked }))}
      /> Look for lights on my network</label
    >
  </div>
  {#if !snapshot.settings.network}
    <p class="muted">
      Off. When on, Lightloom's network plugins look for WLED, Philips Hue, Nanoleaf, LIFX, Govee and Yeelight lights on
      your local network (mDNS and broadcasts, nothing leaves your network) and drive the ones they find.
    </p>
  {:else}
    {#each snapshot.network as svc (svc.plugin)}
      <div class="service">
        <div class="row">
          <strong>{svc.plugin}</strong>
          <span class={`badge ${svc.running ? "good" : "warn"}`}>{svc.running ? "looking" : "stopped"}</span>
          {#if svc.error}<span class="muted">{svc.error}</span>{/if}
        </div>
        {#if svc.controllers.length}
          <ul>
            {#each svc.controllers as c (c.id)}
              <li class="row">
                <span>{c.name}</span>
                {#if c.ip}<span class="muted">{c.ip}</span>{/if}
                {#if c.status}<span class="muted">{c.status}</span>{/if}
                {#if c.device}<span class="badge good">device</span>{/if}
                {#if c.canPair}
                  <button onclick={() => daemon.run(() => api.networkAction(svc.plugin, c.id, "pair"))}>Pair</button>
                {/if}
                {#if svc.canForget}
                  <button class="danger" onclick={() => daemon.run(() => api.networkAction(svc.plugin, c.id, "forget"))}
                    >Forget</button
                  >
                {/if}
              </li>
            {/each}
          </ul>
        {:else}
          <p class="muted">Nothing found yet.</p>
        {/if}
      </div>
    {:else}
      <p class="muted">No network plugin is installed.</p>
    {/each}
    {#if snapshot.network.length}
      <form class="row" onsubmit={add}>
        <span class="muted">Add by IP</span>
        <select bind:value={plugin}>
          {#each snapshot.network as svc (svc.plugin)}
            <option value={svc.plugin}>{svc.plugin}</option>
          {/each}
        </select>
        <input placeholder="192.168.1.50" bind:value={ip} />
        <button type="submit">Add</button>
      </form>
    {/if}
  {/if}
</section>

<style>
  .network {
    margin-bottom: 16px;
  }
  .head {
    justify-content: space-between;
  }
  .service {
    margin-top: 8px;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 4px 0;
  }
  li {
    gap: 8px;
  }
  form {
    margin-top: 12px;
    gap: 8px;
  }
</style>
