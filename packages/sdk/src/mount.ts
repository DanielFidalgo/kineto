// In-browser preview player (spec §4.3): the preview IS the final
// render — same wasm engine, same `renderRGBA` pixels `render.ts` feeds
// to the encoder, painted straight onto a canvas with no scaling (the
// canvas is sized to the document exactly).
import { loadEngine } from "./engine";
import { build } from "./canonical";
import { TIMEBASE } from "./time";
import type { ZoeDocument } from "./types";

export interface MountOptions {
  /** Asset bytes keyed by asset id, forwarded to `loadEngine`. */
  assets?: Map<string, Uint8Array>;
}

export interface Player {
  /** Starts advancing from the current tick in real time (wall clock).
   * No-op if already playing; playback stops on its own at the last
   * tick. Throws after `dispose()`. */
  play(): void;
  /** Freezes playback at the current tick. No-op if not playing. Throws
   * after `dispose()`. */
  pause(): void;
  /** Renders `tick` once (clamped to `[0, durationTicks - 1]`) and stops
   * any in-progress playback. Throws after `dispose()`. */
  seek(tick: number): void;
  /** Stops playback and frees the underlying engine. Safe to call more
   * than once; every other `Player` method throws afterward. */
  dispose(): void;
}

const DISPOSED_ERROR = "zoetrope: player disposed";

/**
 * Mount `d` onto `canvas` for interactive preview: sizes the canvas to
 * the document and returns a `Player` to drive it.
 *
 * Preview purity note: the wall clock (`performance.now()`) only decides
 * *which* tick is currently shown while playing — that's a host/UI
 * concern, not part of rendering. Every painted frame, whether from
 * `play()`'s loop or a `seek()`, still goes through the same pure
 * `(doc, tick) -> pixels` path (`engine.renderRGBA(tick)`) that `render()`
 * uses for export — the determinism rule (spec §5) is about that
 * function, and it never sees wall-clock time.
 */
export async function mount(
  canvas: HTMLCanvasElement,
  d: ZoeDocument,
  opts: MountOptions = {},
): Promise<Player> {
  const engine = await loadEngine(build(d), opts.assets ?? new Map());
  const { width, height, durationTicks } = engine;

  canvas.width = width;
  canvas.height = height;
  const ctx2d = canvas.getContext("2d");
  if (ctx2d === null) {
    throw new Error("zoetrope: canvas 2D context unavailable");
  }
  // Rebound to a `const` of the non-null type so TS's null-narrowing
  // holds inside the closures below (`paint`'s narrowing of `ctx2d`
  // itself doesn't survive into nested function bodies).
  const ctx: CanvasRenderingContext2D = ctx2d;

  let disposed = false;
  let rafId: number | undefined;
  let currentTick = 0;

  function assertNotDisposed(): void {
    if (disposed) {
      throw new Error(DISPOSED_ERROR);
    }
  }

  function clampTick(tick: number): number {
    return Math.min(Math.max(tick, 0), durationTicks - 1);
  }

  // The one and only place pixels reach the canvas — every call site
  // (initial mount, seek, and the play() loop below) is a call to the
  // same pure render(tick), per the purity note above.
  function paint(tick: number): void {
    const rgba = engine.renderRGBA(tick);
    // Copies `rgba`'s bytes into a fresh `Uint8ClampedArray` — simpler
    // and more portable than reusing `rgba.buffer` directly, whose type
    // (`ArrayBufferLike`, i.e. possibly `SharedArrayBuffer`) `ImageData`
    // doesn't accept.
    const imageData = new ImageData(new Uint8ClampedArray(rgba), width, height);
    ctx.putImageData(imageData, 0, 0);
  }

  function stopLoop(): void {
    if (rafId !== undefined) {
      cancelAnimationFrame(rafId);
      rafId = undefined;
    }
  }

  paint(currentTick); // show the first frame immediately on mount

  return {
    play(): void {
      assertNotDisposed();
      if (rafId !== undefined) return; // already playing

      const start = currentTick;
      const t0 = performance.now();

      const step = (): void => {
        const elapsedTicks = ((performance.now() - t0) / 1000) * TIMEBASE;
        const tick = Math.min(start + elapsedTicks, durationTicks - 1);
        currentTick = tick;
        paint(tick);
        if (tick >= durationTicks - 1) {
          rafId = undefined;
          return; // reached the end — stop on our own, no pause() needed
        }
        rafId = requestAnimationFrame(step);
      };

      rafId = requestAnimationFrame(step);
    },

    pause(): void {
      assertNotDisposed();
      stopLoop();
    },

    seek(tick: number): void {
      assertNotDisposed();
      stopLoop();
      currentTick = clampTick(tick);
      paint(currentTick);
    },

    dispose(): void {
      if (disposed) return; // idempotent
      disposed = true;
      stopLoop();
      engine.dispose();
    },
  };
}
