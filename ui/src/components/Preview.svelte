<!-- SPDX-License-Identifier: GPL-3.0-or-later -->
<!-- The live canvas from /v1/preview (BGRA frames with a width/height header). -->
<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { api, wsUrl } from "../lib/api";

  let {
    width = 480,
    tappable = true,
    fill = false,
  }: { width?: number; tappable?: boolean; fill?: boolean } = $props();
  let canvas: HTMLCanvasElement;
  let ws: WebSocket | null = null;
  let closed = false;
  // Frames are drawn once per animation frame (the latest wins), into one reused
  // ImageData: no 256 KB allocation per frame for the garbage collector to pause on.
  let latest: Uint8Array | null = null;
  let raf = 0;
  let image: ImageData | null = null;
  let ctx: CanvasRenderingContext2D | null = null;

  function draw() {
    raf = 0;
    const buf = latest;
    latest = null;
    if (!buf || !canvas) return;
    const w = buf[0] | (buf[1] << 8);
    const h = buf[2] | (buf[3] << 8);
    if (!image || image.width !== w || image.height !== h) image = new ImageData(w, h);
    const px = image.data;
    for (let i = 0, o = 4; i < w * h * 4; i += 4, o += 4) {
      px[i] = buf[o + 2];
      px[i + 1] = buf[o + 1];
      px[i + 2] = buf[o];
      px[i + 3] = 255;
    }
    if (canvas.width !== w) canvas.width = w;
    if (canvas.height !== h) canvas.height = h;
    ctx ??= canvas.getContext("2d");
    ctx?.putImageData(image, 0, 0);
  }

  function open() {
    ws = new WebSocket(wsUrl("/v1/preview?fps=20&step=1"));
    ws.binaryType = "arraybuffer";
    ws.onmessage = (m) => {
      const buf = new Uint8Array(m.data as ArrayBuffer);
      if (buf.length < 4) return;
      const w = buf[0] | (buf[1] << 8);
      const h = buf[2] | (buf[3] << 8);
      if (buf.length < 4 + w * h * 4) return;
      latest = buf;
      if (!raf) raf = requestAnimationFrame(draw);
    };
    ws.onclose = () => {
      if (!closed) setTimeout(open, 2000);
    };
  }

  onMount(open);
  onDestroy(() => {
    closed = true;
    if (raf) cancelAnimationFrame(raf);
    ws?.close();
  });

  function tap(e: MouseEvent) {
    if (!tappable) return;
    const r = canvas.getBoundingClientRect();
    void api.tap(((e.clientX - r.left) / r.width) * 320, ((e.clientY - r.top) / r.height) * 200);
  }
</script>

<canvas
  bind:this={canvas}
  width="320"
  height="200"
  style={fill ? "width:100%;height:100%" : `width:${width}px;height:${(width * 200) / 320}px`}
  onclick={tap}
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
