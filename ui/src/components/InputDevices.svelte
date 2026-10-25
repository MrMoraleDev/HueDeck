<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!-- Settings → Inputs: the output device engine.audio captures and the display engine.zone and
     the meters capture (GET/PATCH /v1/inputs). The choice wins over config.toml's [audio] device
     and [screen] display; the running effect's capture moves at once. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { daemon } from "../lib/store.svelte";
  import type { InputsView, Snapshot } from "../lib/types";

  let { snapshot }: { snapshot: Snapshot } = $props();
  let view = $state<InputsView | null>(null);

  async function load() {
    view = (await daemon.run(() => api.inputs())) ?? null;
  }
  onMount(() => void load());

  const audio = $derived(snapshot.settings.audioDevice ?? "");
  const display = $derived(snapshot.settings.display ?? "");
  // A saved name that is not plugged in now still shows (and stays chosen).
  const audioNames = $derived(
    view ? (audio && !view.audioDevices.includes(audio) ? [...view.audioDevices, audio] : view.audioDevices) : [],
  );
  const displays = $derived(
    view
      ? display && !view.displays.some((d) => d.name === display)
        ? [...view.displays, { name: display, width: 0, height: 0, primary: false, adapter: "" }]
        : view.displays
      : [],
  );

  function choose(patch: { audioDevice?: string | null; display?: string | null }) {
    void daemon.run(() => api.setInputs(patch));
  }
</script>

<section class="panel">
  <h2>Inputs</h2>
  {#if !view}
    <p class="muted">Looking for audio devices and displays…</p>
  {:else if !view.supported}
    <p class="muted">Audio and screen capture are not available on this system yet.</p>
  {:else}
    <label class="field">
      <span>Audio for audio-reactive effects</span>
      <select
        value={audio}
        onchange={(e) => choose({ audioDevice: (e.target as HTMLSelectElement).value || null })}
      >
        <option value="">Default output device</option>
        {#each audioNames as name (name)}
          <option value={name}>{name}{view.audioDevices.includes(name) ? "" : " (not connected)"}</option>
        {/each}
      </select>
    </label>
    <label class="field">
      <span>Screen for screen-reactive effects</span>
      <select value={display} onchange={(e) => choose({ display: (e.target as HTMLSelectElement).value || null })}>
        <option value="">Primary display</option>
        {#each displays as d (d.name)}
          <option value={d.name}
            >{d.name.replace(/^\\\\\.\\/, "")}{d.width ? ` · ${d.width}×${d.height}` : " (not connected)"}{d.primary
              ? " · primary"
              : ""}{d.adapter ? ` · ${d.adapter}` : ""}</option
          >
        {/each}
      </select>
    </label>
    <div class="row">
      <button onclick={load}>Look again</button>
    </div>
    <p class="muted">
      Lightloom captures what this PC plays on the chosen output device, never a microphone. Capture runs only while
      the effect reads audio or the screen.
    </p>
  {/if}
</section>

<style>
  .field {
    display: grid;
    gap: 4px;
    margin-bottom: 10px;
  }
  .field select {
    max-width: 100%;
  }
</style>
