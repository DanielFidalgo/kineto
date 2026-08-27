// Canonical JSON shape for a kineto `Document` (see
// `crates/core/src/doc.rs`). Field names are exactly the camelCase names
// serde emits (`#[serde(rename_all = "camelCase")]` /
// `rename_all_fields = "camelCase"`) so `JSON.stringify` of a value built
// from these types round-trips byte-for-byte with the Rust side (the
// cross-SDK golden, spec §5).

/** Time in ticks at `TIMEBASE` (705_600_000/s). Branded so raw numbers
 * can't be passed where a tick value is expected without going through
 * `seconds`/`ms`/`frames(...).at(...)` (see `time.ts`). */
export type Ticks = number & { readonly __ticks: unique symbol };

export interface ZoeSize {
  w: number;
  h: number;
}

export type ZoeAsset =
  | { type: "image"; src: string }
  | { type: "font"; src: string };

export type Align = "left" | "center" | "right";
export type Ease = "linear" | "inCubic" | "outCubic" | "inOutCubic";
export type Prop = "translate" | "scale" | "rotation" | "opacity";

/** A single track's value: a scalar for `scale`/`rotation`/`opacity`, or
 * an `[x, y]` pair for `translate`. */
export type KeyValue = number | [number, number];

export interface Key {
  t: number;
  v: KeyValue;
  /** Defaults to `"linear"` when omitted (matches `Ease::default()`). */
  ease?: Ease;
}

export interface Track {
  prop: Prop;
  keys: Key[];
}

/** Fields common to every element variant (flattened into the JSON
 * object alongside the variant's own fields, matching `Common` in
 * `doc.rs`). */
export interface Common {
  translate?: [number, number];
  scale?: number;
  rotation?: number;
  opacity?: number;
  animations?: Track[];
}

export type ZoeElement =
  | ({
      type: "image";
      asset: string;
      rect: [number, number, number, number];
    } & Common)
  | ({
      type: "text";
      text: string;
      font: string;
      sizePx: number;
      color: string;
      pos: [number, number];
      maxW?: number;
      /** Defaults to `"left"` when omitted (matches `Align::default()`). */
      align?: Align;
    } & Common)
  | ({
      type: "rect";
      rect: [number, number, number, number];
      fill: string;
    } & Common)
  | ({
      type: "group";
      origin: [number, number];
      children: ZoeElement[];
    } & Common);

export type ZoeTransition = { type: "crossfade"; duration: number };

export interface ZoeScene {
  id: string;
  transition?: ZoeTransition;
  duration: number;
  elements: ZoeElement[];
}

export interface ZoeDocument {
  v: number;
  timebase: number;
  defaultFps?: number;
  size: ZoeSize;
  /** "#RRGGBB" or "#RRGGBBAA"; defaults to "#000000" when omitted. */
  bg?: string;
  assets?: Record<string, ZoeAsset>;
  scenes: ZoeScene[];
}
