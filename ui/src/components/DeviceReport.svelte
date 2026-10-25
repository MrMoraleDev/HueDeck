<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!--
  Device report: what "Works" / "Report a problem" will put in a GitHub issue, shown before the
  browser opens it. Lightloom sends nothing: the button opens github.com/.../issues/new with the
  device-report form filled in, and the user submits it there (or not). The facts are taken once,
  when the dialog opens, so the preview does not move while it is read.
-->
<script lang="ts">
  import { untrack } from "svelte";
  import { token } from "../lib/api";
  import { buildReport, type ReportResult } from "../lib/report";
  import type { DeviceView } from "../lib/types";

  let {
    device,
    version,
    os,
    logs,
    result,
    onclose,
  }: { device: DeviceView; version: string; os: string; logs: string[]; result: ReportResult; onclose: () => void } = $props();

  // A still copy of the live device and the log as they are now.
  const frozen = untrack(() => $state.snapshot(device) as DeviceView);
  const lines = untrack(() => [...logs]);
  let details = $state("");
  let skip = $state(false);
  const report = $derived(buildReport({ device: frozen, version, os, logs: lines, token: token() }, result, details));
  const LABELS: Record<string, string> = {
    result: "Result",
    device: "Device",
    vidpid: "VID:PID / address",
    plugin: "Plugin",
    version: "Lightloom",
    os: "Operating system",
    details: "What happens",
  };

  function opened() {
    if (result === "works" && skip) rememberSkip();
    onclose();
  }

  function rememberSkip() {
    try {
      localStorage.setItem(SKIP_KEY, "1");
    } catch {
      // Private mode: ask again next time.
    }
  }
</script>

<script module lang="ts">
  const SKIP_KEY = "lightloom-report-works-no-preview";

  /** The user chose to open "Works" reports without this preview. */
  export function skipWorksPreview(): boolean {
    try {
      return localStorage.getItem(SKIP_KEY) === "1";
    } catch {
      return false;
    }
  }
</script>

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="panel dialog" role="dialog" aria-modal="true" aria-labelledby="report-title">
    <h2 id="report-title">{result === "works" ? "Report that this device works" : "Report a problem with this device"}</h2>
    <p class="muted">
      Lightloom sends nothing. The button opens a GitHub issue form in your browser with the fields below filled in;
      you can change anything there before you submit it, or just close the page. Serial numbers are masked, and
      your user name in paths, network addresses and the API token are removed.
    </p>

    <dl>
      {#each Object.entries(report.fields) as [id, value] (id)}
        {#if id !== "diagnostics" && id !== "details"}
          <dt>{LABELS[id] ?? id}</dt>
          <dd>{value}</dd>
        {/if}
      {/each}
    </dl>

    {#if result === "problem"}
      <label class="details">
        <span>What happens <span class="muted">(what you expected, what you see instead)</span></span>
        <textarea rows="4" bind:value={details} placeholder="Wrong colours, flicker, only some LEDs, nothing…"></textarea>
      </label>
    {/if}

    <details>
      <summary>
        Diagnostics ({report.fields.diagnostics.split("\n").length} lines{report.droppedLogLines
          ? `, ${report.droppedLogLines} older log lines left out for length`
          : ""})
      </summary>
      <pre>{report.fields.diagnostics}</pre>
    </details>

    <div class="row end">
      {#if result === "works"}
        <label class="row skip"><input type="checkbox" bind:checked={skip} /> Open "Works" reports without this preview</label>
      {/if}
      <button onclick={onclose}>Cancel</button>
      <!-- svelte-ignore a11y_autofocus -->
      <a class="button primary" href={report.url} target="_blank" rel="noopener noreferrer" onclick={opened} autofocus
        >Open on GitHub</a
      >
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.5);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: 40px 16px;
    overflow: auto;
    z-index: 10;
  }
  .dialog {
    width: min(680px, 100%);
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px 12px;
    margin: 10px 0;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .details {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 8px 0;
  }
  textarea {
    font: inherit;
    color: inherit;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 6px 8px;
    resize: vertical;
  }
  summary {
    cursor: pointer;
    color: var(--muted);
  }
  pre {
    max-height: 260px;
    overflow: auto;
    font-size: 12px;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 8px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .end {
    justify-content: flex-end;
    margin-top: 12px;
  }
  .skip {
    flex: 1;
    gap: 6px;
    font-size: 12px;
    color: var(--muted);
  }
  .button {
    display: inline-block;
    border-radius: 6px;
    padding: 6px 12px;
    text-decoration: none;
    border: 1px solid var(--accent);
  }
  .button.primary {
    background: var(--accent);
    color: #08111d;
  }
</style>
