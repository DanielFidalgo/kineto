# Zoetrope MCP server — design spec (2026-08-27)

> Supplements, does not supersede, `2026-08-26-zoetrope-design.md`. Every
> law in that spec still holds — determinism, no system fonts, no
> fast-math, byte-identical parity. This document adds a **fourth
> authoring/invocation surface** alongside the CLI, the Rust SDK, and the
> TS SDK. Working codename **zoetrope** as before; the server's
> user-visible name is brand-adjacent and is decided at brand time.

## 1. Thesis

**MCP is a distribution surface, not a repositioning.** The product
remains "video as a build artifact." This spec exposes the already-headless
native engine to agents over the Model Context Protocol so that an agent
can turn structured event data it already possesses into a watchable MP4.

The bet being tested is narrow and stated plainly so it can be falsified:
**agents cannot show their work.** An agent's output is text, but much of
what agents do is temporal — a terminal session, a browser run, a series
of timestamped screenshots. Those are already zoetrope's input shape. If
that demand is real, usage will show it and a larger repositioning becomes
a later, evidence-backed decision. If it is not real, this spec cost one
leaf crate and changed nothing else.

What this is explicitly **not**: a tool for models to author motion
graphics from imagination. Authoring is a poor fit for MCP — the model
cannot see its output, it competes with Remotion and with generative video,
and zoetrope's differentiator (determinism, byte-identical output, no
per-frame code) is worthless to someone who only wants something to look
nice. Determinism serves CI, not creativity. `render_document` exists as
the primitive that the other tools are built on, not as an invitation.

### 1.1 What already exists

The native target is headless today and needs no change to be driven by a
server. `zoetrope-cast` (`adapters/asciicast/src/main.rs`) runs with no
display and no browser; `Engine::render(tick) -> &[u8]`
(`crates/core/src/render.rs:96`) is pure CPU tiny-skia + cosmic-text; and
`export.rs` is gated `#[cfg(not(target_arch = "wasm32"))]`. "Headless"
is not part of this work. Only the protocol surface is.

### 1.2 The one real gap

Native zoetrope has **no encoder and no muxer**. `mux_with_ffmpeg`
(`crates/core/src/export.rs:80`) shells out to `ffmpeg -c:v libx264`. This
is a bigger gap than the post-v1 "Rust muxer" milestone implies: that
milestone concerns the browser path, where WebCodecs already encodes.
Native would need encode *and* mux.

**This spec does not close that gap and does not need to.** For a local
stdio server on a developer machine, "requires ffmpeg" is an ordinary
install note. A Rust encoder becomes mandatory only for a hosted server,
which §9 places out of scope.

## 2. Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Positioning | MCP as a fourth surface; no repositioning | Demand is hypothesis, not observation. Preserves the CI-parity claim, which is the most defensible technical asset. |
| Location | New Rust crate `crates/mcp` | Calls the native engine directly. No wasm round-trip, no second consumer of the engine. |
| Protocol impl | `rmcp` 3.1.4 (official SDK) | Protocol correctness and version negotiation. `schemars` derives tool input schemas from Rust types, removing hand-maintained JSON Schema. |
| Transport | stdio only | Matches the local trust model and the $0 constraint. |
| Tools | `render_document`, `render_asciicast`, `render_storyboard` | One primitive, two front-ends. |
| Resources | Document schema + `corpus.rs` examples | Worked examples beat a bare schema for a model authoring documents. |
| Render results | MP4 path + metadata + sampled frames as inline images | Closes the agent's feedback loop; without it the tool is write-only. |
| Missing ffmpeg | Loud, explicit error | The CLI's silent `Ok(false)` fallback is a correctness bug in a tool context. |
| Tape adapter | Deferred | `render_storyboard` is designed so the port lands on top of it. |

## 3. Crate layout and dependencies

