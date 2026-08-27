import { expect, test } from "@playwright/test";

// Task 20: mount() preview player (spec §4.3), verified through a real
// browser canvas — same harness as Task 19's render() test (see
// harness.ts): the SDK is exposed on `window.zoetrope`, so this test can
// build a document and drive `mount()` from inside `page.evaluate`.
//
// Tick-advancement verification: Player's public surface is exactly
// `{play, pause, seek, dispose}` (per the brief, that must not grow — no
// tick getter). So "play() advances the current tick" is verified
// indirectly rather than by reading a tick value: scene "a"'s rect
// animates `opacity` linearly from 1 -> 0 across its 2s duration, so the
// pixel `mount()` paints changes measurably between a sample taken right
// after `seek(0)` and a second sample taken after ~300ms of real-time
// `play()`back. If `play()` didn't advance the tick, the two samples
// would be pixel-identical — this is what the "canvas changed" assertion
// below actually checks. (Chosen over an internal-only window test hook:
// no extra test-only surface needed, and it exercises the real paint
// path exactly as a consumer would observe it.)
test.describe("mount()", () => {
  test("seek renders the requested tick; play() advances it; dispose() disables further use", async ({
    page,
  }) => {
    await page.goto("/test-browser/harness.html");
    await page.waitForFunction(() => window.__zoetropeReady === true);

    const result = await page.evaluate(async () => {
      const { doc, scene, rect, withCommon, anim, key, mount, TIMEBASE } = window.zoetrope;

      const sceneDuration = 2 * TIMEBASE; // 2s — long enough that 300ms of
      // real-time playback is a clearly visible fraction of the fade.
      const d = doc({ w: 64, h: 64 });
      d.scenes = [
        scene("a", sceneDuration, [
          withCommon(rect([0, 0, 64, 64], "#00ff00"), {
            animations: [anim("opacity", [key(0, 1), key(sceneDuration, 0)])],
          }),
        ]),
      ];

      const canvas = document.createElement("canvas");
      document.body.appendChild(canvas);

      const player = await mount(canvas, d);
      const ctx = canvas.getContext("2d")!;

      const sizedToDoc = canvas.width === 64 && canvas.height === 64;

      player.seek(0);
      const start = ctx.getImageData(32, 32, 1, 1).data;
      const startPixel = [start[0], start[1], start[2], start[3]];

      player.play();
      await new Promise((resolve) => setTimeout(resolve, 300));
      player.pause();
      const afterPlay = ctx.getImageData(32, 32, 1, 1).data;
      const afterPlayPixel = [afterPlay[0], afterPlay[1], afterPlay[2], afterPlay[3]];

      player.dispose();
      let seekThrewAfterDispose = false;
      let seekThrewMessage = "";
      try {
        player.seek(0);
      } catch (e) {
        seekThrewAfterDispose = true;
        seekThrewMessage = e instanceof Error ? e.message : String(e);
      }

      return { sizedToDoc, startPixel, afterPlayPixel, seekThrewAfterDispose, seekThrewMessage };
    });

    expect(result.sizedToDoc).toBe(true);
    // Fully opaque green rect over the default black background at tick 0.
    expect(result.startPixel).toEqual([0, 255, 0, 255]);
    expect(result.afterPlayPixel).not.toEqual(result.startPixel);
    expect(result.seekThrewAfterDispose).toBe(true);
    expect(result.seekThrewMessage).toBe("zoetrope: player disposed");
  });
});
