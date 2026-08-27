import { expect, test } from "@playwright/test";

// Task 19: render() end-to-end through a real browser's WebCodecs stack.
// Runs against the harness page (harness.html/.ts), which exposes the
// SDK's public surface on `window.kineto` — see harness.ts for why.
test.describe("render()", () => {
  test("renders a 15-frame doc to a valid MP4 blob", async ({ page }) => {
    await page.goto("/test-browser/harness.html");
    await page.waitForFunction(() => window.__kinetoReady === true);

    const result = await page.evaluate(async () => {
      const { doc, scene, rect, crossfade, render, TIMEBASE } = window.kineto;
      const fps = 30;
      const frameTicks = TIMEBASE / fps; // 23,520,000

      // Two flat-color rects (no text/fonts — keeps this test font-free)
      // crossfading into each other. 10 + 10 - 5 = 15 frame-units total,
      // so ceil(durationTicks / frameTicks) === 15 exactly.
      const d = doc({ w: 64, h: 64 });
      d.scenes = [
        scene("a", 10 * frameTicks, [rect([0, 0, 64, 64], "#ff0000")]),
        scene("b", 10 * frameTicks, [rect([0, 0, 64, 64], "#00ff00")], crossfade(5 * frameTicks)),
      ];

      const progress: Array<[number, number]> = [];
      const blob = await render(d, {
        fps,
        onProgress: (done, total) => progress.push([done, total]),
      });

      const bytes = new Uint8Array(await blob.arrayBuffer());
      const header = new TextDecoder().decode(bytes.slice(4, 8));

      return { size: blob.size, header, progressCount: progress.length };
    });

    expect(result.size).toBeGreaterThan(0);
    expect(result.header).toBe("ftyp");
    expect(result.progressCount).toBe(15);
  });
});
