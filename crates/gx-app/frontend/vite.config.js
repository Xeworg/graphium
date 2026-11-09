import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// The build is embedded into the binary by `tauri::generate_context!()` and
// served by the `tauri://` asset protocol from the root of `dist`, so the
// URLs inside `index.html` are relative: an absolute `/assets/...` would ask
// for a path outside the embedded directory.
export default defineConfig({
  plugins: [svelte()],
  base: "./",
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
