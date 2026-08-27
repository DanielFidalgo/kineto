import { readFile } from "node:fs/promises";
import { beforeAll, describe, expect, it } from "vitest";
import { loadEngine } from "../src/engine";

// Smoke test against the real wasm-pack "web" target build in
// crates/wasm/pkg (Task 15's output). engine.ts's real behavioral
// coverage is Task 19's browser tests; this is a best-effort node check
// per the Task 17 brief.
//
// The "web" target's default init() does `fetch(new URL('zoetrope_wasm_bg.wasm',
// import.meta.url))`, and Node's built-in `fetch` does not implement the
// `file:` scheme ("TypeError: fetch failed" / "not implemented... yet...").
// engine.ts itself stays browser-idiomatic (no node-specific branch in its
// wasm init path — only the font loader branches on `isNode`), so this test
// patches `globalThis.fetch` to serve `file:` URLs via `node:fs` for the
// duration of this file only. That is a test-only shim, not a statement
// that engine.ts works unmodified under Node — see the Task 17 report.
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
const DOC = JSON.stringify({
  v: 1,
  timebase: 705_600_000,
  size: { w: 64, h: 64 },
  scenes: [
    {
      id: "s1",
      duration: 705_600_000,
      elements: [
        { type: "rect", rect: [0, 0, 32, 32], fill: "#ff0000" },
        { type: "rect", rect: [32, 32, 32, 32], fill: "#00ff00" },
      ],
    },
  ],
});

describe("loadEngine (node smoke test)", () => {
  it("renders a frame of the expected byte length", async () => {
    const handle = await loadEngine(DOC, new Map());
    try {
      const frame = handle.renderRGBA(0);
      expect(frame.length).toBe(handle.width * handle.height * 4);
    } finally {
      handle.dispose();
    }
  });
});
