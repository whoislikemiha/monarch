import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";
import path from "path";

export default defineConfig(({ mode }) => ({
  plugins: [svelte()],
  resolve: {
    alias: {
      $lib: path.resolve("./src/lib"),
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    hmr: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  // Dev-only desync indicator (MON-14 Phase 2). Defaults ON in `vite dev`
  // unless the user has exported VITE_MONARCH_DEBUG_DESYNC themselves. The
  // define block is omitted in production builds, so the flag is undefined
  // there and the badge is never rendered.
  define:
    mode === "development"
      ? {
          "import.meta.env.VITE_MONARCH_DEBUG_DESYNC": JSON.stringify(
            process.env.VITE_MONARCH_DEBUG_DESYNC ?? "true",
          ),
        }
      : {},
  build: {
    target: "esnext",
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_DEBUG,
  },
}));
