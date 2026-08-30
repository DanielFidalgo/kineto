import { defineConfig } from "vite";

// The public playground: edit a document, watch it compile, export an MP4.
//
// Mirrors packages/demo-tape's config. The SDK imports the wasm-pack "web"
// build from crates/wasm/pkg by relative path, which is outside this package,
// so the dev server's file-serving allow-list is widened to the repo root.
export default defineConfig({
  // Relative so the built site works from any path — a project Pages site, a
  // subdirectory, or `vite preview` — without hard-coding a repository name.
  base: "./",
  build: {
    // The wasm engine is one large chunk; splitting it buys nothing.
    chunkSizeWarningLimit: 1500,
  },
  server: {
    port: 5201,
    strictPort: true,
    fs: {
      allow: [new URL("../..", import.meta.url).pathname],
    },
  },
});
