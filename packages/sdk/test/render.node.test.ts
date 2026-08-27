import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Regression coverage for the encoder-error-propagation bug fixed in
// render.ts: `VideoEncoder`'s `error` callback fires asynchronously (on
// the browser's internal codec task queue), not synchronously from any
// call `render()` makes. The original `error: (e) => { throw e }` threw
// into the void and never rejected `render()`'s promise — and if the
// error fired while the loop was `await`ing the "dequeue" backpressure
// event, no further "dequeue" would ever arrive, so `render()` would hang
// forever with `engine.dispose()` never called.
//
// Exercising this against a real browser's WebCodecs stack isn't
// practical (Playwright has no supported way to force
// `VideoEncoder`'s internal encode path to fail on demand), so this test
// stubs a minimal `VideoEncoder`/`VideoFrame` pair that reproduces the
// exact failure shape: an asynchronous `error` callback that fires while
// the backpressure `await` is pending and never dispatches "dequeue".
// `loadEngine` and `mp4-muxer` are mocked out too, so this is a narrow,
// fast unit test of render.ts's error-routing control flow, not an
// integration test — the real encode/mux path is covered by
// test-browser/render.spec.ts.
const disposeSpy = vi.fn();

vi.mock("../src/engine", () => ({
  loadEngine: vi.fn(async () => ({
    width: 4,
    height: 4,
    // 6 frames at 30fps — enough to cross MAX_QUEUED_FRAMES (4) at least
    // once so the backpressure `await` path in render.ts actually runs.
    durationTicks: 6 * (705_600_000 / 30),
    tickForFrame: (n: number, fps: number) => n * (705_600_000 / fps),
    renderRGBA: () => new Uint8Array(4 * 4 * 4),
    dispose: disposeSpy,
  })),
}));

vi.mock("mp4-muxer", () => ({
  // `render.ts` calls `new Muxer(...)`; a mock backed by an arrow function
  // isn't a valid constructor ("X is not a constructor"), so this needs a
  // real `function` implementation.
  Muxer: vi.fn().mockImplementation(function Muxer() {
    return {
      addVideoChunk: vi.fn(),
      finalize: vi.fn(),
      target: { buffer: new ArrayBuffer(8) },
    };
  }),
  ArrayBufferTarget: vi.fn(),
}));

// Imported after the mocks above so render.ts picks up the mocked
// ../src/engine and mp4-muxer modules.
const { render } = await import("../src/render");
const { doc, rect, scene } = await import("../src/builders");

type EncoderInit = {
  output: (chunk: unknown, meta: unknown) => void;
  error: (e: unknown) => void;
};

/** Minimal WebCodecs `VideoEncoder` stand-in. `encode()` fires `init.error`
 * asynchronously (via `queueMicrotask`) on the 5th call — by then
 * `encodeQueueSize` (5) exceeds render.ts's `MAX_QUEUED_FRAMES` (4), so
 * the render loop is already inside its `await Promise.race([dequeueWait,
 * errorSignal])` backpressure wait by the time the microtask runs.
 * `addEventListener("dequeue", ...)` deliberately never invokes its
 * callback — without the fix, that `await` would never resolve. */
class FakeVideoEncoder {
  static async isConfigSupported() {
    return { supported: true };
  }
  state: "unconfigured" | "configured" | "closed" = "unconfigured";
  encodeQueueSize = 0;
  #encodeCalls = 0;
  constructor(private init: EncoderInit) {}
  configure() {
    this.state = "configured";
  }
  encode() {
    this.encodeQueueSize++;
    this.#encodeCalls++;
    if (this.#encodeCalls === 5) {
      queueMicrotask(() => {
        this.state = "closed";
        this.init.error(new Error("boom"));
      });
    }
  }
  addEventListener(_event: string, _cb: () => void) {
    // Never fires "dequeue" — see class comment.
  }
  async flush() {}
  close() {
    this.state = "closed";
  }
}

class FakeVideoFrame {
  constructor(_data: unknown, _init: unknown) {}
  close() {}
}

describe("render() fps guard", () => {
  // No `globalThis.VideoEncoder` is stubbed for this describe block — these
  // assertions only hold if the fps guard runs and throws *before*
  // render.ts reaches the `typeof VideoEncoder === "undefined"` capability
  // check, which is exactly the ordering being verified here (a plain Node
  // environment has no VideoEncoder at all).
  it("rejects fps 0 with a message mentioning fps and the timebase", async () => {
    const d = doc({ w: 4, h: 4 });
    d.scenes = [scene("a", 705_600_000, [rect([0, 0, 4, 4], "#ff0000")])];

    await expect(render(d, { fps: 0 })).rejects.toThrow(
      "zoetrope: unsupported fps 0: must divide 705600000",
    );
  });

  it("rejects a non-divisor fps like 23", async () => {
    const d = doc({ w: 4, h: 4 });
    d.scenes = [scene("a", 705_600_000, [rect([0, 0, 4, 4], "#ff0000")])];

    await expect(render(d, { fps: 23 })).rejects.toThrow(
      "zoetrope: unsupported fps 23: must divide 705600000",
    );
  });
});

describe("render() encoder error propagation", () => {
  const realVideoEncoder = globalThis.VideoEncoder;
  const realVideoFrame = globalThis.VideoFrame;

  beforeEach(() => {
    disposeSpy.mockClear();
    globalThis.VideoEncoder = FakeVideoEncoder as unknown as typeof VideoEncoder;
    globalThis.VideoFrame = FakeVideoFrame as unknown as typeof VideoFrame;
  });

  afterEach(() => {
    globalThis.VideoEncoder = realVideoEncoder;
    globalThis.VideoFrame = realVideoFrame;
  });

  it("rejects (instead of hanging) when the encoder errors mid-backpressure-wait, and still disposes the engine", async () => {
    const fps = 30;
    const frameTicks = 705_600_000 / fps;
    const d = doc({ w: 4, h: 4 });
    d.scenes = [scene("a", 6 * frameTicks, [rect([0, 0, 4, 4], "#ff0000")])];

    await expect(render(d, { fps })).rejects.toThrow("boom");
    expect(disposeSpy).toHaveBeenCalledOnce();
  });
});
