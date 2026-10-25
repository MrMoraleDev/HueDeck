<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "./lib/api";
  import { daemon } from "./lib/store.svelte";
  import Effects from "./components/Effects.svelte";
  import Layout from "./components/Layout.svelte";
  import Devices from "./components/Devices.svelte";
  import Network from "./components/Network.svelte";
  import Sensors from "./components/Sensors.svelte";
  import Settings from "./components/Settings.svelte";
  import Macros from "./components/Macros.svelte";
  import MacroPrompt from "./components/MacroPrompt.svelte";
  import { prompts } from "./lib/macros.svelte";
  import BatteryBadge from "./components/BatteryBadge.svelte";
  import logotype from "./assets/lightloom-logotype.svg";

  type Tab = "effects" | "layout" | "devices" | "macros" | "settings";
  const tabs: { id: Tab; label: string }[] = [
    { id: "effects", label: "Effects" },
    { id: "layout", label: "Layout" },
    { id: "devices", label: "Devices" },
    { id: "macros", label: "Macros" },
    { id: "settings", label: "Settings" },
  ];
  let tab = $state<Tab>((location.hash.slice(1) as Tab) || "effects");

  function select(t: Tab) {
    tab = t;
    history.replaceState(null, "", `#${t}`);
  }

  onMount(() => {
    // Action URLs: view/<page> switches tabs (and scrolls to a device), app/raiseWindow focuses the page.
    daemon.onAction = (a) => {
      if (prompts.take(a)) return;
      if (a.action === "view") {
        if (tabs.some((x) => x.id === a.tab)) select(a.tab as Tab);
        if (a.uid) {
          const uid = a.uid;
          setTimeout(() => document.getElementById(`device-${uid}`)?.scrollIntoView({ behavior: "smooth" }), 100);
        }
      }
      window.focus();
    };
    daemon.connect();
    const follow = () => {
      const t = location.hash.slice(1) as Tab;
      if (tabs.some((x) => x.id === t)) tab = t;
    };
    window.addEventListener("hashchange", follow);
    return () => window.removeEventListener("hashchange", follow);
  });

  const s = $derived(daemon.snapshot);
  const batteries = $derived(s ? s.devices.filter((d) => d.connected && d.battery) : []);
  let brightness = $state(100);
  // Follows the daemon's brightness, except while the slider is held (pointer or keys).
  let dragging = false;
  $effect(() => {
    const b = s?.lighting.brightness;
    if (b !== undefined && !dragging) brightness = b;
  });
</script>

<header>
  <img class="brand" src={logotype} alt="Lightloom" />
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
          oninput={() => (dragging = true)}
          onblur={() => (dragging = false)}
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
  {#if batteries.length}
    <div class="row batteries">
      {#each batteries as d (d.key)}
        <span class="row battery" title={d.name}
          ><span class="muted name">{d.name}</span> <BatteryBadge battery={d.battery!} name={d.name} /></span
        >
      {/each}
    </div>
  {/if}
  <span class="badge" class:good={daemon.connected} class:bad={!daemon.connected}>
    {daemon.connected ? "connected" : "offline"}
  </span>
</header>

<nav>
  {#each tabs as t (t.id)}
    <button class:active={tab === t.id} onclick={() => select(t.id)}>{t.label}</button>
  {/each}
</nav>

<MacroPrompt />

<main>
  {#if daemon.error}
    <div class="banner bad">{daemon.error}</div>
  {/if}
  {#if daemon.toast}
    <!-- Floats over the page: showing and hiding it does not move the content. -->
    <div class="banner toast" role="status">
      <strong>{daemon.toast.title}</strong>
      {daemon.toast.message}
    </div>
  {/if}
  {#if s}
    {#if s.conflicts.length}
      <div class="banner warn">
        Devices are left alone while other RGB software runs: <strong>{s.conflicts.join(", ")}</strong>. Close it
        (Lightloom never closes other applications); devices start within a few seconds.
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
      <Network snapshot={s} />
      <Devices snapshot={s} />
      <Sensors />
    {:else if tab === "macros"}
      <Macros />
    {:else}
      <Settings snapshot={s} />
    {/if}
  {:else}
    <p class="muted">Connecting to lightloomd…</p>
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
    display: block;
    height: 26px;
    width: auto;
  }
  .now {
    flex: 1;
  }
  .num {
    width: 2.5em;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .now .badge {
    font-variant-numeric: tabular-nums;
  }
  .batteries {
    gap: 12px;
  }
  .battery {
    gap: 6px;
  }
  .battery .name {
    max-width: 140px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
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
  .toast {
    position: fixed;
    right: 20px;
    bottom: 20px;
    z-index: 20;
    max-width: 420px;
    margin: 0;
    background: var(--panel);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
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
