<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->
<!-- The live canvas from /v1/preview (BGRA frames with a width/height header). -->
<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { api, wsUrl } from "../lib/api";

  let { width = 480, tappable = true }: { width?: number; tappable?: boolean } = $props();
  let canvas: HTMLCanvasElement;
  let ws: WebSocket | null = null;
  let closed = false;

  function open() {
    ws = new WebSocket(wsUrl("/v1/preview?fps=20&step=1"));
    ws.binaryType = "arraybuffer";
    ws.onmessage = (m) => {
      const buf = new Uint8Array(m.data as ArrayBuffer);
      if (buf.length < 4) return;
      const w = buf[0] | (buf[1] << 8);
      const h = buf[2] | (buf[3] << 8);
      if (buf.length < 4 + w * h * 4) return;
      const img = new ImageData(w, h);
      for (let i = 0, o = 4; i < w * h * 4; i += 4, o += 4) {
        img.data[i] = buf[o + 2];
        img.data[i + 1] = buf[o + 1];
        img.data[i + 2] = buf[o];
        img.data[i + 3] = 255;
      }
      if (canvas.width !== w) canvas.width = w;
      if (canvas.height !== h) canvas.height = h;
      canvas.getContext("2d")?.putImageData(img, 0, 0);
    };
    ws.onclose = () => {
      if (!closed) setTimeout(open, 2000);
    };
  }

  onMount(open);
  onDestroy(() => {
    closed = true;
    ws?.close();
  });

  function tap(e: MouseEvent) {
    if (!tappable) return;
    const r = canvas.getBoundingClientRect();
    void api.tap(((e.clientX - r.left) / r.width) * 320, ((e.clientY - r.top) / r.height) * 200);
  }
</script>

<canvas bind:this={canvas} width="320" height="200" style={`width:${width}px;height:${(width * 200) / 320}px`} onclick={tap}
></canvas>

<style>
  canvas {
    display: block;
    image-rendering: pixelated;
    border-radius: 6px;
    background: #000;
    max-width: 100%;
  }
</style>
