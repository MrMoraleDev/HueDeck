// SPDX-License-Identifier: GPL-3.0-or-later
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// `npm run build` writes `dist/` (or $LIGHTLOOM_UI_OUT); lightloomd serves it from `ui/`
// next to its executable, or from this tree's `ui/dist` in development builds.
// `npm run dev` proxies the API to a running daemon (default port 53000; set
// LIGHTLOOM_DEV_API, e.g. http://127.0.0.1:17000, when it serves elsewhere).
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
        target: process.env.LIGHTLOOM_DEV_API || "http://127.0.0.1:53000",
        ws: true,
        changeOrigin: true,
      },
    },
  },
});
