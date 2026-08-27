# Zoetrope — design spec (2026-08-26)

> Working codename: **zoetrope** (the pre-film device that spins still frames
> into motion). Public name/brand decided at publish time; nothing below
> depends on it. Fixed vocabulary regardless of brand: a **document** is the
> serializable description of a video; a **scene** is one segment of it; the
> **engine** is the Rust core that turns `(document, tick)` into pixels; an
> **adapter** converts an existing event format into a document.

## 1. Thesis

**Not a screen recorder. A video compiler** — feed it structured data, get
deterministic MP4s, in the browser or in CI.

Zoetrope is a programmatic-video engine: scenes are **declarative,
serializable documents** (not code, not captured pixels), rendered by a
single Rust core that compiles to two targets:

- **wasm in the user's browser** — export happens client-side via WebCodecs
  hardware encoding. Viral/export volume costs the operator nothing.
- **native on a server or CI runner** — headless, no display, no browser,
  faster than realtime.

Same engine, same document, **byte-identical pixels** on both targets. That
parity is the product's spine and is enforced by a CI gate, not a slogan.

Why this wins against the field:

- **Remotion**: scenes are React code welded to headless Chrome; server
  rendering means Chrome fleets (Lambda), and the company license is paid.
  Zoetrope scenes are data; rendering needs no browser; license is
  MIT OR Apache-2.0.
- **Motion Canvas / Revideo**: canvas-in-a-browser at heart; no
  browserless server story, no cross-target determinism.
- **Screen recorders / vhs**: capture pixels from a live run in wall-clock
  time (vhs drives a headless browser internally). Zoetrope renders from
  event data after the fact: headless, deterministic, re-stylable,
  faster-than-realtime, and the sources are KBs of diffable JSON, not GBs
  of MP4.

Positioning line: *"Video as a build artifact."*

