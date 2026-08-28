import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  addAsset,
  anim,
  crossfade,
  doc,
  fontAsset,
  group,
  image,
  imageAsset,
  path as pathEl,
  key,
  rect,
  scene,
  text,
  withCommon,
} from "../src/builders";
import { build } from "../src/canonical";
import { ms, seconds } from "../src/time";
import type { ZoeDocument, ZoeElement } from "../src/types";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// packages/sdk/test -> packages/sdk -> packages -> repo root -> testdata/canonical
const GOLDEN_DIR = path.resolve(__dirname, "../../../testdata/canonical");

function readGolden(name: string): string {
  return readFileSync(path.join(GOLDEN_DIR, name), "utf8");
}

/** vitest's default string diff is unreadable for single-line canonical
 * JSON; report the first differing index and surrounding context instead. */
function expectCanonicalEquals(actual: string, expected: string): void {
  if (actual === expected) return;
  const len = Math.min(actual.length, expected.length);
  let i = 0;
  while (i < len && actual[i] === expected[i]) i++;
  const start = Math.max(0, i - 40);
  const expectedCtx = expected.slice(start, i + 40);
  const actualCtx = actual.slice(start, i + 40);
  throw new Error(
    `canonical bytes diverge at index ${i} (expected len ${expected.length}, actual len ${actual.length})\n` +
      `  expected: ...${expectedCtx}...\n` +
      `  actual:   ...${actualCtx}...`,
  );
}

/** Mirrors `example_doc()` in crates/core/tests/canonical.rs call-for-call. */
function buildExampleDoc(): ZoeDocument {
  const d = doc({ w: 1280, h: 800, fps: 30 });
  addAsset(d, "f01", imageAsset("step-01.jpg"));
  addAsset(d, "f02", imageAsset("step-02.jpg"));
  addAsset(d, "mono", fontAsset("JetBrainsMono-Regular.ttf"));
  d.scenes.push(
    scene("step-1", seconds(0.9), [
      image("f01", [0, 0, 1280, 800]),
      rect([0, 740, 1280, 60], "#0A0A0AE6"),
      withCommon(
        text("Landing on the page. Big cookie banner, ominous.", {
          font: "mono",
          sizePx: 24,
          color: "#D4D4D4",
          pos: [40, 756],
          maxW: 1200,
        }),
        {
          animations: [anim("opacity", [key(0, 0), key(ms(200), 1, "outCubic")])],
        },
      ),
    ]),
  );
  d.scenes.push(
    scene("step-2", seconds(0.9), [image("f02", [0, 0, 1280, 800])], crossfade(ms(150))),
  );
  return d;
}

/** Mirrors `example_full_doc()` in crates/core/tests/canonical.rs
 * call-for-call: group/all four Common transform fields/a Vec2-keyed
 * translate track/non-default Align variants/fractional Scalar values. */
function buildExampleFullDoc(): ZoeDocument {
  const d = doc({ w: 640, h: 360 });
  addAsset(d, "f01", imageAsset("frame.png"));
  addAsset(d, "mono", fontAsset("JetBrainsMono-Regular.ttf"));
  const grp = withCommon(
    group(
      [10, 20],
      [
        withCommon(rect([0, 0, 100, 100], "#FFFFFF"), { opacity: 0.1 }),
        text("Centered", { font: "mono", sizePx: 16, color: "#FFFFFF", pos: [0, 0], align: "center" }),
        text("Right", { font: "mono", sizePx: 16, color: "#FFFFFF", pos: [0, 0], align: "right" }),
      ],
    ),
    {
      translate: [10, 20],
      scale: 0.5,
      rotation: 12.5,
      opacity: 0.5,
      animations: [
        anim("translate", [key(0, [0, 0]), key(ms(500), [100, 50], "inOutCubic")]),
      ],
    },
  );
  const chip = rect([0, 0, 40, 40], "#3366FF", 8.5);
  const band = rect([0, 0, 120, 60], {
    type: "linear",
    from: [0, 0],
    to: [1, 0.5],
    stops: [
      { at: 0, color: "#FF9900" },
      { at: 0.25, color: "#F2F5F7" },
      { at: 1, color: "#4ECDC4" },
    ],
  });
  const arrow = pathEl(
    [
      [0, 0],
      [40, 25.5],
      [0, 51],
    ],
    {
      closed: true,
      stroke: "#FF9900",
      strokeWidth: 2.5,
      fill: "#00FF00",
      cap: "round",
      join: "bevel",
    },
  );
  d.scenes.push(scene("scene-1", seconds(1.0), [grp, chip, band, arrow]));
  return d;
}

function wrapInDoc(el: ZoeElement): ZoeDocument {
  const d = doc({ w: 10, h: 10 });
  d.scenes.push(scene("s", 0, [el]));
  return d;
}

describe("canonical serializer — cross-SDK golden", () => {
  it("rebuilds testdata/canonical/example.json byte-for-byte", () => {
    expectCanonicalEquals(build(buildExampleDoc()), readGolden("example.json"));
  });

  it("rebuilds testdata/canonical/example-full.json byte-for-byte", () => {
    expectCanonicalEquals(build(buildExampleFullDoc()), readGolden("example-full.json"));
  });

  it("emits fractional scale as a decimal", () => {
    const j = build(wrapInDoc(withCommon(rect([0, 0, 10, 10], "#FFFFFF"), { scale: 0.5 })));
    expect(j).toContain('"scale":0.5');
  });

  it("emits integral scale bare (no trailing .0)", () => {
    const j = build(wrapInDoc(withCommon(rect([0, 0, 10, 10], "#FFFFFF"), { scale: 2 })));
    expect(j).toContain('"scale":2');
    expect(j).not.toContain('"scale":2.0');
  });

  it("throws before serializing when a scene id has invalid characters", () => {
    const d = doc({ w: 10, h: 10 });
    d.scenes.push(scene("bad id!", 0, []));
    expect(() => build(d)).toThrow(/invalid id/);
  });

  it("throws before serializing when an asset id has invalid characters", () => {
    const d = doc({ w: 10, h: 10 });
    addAsset(d, "bad/id", imageAsset("x.png"));
    expect(() => build(d)).toThrow(/invalid id/);
  });

  it("auto-injects the default font asset when font \"default\" is used and not declared", () => {
    const d = doc({ w: 10, h: 10 });
    d.scenes.push(
      scene("s", 0, [
        text("hi", { font: "default", sizePx: 12, color: "#FFFFFF", pos: [0, 0] }),
      ]),
    );
    const j = build(d);
    expect(j).toContain('"default":{"type":"font","src":"kineto:inter"}');
  });

  it("does not inject the default font asset when not needed", () => {
    const j = build(wrapInDoc(image("f01", [0, 0, 10, 10])));
    expect(j).not.toContain("kineto:inter");
  });
});