New crate `crates/mcp`, added to the workspace `members`. It is a **leaf
binary crate**: nothing in `core`, `wasm`, or `asciicast` gains a
dependency on it or on anything it pulls in. The single exception is the
`Theme` field-type widening described in §4.2, which touches
`adapters/asciicast` but adds no dependency. The wasm size budget
(spec §5) is untouched, and the engine crates stay synchronous.

```toml
[dependencies]
rmcp = { version = "3.1.4", default-features = false, features = [
    "server", "macros", "transport-io", "base64",
] }
serde = { version = "1.0.229", features = ["derive"] }
serde_json = "1.0.151"
schemars = "1.0"
thiserror = "2.0.20"
tokio = { version = "1", features = ["rt", "macros", "io-std"] }
zoetrope-core = { path = "../core" }
zoetrope-asciicast = { path = "../../adapters/asciicast" }
```

**Verified against this workspace:** `rmcp` 3.1.4 declares
`rust-version = 1.88`; `rust-toolchain.toml` pins 1.97. With
`default-features = false` and the feature set above, the heavy optional
tree — `reqwest`, `hyper`, `oauth2`, `jsonwebtoken`, `process-wrap` —
stays off. What is pulled in: tokio (`io-util`, `io-std`), `tokio-util`
(codec), `schemars`, `uuid`, `futures`, `indexmap`, `pastey`,
`rmcp-macros`.

`base64` is **required, not optional to us**: MCP inline image content is
base64-encoded, and §6 returns sampled frames as images.

Follow the house convention of commenting non-obvious `default-features`
choices in the manifest, as `crates/core/Cargo.toml` does for
`cosmic-text` and `fontdb`.

## 4. Tool surface

Three tools. Names are unprefixed; MCP clients display them under the
server name.

### 4.1 `render_document`

The library, exposed. Every other tool builds a `Document` and delegates
here.

| Field | Type | Default | Notes |
|---|---|---|---|
| `document` | string | — | Canonical document JSON. Mutually exclusive with `document_path`; exactly one required. |
| `document_path` | string | — | Path to a `.json` document. |
| `asset_base_dir` | string | dir of `document_path`, else cwd | Root for resolving asset `src` values. |
| `out` | string | — | Output `.mp4` path. Required unless `validate_only`. |
| `fps` | integer | `default_fps` from the document, else 30 | Validated with the same guard as `Engine::tick_for_frame`. |
| `validate_only` | boolean | `false` | Parse and validate; render nothing. |
| `preview_frames` | integer | 5 | Sampled frames in the result. `0` disables. Capped at 12. |

### 4.2 `render_asciicast`

`adapters/asciicast` minus its `main`.

| Field | Type | Default | Notes |
|---|---|---|---|
| `cast_path` | string | — | Required. |
| `out` | string | — | Required. |
| `fps` | integer | 30 | |
| `theme` | object | `Theme::default()` | Optional overrides for `bg`, `fg`, `size_px`. Remaining `Theme` fields (`cell_w`, `cell_h`, `pad`) are not exposed; they are coupled to font metrics and exposing them invites broken output. |
| `preview_frames` | integer | 5 | |

`Theme`'s fields are `&'static str` for colors
(`adapters/asciicast/src/convert.rs:24`); the tool accepts owned strings,
so either `Theme` gains a `Cow`/`String` variant or the server constructs
its own theme values. Prefer widening `Theme` to `String` — it is a small
change and the `&'static str` is not load-bearing.

### 4.3 `render_storyboard`

The tool that actually tests the §1 hypothesis. Any agent that can take
screenshots can use it, with no adapter and no scene authoring.

| Field | Type | Default | Notes |
|---|---|---|---|
| `frames` | array | — | Ordered. Each: `{ image: string, duration_ms: integer, caption?: string }`. Required, non-empty. |
| `out` | string | — | Required. |
| `fps` | integer | 30 | |
| `size` | `{w, h}` | intrinsic size of the first image | All frames are fitted to this size. |
| `preview_frames` | integer | 5 | |

