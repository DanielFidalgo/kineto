import type { Ticks } from "./types";

/** Ticks per second. Identical to `TIMEBASE` in `crates/core/src/doc.rs`
 * (`crates/sdk` Task 2's Rust builder) — every SDK must agree on this
 * constant for the cross-SDK golden (spec §5) to hold. */
export const TIMEBASE = 705_600_000;

function assertSafeInteger(value: number, context: string): Ticks {
  if (!Number.isSafeInteger(value)) {
    throw new RangeError(
      `${context} produced a non-safe-integer tick (${value}); document durations must fit in a JS safe integer`,
    );
  }
  return value as Ticks;
}

/**
 * Convert seconds to ticks: `Math.round(s * TIMEBASE)`.
 *
 * Negative inputs are rejected outright rather than rounded: JS's
 * `Math.round` rounds half-values toward `+Infinity` (`Math.round(-0.5)
 * === -0`) while Rust's `f64::round` rounds half-values away from zero
 * (`(-0.5_f64).round() == -1.0`), so the two engines would disagree on
 * ticks for negative durations. v1 has no use for negative durations, so
 * the mismatch is closed off instead of reproduced.
 */
export function seconds(s: number): Ticks {
  if (s < 0) {
    throw new RangeError(
      `seconds(${s}): negative durations are not supported (JS and Rust round negative half-values differently)`,
    );
  }
  return assertSafeInteger(Math.round(s * TIMEBASE), `seconds(${s})`);
}

/** Convert milliseconds to ticks: `m * 705_600` (exact, no rounding — a
 * millisecond always divides the timebase evenly). */
export function ms(m: number): Ticks {
  return assertSafeInteger(m * 705_600, `ms(${m})`);
}

/**
 * `frames(n).at(fps)` converts export frame number `n` to ticks at
 * `fps`, throwing unless `fps` divides `TIMEBASE` evenly (matching the
 * Rust `frames()` helper's assertion) — otherwise the conversion would
 * be inexact and silently lossy.
 */
export function frames(n: number): { at(fps: number): Ticks } {
  return {
    at(fps: number): Ticks {
      if (TIMEBASE % fps !== 0) {
        throw new RangeError(`unsupported fps ${fps}: must divide ${TIMEBASE}`);
      }
      return assertSafeInteger(n * (TIMEBASE / fps), `frames(${n}).at(${fps})`);
    },
  };
}
