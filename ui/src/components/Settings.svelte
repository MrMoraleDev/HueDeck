<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import { api } from "../lib/api";
  import { daemon } from "../lib/store.svelte";
  import type { Snapshot } from "../lib/types";

  let { snapshot }: { snapshot: Snapshot } = $props();
  let message = $state<string | null>(null);
  let file: HTMLInputElement;
  let low = $state(15);
  let critical = $state(5);
  // Follow the daemon's values only when they change (a refresh must not undo typing).
  let seen = "";
  $effect(() => {
    const now = `${snapshot.settings.batteryLow}/${snapshot.settings.batteryCritical}`;
    if (now !== seen) {
      seen = now;
      low = snapshot.settings.batteryLow;
      critical = snapshot.settings.batteryCritical;
    }
  });
  const thresholdsValid = $derived(low >= 1 && low <= 95 && critical >= 0 && critical < low);

  function saveThresholds() {
    if (thresholdsValid) void daemon.run(() => api.setSettings({ batteryLow: low, batteryCritical: critical }));
  }

  let port = $state<number | null>(null);
  let moving = $state<string | null>(null);
  const portValue = $derived(port ?? snapshot.settings.port);
  const portValid = $derived(Number.isInteger(portValue) && portValue >= 1024 && portValue <= 65535);

  async function changePort() {
    if (!portValid || portValue === snapshot.settings.port) return;
    const r = await daemon.run(() => api.setPort(portValue));
    if (!r) return;
    moving = r.url;
    // huedeckd serves the page (with its token) on the new port; the old one closes in a few seconds.
    setTimeout(() => location.assign(r.url), 300);
  }

  async function exportState() {
    const state = await daemon.run(() => api.exportState());
    if (!state) return;
    const blob = new Blob([JSON.stringify(state, null, 2)], { type: "application/json" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = `huedeck-settings-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(a.href);
  }

  async function importState() {
    const f = file.files?.[0];
    if (!f) return;
    try {
      const state = JSON.parse(await f.text());
      const r = await daemon.run(() => api.importState(state));
      if (r) message = "Settings imported.";
    } catch (e) {
      message = `Not a HueDeck settings file: ${e instanceof Error ? e.message : e}`;
    }
    file.value = "";
  }
</script>

<div class="cols">
  <section class="panel">
    <h2>SignalRGB</h2>
    {#if snapshot.settings.signalrgbAvailable}
      <p class="muted">
        HueDeck can use the plugins, effects and components of your SignalRGB installation. They are read where they
        are, never copied, and SignalRGB's files are never changed. Close SignalRGB while HueDeck drives your devices.
      </p>
      <label class="check"
        ><input
          type="checkbox"
          checked={snapshot.settings.signalrgb}
          onchange={(e) => daemon.run(() => api.setSettings({ signalrgb: (e.target as HTMLInputElement).checked }))}
        /> Use my SignalRGB plugins, effects and components</label
      >
      <p class="muted">
        Import copies SignalRGB's device placements into a new layout and its component setup of each channel into
        HueDeck's settings.
      </p>
      <button
        onclick={async () => {
          const r = await daemon.run(() => api.importSignalRgb());
          if (r) message = `Imported ${r.placements} placements as layout "${r.layout}" and ${r.channels} channel setups.`;
        }}>Import SignalRGB layout and channels</button
      >
    {:else}
      <p class="muted">No SignalRGB installation was found on this computer.</p>
    {/if}
  </section>

  <section class="panel">
    <h2>Network lights</h2>
    <label class="check"
      ><input
        type="checkbox"
        checked={snapshot.settings.network}
        onchange={(e) => daemon.run(() => api.setSettings({ network: (e.target as HTMLInputElement).checked }))}
      /> Look for lights on my network</label
    >
    <p class="muted">
      Off by default. When on, HueDeck's network plugins look for WLED, Philips Hue, Nanoleaf, LIFX, Govee and Yeelight
      lights on your local network (mDNS and broadcasts; nothing is sent outside it). Found lights, pairing and "add by
      IP" are on the Devices tab.
    </p>
  </section>

  <section class="panel">
    <h2>Graphics card and RAM lighting</h2>
    <label class="check"
      ><input
        type="checkbox"
        checked={snapshot.settings.smbus}
        onchange={(e) => daemon.run(() => api.setSettings({ smbus: (e.target as HTMLInputElement).checked }))}
      /> Drive SMBus devices (GPU and RAM RGB)</label
    >
    <p class="muted">
      On by default. HueDeck looks for supported graphics cards (MSI, through the NVIDIA driver) and, when the PawnIO
      driver and the HueDeck I/O service are installed, supported RAM modules, and drives their lighting. It never
      does so while SignalRGB, OpenRGB or a vendor lighting service is running. Turn it off to leave these devices
      alone.
    </p>
  </section>

  <section class="panel">
    <h2>When HueDeck closes</h2>
    {#each [["restore", "Give each device its own lighting back (its built-in effect, or what it showed before HueDeck)"], ["keep", "Keep the last colours HueDeck showed (until the device is switched off)"], ["off", "Turn the lights off"]] as [mode, label] (mode)}
      <label class="check"
        ><input
          type="radio"
          name="shutdown-mode"
          value={mode}
          checked={snapshot.settings.shutdownMode === mode}
          onchange={() => daemon.run(() => api.setSettings({ shutdownMode: mode as "restore" | "keep" | "off" }))}
        /> {label}</label
      >
    {/each}
    <p class="muted">
      Nothing is stored in the devices: after a power cycle they show what they keep in their own memory (see Devices →
      Save to devices). A device can have its own choice in its settings; devices whose plugin cannot do it fall back to
      their own lighting.
    </p>
  </section>

  <section class="panel">
    <h2>OpenRGB SDK</h2>
    <label class="check"
      ><input
        type="checkbox"
        checked={snapshot.settings.openrgbSdk}
        onchange={(e) => daemon.run(() => api.setSettings({ openrgbSdk: (e.target as HTMLInputElement).checked }))}
      /> Let OpenRGB SDK clients drive HueDeck's devices (127.0.0.1:6742)</label
    >
  </section>

  <section class="panel">
    <h2>Effects</h2>
    <label class="check"
      ><input
        type="checkbox"
        checked={snapshot.settings.gpuCompositing}
        onchange={(e) => daemon.run(() => api.setSettings({ gpuCompositing: (e.target as HTMLInputElement).checked }))}
      /> Composite effects on the GPU</label
    >
    <p class="muted">
      Off (the default) paints effects at a steadier frame rate with less memory; on can help effects with heavy canvas
      filters. Renderer: {snapshot.lighting.renderer.status ?? snapshot.lighting.renderer.kind}.
    </p>
  </section>

  <section class="panel">
    <h2>Battery</h2>
    <div class="row">
      <label class="row"
        >Low below <input type="number" min="1" max="95" style="width:70px" bind:value={low} onchange={saveThresholds} />
        %</label
      >
      <label class="row"
        >Critical below
        <input type="number" min="0" max="94" style="width:70px" bind:value={critical} onchange={saveThresholds} /> %</label
      >
    </div>
    {#if !thresholdsValid}
      <p class="bad">The low level must be 1 to 95 %, and the critical level below it.</p>
    {/if}
    <label class="check"
      ><input
        type="checkbox"
        checked={snapshot.settings.batteryNotifications}
        disabled={!snapshot.settings.notificationsAvailable}
        onchange={(e) =>
          daemon.run(() => api.setSettings({ batteryNotifications: (e.target as HTMLInputElement).checked }))}
      /> Show a system notification when a battery runs low or critical</label
    >
    <p class="muted">
      For wireless devices whose plugin reports a battery. The dashboard warns while a device on battery is at or under
      the low level. Notifications come once per discharge, at the low and at the critical level, and again after the
      device was charged or climbed 5 points above the level.{snapshot.settings.notificationsAvailable
        ? ""
        : " System notifications are not available on this system yet."} Each device can also pulse red while its battery
      is low (Devices).
    </p>
  </section>

  <section class="panel">
    <h2>Backup</h2>
    <div class="row">
      <button onclick={exportState}>Export settings</button>
      <button onclick={() => file.click()}>Import settings…</button>
      <input type="file" accept="application/json,.json" bind:this={file} onchange={importState} hidden />
    </div>
    <p class="muted">Settings are kept in HueDeck's database; the export is one JSON file you can import again.</p>
  </section>

  <section class="panel">
    <h2>Port</h2>
    <div class="row">
      <input
        type="number"
        min="1024"
        max="65535"
        step="1"
        aria-label="Port of the dashboard and the API"
        value={portValue}
        oninput={(e) => (port = (e.target as HTMLInputElement).valueAsNumber)}
        onkeydown={(e) => e.key === "Enter" && changePort()}
      />
      <button disabled={!portValid || portValue === snapshot.settings.port || moving !== null} onclick={changePort}
        >Change port</button
      >
    </div>
    {#if moving}
      <p class="message">Moving to {moving}…</p>
    {:else if !portValid}
      <p class="warn">Choose a port from 1024 to 65535.</p>
    {/if}
    {#if snapshot.settings.portNotice}
      <p class="warn">{snapshot.settings.portNotice}</p>
    {/if}
    <p class="muted">
      The dashboard and the API listen on http://127.0.0.1:{snapshot.settings.port} (default 53000). Only this
      computer can connect: HueDeck listens on the loopback address, never on the network. A new port is used right
      away and this page moves there; bookmarks need the new address. The port saved here wins over
      <code>[api] port</code> in config.toml. The SignalRGB-compatible ports 16038 and 16034 do not change.
    </p>
  </section>

  <section class="panel">
    <h2>Daemon</h2>
    <p class="muted">HueDeck {snapshot.version} · {snapshot.state}{snapshot.settings.dryRun ? " · dry run (nothing is written to devices)" : ""}</p>
    <div class="row">
      <button onclick={() => daemon.run(() => api.rescan())}>Look for devices again</button>
      <button class="danger" onclick={() => daemon.run(() => api.shutdown())}>Stop HueDeck</button>
    </div>
  </section>
</div>

{#if message}
  <p class="message">{message}</p>
{/if}

<section class="panel logs">
  <h2>Log</h2>
  <pre>{daemon.logs.join("\n") || "Nothing logged since this page opened."}</pre>
</section>

<style>
  .cols {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 16px;
  }
  .message {
    color: var(--good);
  }
  .bad {
    color: var(--bad);
  }
  .warn {
    color: var(--warn);
  }
  input[type="number"] {
    width: 9em;
  }
  .logs {
    margin-top: 16px;
  }
  pre {
    max-height: 320px;
    overflow: auto;
    font-size: 12px;
    white-space: pre-wrap;
    margin: 0;
  }
</style>
