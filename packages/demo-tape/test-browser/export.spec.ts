import { expect, test } from "@playwright/test";

// Task 25: the flagship demo (spec success criterion 1), exercised
// end-to-end in a real (headless) Chromium — load the fixture tape,
// wait for the preview to paint a real frame, export to MP4, and
// validate the resulting blob. Also measures the wall-clock realtime
// ratio (videoSeconds / exportSeconds) and logs it; spec §6 treats the
// perf floor as measured, not a hard gate, so a sub-1x ratio only logs a
// loud warning rather than failing the test.
test.describe("tape exporter demo", () => {
  test("loads the fixture tape, previews it, and exports a valid MP4", async ({ page }) => {
    await page.goto("/index.html");

    await page.click("#load-fixture");

    // The preview canvas paints the first tape frame (a full-bleed JPEG
    // screenshot over a black scene background) once mount() resolves —
    // poll until it's no longer entirely black.
    await expect
      .poll(
        () =>
          page.evaluate(() => {
            const canvas = document.querySelector<HTMLCanvasElement>("#preview");
            if (canvas === null || canvas.width === 0 || canvas.height === 0) return false;
            const ctx = canvas.getContext("2d");
            if (ctx === null) return false;
            const { data } = ctx.getImageData(0, 0, canvas.width, canvas.height);
            for (let i = 0; i < data.length; i += 4) {
              if (data[i] !== 0 || data[i + 1] !== 0 || data[i + 2] !== 0) return true;
            }
            return false;
          }),
        { timeout: 15_000 },
      )
      .toBe(true);

    await page.click("#export");

    await expect(page.locator("#download[href]")).toBeVisible({ timeout: 60_000 });

    const result = await page.evaluate(async () => {
      const href = (document.querySelector<HTMLAnchorElement>("#download") as HTMLAnchorElement).href;
      const res = await fetch(href);
      const bytes = new Uint8Array(await res.arrayBuffer());
      const header = new TextDecoder().decode(bytes.slice(4, 8));
      return { size: bytes.length, header, stats: window.__exportStats };
    });

    expect(result.size).toBeGreaterThan(100_000);
    expect(result.header).toBe("ftyp");

    expect(result.stats).toBeDefined();
    const { videoSeconds, exportSeconds } = result.stats!;
    const ratio = videoSeconds / exportSeconds;
    console.log(`realtime ratio: ${ratio.toFixed(2)}x (video=${videoSeconds.toFixed(2)}s, export=${exportSeconds.toFixed(2)}s)`);
    if (ratio < 1) {
      console.log(`PERF FLOOR MISS: export ran slower than realtime (${ratio.toFixed(2)}x) — measured, not asserted (spec §6)`);
    }
  });
});
