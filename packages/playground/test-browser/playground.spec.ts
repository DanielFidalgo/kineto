import { expect, test } from "@playwright/test";

/** Non-black pixel count, as a cheap "something was drawn" signal. */
async function inkOf(page: import("@playwright/test").Page): Promise<number> {
  return page.evaluate(() => {
    const c = document.querySelector<HTMLCanvasElement>("#preview")!;
    const ctx = c.getContext("2d")!;
    const { data } = ctx.getImageData(0, 0, c.width, c.height);
    let n = 0;
    for (let i = 0; i < data.length; i += 4) {
      if (data[i]! > 24 || data[i + 1]! > 24 || data[i + 2]! > 24) n++;
    }
    return n;
  });
}

test.describe("playground", () => {
  test("loads an example and previews it", async ({ page }) => {
    await page.goto("/index.html");
    await expect(page.locator("#error")).toBeHidden();
    await expect(page.locator("#status")).toContainText("scene", { timeout: 30_000 });

    // Seek into the document: at tick 0 the first entrance has not run.
    await page.locator("#scrub").fill("500");
    await page.waitForTimeout(300);
    expect(await inkOf(page)).toBeGreaterThan(500);
  });

  test("editing the document changes what is rendered", async ({ page }) => {
    // The whole point of a playground. If the preview ignored the editor, every
    // other assertion here would still pass.
    await page.goto("/index.html");
    await expect(page.locator("#status")).toContainText("scene", { timeout: 30_000 });
    await page.locator("#scrub").fill("600");
    await page.waitForTimeout(300);
    const before = await inkOf(page);

    const source = await page.locator("#doc").inputValue();
    const doc = JSON.parse(source);
    doc.scenes[0].elements = doc.scenes[0].elements.filter(
      (e: { type: string }) => e.type !== "text",
    );
    await page.locator("#doc").fill(JSON.stringify(doc, null, 2));
    await page.waitForTimeout(900);
    await page.locator("#scrub").fill("600");
    await page.waitForTimeout(300);

    const after = await inkOf(page);
    expect(after).toBeLessThan(before);
    await expect(page.locator("#error")).toBeHidden();
  });

  test("an invalid document reports the error and keeps the last preview", async ({ page }) => {
    await page.goto("/index.html");
    await expect(page.locator("#status")).toContainText("scene", { timeout: 30_000 });
    await page.locator("#doc").fill("{ not json");
    await expect(page.locator("#error")).toBeVisible();
    await expect(page.locator("#error")).toContainText("JSON");
    // Still a canvas, not a blank page: a typo mid-edit must not wipe the view.
    await page.locator("#scrub").fill("600");
    await page.waitForTimeout(200);
    expect(await inkOf(page)).toBeGreaterThan(0);
  });

  test("exports a real MP4", async ({ page }) => {
    await page.goto("/index.html");
    await expect(page.locator("#status")).toContainText("scene", { timeout: 30_000 });
    await page.locator("#export").click();
    await expect(page.locator("#download[href]")).toBeVisible({ timeout: 90_000 });

    const result = await page.evaluate(async () => {
      const href = document.querySelector<HTMLAnchorElement>("#download")!.href;
      const buf = await (await fetch(href)).arrayBuffer();
      return {
        size: buf.byteLength,
        header: new TextDecoder().decode(new Uint8Array(buf, 4, 4)),
      };
    });
    expect(result.header).toBe("ftyp");
    expect(result.size).toBeGreaterThan(5_000);
  });
});
