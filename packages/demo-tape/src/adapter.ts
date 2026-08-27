// mysteryshopper tape -> zoetrope document adapter (spec §4.4 / task 24).
//
// Consumes a parsed tape format v1 directory (see tape-format.ts for the
// frozen contract this reads) and produces the LOCKED demo scene template:
// one scene per tape step, full-bleed screenshot + caption bar + fading-in
// narration + step counter, crossfaded between consecutive scenes.
import {
  addAsset,
  anim,
  crossfade,
  doc,
  image,
  imageAsset,
  key,
  ms,
  rect,
  scene,
  seconds,
  text,
  withCommon,
} from "@zoetrope/sdk";
import type { ZoeDocument } from "@zoetrope/sdk";
import type { TapeFrame, TapeHeader } from "./tape-format";

const ACTIONS_FILE = "actions.jsonl";

const DOC_WIDTH = 1280;
const DOC_HEIGHT = 800;
const DOC_FPS = 30;
const DOC_BG = "#000000";

const CAPTION_BAR_RECT: [number, number, number, number] = [0, 740, 1280, 60];
const CAPTION_BAR_FILL = "#0A0A0AE6";

const NARRATION_FONT = "default";
const NARRATION_SIZE_PX = 24;
const NARRATION_COLOR = "#D4D4D4";
const NARRATION_POS: [number, number] = [40, 756];
const NARRATION_MAX_W = 1200;
const NARRATION_FADE_MS = 200;

const COUNTER_FONT = "default";
const COUNTER_SIZE_PX = 18;
const COUNTER_COLOR = "#8A8A8A";
const COUNTER_POS: [number, number] = [1200, 16];

const CROSSFADE_MS = 150;

/** `seconds(2.2 + 0.035 * narration.length)`, clamped to `[2.5s, 6s]`. */
function sceneDurationTicks(narration: string): number {
  const raw = seconds(2.2 + 0.035 * narration.length);
  return Math.min(Math.max(raw, seconds(2.5)), seconds(6));
}

/** Asset id for a given step number, e.g. step `1` -> `"f01"`. */
function assetIdFor(step: number): string {
  return `f${String(step).padStart(2, "0")}`;
}

/** Scene id for a given step number, e.g. step `1` -> `"step-1"`. */
function sceneIdFor(step: number): string {
  return `step-${step}`;
}

function parseJsonLine<T>(line: string, lineNumber: number): T {
  try {
    return JSON.parse(line) as T;
  } catch (err) {
    const reason = err instanceof Error ? err.message : String(err);
    throw new Error(`tape adapter: malformed line ${lineNumber} in ${ACTIONS_FILE}: ${reason}`);
  }
}

/**
 * Parse a mysteryshopper tape (per tape-format.ts) into a zoetrope
 * `ZoeDocument` plus the raw screenshot bytes it references.
 *
 * `files` is keyed by filename exactly as it appears in the tape
 * directory: `"actions.jsonl"`, `"step-01.jpg"`, `"step-02.jpg"`, ...
 *
 * Throws a clear `Error` when `actions.jsonl` is missing, a line is
 * malformed JSON, or a frame references a screenshot not present in
 * `files`.
 */
export function parseTape(files: Map<string, Uint8Array>): {
  doc: ZoeDocument;
  assets: Map<string, Uint8Array>;
} {
  const jsonlBytes = files.get(ACTIONS_FILE);
  if (jsonlBytes === undefined) {
    throw new Error(`tape adapter: missing ${ACTIONS_FILE} in input files`);
  }

  const raw = new TextDecoder().decode(jsonlBytes).trim();
  const lines = raw.length > 0 ? raw.split("\n") : [];
  if (lines.length === 0) {
    throw new Error(`tape adapter: ${ACTIONS_FILE} is empty (missing header line)`);
  }

  const header = parseJsonLine<TapeHeader>(lines[0]!, 1);
  if (header.v !== 1) {
    throw new Error(`tape adapter: unsupported tape version ${String(header.v)} (expected 1)`);
  }

  const frames = lines.slice(1).map((line, i) => parseJsonLine<TapeFrame>(line, i + 2));
  const total = frames.length;

  const document = doc({ w: DOC_WIDTH, h: DOC_HEIGHT, fps: DOC_FPS, bg: DOC_BG });
  const assets = new Map<string, Uint8Array>();

  frames.forEach((frame, index) => {
    const screenshotBytes = files.get(frame.frame);
    if (screenshotBytes === undefined) {
      throw new Error(
        `tape adapter: missing screenshot '${frame.frame}' referenced by ${ACTIONS_FILE} step ${frame.step}`,
      );
    }

    const assetId = assetIdFor(frame.step);
    addAsset(document, assetId, imageAsset(frame.frame));
    assets.set(assetId, screenshotBytes);

    const narrationText = withCommon(
      text(frame.narration, {
        font: NARRATION_FONT,
        sizePx: NARRATION_SIZE_PX,
        color: NARRATION_COLOR,
        pos: NARRATION_POS,
        maxW: NARRATION_MAX_W,
      }),
      {
        animations: [anim("opacity", [key(0, 0), key(ms(NARRATION_FADE_MS), 1, "outCubic")])],
      },
    );

    const counterText = text(`${frame.step}/${total}`, {
      font: COUNTER_FONT,
      sizePx: COUNTER_SIZE_PX,
      color: COUNTER_COLOR,
      pos: COUNTER_POS,
    });

    document.scenes.push(
      scene(
        sceneIdFor(frame.step),
        sceneDurationTicks(frame.narration),
        [image(assetId, [0, 0, DOC_WIDTH, DOC_HEIGHT]), rect(CAPTION_BAR_RECT, CAPTION_BAR_FILL), narrationText, counterText],
        index > 0 ? crossfade(ms(CROSSFADE_MS)) : undefined,
      ),
    );
  });

  return { doc: document, assets };
}
