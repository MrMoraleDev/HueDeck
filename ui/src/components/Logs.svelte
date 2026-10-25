<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!-- The daemon's log history (GET /v1/logs): the log files when Lightloom runs from the installer
     (lightloomw: this run and the previous one), else this run's in-memory lines. Filters by level,
     text and device/plugin; Copy and Save as file. The daemon redacts the API token. -->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { api } from "../lib/api";
  import { daemon } from "../lib/store.svelte";
  import type { LogEntry, LogsView } from "../lib/types";

  let view = $state<LogsView | null>(null);
  let failed = $state<string | null>(null);
  let from = $state<"" | "file" | "memory">("");
  let level = $state("");
  let text = $state("");
  let source = $state("");
  let follow = $state(true);
  let copied = $state(false);
  let box = $state<HTMLDivElement | null>(null);

  let seq = 0;
  async function refresh() {
    const mine = ++seq;
    try {
      const v = await api.logs({ from: from || undefined, level, q: text.trim(), source, limit: 2000 });
      if (mine !== seq) return;
      const atEnd = !box || box.scrollTop + box.clientHeight >= box.scrollHeight - 8;
      view = v;
      failed = null;
      if (atEnd) requestAnimationFrame(() => box && (box.scrollTop = box.scrollHeight));
    } catch (e) {
      if (mine === seq) failed = e instanceof Error ? e.message : String(e);
    }
  }

  // The filters: at once for the selects, after a pause for typed text.
  let typing: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    void from;
    void level;
    void source;
    untrack(() => void refresh());
  });
  function typed() {
    clearTimeout(typing);
    typing = setTimeout(refresh, 300);
  }

  // Follow: fetch again (at most once a second) when the daemon logs a line.
  let pending: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    void daemon.logs.length;
    if (!follow || pending) return;
    pending = setTimeout(() => {
      pending = undefined;
      void refresh();
    }, 1000);
  });

  onMount(() => () => {
    clearTimeout(typing);
    clearTimeout(pending);
  });

  const line = (e: LogEntry) =>
    [e.time ?? "", (e.level ?? "").padEnd(5), e.target ? `${e.target}:` : "", e.message].filter(Boolean).join(" ");
  const asText = () => (view ? view.entries.map(line).join("\n") + "\n" : "");
  const when = (t?: string) => (t ? new Date(t).toLocaleString() : "");

  async function copy() {
    try {
      await navigator.clipboard.writeText(asText());
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch (e) {
      failed = `Cannot copy: ${e instanceof Error ? e.message : String(e)}`;
    }
  }

  function save() {
    const blob = new Blob([asText()], { type: "text/plain" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-");
    a.download = `lightloom-log-${stamp}.txt`;
    a.click();
    URL.revokeObjectURL(a.href);
  }
</script>

<section class="panel logs">
  <h2>Log</h2>
  <div class="row controls">
    {#if view && view.available.length > 1}
      <select bind:value={from} aria-label="Which log">
        <option value="">Log files (this run and the previous one)</option>
        <option value="memory">This run, with plugin lines</option>
      </select>
    {/if}
    <select bind:value={level} aria-label="Lowest level">
      <option value="">All levels</option>
      <option value="error">Errors</option>
      <option value="warn">Warnings and errors</option>
      <option value="info">Info and above</option>
      <option value="debug">Debug and above</option>
    </select>
    <select bind:value={source} aria-label="Device or plugin">
      <option value="">All devices and plugins</option>
      {#each view?.sources ?? [] as s (s)}
        <option value={s}>{s}</option>
      {/each}
    </select>
    <input type="text" placeholder="Filter text" aria-label="Filter text" bind:value={text} oninput={typed} />
    <label class="row follow"><input type="checkbox" bind:checked={follow} /> Follow</label>
    <button onclick={refresh}>Refresh</button>
    <button disabled={!view?.entries.length} onclick={copy}>{copied ? "Copied" : "Copy"}</button>
    <button disabled={!view?.entries.length} onclick={save}>Save as file</button>
  </div>
  {#if failed}
    <p class="bad">{failed}</p>
  {/if}
  <div class="lines" bind:this={box}>
    {#if !view}
      <span class="muted">Reading the log…</span>
    {:else if !view.entries.length}
      <span class="muted">No lines match.</span>
    {:else}
      {#each view.entries as e, i (i)}
        <div class={`line ${(e.level ?? "none").toLowerCase()}`}>
          <span class="muted" title={e.time}>{when(e.time)}</span>
          {#if e.level}<span class="level">{e.level}</span>{/if}
          {#if e.source}<span class="source">[{e.source}]</span>{/if}
          <span class="message">{e.message}</span>
        </div>
      {/each}
    {/if}
  </div>
  {#if view}
    <p class="muted note">
      {view.entries.length} line{view.entries.length === 1 ? "" : "s"}{view.truncated ? " (the newest; older ones left out)" : ""}
      · {view.from === "file" ? view.files.join(", ") : "kept in memory since Lightloom started"}
    </p>
  {/if}
</section>

<style>
  .logs {
    margin-top: 16px;
  }
  .controls {
    margin-bottom: 8px;
  }
  .controls input[type="text"] {
    flex: 1;
    min-width: 10em;
  }
  .follow {
    gap: 4px;
  }
  .lines {
    max-height: 420px;
    overflow: auto;
    font-family: ui-monospace, monospace;
    font-size: 12px;
  }
  .line {
    white-space: pre-wrap;
    word-break: break-word;
  }
  .line > span + span {
    margin-left: 6px;
  }
  .level {
    font-weight: 600;
  }
  .error .level,
  .error .message {
    color: var(--bad);
  }
  .warn .level {
    color: var(--warn);
  }
  .debug,
  .trace {
    opacity: 0.75;
  }
  .source {
    color: var(--accent);
  }
  .bad {
    color: var(--bad);
  }
  .note {
    margin: 6px 0 0;
    font-size: 12px;
  }
</style>
