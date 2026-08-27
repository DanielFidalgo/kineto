# zoetrope — repo guide for agents

**A video compiler, not a screen recorder.** Declarative scene documents
(JSON, integer-tick time, fps-independent) compiled to deterministic MP4s by
one Rust engine with two targets: **wasm in the browser** (WebCodecs encode,
$0 server cost) and **native headless** (CI, no display, no browser). Dual
authoring surfaces — a Rust crate and a TS package — are typed builders that
emit the same canonical JSON; the engine only ever sees the document.
Positioning: *"video as a build artifact."* Working codename — public
name/brand decided at publish time; keep brand strings out of code.

## Start here

1. **Spec (binding authority, committed 2026-08-26):**
   `docs/superpowers/specs/2026-08-26-zoetrope-design.md` — read it first.
2. **Status: spec written, self-reviewed, committed (`0434123`).
   NEXT GATE: the user has NOT yet approved the spec** (brainstorming
   user-review gate). Two calls were made without explicit user blessing —
   confirm or change them during review:
   - bundled fonts: **Inter** (SDK `"default"`), **JetBrains Mono** (demos)
   - wasm size budget: **< 3 MB gzipped**, tracked in CI logs
3. **After spec approval:** invoke `superpowers:writing-plans` to produce the
   implementation plan, then execute it with
   `superpowers:subagent-driven-development` (per the global guide). Do not
   start coding before the plan exists.

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
