<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!-- A macro's `system.prompt`: one text dialog at a time; Cancel (or Escape) answers null. -->
<script lang="ts">
  import { prompts } from "../lib/macros.svelte";

  const p = $derived(prompts.open[0] ?? null);
  let text = $state("");
  let shown = -1;
  $effect(() => {
    if (p && p.id !== shown) {
      shown = p.id;
      text = "";
    }
  });

  function submit(e: SubmitEvent) {
    e.preventDefault();
    if (p) void prompts.answer(p, text);
  }
</script>

{#if p}
  <div class="backdrop">
    <form class="panel dialog" onsubmit={submit}>
      <h2>{p.title || "Macro"}</h2>
      <p class="muted">{p.script}</p>
      {#if p.message}<p>{p.message}</p>{/if}
      <!-- svelte-ignore a11y_autofocus -->
      <input
        type="text"
        bind:value={text}
        placeholder={p.placeholder}
        autofocus
        onkeydown={(e) => e.key === "Escape" && prompts.answer(p, null)}
      />
      <div class="row buttons">
        <button type="button" onclick={() => prompts.answer(p, null)}>{p.cancelText || "Cancel"}</button>
        <button type="submit" class="primary">{p.confirmText || "OK"}</button>
      </div>
    </form>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 50;
  }
  .dialog {
    min-width: 320px;
    max-width: 480px;
  }
  .dialog input {
    width: 100%;
    box-sizing: border-box;
  }
  .buttons {
    justify-content: flex-end;
    margin-top: 12px;
  }
</style>
