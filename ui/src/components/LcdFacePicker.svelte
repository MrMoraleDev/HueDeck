<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  // A device's LCD: which face it shows, the face's controls (kept per device and face), the frame
  // rate (PUT /v1/devices/{key}/lcd) and a small live preview of the face.
  import { api } from "../lib/api";
  import { daemon } from "../lib/store.svelte";
  import type { DeviceView, LcdFace } from "../lib/types";
  import ControlInput from "./ControlInput.svelte";
  import LcdPreview from "./LcdPreview.svelte";

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

  function setValue(property: string, value: unknown) {
    daemon.run(() => api.setLcd(device.key, { values: { [property]: value } }));
  }

  const valueOf = (property: string, fallback: unknown) =>
    lcd.values && property in lcd.values ? lcd.values[property] : fallback;
</script>

<div class="lcd">
  <LcdPreview deviceKey={device.key} width={lcd.width} height={lcd.height} circular={lcd.circular} />
  <div class="settings">
    <div class="row">
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
    </div>
    {#if current?.description}<span class="muted desc">{current.description}</span>{/if}
    {#if current?.controls?.length}
      <div class="controls">
        {#each current.controls as c (c.property)}
          <ControlInput
            kind={c.type}
            label={c.label || c.property}
            value={valueOf(c.property, c.default)}
            min={c.min}
            max={c.max}
            step={c.step}
            values={c.values}
            tooltip={c.tooltip}
            onchange={(v) => setValue(c.property, v)}
          />
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .lcd {
    display: flex;
    align-items: flex-start;
    gap: 0.8rem;
    margin: 0.4rem 0;
  }
  .settings {
    flex: 1;
    min-width: 0;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem 1rem;
  }
  input[type="number"] {
    width: 4.5rem;
  }
  .desc {
    display: block;
    font-size: 0.85em;
    margin-top: 0.3rem;
  }
  .controls {
    margin-top: 0.3rem;
  }
</style>
