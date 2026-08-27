// In-browser MP4 export (spec §4.3): drives the wasm engine frame-by-frame,
// feeds raw RGBA into WebCodecs' `VideoEncoder`, and muxes the resulting
// H.264 Annex B chunks into an MP4 container via `mp4-muxer`. Browser-only
// — `VideoEncoder`/`VideoFrame` don't exist under Node, hence the capability
// check up front (the exact error text is part of the public contract, see
// README#browser-support).
import { ArrayBufferTarget, Muxer } from "mp4-muxer";
import { loadEngine } from "./engine";
import { build } from "./canonical";
import { TIMEBASE } from "./time";
import type { ZoeDocument } from "./types";

/** Baseline L4.0 first (broadest hardware/software decoder support), then
 * High L4.0 as a fallback for sizes baseline can't cover. Checked via
 * `VideoEncoder.isConfigSupported` rather than assumed — headless test
 * environments and older browsers vary in which profiles they ship. */
const CODEC_CANDIDATES = ["avc1.420028", "avc1.640028"] as const;

/** Encoder back-pressure: once this many frames are queued, wait for the
 * encoder to drain one before submitting more, so `render` doesn't buffer
 * an unbounded number of in-flight `VideoFrame`s for long documents. */
const MAX_QUEUED_FRAMES = 4;

/** Every Nth frame is forced as a keyframe (in addition to frame 0, which
 * is always a keyframe since `n % 150 === 0` includes `n === 0`) so seeking
 * into a long export doesn't require decoding from the very start. */
const KEYFRAME_INTERVAL = 150;

export interface RenderOptions {
  /** Export frame rate. Must divide `TIMEBASE` evenly for exact tick math
   * (same constraint as `frames(n).at(fps)` in `time.ts`). */
  fps: number;
  /** Target bitrate in bits/sec, passed straight to `VideoEncoder`. */
  bitrate?: number;
  /** Asset bytes keyed by asset id, forwarded to `loadEngine`. */
  assets?: Map<string, Uint8Array>;
  /** Called after each frame is encoded with `(framesDone, totalFrames)`. */
  onProgress?: (done: number, total: number) => void;
}

/**
 * Render `d` to an MP4 `Blob` entirely in-browser via WebCodecs +
 * `mp4-muxer`. Throws if the browser has no `VideoEncoder` (see
 * README#browser-support) or if none of `CODEC_CANDIDATES` is supported.
 */
export async function render(d: ZoeDocument, opts: RenderOptions): Promise<Blob> {
  const { fps, bitrate = 6_000_000, assets, onProgress } = opts;

  // Checked first (before the WebCodecs capability check below) so a bad
  // `fps` is reported as a plain, synchronous, Node-testable error rather
  // than only surfacing once tick math has already gone wrong deep inside
  // the encode loop (`TIMEBASE / fps` — same guard as `frames(n).at(fps)`
  // in time.ts and `Engine::tick_for_frame`/`doc::frames` on the Rust side).
  if (!Number.isInteger(fps) || fps <= 0 || TIMEBASE % fps !== 0) {
    throw new Error(`kineto: unsupported fps ${fps}: must divide ${TIMEBASE}`);
  }

  if (typeof VideoEncoder === "undefined") {
    throw new Error("kineto: WebCodecs is required in this browser (see README#browser-support)");
  }

  const engine = await loadEngine(build(d), assets ?? new Map());
  // Tracked outside the `try` body (but only ever assigned from inside it)
  // so `finally` can close the encoder on every exit path, including a
  // thrown encoder error — see the comment on `errorSignal` below for why
  // that's necessary in the first place.
  let encoder: VideoEncoder | undefined;
  try {
    const { width, height } = engine;
    const total = Math.ceil(engine.durationTicks / (TIMEBASE / fps));

    const muxer = new Muxer({
      target: new ArrayBufferTarget(),
      video: { codec: "avc", width, height },
      fastStart: "in-memory",
    });

    // `VideoEncoder`'s `error` callback fires on the browser's internal
    // codec task queue, not synchronously from any call this function
    // makes — `throw e` there throws into the void and never rejects this
    // `render()` promise. Worse, if it fires while the loop below is
    // `await`ing the "dequeue" backpressure event, the encoder transitions
    // to "closed" and may never dispatch another "dequeue", so that await
    // — and `render()` — would hang forever. Route the error through a
    // rejecting promise instead, raced against every await in this
    // function, so an encoder error always surfaces as a rejection with
    // `finally` (and thus `engine.dispose()`) still running.
    let encodeError: unknown;
    let rejectErrorSignal!: (e: unknown) => void;
    const errorSignal = new Promise<never>((_resolve, reject) => {
      rejectErrorSignal = reject;
    });
    // `errorSignal` is raced below, not directly awaited/returned, so it
    // would otherwise trip "unhandled rejection" detection on the settle
    // path where nothing else observes it (e.g. total === 0). Attaching a
    // handler directly to it marks it handled regardless of whether a
    // given render() call ever races it.
    errorSignal.catch(() => {});

    const enc = new VideoEncoder({
      output: (chunk, meta) => muxer.addVideoChunk(chunk, meta),
      error: (e) => {
        encodeError = e;
        rejectErrorSignal(e);
      },
    });
    encoder = enc;

    let config: VideoEncoderConfig | undefined;
    for (const codec of CODEC_CANDIDATES) {
      const candidate: VideoEncoderConfig = { codec, width, height, bitrate, framerate: fps };
      if ((await VideoEncoder.isConfigSupported(candidate)).supported) {
        config = candidate;
        break;
      }
    }
    if (config === undefined) {
      throw new Error("kineto: no supported H.264 encoder config");
    }
    enc.configure(config);

    for (let n = 0; n < total; n++) {
      // Before each encode(): surface any error the callback recorded
      // since the last check (e.g. during the previous iteration's
      // backpressure wait) instead of queuing more work into a dead
      // encoder.
      if (encodeError !== undefined) throw encodeError;

      const rgba = engine.renderRGBA(engine.tickForFrame(n, fps));
      const frame = new VideoFrame(rgba, {
        format: "RGBA",
        codedWidth: width,
        codedHeight: height,
        timestamp: Math.round((n * 1_000_000) / fps),
      });
      enc.encode(frame, { keyFrame: n % KEYFRAME_INTERVAL === 0 });
      frame.close();

      if (enc.encodeQueueSize > MAX_QUEUED_FRAMES) {
        const dequeueWait = new Promise<void>((resolve) =>
          enc.addEventListener("dequeue", () => resolve(), { once: true }),
        );
        await Promise.race([dequeueWait, errorSignal]);
        if (encodeError !== undefined) throw encodeError;
      }
      onProgress?.(n + 1, total);
    }

    if (encodeError !== undefined) throw encodeError;
    await Promise.race([enc.flush(), errorSignal]);
    if (encodeError !== undefined) throw encodeError;

    enc.close();
    muxer.finalize();

    return new Blob([muxer.target.buffer], { type: "video/mp4" });
  } finally {
    if (encoder !== undefined && encoder.state !== "closed") {
      try {
        encoder.close();
      } catch {
        // Already closing/closed via a path this function doesn't control
        // (e.g. the browser tearing down the tab) — nothing to do.
      }
    }
    engine.dispose();
  }
}