Captions render in a fixed bottom band using the bundled JetBrains Mono via
`resolve_reserved_src`. No caption styling knobs in this version — it is a
reporting tool, not a titling tool.

Implementation is a `Document` builder: one scene per frame with
`duration = duration_ms * 705_600` ticks (exact at `TIMEBASE`
705,600,000/s, so no rounding), an `image` element, and an
optional `text` element. Because it is only a builder, the deferred tape
adapter port reduces to "parse `actions.jsonl`, call this builder."

## 5. Asset resolution

`Document.assets` is a `BTreeMap<String, Asset>` where each `Asset` carries
a `src: String` (`crates/core/src/doc.rs:46`). The server resolves each
`src` as follows:

1. If `src` is a reserved font src, defer to
   `zoetrope_core::assets::resolve_reserved_src` (bundled Inter and
   JetBrains Mono).
2. Otherwise treat it as a filesystem path, resolved relative to
   `asset_base_dir`, read into bytes, and registered via
   `AssetStore::add_bytes`.

Absolute `src` paths are accepted and used as-is. A `src` that escapes
`asset_base_dir` via `..` is accepted for the same reason — see §8.
Unreadable or missing assets are a tool error naming the asset id and the
resolved path, never a silent blank frame.

No network fetching, ever. Determinism is law (v1 spec §5); a document
whose pixels depend on a URL is not reproducible.

## 6. Render results

Every successful render returns three things:

1. **Text summary** — one line: output path, dimensions, duration, frame
   count.
2. **Structured metadata** — `{ out, width, height, fps, frame_count, duration_ticks, duration_seconds }`.
3. **Sampled frames** — `preview_frames` PNGs at evenly-spaced ticks
   across the document (first and last always included), as inline MCP
   image content.

The sampled frames are the point. A model that renders, sees frames at
0/25/50/75/100%, notices a clipped caption, fixes the document, and
re-renders has a working loop. Without them the tool is write-only and the
model is asserting success it cannot check.

Frames are produced by the same `Engine::render(tick)` +
`unpremultiply` + PNG path as `export_frames`, so previews are
byte-identical to the corresponding exported frames. They are downscaled
only if the document exceeds a fixed max edge (720 px) — downscaling is
for context cost, and the spec is explicit that previews after downscaling
are no longer byte-comparable to exported frames and must not be used as
parity evidence.

`validate_only` returns the metadata block with no `out` and no frames.

## 7. Error contract

All errors are MCP tool errors carrying an actionable message. Three
classes:

- **Document errors** — `DocError` from `crates/core/src/validate.rs`,
  surfaced verbatim. This is the model's correction signal, so the message
  matters more here than in the CLI.
- **I/O errors** — missing input, unreadable asset, unwritable `out`;
  always naming the resolved path.
- **Environment errors** — ffmpeg missing.

**The ffmpeg case is a behavior change from the CLI and is deliberate.**
`mux_with_ffmpeg` returns `Ok(false)` both when ffmpeg is absent and when
it ran and failed. The CLI handles this correctly for its context by
printing nothing and leaving a PNG sequence behind. A tool returning to a
model must not do that: the model would read a non-error result and report
success for a video that does not exist. The server checks
`ffmpeg_available()` before rendering a single frame and fails immediately
with an install instruction, and treats a `false` return after a
successful availability check as a distinct mux-failure error.

## 8. Trust boundary

A local stdio server reads and writes wherever the calling agent can
already read and write. It inherits that trust rather than widening it —
the agent invoking this server can already run `zoetrope-cast` with the
same paths. Consequently there is no path sandbox, and `..` in an asset
`src` is not treated as an attack.

Two constraints do apply: the server writes only to the caller-supplied
`out` path plus a temporary frame directory it creates and removes, and it
never fetches over the network. If a hosted transport is ever added (§9
says it is not), this section must be rewritten before any of it ships.

## 9. Scope fence

Not in this version:

