<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!-- Macros (docs/macros.md): the runtime's switches, the user's Inputs (enable, options, chain of Actions)
     and Actions (options, Run). Loads GET /v1/macros once, then follows the daemon's `macros` events
     (refetching on reconnect and when the page becomes visible); every update is reconciled into the
     view, so only what changed re-renders. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { macrosApi, type MacroOption, type MacroScript, type MacrosView } from "../lib/macros.svelte";
  import { nameChoices } from "../lib/macro-options";
  import { reconcile } from "../lib/reconcile";
  import { daemon } from "../lib/store.svelte";

  let view = $state<MacrosView | null>(null);
  let failed = $state<string | null>(null);
  let notice = $state<string | null>(null);

  const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

  const isView = (v: unknown): v is MacrosView => !!v && typeof v === "object" && "scripts" in v;

  /** Take a new view: reconciled into the one shown (only changed values fire). */
  function apply(next: MacrosView) {
    if (view) reconcile(view, next);
    else view = next;
    if (failed !== null) failed = null;
  }

  async function refresh() {
    if (document.visibilityState !== "visible") return;
    try {
      apply(await macrosApi.get());
    } catch (e) {
      failed = message(e);
    }
  }

  async function change(call: () => Promise<unknown>) {
    try {
      const r = await call();
      notice = null;
      if (isView(r)) apply(r);
    } catch (e) {
      notice = message(e);
      await refresh();
    }
  }

  onMount(() => {
    void refresh();
    const offMacros = daemon.events.on("macros", (data) => {
      if (isView(data)) apply(data);
    });
    // Events missed while disconnected or hidden: refetch.
    const offOpen = daemon.events.on("open", () => void refresh());
    const onVisible = () => void refresh();
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      offMacros();
      offOpen();
      document.removeEventListener("visibilitychange", onVisible);
    };
  });

  const inputs = $derived(view ? view.scripts.filter((s) => s.kind === "input") : []);
  const actions = $derived(view ? view.scripts.filter((s) => s.kind === "action") : []);
  const usable = $derived(actions.filter((a) => !a.error));
  const actionName = (id: string) => actions.find((a) => a.id === id)?.name ?? id;

  function setOption(s: MacroScript, o: MacroOption, value: unknown) {
    void change(() => macrosApi.setScript(s.id, { options: { [o.property]: value } }));
  }

  function addAction(s: MacroScript, id: string) {
    if (id) void change(() => macrosApi.setScript(s.id, { actions: [...s.actions, id] }));
  }

  function removeAction(s: MacroScript, index: number) {
    void change(() => macrosApi.setScript(s.id, { actions: s.actions.filter((_, i) => i !== index) }));
  }

  const checked = (e: Event) => (e.target as HTMLInputElement).checked;
  const text = (e: Event) => (e.target as HTMLInputElement | HTMLSelectElement).value;
</script>

