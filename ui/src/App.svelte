<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "./lib/api";
  import { daemon } from "./lib/store.svelte";
  import Effects from "./components/Effects.svelte";
  import Layout from "./components/Layout.svelte";
  import Devices from "./components/Devices.svelte";
  import Settings from "./components/Settings.svelte";

  type Tab = "effects" | "layout" | "devices" | "settings";
  const tabs: { id: Tab; label: string }[] = [
    { id: "effects", label: "Effects" },
    { id: "layout", label: "Layout" },
    { id: "devices", label: "Devices" },
    { id: "settings", label: "Settings" },
  ];
  let tab = $state<Tab>((location.hash.slice(1) as Tab) || "effects");

  function select(t: Tab) {
    tab = t;
    history.replaceState(null, "", `#${t}`);
  }

  onMount(() => daemon.connect());

  const s = $derived(daemon.snapshot);
  let brightness = $state(100);
  let dragging = false;
  $effect(() => {
    if (s && !dragging) brightness = s.lighting.brightness;
  });
</script>

<header>
  <div class="brand">HueDeck</div>
  {#if s}
    <div class="row now">
      <span class="muted">Effect</span>
      <strong>{s.lighting.effectName ?? "—"}</strong>
      <span
        class="badge"
        class:good={s.lighting.enabled && s.lighting.renderer.running}
        class:warn={!s.lighting.enabled}
        title={s.lighting.renderer.status ?? ""}
      >
        {!s.lighting.enabled ? "paused" : s.lighting.renderer.kind === "cef" ? `${s.lighting.renderer.fps} fps` : "no renderer"}
      </span>
    </div>
    <div class="row">
      <label class="row" title="Global brightness">
        <span class="muted">Brightness</span>
        <input
          type="range"
          min="0"
          max="100"
          bind:value={brightness}
          onpointerdown={() => (dragging = true)}
          onchange={() => {
            dragging = false;
            void daemon.run(() => api.setLighting({ brightness }));
          }}
        />
        <span class="num">{brightness}</span>
      </label>
      <button
        class:primary={!s.lighting.enabled}
        onclick={() => daemon.run(() => api.setLighting({ enabled: !s!.lighting.enabled }))}
      >
        {s.lighting.enabled ? "Pause lighting" : "Resume lighting"}
      </button>
    </div>
  {/if}
  <span class="badge" class:good={daemon.connected} class:bad={!daemon.connected}>
    {daemon.connected ? "connected" : "offline"}
  </span>
</header>

<nav>
  {#each tabs as t}
    <button class:active={tab === t.id} onclick={() => select(t.id)}>{t.label}</button>
  {/each}
</nav>

<main>
  {#if daemon.error}
    <div class="banner bad">{daemon.error}</div>
  {/if}
  {#if s}
    {#if s.conflicts.length}
      <div class="banner warn">
        Devices are left alone while other RGB software runs: <strong>{s.conflicts.join(", ")}</strong>. Close it
        (HueDeck never closes other applications); devices start within a few seconds.
      </div>
    {/if}
    {#each s.warnings as w}
      <div class="banner warn">{w}</div>
    {/each}
    {#if tab === "effects"}
      <Effects snapshot={s} />
    {:else if tab === "layout"}
      <Layout snapshot={s} />
    {:else if tab === "devices"}
      <Devices snapshot={s} />
    {:else}
      <Settings snapshot={s} />
    {/if}
  {:else}
    <p class="muted">Connecting to huedeckd…</p>
  {/if}
</main>

<style>
  header {
    display: flex;
    align-items: center;
    gap: 24px;
    padding: 12px 20px;
    border-bottom: 1px solid var(--line);
    background: var(--panel);
    flex-wrap: wrap;
  }
  .brand {
    font-weight: 700;
    font-size: 18px;
    background: linear-gradient(90deg, #ff4d6d, #ffb84d, #57d68d, #4fa3ff, #b36bff);
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }
  .now {
    flex: 1;
  }
  .num {
    width: 2.5em;
    text-align: right;
  }
  nav {
    display: flex;
    gap: 4px;
    padding: 8px 20px 0;
    border-bottom: 1px solid var(--line);
  }
  nav button {
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    background: transparent;
    padding: 8px 14px;
  }
  nav button.active {
    border-bottom-color: var(--accent);
    color: var(--accent);
  }
  main {
    padding: 16px 20px 40px;
    max-width: 1400px;
  }
  .banner {
    border-radius: 8px;
    padding: 8px 12px;
    margin-bottom: 10px;
    border: 1px solid;
  }
  .banner.warn {
    border-color: var(--warn);
    color: var(--warn);
  }
  .banner.bad {
    border-color: var(--bad);
    color: var(--bad);
  }
</style>
