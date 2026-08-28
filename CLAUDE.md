# Kineto — repo guide for agents

**A video compiler, not a screen recorder.** Declarative scene documents
(JSON, integer-tick time, fps-independent) compiled to deterministic MP4s by
one Rust engine with two targets: **wasm in the browser** (WebCodecs encode,
$0 server cost) and **native headless** (CI, no display, no browser). Three
authoring/invocation surfaces — a Rust crate, a TS package, and an MCP
server — are typed builders or thin front-ends that all emit the same
canonical JSON; the engine only ever sees the document.
Positioning: *"video as a build artifact."*

**The name is Kineto** (from *kinetic* / Kinetoscope), settled 2026-08-27.
It is no longer a codename — use it in code, docs, and package names.
Availability was verified before the rename: `kineto` is free on crates.io,
free as a bare npm package, and the `@kineto` npm scope is unclaimed. (The
previous codename `zoetrope` was already taken on crates.io; `kinora` was
rejected because the `@kinora` npm scope is owned by an active unrelated
project. Re-verify all three namespaces before ever renaming again.)

**Not yet renamed:** the repo *directory* on disk is still
`~/personal/repos/zoetrope`. Renaming it is a user action — nothing in the
build depends on the directory name.

## Start here

1. **Specs (binding authority):**
   `docs/superpowers/specs/2026-08-26-kineto-design.md` (engine, v1) and
   `docs/superpowers/specs/2026-08-27-kineto-mcp-design.md` (MCP server).
   Read the relevant one first; conflicts resolve against the spec.
2. **Status: v1 BUILT (2026-08-27)** — plan
   `docs/superpowers/plans/2026-08-26-kineto-v1.md`, 27 tasks, merged.
   Browser tape demo exports MP4 at **2.10× realtime**
   (`packages/demo-tape`, port 5200); `kineto-cast` renders headless
   (frames + MP4 via ffmpeg); **parity gate 18/18 byte-identical** (native
   aarch64 vs wasm+simd128); wasm 945 KB gzipped (< 3 MB budget).
3. **Status: MCP SERVER BUILT (2026-08-27)** — plan
   `docs/superpowers/plans/2026-08-27-kineto-mcp.md`, 8 tasks plus a
   whole-branch review and one fix wave, merged. `crates/mcp` speaks MCP
   over stdio via `rmcp` 3.1.4 and exposes three tools —
   `render_document`, `render_asciicast`, `render_storyboard` — plus
   read-only resources for the document JSON Schema and the six corpus
   examples. Render results carry the MP4 path, structured metadata, and
   sampled frames as inline images so a calling model can check its own
   output. Workspace suite is **246 tests**.
4. **NEXT GATE (needs the user): no git remote exists.** Create the GitHub
   repo, push, and watch one full CI run (`rust`, `wasm-parity`, `web`).
   Parity has never executed on x86_64; if it diverges there, the
   pre-agreed lever is disabling tiny-skia's `simd` feature and
   regenerating goldens. The `rust` job now also installs ffmpeg so the
   MCP mux test executes rather than skipping — that step has likewise
   never run on a real runner.

## Known issues (backlog, none blocking)

**Engine (v1):** full-canvas layers pre-clip text/group ink at static
positions before transforms (documented in `raster.rs`; ink-bbox layers are
the future fix, byte-risky under rotation); glyphs re-rasterize per frame
(`get_image_uncached`, ~0.5 ms/frame); text-in-group pivot uses zero-size
text bbox; canonical float byte-identity holds for integral |v|<2^53 or
magnitude ~[1e-5, 1e15] (`scalar.rs`/`canonical.ts`); nothing in CI asserts
the shipped wasm is SIMD-built (a stray `RUSTFLAGS` would silently drop it,
costing ~4×).

**MCP server:** the validate/render-mp4/preview tail is duplicated verbatim
across the three *render* `_impl` functions. `preview_document` (added
2026-08-27) was the fourth tool but not a fourth caller of that tail — it
writes no MP4 — so the trigger has not fired; it did share the fps-resolution
block, which is now `source::resolve_fps`; `render::frame_count`/`describe` are
public and panic on an fps that was not gated, safe today only because every
caller gates first; a storyboard image shorter than the 56 px caption band
renders the band partly off-canvas (`validate::check` does no geometry
bounds-checking); nothing enforces agreement between `crates/mcp/README.md`
and the parameter structs in `tools.rs`, so a new field can drift silently;
the MCP spec's §3 dependency block is stale (the shipped manifest also
carries `image`, `tempfile`, `base64`, and a `sha2` dev-dep).

## Locked decisions (recall — the specs have the detail)

- Standalone library; **demos are the wedge**: (1) mysteryshopper
  tape → captioned MP4 in-browser (flagship), (2) asciinema `.cast` → MP4
  headless CLI (Rust-community wedge; note: `vhs` drives a headless browser
  under the hood — we don't; that's the marketing line).
- **Scene = data.** No per-frame user code in v1; plugin escape hatch only
  under real future pressure.
- Time = `i64` ticks at **705,600,000/s (Flicks)**; fps is an export hint
  only. That number factors as 2⁹·3²·5⁵·7², so a legal fps is any divisor —
  7, 24, 25, 30, 50, 60 all divide it; 11 and 27 do not.
