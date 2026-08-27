import { defineConfig } from "vite";

// Dev server for the Task 19 browser test harness (test-browser/). Not used
// for building/publishing the SDK (spec §8: no bundler for the library
// itself — consumers import ./src/index.ts directly).
export default defineConfig({
  server: {
    port: 5199,
    strictPort: true,
    fs: {
      // engine.ts imports the wasm-pack "web" target build by relative path
      // from crates/wasm/pkg, which lives outside this package (repo root
      // -> crates/wasm/pkg). Vite's dev server refuses to serve files
      // outside its detected workspace root by default, so widen the
      // allow-list to the whole repo root.
      allow: [new URL("../..", import.meta.url).pathname],
    },
  },
});
