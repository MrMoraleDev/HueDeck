<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!-- A small live picture of a device's LCD face: GET /v1/devices/{key}/lcd/preview about twice a
     second, only while the picture is on screen and the page is visible (PreviewLoop). -->
<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { PreviewLoop } from "../lib/preview-loop";

  let { deviceKey, width, height, circular }: { deviceKey: string; width: number; height: number; circular: boolean } =
    $props();

  /** Frames per second of the preview. */
  const EVERY = 500;
  /** Longest side on the page, in CSS pixels. */
  const SIDE = 96;

  let box: HTMLElement | undefined = $state();
  let src = $state<string | null>(null);
  const scale = $derived(Math.min(1, SIDE / Math.max(width, height, 1)));

  onMount(() => {
    const loop = new PreviewLoop({
      every: EVERY,
      load: () => api.lcdPreview(deviceKey),
      show: (url) => (src = url),
    });
    let onScreen = false;
    const follow = () => {
      if (onScreen && document.visibilityState === "visible") loop.start();
      else loop.stop();
    };
    const seen = new IntersectionObserver((entries) => {
      onScreen = entries.some((e) => e.isIntersecting);
      follow();
    });
    if (box) seen.observe(box);
    document.addEventListener("visibilitychange", follow);
    return () => {
      seen.disconnect();
      document.removeEventListener("visibilitychange", follow);
      loop.dispose();
    };
  });
</script>

<div
  class="preview"
  class:round={circular}
  bind:this={box}
  style:width="{Math.round(width * scale)}px"
  style:height="{Math.round(height * scale)}px"
  title="Live preview of the face"
>
  {#if src}<img {src} alt="LCD face preview" />{/if}
</div>

<style>
  .preview {
    flex: none;
    background: #000;
    border: 1px solid var(--line);
    border-radius: 4px;
    overflow: hidden;
  }
  .round {
    border-radius: 50%;
  }
  img {
    display: block;
    width: 100%;
    height: 100%;
    image-rendering: auto;
  }
</style>