- Document → Scenes (local clocks, ordered, `cut`/`crossfade`) → Elements
  (`image`, `text`, `rect`, `path`, `group`). Static base geometry; only
  `translate/scale/rotation/opacity` animate (keyframes, 4 cubic easings).
- **`path` was added after v1** (2026-08-28): open/closed polylines,
  straight segments only, with `stroke`/`strokeWidth`/`cap`/`join`/`fill`.
  Cap and join are format fields because they are rasterizer parameters
  geometry cannot express; **arrowheads deliberately are not** — a filled
  closed path expresses one, and orienting it belongs to the authoring
  layer. No béziers: curve flattening carries a tolerance parameter and is
  the most parity-fragile part of a path renderer. The v1 spec's element
  list predates this and is not being rewritten; this note is the record.
  The `paths-strokes` corpus entry is the only golden exercising diagonal
  AA, miter/round joins and sub-pixel widths — native vs wasm+simd128 was
  20/20 at the time it landed.
- Bundled fonts are referenced by reserved src: **`kineto:inter`** and
  **`kineto:jetbrains-mono`** (`crates/core/src/assets.rs`). These are part
  of the document format — changing them is a breaking format change.
- Stack: `tiny-skia` (CPU raster) + `cosmic-text` (shaping/wrap);
  WebCodecs + `mp4-muxer` (JS) in browser; native export = frame sequence
  + optional ffmpeg shell-out. No pure-Rust encoder.
- **Determinism is law:** pure `(doc, tick) → pixels`; NO system fonts
  (assets only); Rust decodes images on BOTH targets (`image` crate in
  wasm); no fast-math. Enforced by the CI **byte-identical parity gate**
  (native vs wasm) and the **cross-SDK golden**.
  Note the scope of the claim: the *rendered frames* are deterministic. The
  **MP4 container is not** — ffmpeg embeds its thread count and encoder
  versions. Never promise byte-identical MP4s.
- `crates/core`'s `mux_with_ffmpeg` returns `Ok(false)` both when ffmpeg is
  absent and when it ran and failed. That is correct for the CLI, which
  prints and leaves PNGs behind. `crates/mcp` compensates by preflighting
  `ffmpeg_available()` before rendering a frame. **Do not "fix" the core
  contract** — the asymmetry is deliberate.
- License **MIT OR Apache-2.0**. No crates.io/npm publishing yet.
- Repo layout: `crates/core`, `crates/wasm`, `crates/mcp`,
  `adapters/asciicast`, `packages/sdk`, `packages/demo-tape`.
- YAGNI fence: no audio, video elements, effects, WebGPU, expressions,
  React wrapper, editor, Rust encoder, ffmpeg.wasm fallback. For the MCP
  server specifically: no HTTP/SSE transport, no hosted service, and no npm
  wrapper for `npx` distribution until brand/publish.

## Testing notes

- `testdata/golden/hashes.json` holds sha256 of **rendered frame buffers**.
  18 keys are corpus parity (`name@tick`); the rest are raster/render unit
  goldens. Regenerate with `UPDATE_GOLDEN=1`.
- The `raster-text` / `raster-text-tinted` specimens render
  **"Hamburgefons"**, deliberately *not* the product name. A pixel golden
  must change only when the renderer changes, never when the project is
  renamed. Keep it that way.
- `crates/mcp/tests/parity.rs` drives corpus documents through the MCP
  server's own loading path and checks them against the same goldens, so
  the server cannot become a second source of truth.
- Recurring failure mode on this codebase: **tests that cannot fail on the
  bug they target.** Seven were caught during the MCP build. When writing a
  test, ask whether it would still pass if the code under it were stubbed
  to return a default.

## Relationship to other projects

- **mysteryshopper** (`~/personal/repos/mysteryshopper`) is consumer #1.
  Its tape format v1 (frozen: `actions.jsonl` header + `step-NN.jpg`) is the
  tape adapter's input contract. The "Export MP4" button is a
  *mysteryshopper* task. Kineto takes **no code dependency** on it. Note the
  tape adapter currently exists only as TS demo code in
  `packages/demo-tape`; `render_storyboard` is the landing pad for a Rust
  port.
- **Obscura is NOT part of Kineto** — deliberate; Kineto owns its pixels
  (no third-party renderer fidelity risk).

## Constraints & house style

- **$0 budget until revenue** (user's standing constraint): no paid
  services, CI on free tiers only. Nothing in this project needs an LLM or
  any API key.
- TDD; conventional commits; commit after every green test cycle.
- Node ≥ 22 (v24 installed) for `packages/*`; stable Rust via rustup
  (1.97 installed), pinned in `rust-toolchain.toml`.

## Machine notes (this box)

- The shell sandbox may refuse compound commands, heredocs, and redirects —
  write files with the Write tool and run plain commands.
- The shell filters `grep`/`ls`/`git diff` stdout and can silently mangle
  or truncate it. If output looks wrong, re-derive it another way (e.g. a
  small Python script) rather than trusting the filtered view.
- ffmpeg 8.0 and `wasm-pack` 0.14 are installed locally, so the mux tests
  and the full parity gate both run here.
- `crates/wasm/pkg/` is gitignored build output whose filenames derive from
  the crate name. After any crate rename, run
  `wasm-pack build crates/wasm --target web --release` or the TS packages
  will fail to resolve the old `*_wasm.js`.
