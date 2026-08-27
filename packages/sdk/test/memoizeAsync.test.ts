import { describe, expect, it, vi } from "vitest";
import { memoizeAsync } from "../src/engine";

// Regression coverage for the `wasmInit` poisoning bug: `ensureWasmInit`
// in engine.ts is `memoizeAsync(() => init())`. Testing the real wasm
// `init()` failing-then-succeeding would require heavy mocking of the
// wasm-bindgen module; `memoizeAsync` is a small, pure, generic seam that
// isolates exactly the caching behavior responsible for the bug, so it's
// tested directly instead.
describe("memoizeAsync", () => {
  it("caches a resolved value across calls", async () => {
    const fn = vi.fn(async () => 42);
    const memoized = memoizeAsync(fn);

    await expect(memoized()).resolves.toBe(42);
    await expect(memoized()).resolves.toBe(42);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("shares one in-flight call across concurrent callers", async () => {
    const fn = vi.fn(async () => "value");
    const memoized = memoizeAsync(fn);

    const [a, b] = await Promise.all([memoized(), memoized()]);

    expect(a).toBe("value");
    expect(b).toBe("value");
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("does not poison future calls after a rejection (transient failure recovers)", async () => {
    let attempt = 0;
    const fn = vi.fn(async () => {
      attempt += 1;
      if (attempt === 1) {
        throw new Error("transient failure");
      }
      return "ok";
    });
    const memoized = memoizeAsync(fn);

    await expect(memoized()).rejects.toThrow("transient failure");
    await expect(memoized()).resolves.toBe("ok");
    await expect(memoized()).resolves.toBe("ok");
    expect(fn).toHaveBeenCalledTimes(2);
  });
});
