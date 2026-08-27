# zoetrope — repo guide for agents

**A video compiler, not a screen recorder.** Declarative scene documents
(JSON, integer-tick time, fps-independent) compiled to deterministic MP4s by
one Rust engine with two targets: **wasm in the browser** (WebCodecs encode,
$0 server cost) and **native headless** (CI, no display, no browser). Dual
authoring surfaces — a Rust crate and a TS package — are typed builders that
emit the same canonical JSON; the engine only ever sees the document.
Positioning: *"video as a build artifact."* Working codename — public
name/brand decided at publish time; keep brand strings out of code.
**Note (2026-08-27): `zoetrope` is already taken on crates.io** — the
codename can never be the published name; at brand time pick a fresh name
and verify crates.io + npm availability before any rename/publish work.

## Start here

1. **Spec (binding authority, committed 2026-08-26):**
   `docs/superpowers/specs/2026-08-26-zoetrope-design.md` — read it first.
2. **Status: spec APPROVED by the user (2026-08-26 review)**, including the
   two previously-unblessed calls (Inter/JetBrains Mono bundled fonts;
   < 3 MB gzipped wasm budget). Review also settled: v1 keeps the
   `mp4-muxer` JS lib behind `render()`; a Rust muxer is the **first
   post-v1 engine milestone**; no Rust encoder in v1 (rationale recorded
   in spec §4.3/§12).
3. **Status: v1 BUILT (2026-08-27)** — plan
   `docs/superpowers/plans/2026-08-26-zoetrope-v1.md` executed to
   completion via subagent-driven development (27 tasks + final review),
   merged to main. All success criteria met locally: browser tape demo
   exports MP4 at **2.10× realtime** (`packages/demo-tape`, port 5200);
   `zoetrope-cast` CLI renders headless (frames + MP4 via ffmpeg);
   **parity gate 18/18 byte-identical** (native aarch64 vs wasm+simd128);
   cross-SDK canonical goldens green; wasm 945 KB gzipped (< 3 MB budget).
4. **NEXT GATE (needs the user): no git remote exists.** Create the
   GitHub repo, push, and watch one full CI run (`rust`, `wasm-parity`,
   `web` jobs) — parity has never executed on x86_64; if it diverges
   there, the pre-agreed lever is disabling tiny-skia's `simd` feature
   and regenerating goldens (documented in the Task 16 report reasoning).
5. **Known issues (post-v1 backlog):** full-canvas layers pre-clip
   text/group ink at static positions before transforms (documented in
   `raster.rs`; ink-bbox layers are the future fix, byte-risky under
   rotation); glyphs re-rasterize per frame (`get_image_uncached`,
   ~0.5 ms/frame); text-in-group pivot uses zero-size text bbox;
   canonical float byte-identity holds for integral |v|<2^53 or magnitude
   ~[1e-5, 1e15] (documented in `scalar.rs`/`canonical.ts`); nothing in
   CI asserts the shipped wasm is SIMD-built (a stray RUSTFLAGS env would
   silently drop it, costing ~4×).

## Locked decisions (recall — the spec has the detail)

- Standalone library; **demos are the wedge**: (1) mysteryshopper
  tape → captioned MP4 in-browser (flagship), (2) asciinema `.cast` → MP4
  headless CLI (Rust-community wedge; note: `vhs` drives a headless browser
  under the hood — we don't; that's the marketing line).
- **Scene = data.** No per-frame user code in v1; plugin escape hatch only
  under real future pressure.
- Time = `i64` ticks at **705,600,000/s (Flicks)**; fps is an export hint
  only. SDK sugar (`seconds/ms/frames(n).at(fps)`) converts exactly.
- Document → Scenes (local clocks, ordered, `cut`/`crossfade`) → Elements
  (`image`, `text`, `rect`, `group`). Static base geometry; only
  `translate/scale/rotation/opacity` animate (keyframes, 4 cubic easings).
- Stack: `tiny-skia` (CPU raster) + `cosmic-text` (shaping/wrap);
  WebCodecs + `mp4-muxer` (JS) in browser; native export = frame sequence
  + optional ffmpeg shell-out. No pure-Rust encoder in v1.
- **Determinism is law:** pure `(doc, tick) → pixels`; NO system fonts
  (assets only); Rust decodes images on BOTH targets (`image` crate in
  wasm); no fast-math. Enforced by the CI **byte-identical parity gate**
  (native vs wasm) and the **cross-SDK golden** (Rust and TS builders emit
  identical bytes; canonical form = serde_json compact of core structs).
- License **MIT OR Apache-2.0**. No crates.io/npm publishing until brand.
- Repo layout: `crates/core`, `crates/wasm`, `adapters/asciicast`,
  `packages/sdk`, `packages/demo-tape`.
- v1 YAGNI fence (spec §9): no audio, video elements, effects, WebGPU,
  expressions, React wrapper, editor, Rust encoder, ffmpeg.wasm fallback.

## Relationship to other projects

- **mysteryshopper** (`~/personal/repos/mysteryshopper`) is consumer #1.
  Its tape format v1 (frozen: `actions.jsonl` header + `step-NN.jpg`) is the
  tape adapter's input contract. The "Export MP4" button is a
  *mysteryshopper* task that lands after zoetrope v1. Zoetrope takes **no
  code dependency** on mysteryshopper.
- **Obscura is NOT part of zoetrope** — deliberate; zoetrope owns its
  pixels (no third-party renderer fidelity risk).

## Constraints & house style

- **$0 budget until revenue** (user's standing constraint): no paid
  services, CI on free tiers only. Nothing in this project needs an LLM or
  any API key.
- TDD; conventional commits; commit after every green test cycle.
- Node ≥ 22 (v24 installed) for `packages/*`; stable Rust via rustup
  (1.97 installed); pin toolchain via `rust-toolchain.toml` when scaffolding.

## Machine notes (this box)

- The shell sandbox may refuse compound commands, heredocs, and redirects —
  write files with the Write tool and run plain commands.
- The shell can filter `grep`/`ls` stdout; if output looks truncated,
  redirect to a file and read it.
