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

  async function exportState() {
    const state = await daemon.run(() => api.exportState());
    if (!state) return;
    const blob = new Blob([JSON.stringify(state, null, 2)], { type: "application/json" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = `lightloom-settings-${new Date().toISOString().slice(0, 10)}.json`;
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
      message = `Not a Lightloom settings file: ${e instanceof Error ? e.message : e}`;
    }
    file.value = "";
  }
</script>

<div class="cols">
  <section class="panel">
    <h2>SignalRGB</h2>
    {#if snapshot.settings.signalrgbAvailable}
      <p class="muted">
        Lightloom can use the plugins, effects and components of your SignalRGB installation. They are read where they
        are, never copied, and SignalRGB's files are never changed. Close SignalRGB while Lightloom drives your devices.
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
        Lightloom's settings.
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
    <h2>Graphics card and RAM lighting</h2>
    <label class="check"
      ><input
        type="checkbox"
        checked={snapshot.settings.smbus}
        onchange={(e) => daemon.run(() => api.setSettings({ smbus: (e.target as HTMLInputElement).checked }))}
      /> Drive SMBus devices (GPU and RAM RGB)</label
    >
    <p class="muted">
      On by default. Lightloom looks for supported graphics cards (MSI, through the NVIDIA driver) and, when the PawnIO
      driver and the Lightloom I/O service are installed, supported RAM modules, and drives their lighting. It never
      does so while SignalRGB, OpenRGB or a vendor lighting service is running. Turn it off to leave these devices
      alone.
    </p>
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
    <p class="muted">Settings are kept in Lightloom's database; the export is one JSON file you can import again.</p>
  </section>

  <section class="panel">
    <h2>Daemon</h2>
    <p class="muted">Lightloom {snapshot.version} · {snapshot.state}{snapshot.settings.dryRun ? " · dry run (nothing is written to devices)" : ""}</p>
    <div class="row">
      <button onclick={() => daemon.run(() => api.rescan())}>Look for devices again</button>
      <button class="danger" onclick={() => daemon.run(() => api.shutdown())}>Stop Lightloom</button>
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
