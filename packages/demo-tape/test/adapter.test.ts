import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { build } from "@kineto/sdk";
import { seconds } from "@kineto/sdk";
import { parseTape } from "../src/adapter";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const FIXTURE_DIR = path.resolve(__dirname, "../fixtures/tape-fixture");

function loadFixtureFiles(dir: string): Map<string, Uint8Array> {
  const files = new Map<string, Uint8Array>();
  for (const name of readdirSync(dir)) {
    files.set(name, new Uint8Array(readFileSync(path.join(dir, name))));
  }
  return files;
}

describe("parseTape", () => {
  it("produces a 3-scene document with crossfades on scenes 2 and 3 only", () => {
    const { doc } = parseTape(loadFixtureFiles(FIXTURE_DIR));

    expect(doc.scenes).toHaveLength(3);
    expect(doc.scenes[0]!.transition).toBeUndefined();
    expect(doc.scenes[1]!.transition).toEqual({ type: "crossfade", duration: 105_840_000 });
    expect(doc.scenes[2]!.transition).toEqual({ type: "crossfade", duration: 105_840_000 });
  });

  it("returns an assets map with all 3 screenshots", () => {
    const { assets } = parseTape(loadFixtureFiles(FIXTURE_DIR));

    expect(assets.size).toBe(3);
    for (const bytes of assets.values()) {
      expect(bytes.length).toBeGreaterThan(0);
    }
  });

  it("builds to canonical JSON without throwing, and it mentions step 2 and the default font", () => {
    const { doc } = parseTape(loadFixtureFiles(FIXTURE_DIR));

    const json = build(doc);
    expect(json).toContain("step-2");
    expect(json).toContain('"default":{"type":"font","src":"kineto:inter"}');
  });

  it("spot-checks the duration formula for a known narration length", () => {
    const { doc } = parseTape(loadFixtureFiles(FIXTURE_DIR));

    // step 2's narration is exactly 50 characters long:
    // "Found a product page. Adding the item to the cart."
    const narration = "Found a product page. Adding the item to the cart.";
    expect(narration).toHaveLength(50);
    const expectedDuration = seconds(2.2 + 0.035 * 50);

    expect(doc.scenes[1]!.duration).toBe(expectedDuration);
    // sanity: within the [2.5s, 6s] clamp range, unclamped in this case.
    expect(doc.scenes[1]!.duration).toBeGreaterThan(seconds(2.5));
    expect(doc.scenes[1]!.duration).toBeLessThan(seconds(6));
  });

  it("throws a clear error when actions.jsonl is missing", () => {
    const files = loadFixtureFiles(FIXTURE_DIR);
    files.delete("actions.jsonl");

    expect(() => parseTape(files)).toThrow(/actions\.jsonl/);
  });

  it("throws a clear error when a referenced screenshot is missing", () => {
    const files = loadFixtureFiles(FIXTURE_DIR);
    files.delete("step-02.jpg");

    expect(() => parseTape(files)).toThrow(/step-02\.jpg/);
  });

  it("throws a clear error on a malformed line", () => {
    const files = loadFixtureFiles(FIXTURE_DIR);
    const original = new TextDecoder().decode(files.get("actions.jsonl")!);
    const corrupted = original.replace('"step":2', "not json");
    files.set("actions.jsonl", new TextEncoder().encode(corrupted));

    expect(() => parseTape(files)).toThrow(/malformed line/);
  });
});
