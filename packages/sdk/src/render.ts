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
  if (typeof VideoEncoder === "undefined") {
    throw new Error("zoetrope: WebCodecs is required in this browser (see README#browser-support)");
  }

  const { fps, bitrate = 6_000_000, assets, onProgress } = opts;

  const engine = await loadEngine(build(d), assets ?? new Map());
  try {
    const { width, height } = engine;
    const total = Math.ceil(engine.durationTicks / (TIMEBASE / fps));

    const muxer = new Muxer({
      target: new ArrayBufferTarget(),
      video: { codec: "avc", width, height },
      fastStart: "in-memory",
    });

    const encoder = new VideoEncoder({
      output: (chunk, meta) => muxer.addVideoChunk(chunk, meta),
      error: (e) => {
        throw e;
      },
    });

    let config: VideoEncoderConfig | undefined;
    for (const codec of CODEC_CANDIDATES) {
      const candidate: VideoEncoderConfig = { codec, width, height, bitrate, framerate: fps };
      if ((await VideoEncoder.isConfigSupported(candidate)).supported) {
        config = candidate;
        break;
      }
    }
    if (config === undefined) {
      throw new Error("zoetrope: no supported H.264 encoder config");
    }
    encoder.configure(config);

    for (let n = 0; n < total; n++) {
      const rgba = engine.renderRGBA(engine.tickForFrame(n, fps));
      const frame = new VideoFrame(rgba, {
        format: "RGBA",
        codedWidth: width,
        codedHeight: height,
        timestamp: Math.round((n * 1_000_000) / fps),
      });
      encoder.encode(frame, { keyFrame: n % KEYFRAME_INTERVAL === 0 });
      frame.close();

      if (encoder.encodeQueueSize > MAX_QUEUED_FRAMES) {
        await new Promise<void>((resolve) =>
          encoder.addEventListener("dequeue", () => resolve(), { once: true }),
        );
      }
      onProgress?.(n + 1, total);
    }

    await encoder.flush();
    encoder.close();
    muxer.finalize();

    return new Blob([muxer.target.buffer], { type: "video/mp4" });
  } finally {
    engine.dispose();
  }
}
