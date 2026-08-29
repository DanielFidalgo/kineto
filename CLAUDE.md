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

The repo directory on disk is `~/personal/repos/kineto`; the old
`zoetrope` path is gone. Nothing in the build ever depended on it.

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
   output. Workspace suite is **329 tests**.
4. **Status: PUBLIC AND RELEASED (2026-08-29).** Remote is
   `github.com/DanielFidalgo/kineto`, default branch `main`. CI is green,
   and **parity passed 27/27 on x86_64** — the largest open unknown of the
   v1 build, now closed, so the tiny-skia `simd`-disabling lever was never
   needed. `just release <version>` tags; the tag drives
   `.github/workflows/release.yml`. **v0.1.0 is published** with four
   platform archives, each verified end to end (checksum, extract, render)
   rather than assumed from a green check.

## Known issues (backlog, none blocking)

**Engine (v1):** full-canvas layers pre-clip text/group ink at static
positions before transforms (documented in `raster.rs`; ink-bbox layers are
the future fix, byte-risky under rotation); glyphs re-rasterize per frame
(`get_image_uncached`, ~0.5 ms/frame); text-in-group pivot uses zero-size
text bbox; canonical float byte-identity holds for integral |v|<2^53 or
magnitude ~[1e-5, 1e15] (`scalar.rs`/`canonical.ts`); nothing in CI asserts
the shipped wasm is SIMD-built (a stray `RUSTFLAGS` would silently drop it,
costing ~4×).

**Output scaling and stills** (2026-08-28): `--width` resamples on *export*,
never in `Engine::render` — the parity gate compares rendered frames, and a
resampler inside that would sit within the thing being proven identical.
Triangle filter, for determinism. Dimensions are forced even because h264's
4:2:0 chroma cannot represent an odd one. A `.png` output is a single frame
via `write_still` and never touches the muxer, which rejects `.png`
explicitly rather than encoding a one-frame video. Together these removed
the last two ffmpeg calls from `just media`.

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

**Release pipeline** (2026-08-29, both defects found only by running it):
`macos-13` is being retired — a job requesting it is never assigned a runner
and sits queued indefinitely, and because `publish` needs all four builds,
one dead label blocks the whole release. `x86_64-apple-darwin` is therefore
cross-compiled on `macos-14`. Separately, **do not restore
`dtolnay/rust-toolchain`'s `targets:` input**: it installs into the toolchain
that action selects, while `rust-toolchain.toml` pins 1.97 for anything run
inside the checkout, so the target never reaches the toolchain that builds
and the job dies on ``can't find crate for `core` ``. The explicit
`rustup target add` step honours the toolchain file. Both hid for the same
reason — `x86_64-apple-darwin` is the *only* genuine cross-compile in the
matrix; the other three are host targets needing neither a scarce runner nor
an installed std, so a four-wide matrix was exercising one path.

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
- **Decoded images are bounded, not retained** (2026-08-28): `AssetStore`
  decodes lazily behind a byte-budgeted LRU (`DEFAULT_IMAGE_BUDGET_BYTES`,
  32 MB). `prepare` still decodes every image once and may drop it again —
  that is deliberate, because `validateOnly` promises a corrupt asset is
  reported before anything renders. Measured on a 1280x800 tape: peak RSS
  was linear at ~4 MB/frame (300 frames = 1185 MB, 10k frames would have
  been ~40 GB) and is now flat at ~53 MB regardless of length. This is what
  makes screen-recording-density image sequences viable at all, and it is
  why `render_storyboard`'s 10000-frame cap is now honest rather than a
  promise that OOMs at ~300. Eviction cannot affect pixels — decode is pure
  in the staged bytes — and the unchanged goldens plus 20/20 parity are the
  proof.
- **Animated WebP output was added after v1** (2026-08-28): the output
  extension chooses the format (`.mp4` h264 | `.webp` animated), validated
  *before* rendering rather than after; anything else is an error rather than
  a silent h264 stream in a mis-named container. **Choose by length**:
  animated WebP has no inter-frame prediction, so it costs ~280 KB/s at 720p,
  ~28x h264, and that is structural — q=55 still measured 1.1 MB against
  1.5 MB at q=85, and the presets spanned 1462-1578 KB. Quality stays high
  (`q:v 85`, `preset picture`) because banding gradients and shadows is
  exactly what WebP was chosen over GIF to avoid. `render_to_mp4` is now
  `render_to_file`, and `RenderOutcome` reports `bytes`.
- **Gradients were added after v1** (2026-08-28): `fill` on `rect` and
  `path` is now `Paint` — an *untagged* union of a colour string and a
  gradient object, so every gradient-free document serialises byte-identically
  to before and not one golden moved when it landed. Coordinates are unit
  space over the element's own bbox, so a gradient is reusable across sizes;
  tiny-skia transforms the shader alongside the path (`painter.rs`), so a
  rotation carries the gradient with it for free. 2-8 stops, strictly
  increasing over 0..1. `stroke` stays a flat colour. Corpus entry
  `gradients`; parity 22/22 at landing.
- **Drop shadows were added after v1** (2026-08-28): `Common.shadow`
  (color, blur, dx, dy) on `rect`, `path` and `image` — the three kinds with
  a silhouette. **Rejected on `text` and `group`**, which render through
  isolated layers and would need the layer itself blurred. The blur is three
  separable box passes in **integer arithmetic**, chosen for that reason: no
  floating point means no new surface for native and wasm to disagree on, so
  a shadow did not re-open the parity question a float filter would have.
  Blur is capped at 128.
