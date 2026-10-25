<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<script lang="ts">
  import type { BatteryView } from "../lib/types";

  let { battery, name }: { battery: BatteryView; name?: string } = $props();

  const STATES: Record<string, string> = {
    unknown: "state unknown",
    discharging: "on battery",
    charging: "charging",
    fullCharging: "full, on the charger",
    full: "full",
    wirelessCharging: "charging wirelessly",
  };

  const tone = $derived(
    battery.status === "critical" ? "bad" : battery.status === "low" ? "warn" : battery.charging ? "good" : "",
  );
  const fill = $derived(Math.max(0, Math.min(100, battery.level ?? 0)));
  const title = $derived(
    [
      name,
      battery.level === null || battery.level === undefined ? "level unknown" : `${battery.level} %`,
      STATES[battery.state] ?? battery.state,
      battery.updatedMs ? `reported ${new Date(battery.updatedMs).toLocaleTimeString()}` : null,
    ]
      .filter(Boolean)
      .join(" · "),
  );
</script>

<span class={`badge battery ${tone}`} {title}>
  <svg viewBox="0 0 24 12" width="20" height="10" aria-hidden="true">
    <rect x="0.5" y="0.5" width="20" height="11" rx="2" fill="none" stroke="currentColor" />
    <rect x="21" y="3.5" width="2.5" height="5" rx="1" fill="currentColor" />
    <rect x="2" y="2" width={(17 * fill) / 100} height="8" rx="1" fill="currentColor" />
  </svg>
  {battery.level === null || battery.level === undefined ? "?" : `${battery.level} %`}
  {#if battery.charging}
    <svg class="bolt" viewBox="0 0 10 14" width="8" height="12" aria-label="charging">
      <path d="M6 0 0 8h4l-1 6 7-9H6z" fill="currentColor" />
    </svg>
  {/if}
</span>

<style>
  .battery {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }
</style>
