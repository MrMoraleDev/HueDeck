<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!-- Hardware sensors (read-only). Polls GET /v1/sensors every 2 s while shown and the page is visible;
     the daemon polls its sources only while somebody asks (docs/sensors.md). -->
<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { reconcile } from "../lib/reconcile";
  import type { SensorsView } from "../lib/types";

  const EVERY = 2000;
  let view = $state<SensorsView | null>(null);
  let failed = $state<string | null>(null);

  async function refresh() {
    if (document.visibilityState !== "visible") return;
    try {
      const next = await api.sensors();
      // In place: only the readings that moved re-render.
      if (view) reconcile(view, next);
      else view = next;
      if (failed !== null) failed = null;
    } catch (e) {
      failed = e instanceof Error ? e.message : String(e);
    }
  }

  onMount(() => {
    void refresh();
    const timer = setInterval(refresh, EVERY);
    const onVisible = () => void refresh();
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisible);
    };
  });

  const fmt = (v: number) => (Math.abs(v) >= 100 ? v.toFixed(0) : v.toFixed(1));
  const share = (v: number, min: number, max: number) =>
    max > min ? Math.min(100, Math.max(0, ((v - min) / (max - min)) * 100)) : 0;
  const failing = $derived(view ? view.sources.filter((s) => s.error) : []);
</script>

<section class="panel sensors">
  <h2>Sensors</h2>
  {#if failed}
    <p class="muted">Sensors are not available: {failed}</p>
  {:else if !view}
    <p class="muted">Reading sensors…</p>
  {:else if !view.enabled}
    <p class="muted">Off (<code>[sensors] enabled = false</code> in config.toml).</p>
  {:else}
    {#if view.sensors.length}
      <table>
        <tbody>
          {#each view.sensors as s (s.name)}
            <tr>
              <td class="name">{s.name}</td>
              <td class="value">{fmt(s.value)} <span class="muted">{s.units}</span></td>
              <td class="bar"
                ><span class="meter"><span style={`width: ${share(s.value, s.min, s.max)}%`}></span></span></td
              >
              <td class="muted">{s.type}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p class="muted">No sensor found yet.</p>
    {/if}
    {#each failing as f (f.name)}
      <p class="muted">{f.name}: {f.error}</p>
    {/each}
    <p class="muted note">
      Read-only, at most once a second and only while this page or an effect reads them. CPU temperature on Windows
      comes from LibreHardwareMonitor when its web server runs and <code>[sensors] lhm_url</code> points at it. Lightloom
      never controls fans.
    </p>
  {/if}
</section>

<style>
  .sensors {
    margin-top: 16px;
  }
  table {
    border-collapse: collapse;
    width: 100%;
  }
  td {
    padding: 3px 8px 3px 0;
    white-space: nowrap;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 320px;
  }
  .value {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .bar {
    width: 40%;
  }
  .meter {
    display: block;
    height: 6px;
    border-radius: 3px;
    background: var(--line);
    overflow: hidden;
  }
  .meter span {
    display: block;
    height: 100%;
    background: var(--accent);
  }
  .note {
    font-size: 12px;
  }
</style>
