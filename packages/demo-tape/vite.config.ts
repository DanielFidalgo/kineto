import { defineConfig } from "vite";

// Dev server for the flagship in-browser tape exporter demo (Task 25, spec
// success criterion 1). Mirrors packages/sdk/vite.config.ts: the SDK
// imports the wasm-pack "web" target build from crates/wasm/pkg by
// relative path, which lives outside this package (repo root ->
// crates/wasm/pkg), so Vite's dev-server file-serving allow-list is
// widened to the repo root. The fixture tape under fixtures/tape-fixture/
// is inside this package's root, so it needs no extra allow-listing —
// it's fetched straight off the dev server (see src/main.ts).
export default defineConfig({
  server: {
    port: 5200,
    strictPort: true,
    fs: {
      allow: [new URL("../..", import.meta.url).pathname],
    },
  },
});
