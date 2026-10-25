<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  // A device's mode: follow the effect, or one of its built-in hardware modes with its colours,
  // speed and brightness (PUT /v1/devices/{key}/mode). Live only: nothing is stored in the device.
  import { api } from "../lib/api";
  import { daemon } from "../lib/store.svelte";
  import type { DeviceHardwareMode, DeviceView, HardwareModeView } from "../lib/types";

  let { device }: { device: DeviceView } = $props();

  const modes = $derived(device.hardwareModes ?? []);
  const chosen = $derived(device.hardwareMode ?? null);
  const mode = $derived(modes.find((m) => m.id === chosen?.id) ?? null);

  // The sliders follow the daemon except while one is held.
  let speed = $state(50);
  let brightness = $state(100);
  let dragging = false;
  $effect(() => {
    const s = chosen?.speed;
    const b = chosen?.brightness;
    if (dragging) return;
    if (s != null) speed = s;
    if (b != null) brightness = b;
  });

  /** The choice for `m`, keeping the colours, speed and brightness set so far. */
  function choiceFor(m: HardwareModeView, patch: Partial<DeviceHardwareMode> = {}): DeviceHardwareMode {
    const colors = Array.from({ length: m.colors }, (_, i) => chosen?.colors[i] ?? "#ffffff");
    return {
      id: m.id,
      colors,
      speed: m.speed ? speed : null,
      brightness: m.brightness ? brightness : null,
      ...patch,
    };
  }

  function choose(id: string) {
    const m = modes.find((x) => x.id === id);
    void daemon.run(() => api.setHardwareMode(device.key, m ? choiceFor(m) : null));
  }

  function update(patch: Partial<DeviceHardwareMode>) {
    if (!mode) return;
    dragging = false;
    void daemon.run(() => api.setHardwareMode(device.key, choiceFor(mode, patch)));
  }

  function setColor(i: number, value: string) {
    if (!mode || !chosen) return;
    const colors = choiceFor(mode).colors;
    colors[i] = value;
    update({ colors });
  }
</script>

<div class="hwmode">
  <label
    >Mode
    <select value={chosen?.id ?? ""} onchange={(e) => choose(e.currentTarget.value)}>
      <option value="">Follow effect</option>
      {#each modes as m (m.id)}
        <option value={m.id}>{m.name}</option>
      {/each}
    </select>
  </label>
  {#if mode && chosen}
    {#each chosen.colors as c, i (i)}
      <label
        >{mode.colorNames?.[i] ?? (mode.colors > 1 ? `Colour ${i + 1}` : "Colour")}
        <input type="color" value={c} onchange={(e) => setColor(i, e.currentTarget.value)} /></label
      >
    {/each}
    {#if mode.speed}
      <label
        >Speed
        <input
          type="range"
          min="0"
          max="100"
          bind:value={speed}
          onpointerdown={() => (dragging = true)}
          onchange={() => update({ speed })}
        /></label
      >
    {/if}
    {#if mode.brightness}
      <label
        >Brightness
        <input
          type="range"
          min="0"
          max="100"
          bind:value={brightness}
          onpointerdown={() => (dragging = true)}
          onchange={() => update({ brightness })}
        /></label
      >
    {/if}
    <span class="muted note">The device runs its own effect: the canvas does not reach it. Not stored in the device.</span>
  {/if}
</div>

<style>
  .hwmode {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
    align-items: center;
    margin: 6px 0 0;
    font-size: 13px;
  }
  label {
    display: inline-flex;
    gap: 6px;
    align-items: center;
  }
  input[type="color"] {
    width: 34px;
    height: 24px;
    padding: 0;
    border: 1px solid var(--line);
    background: none;
  }
  .note {
    font-size: 12px;
  }
</style>
