<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  // A device's LCD: which face it shows and at what frame rate (PUT /v1/devices/{key}/lcd).
  import { api } from "../lib/api";
  import { daemon } from "../lib/store.svelte";
  import type { DeviceView, LcdFace } from "../lib/types";

  let { device, faces }: { device: DeviceView; faces: LcdFace[] } = $props();

  const lcd = $derived(device.lcd!);
  const current = $derived(faces.find((f) => f.id === lcd.face));

  function setFace(face: string) {
    daemon.run(() => api.setLcd(device.key, { face }));
  }

  function setRate(value: string) {
    const framerate = Math.round(Number(value));
    if (Number.isFinite(framerate) && framerate >= 1 && framerate <= 60) {
      daemon.run(() => api.setLcd(device.key, { framerate }));
    }
  }
</script>

<div class="lcd">
  <span class="muted">LCD {lcd.width}×{lcd.height}{lcd.circular ? ", round" : ""}</span>
  <label
    >Face
    <select value={lcd.face ?? ""} onchange={(e) => setFace((e.target as HTMLSelectElement).value)}>
      {#if !lcd.face}<option value="">(none installed)</option>{/if}
      {#each faces as f (f.id)}
        <option value={f.id}>{f.title || f.id}</option>
      {/each}
    </select>
  </label>
  <label
    >Frame rate
    <input
      type="number"
      min="1"
      max="60"
      value={lcd.framerate}
      onchange={(e) => setRate((e.target as HTMLInputElement).value)}
    /> fps</label
  >
  {#if lcd.renderer === "mock"}
    <span class="badge warn" title="lightloom-renderer is not available, or no face is installed"
      >test pattern</span
    >
  {:else if !lcd.running}
    <span class="badge warn" title={lcd.status ?? ""}>face not running</span>
  {/if}
  {#if current?.description}<span class="muted desc">{current.description}</span>{/if}
</div>

<style>
  .lcd {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem 1rem;
    margin: 0.4rem 0;
  }
  input[type="number"] {
    width: 4.5rem;
  }
  .desc {
    flex-basis: 100%;
    font-size: 0.85em;
  }
</style>
