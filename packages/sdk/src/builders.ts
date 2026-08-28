// Typed builder surface for `ZoeDocument` (spec §3.6). Mirrors the Rust
// builder API in `crates/core/src/doc.rs` call-for-call so a builder
// program can be transliterated 1:1 between the two SDKs — that's what
// makes the cross-SDK golden test in `test/canonical.test.ts` meaningful.
// These builders only assemble plain data; `canonical.ts#build` is what
// turns a `ZoeDocument` into the canonical byte string.
import { TIMEBASE } from "./time";
import type {
  Cap,
  Join,
  Paint,
  Align,
  Common,
  Ease,
  Key,
  KeyValue,
  Prop,
  Track,
  ZoeAsset,
  ZoeDocument,
  ZoeElement,
  ZoeScene,
  ZoeSize,
  ZoeTransition,
} from "./types";

export function doc(opts: { w: number; h: number; fps?: number; bg?: string }): ZoeDocument {
  const size: ZoeSize = { w: opts.w, h: opts.h };
  const d: ZoeDocument = { v: 1, timebase: TIMEBASE, size, scenes: [] };
  if (opts.fps !== undefined) d.defaultFps = opts.fps;
  if (opts.bg !== undefined) d.bg = opts.bg;
  return d;
}

/** Mutates `d.assets` in place (mirrors `Document::add_asset`'s `&mut
 * self` mutation on the Rust side) and returns `d` for chaining. */
export function addAsset(d: ZoeDocument, id: string, asset: ZoeAsset): ZoeDocument {
  if (d.assets === undefined) d.assets = {};
  d.assets[id] = asset;
  return d;
}

export function imageAsset(src: string): ZoeAsset {
  return { type: "image", src };
}

export function fontAsset(src: string): ZoeAsset {
  return { type: "font", src };
}

export function scene(
  id: string,
  duration: number,
  elements: ZoeElement[],
  transition?: ZoeTransition,
): ZoeScene {
  const s: ZoeScene = { id, duration, elements };
  if (transition !== undefined) s.transition = transition;
  return s;
}

export function crossfade(duration: number): ZoeTransition {
  return { type: "crossfade", duration };
}

export function image(asset: string, rect: [number, number, number, number]): ZoeElement {
  return { type: "image", asset, rect };
}

export function path(
  points: [number, number][],
  opts: {
    closed?: boolean;
    stroke?: string;
    strokeWidth?: number;
    cap?: Cap;
    join?: Join;
    fill?: Paint;
  } = {},
): ZoeElement {
  return { type: "path", points, ...opts };
}

export function rect(r: [number, number, number, number], fill: Paint): ZoeElement {
  return { type: "rect", rect: r, fill };
}

export function text(
  str: string,
  opts: {
    font: string;
    sizePx: number;
    color: string;
    pos: [number, number];
    maxW?: number;
    align?: Align;
  },
): ZoeElement {
  const el: ZoeElement = {
    type: "text",
    text: str,
    font: opts.font,
    sizePx: opts.sizePx,
    color: opts.color,
    pos: opts.pos,
  };
  if (opts.maxW !== undefined) el.maxW = opts.maxW;
  if (opts.align !== undefined) el.align = opts.align;
  return el;
}

export function group(origin: [number, number], children: ZoeElement[]): ZoeElement {
  return { type: "group", origin, children };
}

export function anim(prop: Prop, keys: Key[]): Track {
  return { prop, keys };
}

export function key(t: number, v: KeyValue, ease?: Ease): Key {
  const k: Key = { t, v };
  if (ease !== undefined) k.ease = ease;
  return k;
}

/** Merge `common`'s transform/animation fields onto `el`, returning a new
 * element (does not mutate `el`). Only the fields present in `common` are
 * set — the rest keep whatever `el` already had. */
export function withCommon<T extends ZoeElement>(el: T, common: Common): T {
  return { ...el, ...common };
}
