# Kineto

Kineto is a declarative video compiler: you author a scene document (JSON,
integer-tick time, resolution/fps-independent) and it compiles to a
deterministic MP4, the same way a build compiles source to a binary — video
as a build artifact, not a screen recording. One Rust engine renders that
document on two targets — in the browser via WebCodecs (wasm, $0 server
cost) and natively headless (CI, no display, no browser) — and both targets
are byte-identical for the same document. Typed authoring surfaces (a Rust
crate, a TS package, and an MCP server) build the same canonical JSON; the engine only
ever sees the document, never your code.

## Requirements

- [rustup](https://rustup.rs/) (stable toolchain; pinned via
  `rust-toolchain.toml`)
- [Node.js](https://nodejs.org/) ≥ 22
- (optional) [ffmpeg](https://ffmpeg.org/) on `PATH` — only needed for the
  CLI demo to mux its exported frames into an `.mp4`; without it the CLI
  still writes the frame sequence.

## Browser demo quickstart

The flagship demo turns a [mysteryshopper](#relationship-to-mysteryshopper)
tape into a captioned, crossfaded MP4 entirely in your browser.

```sh
npm ci
cargo install wasm-pack
wasm-pack build crates/wasm --target web --release
npm -w @kineto/demo-tape run dev
```

Open <http://localhost:5200>, click **Load fixture tape**, then click
**Export MP4**.

## CLI demo quickstart

The CLI demo converts an [asciinema](https://asciinema.org/) `.cast`
terminal recording to an MP4 headlessly (no browser involved at all — this
is the Rust-community wedge; note that tools like `vhs` drive a headless
browser under the hood to do this, kineto does not).

```sh
cargo run -p kineto-asciicast --bin kineto-cast -- adapters/asciicast/tests/fixture.cast -o out
```

This writes a PNG frame sequence to `out/`. If `ffmpeg` is on `PATH`, it
also muxes those frames into `out/out.mp4`.

## MCP server

`crates/mcp` is a fourth surface onto the same native engine — an MCP
server (`kineto-mcp`) that exposes document, asciicast, and storyboard
rendering to MCP-speaking agents over stdio, alongside the CLI above and
the two authoring SDKs. It has no published package yet and must be built
from source (`cargo build -p kineto-mcp --release`); see
[`crates/mcp/README.md`](crates/mcp/README.md) for the ffmpeg prerequisite,
client configuration, and worked tool examples, and the design spec at
[`docs/superpowers/specs/2026-08-27-kineto-mcp-design.md`](docs/superpowers/specs/2026-08-27-kineto-mcp-design.md)
for the full rationale.

## Browser support

The in-browser exporter (`render()` in `@kineto/sdk`, and the browser demo
above) requires [WebCodecs](https://developer.mozilla.org/en-US/docs/Web/API/WebCodecs_API)
— specifically a global `VideoEncoder` that supports H.264. This means:

- **Supported**: current Chrome, Edge, and other Chromium-based browsers.
  Safari shipped WebCodecs in 16.4, but whether its `VideoEncoder` accepts
  H.264 depends on the Safari version and the device's hardware encoder —
  `render()` probes this itself via `VideoEncoder.isConfigSupported` across
  `CODEC_CANDIDATES` and throws `"kineto: no supported H.264 encoder
  config"` if none match, rather than assuming support either way.
- **Not supported**: browsers without a `VideoEncoder` global at all (e.g.
  Firefox does not ship WebCodecs support at the time of writing).

When WebCodecs is unavailable, `render()` throws before doing any work,
with this exact message:

```
kineto: WebCodecs is required in this browser (see README#browser-support)
```

Live preview via `mount()` does not need WebCodecs and works in any modern
browser — only the MP4 export path (`render()`) is gated on it. There is no
polyfill or fallback path in v1 (see the design spec's YAGNI fence, §9);
native/CLI export (see the CLI demo above) has no such requirement.

## Document format

kineto's scene document format — time model, scenes/elements/animations,
canonical serialization, and the architecture that renders it — is
specified in full in
[`docs/superpowers/specs/2026-08-26-kineto-design.md`](docs/superpowers/specs/2026-08-26-kineto-design.md).
That document is the binding source of truth; this README only summarizes
enough to get the two demos running.

## Relationship to mysteryshopper

The browser demo's tape adapter (`packages/demo-tape`) consumes
mysteryshopper's tape format v1 (`actions.jsonl` + `step-NN.jpg`). kineto
takes no code dependency on mysteryshopper — the adapter only reads that
frozen file format.

## License

Licensed under either of

- MIT license ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.

The bundled fonts under [`assets/fonts/`](assets/fonts/) — Inter
(`Inter-Regular.ttf`) and JetBrains Mono (`JetBrainsMono-Regular.ttf`) — are
each licensed separately under the SIL Open Font License 1.1; see
[`assets/fonts/OFL-Inter.txt`](assets/fonts/OFL-Inter.txt) and
[`assets/fonts/OFL-JetBrainsMono.txt`](assets/fonts/OFL-JetBrainsMono.txt).
