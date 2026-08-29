# kineto-mcp

An MCP (Model Context Protocol) server that exposes the native kineto
engine to agents over stdio. It's a fourth invocation surface alongside the
existing native CLI (`kineto-cast`) and the two authoring SDKs (Rust, TS)
— same deterministic engine, same canonical document format, just reachable
by an MCP client instead of a shell or a build script.

It speaks JSON-RPC 2.0 over stdio (the MCP `transport-io` transport; there
is no HTTP transport). It exposes five tools — `render_document`,
`preview_document`, `check_document`, `render_asciicast`,
`render_storyboard` — plus read-only resources for the document JSON Schema
and the golden example corpus.

## Output formats

The output path's extension chooses the format. An unrecognised extension is
an error, checked *before* any frame is rendered rather than after.

| extension | codec | when |
|---|---|---|
| `.mp4` | h264 | anything longer than a few seconds; universal, but needs a player or an upload to embed |
| `.webp` | animated WebP | short loops embedded inline in markdown; 24-bit colour and real alpha |

**Choose by length.** Animated WebP has no inter-frame prediction — every
frame is essentially a standalone VP8 image — so its size is structural, not a
tuning problem. Measured on a 5.5 s 720p clip: **~280 KB per second, about
28x h264**, and quality barely moves it (q=55 still produced 1.1 MB against
1.5 MB at q=85; the content presets spanned only 1462–1578 KB). A few seconds
of WebP is a README loop. A minute of it is ~17 MB.

Quality is therefore kept high (`q:v 85`, `preset picture`) rather than traded
for a saving that is not available — gradients and soft shadows are exactly
what a low setting bands, and they are the reason to pick WebP over GIF in the
first place. `-loop 0` is set explicitly: ffmpeg's WebP muxer defaults to
playing once and stopping on the last frame.

The render result reports the file size, because choosing between the two is a
size decision and a caller cannot make it without the number.

## Prerequisite: ffmpeg

Every *render* tool shells out to `ffmpeg` to mux the rendered frame sequence
into an `.mp4`. If `ffmpeg` is not on `PATH`, the server preflights this and
returns a clear tool error rather than silently writing frames without a
video — do not skip installing it. (`validateOnly` calls never mux, so they
work without it, and `preview_document` never muxes at all.)

Note what "deterministic" covers: the rendered *frames* are byte-identical
for a given document. The `.mp4` container is not — it records the encoder
version and thread count, so two ffmpeg runs over the same frames can differ
byte-for-byte.

- macOS: `brew install ffmpeg`
- Debian/Ubuntu: `sudo apt-get install -y ffmpeg`
- Windows: see <https://ffmpeg.org/download.html>

## Build

There is no published package yet — nothing is pushed to crates.io or npm
until the project is ready to publish. You must build the binary from
source:

```sh
cargo build -p kineto-mcp --release
```

This produces `target/release/kineto-mcp`.

## Client configuration

Point your MCP client at the built binary. For example, in a client that
reads a JSON config of `mcpServers`:

```json
{
  "mcpServers": {
    "kineto": {
      "command": "/absolute/path/to/kineto/target/release/kineto-mcp"
    }
  }
}
```

The server takes no arguments and reads no environment variables; all
configuration is per-call, in tool parameters.

## Tools

Parameter names are camelCase over the wire (the Rust structs in
[`src/tools.rs`](src/tools.rs) derive `JsonSchema`, so the wire schema is
generated from these types — the field names below are exact, not
illustrative).

All render tools bound the canvas before building an engine: each edge at
most 16384 px, and at most 67108864 pixels in total (64 Mpx, comfortably
above 8K). This applies to `validateOnly` too, which decodes assets and
allocates pixmaps even though it renders no frames.

### `render_document`

Renders a canonical kineto scene document to an MP4. Provide exactly one
of `document` (inline JSON string) or `documentPath`. `out` is required
unless `validateOnly` is true. `previewFrames` (default 5, capped at 12)
returns evenly spaced frames as inline images so the caller can check the
result without opening the file. `fps` defaults to the document's own
`defaultFps`, or 30 if it declares none; it must be at most 1000 and divide
705600000 exactly.

```json
{
  "name": "render_document",
  "arguments": {
    "document": "{\"v\":1,\"timebase\":705600000,\"defaultFps\":30,\"size\":{\"w\":640,\"h\":360},\"bg\":\"#000000\",\"scenes\":[{\"id\":\"s1\",\"duration\":70560000,\"elements\":[{\"type\":\"rect\",\"rect\":[40,40,200,120],\"fill\":\"#4040FF\"}]}]}",
    "out": "/tmp/rect-demo.mp4",
    "fps": 30,
    "previewFrames": 3
  }
}
```