- **npm wrapper for `npx` distribution.** It is the right distribution
  answer — MCP configs in the wild are overwhelmingly `npx -y ...`, and
  the esbuild/biome pattern of an npm package that downloads a release
  binary applies directly. It cannot ship regardless: there is no git
  remote, and the standing rule is no crates.io or npm publishing until
  brand. Follow-on task.
- **Tape adapter port.** Deferred; `render_storyboard` is the landing pad.
- **HTTP, SSE, or streamable-http transport.** stdio only.
- **Hosted service.** This is the only scenario that would make a Rust
  H.264 encoder mandatory, and it violates the $0 constraint.
- **A Rust encoder or muxer.** Unchanged from the v1 spec.
- **Server naming.** The server name is user-visible in MCP client config
  and therefore brand-adjacent. Codename internally; rename at brand time
  with everything else. Keep brand strings out of code.
- Everything in the v1 spec §9 YAGNI fence.

## 10. Testing

TDD, per house style. Both halves are deterministic, which makes this
straightforward.

**Protocol tests — golden JSON-RPC transcripts.** Drive the server over
stdio with `initialize`, `tools/list`, `tools/call`, `resources/list`,
`resources/read`; assert the response frames against committed goldens.
This catches schema drift in the `schemars`-derived tool definitions,
which is the most likely silent regression.

**Render tests.** The tools produce the same bytes the CLI does, so
frames are hashed the way `crates/core/src/bin/dump-parity.rs` hashes. A
`render_document` call against a `corpus()` document must produce frame
hashes matching the existing corpus goldens — this ties the MCP surface to
the parity gate rather than creating a second source of truth. These tests
hash **exported** frames, never previews, because §6 allows previews to be
downscaled and a downscaled preview would not match a golden. A separate,
narrower test asserts preview bytes equal exported-frame bytes for a
document below the 720 px cap, which is what makes §6's byte-identity
claim testable at all.

**Error tests.** Each error class in §7 gets a test asserting an error
result, not a success result with a bad payload. The ffmpeg-missing case
is tested by stubbing availability, since it cannot be tested by removing
ffmpeg from the CI runner.

**ffmpeg dependency.** Most tests use `validate_only` and the
frame-sampling path, which need no ffmpeg. Exactly one integration test
performs a real mux.

## 11. CI

Add `crates/mcp` to the existing `rust` job. **The CI runner installs
ffmpeg** so the single mux integration test actually executes rather than
skipping. A silently-skipped mux test is precisely how the `Ok(false)`
bug in §7 would ship.

No change to the `wasm-parity` or `web` jobs — this crate is native-only
and is not in the wasm build graph.

**Sequencing note, not a dependency.** The project's open gate is that no
git remote exists and the byte-identical parity gate has never run on
x86_64. This work is independent of that gate and does not make it worse,
but neither does it resolve it. That remains the riskiest unknown in the
project. If parity diverges on x86_64, the pre-agreed lever is disabling
tiny-skia's `simd` feature and regenerating goldens — which would
regenerate the corpus hashes this spec's render tests depend on, so those
tests must read the shared goldens rather than copy them.

## 12. Risks

- **The hypothesis is unvalidated.** §1 states it plainly. The mitigation
  is the cost shape: one leaf crate, no changes to the engine, no
  repositioning, nothing that has to be undone if the answer is no.
- **ffmpeg friction.** Every render tool fails without it. §7 makes the
  failure legible, but it is still an install step between a user and
  their first successful call. Watch whether this is what kills adoption;
  it is the strongest argument for a Rust encoder becoming a real
  milestone rather than a deferred one.
- **Distribution is unsolved until the npm wrapper exists.** Until then
  the server is installable only by people who will build it from source,
  which is a much smaller group than the one the hypothesis is about.
  Real validation may not be possible until §9's follow-on task ships.
- **Context cost of frame previews.** Five images per call is not free.
  `preview_frames: 0` is the escape hatch, and the 720 px cap bounds the
  worst case, but if agents call these tools in loops the images may need
  to become opt-in rather than default.