Origin: generalized from mysteryshopper's planned tape→MP4 export
(that spec's §13 fast-follow). mysteryshopper becomes consumer #1.

## 2. Locked decisions

- **Standalone library, new repo** (this one). The flagship demo is the
  mysteryshopper tape exporter; the library is the product.
- **Scene = data.** A declarative scene graph interpreted by the engine. No
  per-frame user code in v1. A versioned "custom element" plugin interface
  is the known escape hatch **only when something real demands it** — it is
  explicitly out of scope now and, when added, will be marked as breaking
  document portability for that element.
- **Dual authoring surfaces, one format.** A Rust crate and a TypeScript
  package are both *typed builders that emit the same JSON document*.
  Neither SDK has private powers; the engine only ever sees the document.
- **Two demos ship with v1:**
  1. **Tape exporter** (flagship, in-browser): mysteryshopper tape
     (`actions.jsonl` + `step-NN.jpg`) → captioned MP4 with crossfades.
  2. **CLI-demo renderer** (Rust-community wedge, headless): asciinema
     `.cast` file → MP4, in CI, no display, no browser.
- **License: MIT OR Apache-2.0** (dual, Rust convention). The permissive
  license is part of the wedge.
- Package publishing (crates.io / npm names) is deferred to brand time;
  v1's distribution is this repo + README.

## 3. Document format (the v1 contract)

Three levels: **Document → Scenes → Elements.** The document is versioned
(`"v": 1`); unknown top-level or element fields are a validation error
(forward compatibility happens by version bump, mirroring the tape-format
discipline in mysteryshopper).

### 3.1 Time

- All times are **integer ticks**: `i64`, at a fixed timebase of
  **705,600,000 ticks per second** (the "Flicks" unit). Every common frame
  rate (24, 25, 30, 48, 50, 60, 90, 120) and audio sample rate (44.1k, 48k)
  divides it exactly.
- **fps is not authoring truth.** The document carries `defaultFps` as a
  preview/export *hint only*; no timing field references it. Rendering
  frame `n` at fps `f` samples the timeline at `t = n × (705_600_000 / f)`
  — an exact integer for every supported `f`.
- Consequence: cuts, transitions, and keyframes sit at arbitrary ticks —
  never quantized to a frame grid. Re-exporting at a different fps or
  retiming a scene loses nothing.
- SDK sugar (`seconds(0.9)`, `ms(150)`, `frames(27).at(30)`) converts to
  ticks **exactly at build time** in both languages. Humans never write raw
  ticks; the JSON is an artifact format (like Lottie), not a hand-authoring
  format.

### 3.2 Scenes and the timeline

- The document holds an **ordered list of scenes**. Each scene has a unique
  `id`, a `duration` (ticks), and an element tree.
- **Scene-local clocks:** element keyframe times count from *their scene's*
  tick 0. This makes scenes reorderable, reusable, and independently
  renderable (per-scene parallelism).
- The document timeline is the concatenation of scenes. **Transitions** on
  a scene describe how it enters from the previous scene:
  - `cut` (default): no overlap.
  - `crossfade { duration }`: the outgoing scene's last `duration` ticks
    and the incoming scene's first `duration` ticks render simultaneously
    with a linear opacity ramp. Total document duration =
    Σ scene durations − Σ overlaps. `duration` must not exceed either
    adjacent scene's duration (validation error).
- No nested scenes, no overlapping/parallel scenes in v1 — `group`
  elements inside a scene cover composition needs.

### 3.3 Elements

v1 element set: `image`, `text`, `rect`, `group`.

- **Base geometry (static, per type)** positions the element: `rect`
  [x, y, w, h] for `rect`/`image`, `pos` [x, y] (top-left of the layout
  box) for `text`, `origin` [x, y] for `group`.
- **Common animatable properties (the transform), on every element:**
  `translate` [dx, dy] (default [0, 0]), `scale` (default 1), `rotation`
  (degrees, default 0), `opacity` (0–1, default 1). Rotation and scale are
  applied about the **geometric center of the element's box**; `translate`
  applies after. Each animatable property has a static value; a keyframe
  track (§3.4) overrides it over time. Base geometry is never animated —
  motion goes through the transform.
- **Paint order = document order** (later elements on top).
- `rect`: `fill` color.
- `image`: `asset` (id); the source is scaled to fill the destination
  `rect` (stretch). `cover`/`contain` fit modes are backlog, not v1.
- `text`: `text` (string), `font` (asset id or `"default"`), `sizePx`,
  `color`, optional `maxW` (wrap width; cosmic-text wraps), `align`
  (`left` | `center` | `right`, default `left`).
- `group`: `children` (element list); child coordinates are relative to
  `origin`. The group's transform applies to the composed result, and
  group `opacity` composites the group **as an isolated unit** (rendered
  to a layer, then blended) — not multiplied per-child.
- **Colors** are `#RRGGBB` / `#RRGGBBAA` strings in the document. Everything
  is sRGB; no color management in v1. The document has a top-level `bg`
  color (default `#000000`).

### 3.4 Animations

- Per-property **keyframe tracks**:
  `{ "prop": "opacity", "keys": [ { "t": <ticks>, "v": <value>, "ease": <name> } ] }`.
  Tracks may target exactly the four transform properties (`translate`,
  `scale`, `rotation`, `opacity`) — nothing else is animatable in v1.
  `t` is scene-local; keys must be strictly increasing (validation error
  otherwise); value before the first key = first key's value, after the
  last = last key's value.
- `ease` names the curve *into* that key from the previous one. v1 easing
  set, exactly: `linear` (default), `inCubic`, `outCubic`, `inOutCubic`.
  Springs and cubic-bezier parameters are backlog.
- Keyframe *positions* are exact integers; value interpolation is floating
  point (pure function of exact inputs — see §5).

### 3.5 Assets

- Document-level map `assets: { id → { type, src } }`; elements reference
  by id. v1 asset types: `image` (JPEG/PNG), `font` (TTF/OTF).
- `src` resolution is host-defined: the TS SDK accepts URLs/`Blob`s/bytes;
  the Rust API accepts paths/bytes. The engine receives resolved bytes —
  the document's `src` strings are for tooling, not for engine I/O.

### 3.6 Example

```jsonc
{
  "v": 1,
  "timebase": 705600000,
  "defaultFps": 30,
  "size": { "w": 1280, "h": 800 },
  "bg": "#000000",
  "assets": {
    "f01":  { "type": "image", "src": "step-01.jpg" },
    "f02":  { "type": "image", "src": "step-02.jpg" },
    "mono": { "type": "font",  "src": "JetBrainsMono-Regular.ttf" }
  },
  "scenes": [
    {
      "id": "step-1",
      "duration": 635040000,
      "elements": [
        { "type": "image", "asset": "f01", "rect": [0, 0, 1280, 800] },
        { "type": "rect", "rect": [0, 740, 1280, 60], "fill": "#0A0A0AE6" },
        { "type": "text",
          "text": "Landing on the page. Big cookie banner, ominous.",
          "font": "mono", "sizePx": 24, "color": "#D4D4D4",
          "pos": [40, 756], "maxW": 1200,
          "animations": [
            { "prop": "opacity",
              "keys": [ { "t": 0, "v": 0 },
                        { "t": 141120000, "v": 1, "ease": "outCubic" } ] }
          ] }
      ]
    },
    { "id": "step-2",
      "transition": { "type": "crossfade", "duration": 105840000 },
      "duration": 635040000,
      "elements": [ { "type": "image", "asset": "f02", "rect": [0, 0, 1280, 800] } ] }
  ]
}
```

### 3.7 Canonical serialization

Cross-SDK identity has to be byte-level to be testable: the canonical form
is **serde_json compact output of the core Rust structs** (field order as
declared in `zoetrope-core`, integers without decoration). The TS SDK's
serializer matches it, enforced by the cross-SDK golden test (§6). Documents
that only *semantically* match are not good enough.

## 4. Architecture

```
crates/core        document model + validation, timeline evaluator,
                   rasterizer (tiny-skia), text (cosmic-text)
crates/wasm        wasm-bindgen shim over core
adapters/asciicast Rust: .cast → Document (+ small CLI: cast → frames/MP4)
packages/sdk       TS: builders, render() via WebCodecs, mount() preview player
packages/demo-tape TS: tape adapter + the in-browser tape exporter demo
```

### 4.1 `crates/core` — four units, independently testable

1. **Document model**: serde structs + validation. All errors are typed and
   surface at **load time**, never at render time: unknown asset id,
   non-increasing keyframes, transition longer than an adjacent scene,
   transition on the first scene, duplicate scene id, unknown field,
   unknown animatable property, bad color literal.
2. **Timeline evaluator**: pure `(document, tick) → resolved frame state`
   (world transforms, opacities, laid-out text runs after interpolation;
   crossfade overlap resolved to two scene states + ramp weights).
3. **Rasterizer**: tiny-skia, CPU-only in v1. Consumes resolved frame
   state, produces an RGBA pixmap.
4. **Text**: cosmic-text for shaping, wrapping, and layout, fed only by
   explicitly-loaded font assets.

### 4.2 Targets

- **Native**: `render_frame(&doc, tick) → Pixmap`; export writes a
  **PNG/JPEG frame sequence** and optionally shells out to `ffmpeg` if
  present on PATH to mux/encode MP4. **No pure-Rust encoder in v1.**
- **wasm** (`crates/wasm`): `load(docJson, assets)` once, then
  `render_frame(tick)` into a shared RGBA buffer — no per-frame JSON
  parsing or allocation churn.

### 4.3 TS package (`packages/sdk`)

- **Builders**: factory functions returning plain typed objects (no
  classes, no JSX, no framework coupling), emitting canonical JSON.
- **`render(doc, { fps, bitrate, onProgress }) → Blob`**: drives the wasm
  engine frame-by-frame → WebCodecs `VideoEncoder` (hardware) → MP4 muxing
  via the small proven `mp4-muxer` JS library. (A Rust muxer is the
  first post-v1 engine milestone — decided at spec review 2026-08-26; it
  sits behind this API and can replace mp4-muxer without API change.)
  **v1 requires WebCodecs** — unsupported browsers get a clear capability
  error; an ffmpeg.wasm fallback is backlog, not v1.
- **`mount(canvas, doc) → { play, pause, seek(tick), dispose }`**:
  framework-agnostic preview player rendering via the same wasm engine
  (the preview *is* the final render — same pixels). React wrappers are
  backlog.

### 4.4 Adapters

An adapter is a thin converter from an existing event format to a
Document — adapters contain no rendering logic and templates are just
functions (no template DSL):

- **Tape adapter** (TS, in `packages/demo-tape`): mysteryshopper tape
  format v1 (`actions.jsonl` header + frames + `step-NN.jpg`) → one scene
  per step (frame image, caption bar, fading narration text, step
  counter), crossfades between steps.
- **Asciicast adapter** (`adapters/asciicast`, Rust): asciinema v2 `.cast`
  (header + timed stdout events) → terminal grid states via a VTE parser →
  **one scene per distinct grid state, cut-joined** (elements have no
  per-element time windows, so state changes are expressed as scene
  boundaries — cuts are free), each scene monospace `text` runs +
  `rect` cells. Ships a small CLI:
  `zoetrope-cast demo.cast -o out/` (frame sequence, or MP4 when ffmpeg is
  present). Scope fence: 16-color + 256-color SGR, cursor, clears; no
  scrollback, no alternate-screen apps beyond what the demo needs.

## 5. Determinism rules (load-bearing)

Everything renderable is a **pure function of `(document, tick)`** — no
wall clock, no randomness, no I/O at render time. Two non-obvious rules:

1. **No system fonts, ever.** OS font fallback would silently break
   byte-identical output. Fonts are explicit assets only. The SDK bundles
   **Inter** (OFL) as the `"default"` font so hello-world needs no font
   file; the demos bundle **JetBrains Mono** (OFL) as an asset.
2. **Rust decodes images on both targets** (`image` crate compiled into
   wasm too). Browser-native decoding is not bit-stable across engines.
   Costs wasm size (see §8); buys the parity gate.

Float math is allowed in value interpolation and rasterization (pure,
IEEE-754); keyframe positions and all timing are exact integers. No
`fast-math`-style flags in any build profile.

## 6. Testing

- **Unit tests** per core unit (§4.1): validation cases, evaluator
  interpolation/overlap math, text layout snapshots, rasterizer primitives.
- **Golden parity gate (CI-blocking):** a corpus of documents (crafted to
  cover every element type, easing, transition, wrap, group nesting) is
  rendered by the native engine and the wasm engine; outputs must be
  **byte-identical**. Any mismatch fails CI.
- **Cross-SDK golden:** equivalent Rust and TS builder programs must emit
  **byte-identical canonical JSON** (§3.7).
- **Demos as integration tests:** the asciicast CLI renders a checked-in
  `.cast` in CI (headless); the tape demo renders a checked-in fixture tape
  in a browser test environment.
- **Performance floor, measured not asserted:** the tape demo must export
  1280×800 @ 30fps at ≥ 1× realtime on a mainstream laptop, CPU-only.
  A benchmark script reports the number; regressions are visible in CI
  logs even if not blocking.

## 7. Repo & workflow

- Layout as in §4. Node ≥ 22 for `packages/*`; stable Rust toolchain
  pinned via `rust-toolchain.toml`.
- CI: `cargo test` (unit + native goldens), wasm build + parity gate,
  TS typecheck + cross-SDK golden + demo test. **CI never touches the
  network** beyond dependency installation.
- Conventional commits; commit after every green test cycle (house style).

## 8. Risks — named, with mitigations

- **Text is the boss fight.** Shaping, wrapping, fallback, emoji — where
  DIY renderers die. Mitigation: cosmic-text carries shaping/layout; v1
  constrains scope (explicit fonts, no bidi/emoji guarantees — they render
  as the loaded fonts allow); text layout snapshots in the golden corpus
  from day one.
- **wasm size** (image decoders + engine + bundled font). Mitigation:
  feature-gate decoders to JPEG/PNG only, size budget tracked in CI logs;
  target < 3 MB gzipped for the engine bundle, revisit if breached.
- **tiny-skia throughput** at 1280×800 with per-frame text. Mitigation:
  measure in week one via the benchmark script; layout caching (shape a
  text run once per scene, not per frame) is the first lever; the
  WebGPU/vello tier is the eventual lever and stays out of v1.
- **WebCodecs coverage** (Chromium broad; Safari/Firefox recent). v1
  requires it and says so; the demo detects and messages. ffmpeg.wasm
  fallback is backlog.
- **mp4-muxer (JS dependency) correctness/limits** — acceptable for v1;
  isolated behind `render()` so a Rust muxer can replace it silently.

## 9. Out of scope for v1 (the YAGNI fence)

Audio · video-as-element · effects beyond transform/opacity/crossfade
(no blur/shadows/masks) · WebGPU/vello tier · expressions/scripting ·
custom-element plugins · nested scenes/precomps · overlapping scenes ·
`cover`/`contain` image fit · springs/bezier easings · React wrapper ·
GUI editor · pure-Rust encoder/muxer · ffmpeg.wasm fallback ·
template DSL · package publishing (crates.io/npm) · brand/naming.

Revisited only under real pressure, never speculatively.

## 10. Success criteria

1. **Tape demo:** a checked-in fixture tape (and a real mysteryshopper
   tape) exports to a captioned, crossfaded MP4 **in the browser**, at
   ≥ 1× realtime, with zero server involvement.
2. **CLI demo:** `zoetrope-cast fixture.cast` produces an MP4 (via ffmpeg)
   or frame sequence **headless in CI**, no display, no browser.
3. **Parity gate green:** native and wasm renders of the golden corpus are
   byte-identical.
4. **Cross-SDK golden green:** Rust and TS builders emit identical bytes.
5. **A stranger can run both demos from the README** on a clean machine.

## 11. Relationship to mysteryshopper

- mysteryshopper is **consumer #1**: its tape format v1 (frozen contract)
  is the tape adapter's input. The "Export MP4" button in mysteryshopper's
  tape page is a *mysteryshopper* task that lands after zoetrope v1 — it
  imports `packages/sdk` + the tape adapter; zoetrope takes no dependency
  on mysteryshopper.
- Obscura is **not** part of zoetrope. Zoetrope owns its pixels; that
  independence is deliberate (no third-party renderer fidelity risk).

## 12. Future (do not build now, do not preclude)

WebGPU/vello render tier · audio tracks · video-as-element (WebCodecs
decode) · plugin custom elements (versioned, portability-breaking flag) ·
more adapters (test traces, benchmark history, game replays, tracing
spans) · Rust muxer (**first post-v1 engine milestone**, per spec review
2026-08-26: single H.264 track, no B-frames, box-writer style; drops the
`mp4-muxer` dep and is the container half of single-binary native MP4) ·
native Rust encoder (the other half; realistically rav1e/AV1 behind a
native-only feature flag — the browser keeps WebCodecs regardless) ·
template
gallery · hosted render API (the native target is the seed of a paid
service if the library earns adoption).
