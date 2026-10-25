<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!-- Effect playlists and the auto-next timer: play, stop, create and edit. -->
<script lang="ts">
  import { api } from "../lib/api";
  import { daemon } from "../lib/store.svelte";
  import type { Playlist, PlaylistEntry, Snapshot } from "../lib/types";

  let { snapshot }: { snapshot: Snapshot } = $props();

  /** The playlist being edited (`id` empty: a new one). */
  let draft = $state<Playlist | null>(null);
  let interval = $state(300);
  // Follows the daemon's interval, except while the field has the focus (typing).
  let typing = false;
  $effect(() => {
    const v = snapshot.lighting.timer.interval;
    if (!typing) interval = v;
  });

  const playing = $derived(snapshot.lighting.playlist ?? null);
  const nextAt = $derived(
    snapshot.lighting.nextSwitchMs ? new Date(snapshot.lighting.nextSwitchMs).toLocaleTimeString() : null,
  );

  function title(id: string): string {
    return snapshot.effects.find((e) => e.id === id)?.title ?? `${id} (not installed)`;
  }

  function presetsOf(id: string): string[] {
    return snapshot.effects.find((e) => e.id === id)?.presets ?? [];
  }

  function edit(p: Playlist | null) {
    draft = p
      ? { ...p, entries: p.entries.map((e) => ({ ...e })) }
      : { id: "", name: "", shuffle: false, entries: [], updatedAt: "" };
  }

  function addCurrent() {
    const id = snapshot.lighting.effect;
    if (draft && id) draft.entries = [...draft.entries, { effect: id }];
  }

  function move(i: number, by: number) {
    if (!draft) return;
    const list = [...draft.entries];
    const j = i + by;
    if (j < 0 || j >= list.length) return;
    [list[i], list[j]] = [list[j], list[i]];
    draft.entries = list;
  }

  function setDuration(e: PlaylistEntry, raw: string) {
    const n = Math.round(Number(raw));
    e.duration = raw.trim() === "" || !Number.isFinite(n) || n < 1 ? undefined : n;
  }

  async function save() {
    if (!draft) return;
    const d = $state.snapshot(draft);
    const ok = d.id
      ? await daemon.run(() => api.savePlaylist(d))
      : await daemon.run(() => api.createPlaylist({ name: d.name, shuffle: d.shuffle, entries: d.entries }));
    if (ok) draft = null;
  }
</script>

<div class="panel">
  <div class="row head">
    <h2>Playlists</h2>
    <button onclick={() => edit(null)} title="New playlist">+ New</button>
  </div>

  <div class="row timer">
    <label title="While no playlist plays, show the next effect by itself (game integrations skipped)">
      <input
        type="checkbox"
        checked={snapshot.lighting.timer.enabled}
        onchange={(ev) => daemon.run(() => api.setTimer({ enabled: ev.currentTarget.checked }))}
      />
      Next effect every
    </label>
    <input
      class="secs"
      type="number"
      min="1"
      max="86400"
      bind:value={interval}
      onfocus={() => (typing = true)}
      onblur={() => (typing = false)}
      onchange={() => daemon.run(() => api.setTimer({ interval: Math.max(1, Math.round(interval)) }))}
    />
    <span class="muted">s</span>
  </div>

  {#if playing}
    <div class="row playing">
      <span>▶ <b>{playing.name}</b> <span class="muted">· {playing.entry + 1}{nextAt ? ` · next at ${nextAt}` : ""}</span></span>
      <button onclick={() => daemon.run(() => api.stopPlaylist())}>Stop</button>
    </div>
  {:else if nextAt}
    <p class="muted small">Next effect at {nextAt}</p>
  {/if}

  {#each snapshot.playlists as p (p.id)}
    <div class="row item" class:active={playing?.id === p.id}>
      <span class="name">{p.name}</span>
      <span class="muted small">{p.entries.length} {p.entries.length === 1 ? "effect" : "effects"}{p.shuffle ? " · shuffle" : ""}</span>
      <button title="Play" disabled={!p.entries.length} onclick={() => daemon.run(() => api.playPlaylist(p.id))}>▶</button>
      <button title="Edit" onclick={() => edit(p)}>✎</button>
      <button class="x" title="Delete" onclick={() => daemon.run(() => api.deletePlaylist(p.id))}>×</button>
    </div>
  {:else}
    {#if !draft}<p class="muted small">No playlist yet.</p>{/if}
  {/each}

  {#if draft}
    <div class="editor">
      <div class="row">
        <input type="text" placeholder="Playlist name" bind:value={draft.name} />
        <label><input type="checkbox" bind:checked={draft.shuffle} /> Shuffle</label>
      </div>
      {#each draft.entries as e, i (i)}
        <div class="row entry">
          <span class="name" title={e.effect}>{title(e.effect)}</span>
          <select
            value={e.preset ?? ""}
            onchange={(ev) => (e.preset = ev.currentTarget.value || undefined)}
            title="Preset loaded when the entry starts"
          >
            <option value="">current values</option>
            {#each presetsOf(e.effect) as name (name)}<option value={name}>{name}</option>{/each}
          </select>
          <input
            class="secs"
            type="number"
            min="1"
            placeholder={String(snapshot.lighting.timer.interval)}
            title="Seconds (empty: the timer's interval)"
            value={e.duration ?? ""}
            onchange={(ev) => setDuration(e, ev.currentTarget.value)}
          />
          <button title="Up" onclick={() => move(i, -1)}>↑</button>
          <button title="Down" onclick={() => move(i, 1)}>↓</button>
          <button class="x" title="Remove" onclick={() => draft && (draft.entries = draft.entries.filter((_, k) => k !== i))}>×</button>
        </div>
      {/each}
      <div class="row">
        <button disabled={!snapshot.lighting.effect} onclick={addCurrent}>+ Add the current effect</button>
        <span class="spacer"></span>
        <button onclick={() => (draft = null)}>Cancel</button>
        <button disabled={!draft.name.trim()} onclick={save}>Save</button>
      </div>
    </div>
  {/if}
</div>

<style>
  .head h2 {
    flex: 1;
    margin: 0;
  }
  .timer,
  .playing,
  .item,
  .entry {
    align-items: center;
  }
  .secs {
    width: 72px;
  }
  .small {
    font-size: 12px;
    margin: 4px 0;
  }
  .playing {
    justify-content: space-between;
  }
  .item .name,
  .entry .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .item.active .name {
    color: var(--accent);
    font-weight: 600;
  }
  .editor {
    border-top: 1px solid #2a3442;
    margin-top: 8px;
    padding-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .spacer {
    flex: 1;
  }
  .x {
    padding: 6px 8px;
  }
</style>