- **Clip windows and image fit were added after v1** (2026-08-28):
  `Common.clip` is a static window (rect + optional radius) in the element's
  **parent** space, deliberately *not* carried by the element's own transform
  — a clip that travelled with its content could never reveal anything, so
  content animates behind a fixed window. That is how a wipe, a crop or a
  progress fill is expressed in a format where only transforms animate.
  `image.fit` is `stretch` (default, the v1 behaviour) | `contain` |
  `cover`; cover crops to the element's own box via an internally-built mask.
  Every draw call already took a mask argument and passed `None`.
- **Corner radius and six easings were added after v1** (2026-08-28):
  `rect.radius` (optional, clamped at draw time to half the shorter edge, so
  an absurd value degrades to a stadium rather than folding the path), and
  `inBack`/`outBack`/`inOutBack`/`inExpo`/`outExpo`/`inOutExpo`. `ease` now
  guards its endpoints: every curve returns exactly 0 at 0 and 1 at 1, because
  `InBack(1.0)` evaluates to 0.9999999999999998 in floating point and an
  animation that never quite arrives is a real defect. **`back` overshoots
  0..1 by design**, so `resolve_common` clamps opacity — geometry may
  overshoot, an alpha handed to tiny-skia may not.
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
- **Publishing (2026-08-29, decided but not yet executed):** the bare name
  `kineto` on crates.io belongs to the **CLI + MCP package** (`crates/mcp`,
  renamed from `kineto-mcp`), so `cargo install kineto` yields the `kineto`
  and `kineto-mcp` binaries; the engine stays `kineto-core` for
  `cargo add kineto-core`. The ripgrep pattern, chosen because the audience
  runs Kineto rather than embeds it. **The binary name `kineto-mcp` and the
  MCP `serverInfo.name` did not change** — only the package. `kineto-wasm` is
  `publish = false` (a cdylib consumed through wasm-pack, and its test
  fixtures live outside the crate). Publishing happens **only from CI**
  against a repo secret, never a laptop, so no local credential decides which
  account ships. crates.io is irreversible: a name can never be freed and a
  version can only be yanked, so the `crates-io` job runs last and dry-runs
  every crate before publishing any.
- Path dependencies live in `[workspace.dependencies]` with a `version`,
  which crates.io requires. `crates/wasm` is deliberately **not** routed
  through it: a member's `default-features = false` is silently ignored for a
  workspace dependency, which would compile the bundled fonts into the wasm
  binary. `crates/mcp/tests/manifest.rs` fails if those versions drift from
  `workspace.package.version` — cargo catches a major/minor mismatch itself,
  but `^0.1.0` matches `0.1.1`, so a patch bump slips through unaided.
- **npm distribution (2026-08-29, built; not yet published):** `@kineto/mcp`
  is a thin wrapper (`packages/mcp`) whose four `optionalDependencies` carry
  the prebuilt `kineto-mcp` binary; npm selects one via `os`/`cpu`. The
  esbuild pattern, chosen over a postinstall download because postinstall
  scripts are routinely disabled (`--ignore-scripts`) and a tool that claims
  to work in CI cannot have an install step that silently no-ops. The
  platform packages are **repackaged from the release archives**, never built
  a second time, so npm cannot ship different bytes than the tarballs users
  checksum. Publish order is platforms first, wrapper last — the wrapper pins
  exact versions and is broken until they exist.
- **`scripts/guard-publish.mjs` refuses to publish outside GitHub Actions**,
  and runs as `prepublishOnly` in every publishable npm package (the build
  script copies it into each generated one). This is not ceremony: cargo takes
  its credential from an env var, but npm reads `~/.npmrc`, which is global to
  the machine — a stray `npm publish` here would use whatever account is
  logged in, and npm only allows unpublishing for 72 hours while the name
  stays taken forever. **Do not remove it to "just publish quickly".**
- The shim must **never write to stdout** — an MCP client reads it as
  JSON-RPC, so a diagnostic there is a corrupt stream, not a bad message.
  `packages/mcp/test/shim.test.mjs` asserts this. It also asserts
  `targets.mjs` matches the release build matrix, which is the invariant that
  can actually break; comparing `PACKAGES` to `TARGETS` would be tautological
  since one is derived from the other.
- Node resolves a module's realpath, so under `npm link`, workspaces or pnpm
  the shim's `import.meta.url` points outside the install tree. It falls back
  to `process.argv[1]` then `cwd`. A registry install needs none of that.
- **Provenance (`npm publish --provenance`) is not enabled yet** because it
  requires a `repository` field, which is being held until the repo's
  permanent home is settled. It is the strongest available answer to "prove
  which repo built this" — worth turning on the moment that is decided.
- License **MIT OR Apache-2.0**.
- Repo layout: `crates/core`, `crates/wasm`, `crates/mcp`,
  `adapters/asciicast`, `packages/sdk`, `packages/demo-tape`.
- YAGNI fence: no audio, video elements, effects, WebGPU, expressions,
  React wrapper, editor, Rust encoder, ffmpeg.wasm fallback. For the MCP
  server specifically: no HTTP/SSE transport, no hosted service, and no npm
  wrapper for `npx` distribution until brand/publish.

## Testing notes

- `testdata/golden/hashes.json` holds sha256 of **rendered frame buffers**.
  27 keys are corpus parity (`name@tick`); the rest are raster/render unit
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
