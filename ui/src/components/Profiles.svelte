<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!-- Profiles: named snapshots of the whole lighting setup (effect and its values, brightness, layout,
     device settings). Save, apply, rename, duplicate, delete, export / import as a file, and the one
     applied when Lightloom starts. -->
<script lang="ts">
  import { api } from "../lib/api";
  import { daemon } from "../lib/store.svelte";
  import type { Snapshot } from "../lib/types";

  let { snapshot }: { snapshot: Snapshot } = $props();

  const profiles = $derived(snapshot.profiles ?? []);
  const current = $derived(snapshot.lighting.profile ?? null);
  const defaultProfile = $derived(snapshot.settings.defaultProfile ?? null);

  let newName = $state("");
  /** The profile being renamed or duplicated, and the name typed for it. */
  let editing = $state<{ name: string; to: string; kind: "rename" | "duplicate" } | null>(null);
  let notes = $state<string[]>([]);
  let file: HTMLInputElement;

  const exists = (name: string) => profiles.some((p) => p.name === name.trim());

  async function saveAs() {
    const name = newName.trim();
    if (!name) return;
    if (exists(name) && !window.confirm(`Replace the profile "${name}" with the current setup?`)) return;
    if (await daemon.run(() => api.saveProfile(name))) newName = "";
  }

  async function apply(name: string) {
    const r = await daemon.run(() => api.applyProfile(name));
    notes = r?.notes ?? [];
  }

  async function finishEdit() {
    if (!editing) return;
    const { name, to, kind } = editing;
    if (!to.trim()) return;
    const ok =
      kind === "rename"
        ? await daemon.run(() => api.renameProfile(name, to.trim()))
        : await daemon.run(() => api.duplicateProfile(name, to.trim()));
    if (ok) editing = null;
  }

  async function remove(name: string) {
    if (!window.confirm(`Delete the profile "${name}"?`)) return;
    await daemon.run(() => api.deleteProfile(name));
  }

  async function exportProfile(name: string) {
    const doc = await daemon.run(() => api.getProfile(name));
    if (!doc) return;
    const blob = new Blob([JSON.stringify(doc, null, 2)], { type: "application/json" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = `${name.replace(/[\\/:*?"<>|]/g, "_")}.lightloom-profile.json`;
    a.click();
    URL.revokeObjectURL(a.href);
  }

  async function importProfile() {
    const f = file.files?.[0];
    file.value = "";
    if (!f) return;
    let doc: unknown;
    try {
      doc = JSON.parse(await f.text());
    } catch {
      daemon.error = `${f.name} is not a JSON file`;
      return;
    }
    await daemon.run(() => api.importProfile(doc));
  }

  const when = (t: string) => (t ? new Date(t).toLocaleString() : "");
  const effectTitle = (id: string | null | undefined) =>
    id ? (snapshot.effects.find((e) => e.id === id)?.title ?? id) : "—";
</script>

<section class="panel">
  <div class="row head">
    <h2>Profiles</h2>
    <span class="muted"
      >A profile holds the whole lighting setup: the effect and its values, the brightness, the layout and each
      device's settings (including its hardware mode).</span
    >
  </div>

  <div class="row save">
    <input
      type="text"
      placeholder="Profile name"
      maxlength="100"
      bind:value={newName}
      onkeydown={(e) => e.key === "Enter" && void saveAs()}
    />
    <button class="primary" disabled={!newName.trim()} onclick={saveAs}
      >{exists(newName) ? "Replace with the current setup" : "Save the current setup"}</button
    >
    <span class="spacer"></span>
    <button onclick={() => file.click()}>Import…</button>
    <input type="file" accept="application/json,.json" bind:this={file} onchange={importProfile} hidden />
  </div>

  {#if notes.length}
    <div class="notes">
      {#each notes as n (n)}<p class="muted small">{n}</p>{/each}
    </div>
  {/if}

  <div class="list">
    {#each profiles as p (p.name)}
      <div class="row item" class:active={current === p.name}>
        <div class="name">
          {#if editing?.name === p.name}
            <input
              type="text"
              maxlength="100"
              bind:value={editing.to}
              onkeydown={(e) => {
                if (e.key === "Enter") void finishEdit();
                if (e.key === "Escape") editing = null;
              }}
            />
          {:else}
            <strong>{p.name}</strong>
            <span class="muted small"
              >{effectTitle(p.effect)} · layout {p.layout} · {p.devices} {p.devices === 1 ? "device" : "devices"} · saved {when(
                p.updatedAt,
              )}</span
            >
          {/if}
        </div>
        {#if editing?.name === p.name}
          <button disabled={!editing.to.trim()} onclick={finishEdit}>{editing.kind === "rename" ? "Rename" : "Duplicate"}</button>
          <button onclick={() => (editing = null)}>Cancel</button>
        {:else}
          <label class="default" title="Apply this profile when Lightloom starts"
            ><input
              type="radio"
              name="default-profile"
              checked={defaultProfile === p.name}
              onchange={() => daemon.run(() => api.setDefaultProfile(p.name))}
            /> At start</label
          >
          <button class:primary={current !== p.name} onclick={() => apply(p.name)}>Apply</button>
          <button title="Replace with the current setup" onclick={() => {
            if (window.confirm(`Replace the profile "${p.name}" with the current setup?`)) void daemon.run(() => api.saveProfile(p.name));
          }}>Update</button>
          <button onclick={() => (editing = { name: p.name, to: p.name, kind: "rename" })}>Rename</button>
          <button onclick={() => (editing = { name: p.name, to: `${p.name} copy`, kind: "duplicate" })}>Duplicate</button>
          <button onclick={() => exportProfile(p.name)}>Export</button>
          <button class="x" title="Delete" onclick={() => remove(p.name)}>×</button>
        {/if}
      </div>
    {:else}
      <p class="muted">No profile yet: set up the lighting, then save it here.</p>
    {/each}
  </div>

  {#if profiles.length}
    <label class="row default-none"
      ><input
        type="radio"
        name="default-profile"
        checked={!defaultProfile}
        onchange={() => daemon.run(() => api.setDefaultProfile(null))}
      /> At start, keep the setup as it was left</label
    >
  {/if}
</section>

<style>
  .head h2 {
    margin: 0;
  }
  .save {
    margin: 10px 0;
  }
  .save input {
    min-width: 240px;
  }
  .spacer {
    flex: 1;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .item {
    align-items: center;
    padding: 6px 8px;
    border: 1px solid var(--line);
    border-radius: 6px;
  }
  .item.active {
    border-color: var(--accent);
  }
  .item .name {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .item .name strong,
  .item .name span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .small {
    font-size: 12px;
    margin: 2px 0;
  }
  .default {
    font-size: 12px;
    display: flex;
    gap: 4px;
    align-items: center;
  }
  .default-none {
    margin-top: 10px;
    font-size: 12px;
  }
  .x {
    padding: 6px 8px;
  }
</style>
