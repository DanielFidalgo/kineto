// Hand-rolled canonical JSON serializer for `ZoeDocument` — deliberately
// NOT `JSON.stringify(d)`. It emits object keys in the exact orders serde
// produces on the Rust side (`crates/core/src/doc.rs`) and applies the same
// omit-default rules, so `build(d)` is byte-identical to
// `Document::canonical_json()` for the same logical document (the
// cross-SDK golden, spec §3.7/§6). Field order tables mirror the struct
// declaration order there (`Common` is `#[serde(flatten)]`ed last on every
// element variant, so it's appended after each element's own order).
import type {
  Paint,
  Common,
  Key,
  KeyValue,
  Track,
  ZoeAsset,
  ZoeDocument,
  ZoeElement,
  ZoeScene,
  ZoeSize,
  ZoeTransition,
} from "./types";

const ORDER = {
  document: ["v", "timebase", "defaultFps", "size", "bg", "assets", "scenes"],
  size: ["w", "h"],
  asset: ["type", "src"],
  scene: ["id", "transition", "duration", "elements"],
  transition: ["type", "duration"],
  image: ["type", "asset", "rect"],
  text: ["type", "text", "font", "sizePx", "color", "pos", "maxW", "align"],
  rect: ["type", "rect", "fill", "radius"],
  path: ["type", "points", "closed", "stroke", "strokeWidth", "cap", "join", "fill"],
  linear: ["type", "from", "to", "stops"],
  radial: ["type", "center", "radius", "stops"],
  stop: ["at", "color"],
  group: ["type", "origin", "children"],
  common: ["translate", "scale", "rotation", "opacity", "animations"],
  track: ["prop", "keys"],
  key: ["t", "v", "ease"],
} as const;

const ID_RE = /^[A-Za-z0-9_-]{1,64}$/;
const DEFAULT_BG = "#000000";
const DEFAULT_ALIGN = "left";
const DEFAULT_CAP = "butt";
const DEFAULT_JOIN = "miter";
const DEFAULT_EASE = "linear";
const DEFAULT_FONT_ID = "default";
const DEFAULT_FONT_ASSET: ZoeAsset = { type: "font", src: "kineto:inter" };

function serStr(s: string): string {
  return JSON.stringify(s);
}

/** Rust's `Scalar` prints integral values (|v| < 2^53) bare via
 * `serialize_i64`, else as a float. `String(n)` already produces the bare
 * form for JS integral numbers (no trailing `.0`) and matches JS's
 * shortest-round-trip formatting for fractional ones, which is what makes
 * this byte-identical to Rust's Ryu output within our numeric domain.
 * True domain: byte-identity is guaranteed for values that are integral
 * (|v| < 2^53) or whose magnitude falls in roughly [1e-5, 1e15]; outside
 * that range Ryu (Rust) and `String(n)` (JS) can diverge, since each
 * switches to scientific notation at different magnitude thresholds. */
function serNum(n: number): string {
  if (!Number.isFinite(n)) {
    throw new RangeError(`cannot serialize non-finite number: ${n}`);
  }
  return String(n);
}

/** A flat colour serialises as the bare string it always did; only an object
 * takes the gradient branch. That is what keeps every gradient-free document
 * byte-identical to before this existed. */
function serPaint(p: Paint): string {
  if (typeof p === "string") return serStr(p);
  const stops = `[${p.stops
    .map((s) => emit(ORDER.stop, { at: serNum(s.at), color: serStr(s.color) }))
    .join(",")}]`;
  if (p.type === "linear") {
    return emit(ORDER.linear, {
      type: serStr("linear"),
      from: serArr(p.from),
      to: serArr(p.to),
      stops,
    });
  }
  return emit(ORDER.radial, {
    type: serStr("radial"),
    center: serArr(p.center),
    radius: serNum(p.radius),
    stops,
  });
}

function serArr(vals: readonly number[]): string {
  return `[${vals.map(serNum).join(",")}]`;
}

/** Emit an object with keys in `order`, skipping any field whose value is
 * `undefined` (the omit-default mechanism throughout this module). */
function emit(order: readonly string[], fields: Record<string, string | undefined>): string {
  const parts: string[] = [];
  for (const k of order) {
    const v = fields[k];
    if (v !== undefined) parts.push(`${serStr(k)}:${v}`);
  }
  return `{${parts.join(",")}}`;
}

function serKeyValue(v: KeyValue): string {
  return Array.isArray(v) ? serArr(v) : serNum(v);
}

function serKey(k: Key): string {
  return emit(ORDER.key, {
    t: serNum(k.t),
    v: serKeyValue(k.v),
    ease: k.ease !== undefined && k.ease !== DEFAULT_EASE ? serStr(k.ease) : undefined,
  });
}

function serTrack(t: Track): string {
  return emit(ORDER.track, {
    prop: serStr(t.prop),
    keys: `[${t.keys.map(serKey).join(",")}]`,
  });
}

function serCommonFields(c: Common): Record<string, string | undefined> {
  return {
    translate: c.translate !== undefined ? serArr(c.translate) : undefined,
    scale: c.scale !== undefined ? serNum(c.scale) : undefined,
    rotation: c.rotation !== undefined ? serNum(c.rotation) : undefined,
    opacity: c.opacity !== undefined ? serNum(c.opacity) : undefined,
    animations:
      c.animations !== undefined && c.animations.length > 0
        ? `[${c.animations.map(serTrack).join(",")}]`
        : undefined,
  };
}