`documentPath` (a path to a `.json` file on disk) works the same way in
place of `document`; use `assetBaseDir` to point image/font `src` values at
a directory other than the document's own. To validate a document without
rendering anything (no `out` needed), add `"validateOnly": true` in place of
`out`.

### `preview_document`

Renders *chosen moments* of a document as inline images without producing a
video. This is the cheap way to check a document while still changing it:
it writes no file, needs no ffmpeg, and rasterizes only the frames asked
for rather than the whole timeline.

Name moments with `atMs`, `atScenes`, or both — at least one is required,
and at most 12 moments in total per call.

`atMs` takes whole milliseconds from the start of the document. A millisecond is exactly 705600
ticks, so the conversion is integer-only and cannot round. Each moment is
snapped to the frame containing it at the effective `fps`, which means every
image returned is a frame `render_document` would also have written.

`atScenes` takes scene ids, and previews each at that scene's **midpoint**.
Prefer it for anything long: it needs no arithmetic over scene durations and
survives edits that shift the timeline. The midpoint rather than the start
because a crossfaded scene is fully transparent at its own start tick — a
frame there shows the *previous* scene.

The reply reports what each moment actually resolved to, in
`structuredContent.samples`:

```json
{ "requestedMs": 500, "frameIndex": 15, "tick": 352800000, "actualMs": 500,
  "sceneId": "s01", "sceneLocalMs": 120 }
```

`sceneId` names the scene dominating that frame, which is not always the one
requested — inside a crossfade it is the neighbour, which is how a caller
discovers it is looking at a transition rather than the scene it had in mind.
`actualMs` is rounded to the nearest millisecond so that feeding it back as
`atMs` returns the same frame.

Moments past the end of the document clamp to the last frame — compare
`requestedMs` against `actualMs` to detect it. Several moments landing on
one frame are encoded once but reported individually. Each image is preceded
by a text label naming its frame and the request it answers.

`document`/`documentPath`/`assetBaseDir`/`fps` behave exactly as in
`render_document`. There is no `out` and no `validateOnly`: this tool never
writes anything, and always validates.

```json
{
  "name": "preview_document",
  "arguments": {
    "documentPath": "/tmp/scene.json",
    "atMs": [0, 500],
    "atScenes": ["intro", "outro"]
  }
}
```

### The `timeline` block

Every tool's `structuredContent` carries a `timeline` describing the
document's scene spans:

```json
"timeline": {
  "nominalMs": 60000, "actualMs": 53667, "transitionOverlapMs": 6333,
  "scenes": [ { "id": "s00", "startMs": 0, "durationMs": 3000 },
              { "id": "s01", "startMs": 2667, "durationMs": 3000 } ]
}
```

**Watch `nominalMs` against `actualMs`.** Crossfades overlap the scenes they
join, so a document of twenty 3-second scenes joined by 19 third-of-a-second
crossfades is 53.667 s long, not 60 s. Nothing in the document says this, and
summing scene durations gets it wrong — as do the scene start times, which is
why they are reported rather than left to the caller.

Prefer `documentPath` while iterating: edit the file and preview again,
rather than resending the whole document on every call.

### `check_document`

Reports what is *wrong* with a document at chosen moments, and returns no
images. Nothing is rasterized, no file is written, and ffmpeg is not needed.

It takes the same `atMs` / `atScenes` moment addressing as
`preview_document`. The division of labour is deliberate: **use
`check_document` for correctness, `preview_document` for judgment.** Most
iterations are correctness, and a check costs roughly a tenth of a preview
image while giving a definite answer rather than something to squint at.

Rules, all decidable from geometry, resolved animation and colour arithmetic:

Every issue carries a `category`. **`correctness`** means the document does
not draw what it claims to, and no amount of taste makes that acceptable.
**`design`** means it draws correctly but breaks a rule of thumb that happens
to be checkable as arithmetic. Block on the first; report the second.

