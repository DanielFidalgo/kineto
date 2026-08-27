import { defineConfig } from "vitest/config";

// Dedicated config for bench/render-bench.test.ts (Task 26): kept out of
// the normal `npm test` run (see the "bench/**" exclusion in
// vitest.config.ts) and run in isolation via `npm run bench` so its
// timing output isn't interleaved with, or slowed down by, the regular
// unit-test suite. A generous timeout — wasm init + 90 renders is fast in
// practice, but CI runners can be slow/noisy and this is a diagnostic,
// not a gate, so there's no value in a tight timeout failing the step.
export default defineConfig({
  test: {
    include: ["bench/**/*.test.ts"],
    testTimeout: 30_000,
    // Explicit: in a non-TTY shell (every CI runner, and this repo's
    // sandboxed dev shell) vitest's default reporter selection silently
    // drops `console.log` output from passing tests unless "default" is
    // forced — found while validating this bench locally (`npm run
    // bench` printed nothing; `--reporter=default` printed the numbers).
    // The whole point of this bench is numbers in CI logs, so pin it.
    reporters: ["default"],
  },
});
