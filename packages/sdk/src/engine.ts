// wasm-bindgen "web" target output from Task 15 (built via `wasm-pack
// build --target web`). Imported by relative path — no publishing to npm
// in v1 (spec §8). `kineto_wasm.js` carries a `@ts-self-types` pragma
// pointing at the sibling `kineto_wasm.d.ts`, which is how TypeScript
// resolves types for this plain relative `.js` import.
import init, { WasmEngine } from "../../../crates/wasm/pkg/kineto_wasm.js";
import type { ZoeDocument } from "./types";

export interface EngineHandle {
  readonly width: number;
  readonly height: number;
  readonly durationTicks: number;
  tickForFrame(n: number, fps: number): number;
  /** Renders `tick` and returns a straight-alpha RGBA8 copy of the frame
   * (`width * height * 4` bytes) — the layout WebCodecs' `VideoFrame`
   * wants for the `RGBA` format. */
  renderRGBA(tick: number): Uint8Array;
  /** Frees the wasm-side `WasmEngine`. The handle must not be used after
   * calling this. */
  dispose(): void;
}

const DEFAULT_FONT_ASSET_ID = "default";
const DEFAULT_FONT_RESERVED_SRC = "kineto:inter";

/**
 * Memoize a zero-arg async factory, but only cache the *resolved* value —
 * a rejection is not cached, so the next call re-invokes `fn` instead of
 * repeating the same failure forever. Exported (but not re-exported from
 * `index.ts`) purely so it can be unit-tested in isolation without
 * touching the real wasm module.
 */
export function memoizeAsync<T>(fn: () => Promise<T>): () => Promise<T> {
  let cached: Promise<T> | undefined;
  return () => {
    if (cached === undefined) {
      cached = fn().catch((err: unknown) => {
        // Clear the cache *before* rethrowing so a transient failure
        // (e.g. a network blip fetching the .wasm) doesn't permanently
        // poison every later call — the next call re-runs `fn`.
        cached = undefined;
        throw err;
      });
    }
    return cached;
  };
}

// `init()` instantiates the wasm module as a side effect; wasm-bindgen
// does not support instantiating it twice, so memoize the promise across
// calls to `loadEngine` (but only on success — see `memoizeAsync`).
const ensureWasmInit = memoizeAsync(() => init());

// Node has no `window`; browsers (including workers) have no
// `process.versions.node`. Simplest reliable discriminator for "are we
// on Node" without pulling in a platform-detection dependency.
const isNode =
  typeof process !== "undefined" && process.versions?.node !== undefined;

/**
 * Bytes for the SDK-bundled Inter font (`packages/sdk/assets/`, copied
 * from `assets/fonts/` in the repo root), resolved relative to this
 * module so it works regardless of the SDK consumer's own working
 * directory.
 */
async function loadBundledInterBytes(): Promise<Uint8Array> {
  const url = new URL("../assets/Inter-Regular.ttf", import.meta.url);
  if (isNode) {
    const { readFile } = await import("node:fs/promises");
    return new Uint8Array(await readFile(url));
  }
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(
      `failed to fetch bundled font at ${url}: ${res.status} ${res.statusText}`,
    );
  }
  return new Uint8Array(await res.arrayBuffer());
}

function needsBundledDefaultFont(
  doc: ZoeDocument,
  assetBytes: Map<string, Uint8Array>,
): boolean {
  if (assetBytes.has(DEFAULT_FONT_ASSET_ID)) {
    return false;
  }
  const asset = doc.assets?.[DEFAULT_FONT_ASSET_ID];
  return (
    asset !== undefined &&
    asset.type === "font" &&
    asset.src === DEFAULT_FONT_RESERVED_SRC
  );
}

/**
 * Instantiate the wasm engine for `docJson`, staging every entry of
 * `assetBytes` (plus the SDK-bundled Inter font for the reserved
 * `"default"` / `"kineto:inter"` asset, when the doc references it and
 * no bytes were supplied) before calling `ready()`.
 *
 * `ready()` is one-shot on the wasm side — calling it a second time
 * errors rather than no-op-ing — so a failed `loadEngine` call is not
 * retried here; callers that want to retry must construct a fresh
 * `WasmEngine` via a new `loadEngine` call.
 *
 * Callers must not use the returned handle's `width`/`height`/etc.
 * before this promise resolves — those wasm methods trap if called
 * before `ready()`, which is exactly what this function guarantees has
 * happened by the time it returns.
 */
export async function loadEngine(
  docJson: string,
  assetBytes: Map<string, Uint8Array>,
): Promise<EngineHandle> {
  await ensureWasmInit();

  const engine = new WasmEngine(docJson);

  for (const [id, bytes] of assetBytes) {
    engine.add_asset(id, bytes);
  }

  const doc = JSON.parse(docJson) as ZoeDocument;
  if (needsBundledDefaultFont(doc, assetBytes)) {
    engine.add_asset(DEFAULT_FONT_ASSET_ID, await loadBundledInterBytes());
  }

  engine.ready();

  return {
    width: engine.width(),
    height: engine.height(),
    durationTicks: engine.duration_ticks(),
    tickForFrame(n: number, fps: number): number {
      return engine.tick_for_frame(n, fps);
    },
    renderRGBA(tick: number): Uint8Array {
      engine.render(tick);
      return engine.frame_unpremultiplied();
    },
    dispose(): void {
      engine.free();
    },
  };
}