{#snippet optionInput(s: MacroScript, o: MacroOption)}
  <label class="row option" title={o.description ?? ""}>
    <span class="muted">{o.label}</span>
    {#if o.type === "boolean"}
      <input type="checkbox" checked={o.value === true} onchange={(e) => setOption(s, o, checked(e))} />
    {:else if o.type === "combobox"}
      <select value={String(o.value ?? "")} onchange={(e) => setOption(s, o, text(e))}>
        {#each o.values ?? [] as c (c.key)}
          <option value={c.key}>{c.label}</option>
        {/each}
      </select>
    {:else if o.type === "number" || o.type === "hue"}
      <input
        type="number"
        min={o.min}
        max={o.max}
        step={o.step ?? 1}
        value={Number(o.value ?? 0)}
        onchange={(e) => setOption(s, o, Number(text(e)))}
      />
    {:else if o.type === "color"}
      <input type="color" value={String(o.value ?? "#ff0000").slice(0, 7)} onchange={(e) => setOption(s, o, text(e))} />
    {:else if o.type === "layoutname" || o.type === "profilename"}
      {@const names = nameChoices(o.type, o.value, daemon.snapshot) ?? []}
      <select value={String(o.value ?? "")} onchange={(e) => setOption(s, o, text(e))}>
        <option value="">{o.type === "layoutname" ? "Choose a layout…" : "Choose a profile…"}</option>
        {#each names as c (c.name)}
          <option value={c.name}>{c.name}{c.missing ? " (missing)" : ""}</option>
        {/each}
      </select>
    {:else if o.type === "effectpreset" || o.type === "list"}
      <code class="muted">{JSON.stringify(o.value)}</code>
    {:else}
      <input
        type="text"
        value={String(o.value ?? "")}
        pattern={o.filter}
        placeholder={o.type === "keycode" ? "Ctrl + Shift + F1" : o.type === "mousecode" ? "Right Click" : ""}
        onchange={(e) => setOption(s, o, text(e))}
      />
    {/if}
  </label>
{/snippet}

{#snippet script(s: MacroScript)}
  <li class="script">
    <div class="row">
      {#if s.kind === "input"}
        <input
          type="checkbox"
          title="Run this Input"
          checked={s.enabled}
          disabled={!!s.error || !view?.enabled}
          onchange={(e) => change(() => macrosApi.setScript(s.id, { enabled: checked(e) }))}
        />
      {/if}
      <strong>{s.name}</strong>
      {#if s.source === "signalrgb"}<span class="badge">SignalRGB</span>{/if}
      {#if s.running}<span class="badge good">running</span>{/if}
      {#if s.error}<span class="badge bad" title={s.error}>cannot run</span>{/if}
      {#if s.kind === "action"}
        <button disabled={!!s.error || !view?.enabled} onclick={() => change(() => macrosApi.run(s.id))}>Run</button>
      {/if}
    </div>
    {#if s.description}<p class="muted">{s.description}</p>{/if}
    {#if s.error}<p class="muted error">{s.error}</p>{/if}
    {#if s.options.length}
      <div class="options">
        {#each s.options as o (o.property)}
          {@render optionInput(s, o)}
        {/each}
      </div>
    {/if}
    {#if s.kind === "input" && !s.error}
      <div class="row chain">
        <span class="muted">Then run</span>
        {#each s.actions as a, i (i)}
          <span class="badge">{actionName(a)} <button class="link" title="Remove" onclick={() => removeAction(s, i)}>×</button></span>
        {/each}
        <select value="" onchange={(e) => addAction(s, text(e))}>
          <option value="">Add an Action…</option>
          {#each usable as a (a.id)}
            <option value={a.id}>{a.name}</option>
          {/each}
        </select>
      </div>
    {/if}
    {#if s.warnings.length}<p class="muted">{s.warnings.join("; ")}</p>{/if}
  </li>
{/snippet}

<section class="panel macros">
  <div class="row head">
    <h2>Macros</h2>
    {#if view}
      <label class="row"
        ><input
          type="checkbox"
          checked={view.enabled}
          onchange={(e) => change(() => macrosApi.setSettings({ enabled: checked(e) }))}
        /> Run macros</label
      >
    {/if}
  </div>
  {#if failed}
    <p class="muted">Macros are not available: {failed}</p>
  {:else if !view}
    <p class="muted">Loading…</p>
  {:else}
    {#if notice}<div class="banner bad">{notice}</div>{/if}
    <p class="muted">
      Macro scripts are SignalRGB-shaped Inputs (they watch for something) and Actions (they do something). Put
      them in
      {#each view.folders.filter((f) => f.source === "user") as f, i (f.path)}{#if i}{" and "}{/if}<code>{f.path}</code>{/each};
      the folders are watched. Off by default: nothing runs until you turn macros on.
    </p>
    <div class="settings">
      <label class="row"
        ><input
          type="checkbox"
          checked={view.keyboardHook}
          disabled={!view.hooksAvailable}
          onchange={(e) => change(() => macrosApi.setSettings({ keyboardHook: checked(e) }))}
        /> Let macros watch the keyboard</label
      >
      <label class="row"
        ><input
          type="checkbox"
          checked={view.mouseHook}
          disabled={!view.hooksAvailable}
          onchange={(e) => change(() => macrosApi.setSettings({ mouseHook: checked(e) }))}
        /> Let macros watch the mouse buttons</label
      >
      <p class="muted note">
        Privacy: a keyboard or mouse hook sees every key you press and every click, in every program. Lightloom
        installs one only while it is allowed here, macros are on and an enabled Input listens for keys or clicks;
        events are handled in memory and never logged or sent anywhere. An Input may swallow the keys it reacts to.
        {view.emergencyStop} turns all macros off.
        {#if !view.hooksAvailable}Not available on this system.{/if}
        Now: keyboard hook {view.hooks.keyboard ? "on" : "off"}, mouse hook {view.hooks.mouse ? "on" : "off"}.
      </p>
      {#if view.hookError}<p class="muted error">{view.hookError}</p>{/if}
      <label class="row"
        ><input
          type="checkbox"
          checked={view.allowLaunch}
          onchange={(e) => change(() => macrosApi.setSettings({ allowLaunch: checked(e) }))}
        /> Let macros start programs</label
      >
      <p class="muted note">
        <code>system.openApplication</code> starts any program with any arguments; allow it only for scripts you trust.
        Web links open in your browser; other kinds of links are refused.
      </p>
    </div>

    <h3>Inputs</h3>
    {#if inputs.length}
      <ul>
        {#each inputs as s (s.id)}{@render script(s)}{/each}
      </ul>
    {:else}
      <p class="muted">No Input scripts.</p>
    {/if}
    <h3>Actions</h3>
    {#if actions.length}
      <ul>
        {#each actions as s (s.id)}{@render script(s)}{/each}
      </ul>
    {:else}
      <p class="muted">No Action scripts.</p>
    {/if}
  {/if}
</section>

<style>
  .head {
    justify-content: space-between;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .script {
    border-top: 1px solid var(--line);
    padding: 10px 0;
  }
  .script p {
    margin: 4px 0;
  }
  .options {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 16px;
    margin-top: 6px;
  }
  .option {
    gap: 6px;
  }
  .chain {
    margin-top: 6px;
    flex-wrap: wrap;
  }
  .settings {
    margin-bottom: 12px;
  }
  .note {
    font-size: 12px;
    margin: 2px 0 8px 24px;
  }
  .error {
    color: var(--bad);
  }
  button.link {
    border: none;
    background: transparent;
    padding: 0 2px;
  }
  .banner {
    border-radius: 8px;
    padding: 8px 12px;
    margin-bottom: 10px;
    border: 1px solid var(--bad);
    color: var(--bad);
  }
</style>