| kind | category | what it catches |
|---|---|---|
| `lowContrast` | correctness | text below 2:1 against the background — invisible, and unreachable by any validator |
| `offCanvas` | correctness | transformed bounds entirely outside the canvas at that tick |
| `textOverflow` | correctness | laid-out text running past the canvas edge |
| `fullyTransparent` | correctness | opacity never rises above zero anywhere in the scene |
| `zeroSize` | correctness | geometry collapsed to nothing |
| `tooFast` | design | more words than can be read in the scene's duration, at 300 wpm |
| `tooSmall` | design | text under 1.6% of canvas height — unreadable once scaled to a phone |
| `tooDense` | design | more than 40 words on screen at once |
| `deckShaped` | design | most scenes contain nothing but text — a slide deck rather than a video |

`deckShaped` is judged over the whole document, so a text-only title card is
fine and a document of them is not. It is reported once, in
`structuredContent.documentIssues`, rather than repeated against every moment
checked.

The design rules are the ones that make output *consistently* watchable
rather than merely correct. `tooFast` is the commonest mistake in explainer
video and is pure arithmetic: word count against scene duration. Scene-level
issues carry no `element` index.

Each issue names the scene id and the element's index within it, so it can be
found in a document with twenty scenes.

```json
{
  "name": "check_document",
  "arguments": { "documentPath": "/tmp/scene.json", "atScenes": ["intro", "outro"] }
}
```

A clean document answers `no issues across 2 moment(s)...` in a few tokens.

Known limits: a linter only catches what it enumerates, so it says nothing
about whether a composition is good or the pacing works — that is what
`preview_document` is for. Contrast is measured against the document
background, not against elements layered beneath the text.

### `render_asciicast`

Renders an asciinema v2 `.cast` terminal recording to an MP4 — from the
event data, not captured pixels, so it's deterministic and faster than
realtime. `theme` is optional and only overrides `bg`, `fg`, and `sizePx`
(cell metrics are deliberately not exposed — they're coupled to the bundled
monospace font's advance width).

```json
{
  "name": "render_asciicast",
  "arguments": {
    "castPath": "adapters/asciicast/tests/fixture.cast",
    "out": "/tmp/cast-demo.mp4",
    "fps": 30,
    "theme": { "bg": "#101820", "fg": "#e0e0e0", "sizePx": 16 },
    "previewFrames": 5
  }
}
```

### `render_storyboard`

Renders an ordered list of images into an MP4, each held for a given
duration with an optional caption band. `width`/`height` default to the
first image's dimensions if omitted (provide both or neither). At most
10000 frames, each `durationMs` between 1 and 86400000 (24 hours).

Long sequences are fine: decoded images are held behind a bounded cache, so
memory is flat in the number of frames rather than linear. A 300-frame
1280x800 tape renders 30s of video in ~8s at ~60 MB resident.

```json
{
  "name": "render_storyboard",
  "arguments": {
    "frames": [
      { "image": "/tmp/step-01.png", "durationMs": 1500, "caption": "Open the app" },
      { "image": "/tmp/step-02.png", "durationMs": 2000, "caption": "Click submit" }
    ],
    "out": "/tmp/storyboard.mp4",
    "fps": 30
  }
}
```

## Resources

The server also exposes read-only MCP resources:

- `kineto://schema/document` — the JSON Schema for the canonical document
  format accepted by `render_document` and `preview_document`.
- `kineto://example/<name>` — **reference documents to imitate**, each a
  different *shot type*, because a video reads as a deck when every scene has
  the same shape:

  | name | shot |
  |---|---|
  | `statement` | full bleed, no chrome, one sentence — breaks the rhythm |
  | `split` | text against a rounded, gradient-filled, shadowed panel |
  | `cards` | a set of peers entering in sequence, overshooting slightly |
  | `reveal` | content sliding in from behind a fixed clip window |
  | `flow` | a relationship drawn as a path between things |
  | `metric` | one number, large, with the quantity actually shown |
  | `steps` | one idea per scene, with progress visible |

  Small, self-contained, and tested against the same lint a caller's output is
  judged by.
- `kineto://corpus/<name>` — renderer *test* documents covering every element
  type, easing, crossfade, wrap and group nesting. Valid and byte-stable, but
  written to exercise features rather than to be copied — an agent imitating
  `kitchen-sink` produces coloured rectangles. Prefer `kineto://example/`.

Use `resources/list` to enumerate them.

## Design

For the full design rationale (why preflighting ffmpeg rather than
propagating the CLI's `Ok(false)` contract, the schema hand-write, resource
shape, etc.), see
[`docs/superpowers/specs/2026-08-27-kineto-mcp-design.md`](../../docs/superpowers/specs/2026-08-27-kineto-mcp-design.md).