function serElement(el: ZoeElement): string {
  switch (el.type) {
    case "image":
      return emit([...ORDER.image, ...ORDER.common], {
        type: serStr("image"),
        asset: serStr(el.asset),
        rect: serArr(el.rect),
        ...serCommonFields(el),
      });
    case "text":
      return emit([...ORDER.text, ...ORDER.common], {
        type: serStr("text"),
        text: serStr(el.text),
        font: serStr(el.font),
        sizePx: serNum(el.sizePx),
        color: serStr(el.color),
        pos: serArr(el.pos),
        maxW: el.maxW !== undefined ? serNum(el.maxW) : undefined,
        align: el.align !== undefined && el.align !== DEFAULT_ALIGN ? serStr(el.align) : undefined,
        ...serCommonFields(el),
      });
    case "rect":
      return emit([...ORDER.rect, ...ORDER.common], {
        type: serStr("rect"),
        rect: serArr(el.rect),
        fill: serPaint(el.fill),
        radius: el.radius !== undefined ? serNum(el.radius) : undefined,
        ...serCommonFields(el),
      });
    case "path":
      return emit([...ORDER.path, ...ORDER.common], {
        type: serStr("path"),
        points: `[${el.points.map((p) => serArr(p)).join(",")}]`,
        closed: el.closed ? "true" : undefined,
        stroke: el.stroke !== undefined ? serStr(el.stroke) : undefined,
        strokeWidth: el.strokeWidth !== undefined ? serNum(el.strokeWidth) : undefined,
        cap: el.cap !== undefined && el.cap !== DEFAULT_CAP ? serStr(el.cap) : undefined,
        join: el.join !== undefined && el.join !== DEFAULT_JOIN ? serStr(el.join) : undefined,
        fill: el.fill !== undefined ? serPaint(el.fill) : undefined,
        ...serCommonFields(el),
      });
    case "group":
      return emit([...ORDER.group, ...ORDER.common], {
        type: serStr("group"),
        origin: serArr(el.origin),
        children: `[${el.children.map(serElement).join(",")}]`,
        ...serCommonFields(el),
      });
  }
}

function serAsset(a: ZoeAsset): string {
  return emit(ORDER.asset, { type: serStr(a.type), src: serStr(a.src) });
}

function serTransition(t: ZoeTransition): string {
  return emit(ORDER.transition, { type: serStr(t.type), duration: serNum(t.duration) });
}

function serScene(s: ZoeScene): string {
  return emit(ORDER.scene, {
    id: serStr(s.id),
    transition: s.transition !== undefined ? serTransition(s.transition) : undefined,
    duration: serNum(s.duration),
    elements: `[${s.elements.map(serElement).join(",")}]`,
  });
}

function serSize(sz: ZoeSize): string {
  return emit(ORDER.size, { w: serNum(sz.w), h: serNum(sz.h) });
}

function elementsUseDefaultFont(elements: readonly ZoeElement[]): boolean {
  return elements.some((el) => {
    if (el.type === "text" && el.font === DEFAULT_FONT_ID) return true;
    if (el.type === "group") return elementsUseDefaultFont(el.children);
    return false;
  });
}

/** If any text element uses font `"default"` and the document doesn't
 * already declare a `default` asset, inject
 * `default: {type:"font", src:"kineto:inter"}` (the SDK's bundled Inter
 * font, spec §2). Returns `d` unchanged when no injection is needed;
 * otherwise returns a shallow copy so builder-supplied documents aren't
 * mutated by `build()`. */
function withDefaultFontInjected(d: ZoeDocument): ZoeDocument {
  if (d.assets !== undefined && Object.prototype.hasOwnProperty.call(d.assets, DEFAULT_FONT_ID)) {
    return d;
  }
  if (!d.scenes.some((s) => elementsUseDefaultFont(s.elements))) {
    return d;
  }
  return {
    ...d,
    assets: { ...(d.assets ?? {}), [DEFAULT_FONT_ID]: DEFAULT_FONT_ASSET },
  };
}

/** Scene ids and asset ids must match `[A-Za-z0-9_-]{1,64}` (matches
 * `crates/core/src/validate.rs`). Throws before any serialization happens. */
export function validateIds(d: ZoeDocument): void {
  for (const s of d.scenes) {
    if (!ID_RE.test(s.id)) {
      throw new Error(`invalid id '${s.id}': must match [A-Za-z0-9_-]{1,64}`);
    }
  }
  if (d.assets !== undefined) {
    for (const id of Object.keys(d.assets)) {
      if (!ID_RE.test(id)) {
        throw new Error(`invalid id '${id}': must match [A-Za-z0-9_-]{1,64}`);
      }
    }
  }
}

/** Serialize `d` to the canonical JSON string — byte-identical to the
 * Rust engine's `Document::canonical_json()` for the same logical
 * document. NOT `JSON.stringify(d)`: applies default-font injection, id
 * validation, fixed key ordering, and default omission first. */
export function build(d: ZoeDocument): string {
  const doc = withDefaultFontInjected(d);
  validateIds(doc);

  const assetIds = doc.assets !== undefined ? Object.keys(doc.assets) : [];
  const assets =
    assetIds.length > 0
      ? `{${assetIds
          .slice()
          .sort()
          .map((id) => `${serStr(id)}:${serAsset(doc.assets![id]!)}`)
          .join(",")}}`
      : undefined;

  return emit(ORDER.document, {
    v: serNum(doc.v),
    timebase: serNum(doc.timebase),
    defaultFps: doc.defaultFps !== undefined ? serNum(doc.defaultFps) : undefined,
    size: serSize(doc.size),
    bg: doc.bg !== undefined && doc.bg !== DEFAULT_BG ? serStr(doc.bg) : undefined,
    assets,
    scenes: `[${doc.scenes.map(serScene).join(",")}]`,
  });
}
