<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!-- Settings → Updates: a manual check, the opt-in weekly check and beta channel, the
     release notes (plain text) and the verified download. -->
<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { daemon } from "../lib/store.svelte";
  import { isReleaseUrl, updates, type UpdateView } from "../lib/update";

  let view = $state<UpdateView | null>(null);
  let busy = $state(false);
  let timer: ReturnType<typeof setInterval> | undefined;

  async function load(call: () => Promise<UpdateView>) {
    const v = await daemon.run(call);
    if (v) view = v;
    follow();
  }

  // While a download runs, read its progress every second.
  function follow() {
    const running = view?.job?.running ?? false;
    if (running && !timer) {
      timer = setInterval(async () => {
        try {
          view = await updates.get();
        } catch {
          // The installer may already have stopped the daemon.
        }
        if (!view?.job?.running) {
          clearInterval(timer);
          timer = undefined;
        }
      }, 1000);
    }
  }

  onMount(() => void load(updates.get));
  onDestroy(() => clearInterval(timer));

  async function check() {
    busy = true;
    await load(updates.check);
    busy = false;
  }

  const release = $derived(view?.available ?? null);
  const job = $derived(view?.job?.version === release?.version ? (view?.job ?? null) : null);
  const pageUrl = $derived(release && isReleaseUrl(release.url) ? release.url : null);
  const mb = (n: number) => `${(n / 1048576).toFixed(1)} MB`;
  const when = (secs: number) => new Date(secs * 1000).toLocaleString();
</script>

<section class="panel">
  <h2>Updates</h2>
  {#if view}
    <p class="muted">
      Lightloom {view.currentVersion}{view.lastCheck ? ` · last checked ${when(view.lastCheck)}` : " · not checked yet"}
    </p>
    <div class="row">
      <button disabled={busy || view.checking} onclick={check}>{busy ? "Checking…" : "Check for updates"}</button>
    </div>
    {#if view.error}
      <p class="failed">{view.error}</p>
    {:else if view.lastCheck && !release}
      <p class="message">Lightloom is up to date.</p>
    {/if}

    {#if release}
      <div class="release">
        <p>
          <strong>Lightloom {release.version}</strong> is available{release.prerelease ? " (pre-release)" : ""}{release.publishedAt
            ? ` · ${new Date(release.publishedAt).toLocaleDateString()}`
            : ""}
        </p>
        {#if release.notes}
          <pre class="notes">{release.notes}</pre>
        {/if}
        <div class="row">
          {#if view.installable && release.asset}
            <button class="primary" disabled={job?.running} onclick={() => load(updates.install)}
              >Download and install ({mb(release.asset.size)})</button
            >
          {:else if pageUrl}
            <a class="button primary" href={pageUrl} target="_blank" rel="noopener noreferrer"
              >Download{release.asset ? ` ${release.asset.name}` : ""} from the release page</a
            >
          {/if}
          {#if pageUrl}
            <a href={pageUrl} target="_blank" rel="noopener noreferrer">Release page</a>
          {/if}
        </div>
        {#if !view.installable && view.installNote}
          <p class="muted">{view.installNote}</p>
        {/if}
        {#if job}
          <p class:message={job.ok === true} class:failed={job.ok === false}>
            {job.running && job.total ? `${job.message} ${mb(job.downloaded)} of ${mb(job.total)}` : job.message}
          </p>
        {/if}
      </div>
    {/if}

    <label class="check"
      ><input
        type="checkbox"
        checked={view.checkWeekly}
        onchange={(e) => load(() => updates.setOptions({ checkWeekly: (e.target as HTMLInputElement).checked }))}
      /> Check once a week</label
    >
    <label class="check"
      ><input
        type="checkbox"
        checked={view.beta}
        onchange={(e) => load(() => updates.setOptions({ beta: (e.target as HTMLInputElement).checked }))}
      /> Include pre-releases (beta channel)</label
    >
    <p class="muted">
      A check is one anonymous request to GitHub's release API (api.github.com) with no account, cookie or identifier;
      nothing is checked unless you press the button or turn the weekly check on. Downloads start only when you click
      them and are checked against the release's SHA-256 before the installer opens.
    </p>
  {:else}
    <p class="muted">Loading…</p>
  {/if}
</section>

<style>
  .message {
    color: var(--good);
  }
  .failed {
    color: var(--bad);
  }
  .release {
    margin: 12px 0;
  }
  .notes {
    max-height: 240px;
    overflow: auto;
    font-size: 12px;
    white-space: pre-wrap;
    margin: 0 0 8px;
  }
  a.button {
    display: inline-block;
    padding: 6px 12px;
    border-radius: 6px;
    text-decoration: none;
    color: inherit;
    background: var(--accent);
  }
</style>
