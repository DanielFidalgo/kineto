// Perf diagnostic (spec §6: "measured, not asserted") for the flagship
// tape demo's render loop, isolated from WebCodecs encode. Task 25 found
// the demo's full export (render + encode + mux) ran at ~0.57x realtime
// in headless Chromium; this bench times *only* the engine's
// `renderRGBA` calls (the same loop `render.ts` runs, minus
// `VideoEncoder`/`Muxer`) to see how much of that cost is render vs
// encode.
//
// Why this is a vitest test file and not `node bench/render-bench.mjs`
// (as first attempted): packages/sdk's TS sources use extensionless
// relative imports (e.g. `from "./types"` in src/index.ts), which plain
// Node ESM resolution rejects even with `--experimental-strip-types`:
//
//   $ node --experimental-strip-types -e \
//       "import('./src/adapter.ts')"
//   Cannot find module '.../packages/sdk/src/types' imported from
//   .../packages/sdk/src/index.ts
//
// Vitest's transform pipeline already resolves this correctly (it's
// exactly what test/adapter.test.ts and packages/sdk/test/engine.node
// .test.ts rely on), so the bench reuses that pipeline as a vitest test
// file instead of reimplementing module resolution. It is deliberately
// kept out of the normal `npm test` run (excluded in vitest.config.ts)
// and run in isolation via a dedicated `npm run bench` script
// (vitest.bench.config.ts) so it doesn't slow down or add flaky timing
// output to the regular unit-test suite.
import { readdirSync, readFileSync } from "node:fs";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, it } from "vitest";
import { build, loadEngine } from "@kineto/sdk";
import { parseTape } from "../src/adapter";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_DIR = path.resolve(__dirname, "../fixtures/tape-fixture");

const FPS = 30;
const FRAME_COUNT = 90;

// Same Node-only fetch shim as packages/sdk/test/engine.node.test.ts:
// wasm-pack's "web" target init() does
// `fetch(new URL('kineto_wasm_bg.wasm', import.meta.url))`, and Node's
// built-in fetch does not implement the `file:` scheme.
const realFetch = globalThis.fetch;
beforeAll(() => {
  globalThis.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = input instanceof Request ? input.url : input.toString();
    if (url.startsWith("file:")) {
      const bytes = await readFile(new URL(url));
      return new Response(bytes, {
        headers: { "Content-Type": "application/wasm" },
      });
    }
    return realFetch(input, init);
  }) as typeof fetch;
});

function loadFixtureFiles(dir: string): Map<string, Uint8Array> {
  const files = new Map<string, Uint8Array>();
  for (const name of readdirSync(dir)) {
    files.set(name, new Uint8Array(readFileSync(path.join(dir, name))));
  }
  return files;
}

describe("render-bench (perf diagnostic, not a gate)", () => {
  it(`renders ${FRAME_COUNT} frames of the fixture tape at ${FPS}fps, render-only`, async () => {
    const { doc, assets } = parseTape(loadFixtureFiles(FIXTURE_DIR));
    const docJson = build(doc);
    const handle = await loadEngine(docJson, assets);
    try {
      const perFrameMs: number[] = [];
      for (let n = 0; n < FRAME_COUNT; n++) {
        const tick = handle.tickForFrame(n, FPS);
        const t0 = performance.now();
        handle.renderRGBA(tick);
        perFrameMs.push(performance.now() - t0);
      }

      const firstFrameMs = perFrameMs[0]!;
      const steadyMs = perFrameMs.slice(1);
      const steadyAvgMs = steadyMs.reduce((a, b) => a + b, 0) / steadyMs.length;
      const totalMs = perFrameMs.reduce((a, b) => a + b, 0);
      const overallMsPerFrame = totalMs / FRAME_COUNT;
      const renderOnlyFps = 1000 / overallMsPerFrame;
      const videoSeconds = FRAME_COUNT / FPS;
      const renderWallSeconds = totalMs / 1000;
      const realtimeRatio = videoSeconds / renderWallSeconds;

      // Printed (not asserted) per spec §6 — regressions are visible in
      // CI logs, not gated. The first-frame/steady-state split isolates
      // one-time setup cost (e.g. first-use shaping caches) from
      // per-frame text re-shaping, per Task 25's open perf question.
      console.log(`\nrender-bench: ${FRAME_COUNT} frames @ ${FPS}fps, render-only (no encode)`);
      console.log(`  first frame:                       ${firstFrameMs.toFixed(2)}ms`);
      console.log(
        `  steady-state (frames 2-${FRAME_COUNT}): avg ${steadyAvgMs.toFixed(2)}ms/frame`,
      );
      console.log(
        `  overall:                           ${overallMsPerFrame.toFixed(2)}ms/frame, ${renderOnlyFps.toFixed(1)} fps render-only`,
      );
      console.log(
        `  render-only realtime ratio @${FPS}fps:    ${realtimeRatio.toFixed(2)}x`,
      );
    } finally {
      handle.dispose();
    }
  });
});
