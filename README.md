<div align="center">

# Kineto

**Video as a build artifact.** You write a document; it compiles to a video —
the same way source compiles to a binary.

![Kineto](docs/media/kineto-loop.webp)

[Watch the 27-second tour](docs/media/kineto-hero.mp4) ·
[Quick start](#quick-start) · [Why](#why-this-is-different) ·
[Document format](#the-document) · [License](#license)

</div>

---

## Give your agent a camera

Kineto ships an **MCP server**. Point Claude Code — or any MCP client — at it,
and an agent can render, inspect and correct video without a browser, a
display, or a render farm.

**Nothing to install** — `npx` fetches the binary for your platform on first
use:

```sh
claude mcp add --scope user kineto npx kineto-mcp
```

**Or as a binary on your PATH**, which also gives you the `kineto` CLI:

```sh
cargo install kineto
claude mcp add --scope user kineto "$(which kineto-mcp)"
```

**Without Rust** — grab a build from
[Releases](https://github.com/DanielFidalgo/kineto/releases) for macOS or
Linux, on arm64 or x64:

```sh
tar xzf kineto-v<version>-<target>.tar.gz
sudo mv kineto-v<version>-<target>/kineto* /usr/local/bin/
claude mcp add --scope user kineto /usr/local/bin/kineto-mcp
```

**From source:**

```sh
git clone https://github.com/DanielFidalgo/kineto.git && cd kineto
just install
```

That builds the server, copies it somewhere stable, and registers it. Then, in
any session:

> Turn these screenshots into a 20-second clip with captions.

> Render a release video from the last ten commits.

> Explain this architecture as a diagram, then make it move.

No `just`? [It's one line to install](https://just.systems/man/en/packages.html),
or skip the clone entirely and use `cargo install kineto` above.

> Use `--scope user`, not the default. Project scope registers the server for
> one directory only, which is a confusing way to discover that your other
> sessions cannot see it.

### The seven tools

The pipeline is cheapest-first. Most of what an agent does costs no pixels at
all.

| tool | cost | answers |
|---|---|---|
| `check_document` | ~20 tokens | is it correct, and readable? |
| `preview_document` | ~390 tokens/frame | how does it *look*? |
| `build_chart` | — | data → a line, area or bar chart document |
| `compile_session` | — | turn a work journal into a document |
| `session_append` | — | record one thing that happened |
| `render_document` | seconds + a file | ship it |
| `render_asciicast` | seconds + a file | a terminal recording → video |
| `render_storyboard` | seconds + a file | screenshots + captions → video |

`build_chart` emits ordinary paths, rects and text — there is no chart
element in the format, because every choice a chart makes is opinion and the
engine has none. Axes are *measured*: the left margin is the width of the
widest tick label, categories are centred by their own width, and ticks land
on round numbers. The result is a document you can edit afterwards like any
other.

`check_document` is the unusual one. It reports what's wrong *before* anything
renders — text invisible against its background, an element animated off the
canvas, text running past the edge, a scene too short to read at 300 wpm — and
returns no images, so it costs a fraction of a look. It catches the class of
mistake that is invisible in the JSON and obvious on screen.

The server also exposes reference documents at `kineto://example/` — seven
different *shot types*, because a video reads as a slide deck when every scene
has the same shape.

---

## Why this is different

**No browser. No display.** Everything renders on the CPU through
[`tiny-skia`](https://github.com/RazrFalcon/tiny-skia). It runs in CI, in a
container, over SSH. `vhs` drives a headless browser to do this; Kineto
doesn't.

**Byte-identical across targets.** The same document produces the same frames
on native aarch64 and on WebAssembly with SIMD — currently **27/27 corpus
frames**, enforced by CI on every push. That is what makes rendering
checkable, cacheable and diffable rather than merely repeatable-ish.

**The document is data.** Not code, not a timeline file — JSON an agent can
read, edit, diff and reason about. Time is integer ticks at 705,600,000/s
([flicks](https://github.com/facebookarchive/Flicks)), so 24, 25, 30, 50 and
60 fps are all exact and fps is an export hint rather than a commitment.

**One engine, two targets.** The same Rust renders natively and in the browser
via WebCodecs, at $0 server cost.

> **Scope of the determinism claim:** the *frames* are byte-identical. The MP4
> container is not — ffmpeg records its own version and thread count. Never
> promise reproducible MP4 bytes; promise reproducible pixels.

---

## Quick start

```sh
just            # list every recipe
just build      # the `kineto` CLI and the MCP server
just check      # fmt, clippy, tests, and the parity gate
just install    # build + register the MCP server
just demo       # the browser demo on localhost:5200
```

### Without an agent

There is a CLI. Write a document, compile it:

```sh
just build
kineto scene.json --check                    # report problems, render nothing
kineto scene.json -o scene.mp4               # or scene.webp
kineto scene.json -o poster.png --at 1500    # one frame, for a thumbnail
kineto scene.json -o small.mp4 --width 960   # scale on the way out
```

`--check` is worth using before every render: it reports text invisible
against its background, elements animated off the canvas, text past the edge,
and scenes too short to read — and exits nonzero on anything that is actually
wrong. It costs no pixels.

Everything at the top of this page is built exactly that way. The document is
committed at [`docs/media/hero.json`](docs/media/hero.json), and `just media`
rebuilds the video, the inline loop and the poster from it — check, then
render three times. No other tool is involved.

Turn an [asciinema](https://asciinema.org/) recording into a video, headlessly:

```sh
just cast adapters/asciicast/tests/fixture.cast out/demo
```

That writes a PNG per frame into `out/demo/`, then muxes them to
`out/demo/out.mp4` if ffmpeg is present — and leaves the frames behind if it
isn't, which is the deterministic artifact anyway.

### Output formats

The output extension chooses the format.

| extension | when |
|---|---|
| `.mp4` | anything longer than a few seconds — h264, ~28× smaller |
| `.webp` | short loops embedded inline in markdown — 24-bit colour and real alpha |
| `.png` | a single frame: a poster, a thumbnail, an `og:image` |

Choose by length. Animated WebP has no inter-frame prediction, so every frame
is essentially a standalone image: roughly **280 KB per second at 720p**. A few
seconds is a README loop; a minute is 17 MB. (The loop at the top of this page
is WebP; the 27-second tour is MP4.)

---

## The document

```json
{
  "v": 1,
  "timebase": 705600000,
  "size": { "w": 1280, "h": 720 },
  "bg": "#0B1116",
  "assets": { "body": { "type": "font", "src": "kineto:inter" } },
  "scenes": [{
    "id": "title",
    "duration": 2116800000,
    "elements": [
      { "type": "rect", "rect": [80, 300, 420, 90], "radius": 16,
        "fill": { "type": "linear", "from": [0, 0], "to": [1, 0],
                  "stops": [{ "at": 0, "color": "#FF9F45" },
                            { "at": 1, "color": "#C77DFF" }] },
        "shadow": { "color": "#00000059", "blur": 20, "dy": 10 } },
      { "type": "text", "text": "Kineto", "font": "body", "sizePx": 64,
        "color": "#F4F7F9", "pos": [110, 318],
        "animations": [{ "prop": "opacity", "keys": [
          { "t": 0, "v": 0 },
          { "t": 176400000, "v": 1, "ease": "outBack" }]}] }
    ]
  }]
}
```

Five element types — `image`, `text`, `rect`, `path`, `group` — with gradient
fills, corner radius, drop shadows, clip windows and image fit modes. Only
`translate`, `scale`, `rotation` and `opacity` animate, across ten easing
curves. Scenes join with a cut or a crossfade.

The full JSON Schema is served by the MCP server at
`kineto://schema/document`.

### Authoring surfaces

Three typed front-ends emit the same canonical JSON, byte-for-byte — a
cross-SDK golden test enforces it:

- **Rust** — `kineto_core`'s builders
- **TypeScript** — `@kineto/sdk`, plus in-browser export via WebCodecs
- **MCP** — the tools above

The engine only ever sees the document.

---

## Repository

```
crates/core        the engine — document, timeline, raster, export
crates/wasm        WebAssembly bindings
crates/mcp         the MCP server
adapters/asciicast .cast → document, and the kineto-cast CLI
                   (the `kineto` CLI lives in crates/mcp, beside the document
                   loading and encoding it reuses)
packages/sdk       TypeScript authoring + browser export
packages/demo-tape the flagship browser demo
```

`crates/core` depends on nothing else in the repo; everything else depends on
it and never the other way round.

## Requirements

- [Rust](https://rustup.rs/) (stable, pinned in `rust-toolchain.toml`)
- [ffmpeg](https://ffmpeg.org/) on `PATH` — for encoding only; frames render
  without it
- [Node.js](https://nodejs.org/) ≥ 22 — for the TypeScript packages
- [just](https://just.systems/) — optional, but every command here assumes it

## Contributing

`just check` runs exactly what CI runs: formatting, clippy with warnings
denied, the full test suite, and the native-vs-wasm parity gate.

Two things to know before changing the renderer. Golden hashes in
`testdata/golden/hashes.json` are the sha256 of *frame buffers* — if one moves
without an intended visual change, that is the bug, not the golden. And the
parity gate is the instrument for anything touching rasterisation; a change
that passes tests but breaks parity has broken the central promise.

## License

MIT OR Apache-2.0, at your option.
