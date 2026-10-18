// SPDX-License-Identifier: Apache-2.0 OR MIT
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// `npm run build` writes `dist/` (or $LIGHTLOOM_UI_OUT); lightloomd serves it from `ui/`
// next to its executable, or from this tree's `ui/dist` in development builds.
// `npm run dev` proxies the API to a running daemon.
export default defineConfig({
  plugins: [svelte()],
  base: "./",
  build: {
    outDir: process.env.LIGHTLOOM_UI_OUT || "dist",
    emptyOutDir: true,
    target: "es2022",
    sourcemap: false,
  },
  server: {
    proxy: {
      "/v1": {
        target: "http://127.0.0.1:16138",
        ws: true,
        changeOrigin: true,
      },
    },
  },
});
