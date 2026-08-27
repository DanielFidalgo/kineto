# zoetrope

*Working codename — no public name or brand is decided yet; treat "zoetrope"
as an internal label, not a product name.*

zoetrope is a declarative video compiler: you author a scene document (JSON,
integer-tick time, resolution/fps-independent) and it compiles to a
deterministic MP4, the same way a build compiles source to a binary — video
as a build artifact, not a screen recording. One Rust engine renders that
document on two targets — in the browser via WebCodecs (wasm, $0 server
cost) and natively headless (CI, no display, no browser) — and both targets
are byte-identical for the same document. Two typed authoring surfaces (a
Rust crate and a TS package) build the same canonical JSON; the engine only
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
npm -w @zoetrope/demo-tape run dev
```

Open <http://localhost:5200>, click **Load fixture tape**, then click
**Export MP4**.

## CLI demo quickstart

The CLI demo converts an [asciinema](https://asciinema.org/) `.cast`
terminal recording to an MP4 headlessly (no browser involved at all — this
is the Rust-community wedge; note that tools like `vhs` drive a headless
browser under the hood to do this, zoetrope does not).

```sh
cargo run -p zoetrope-asciicast --bin zoetrope-cast -- adapters/asciicast/tests/fixture.cast -o out
```

This writes a PNG frame sequence to `out/`. If `ffmpeg` is on `PATH`, it
also muxes those frames into `out/out.mp4`.

## Browser support

The in-browser exporter (`render()` in `@zoetrope/sdk`, and the browser demo
above) requires [WebCodecs](https://developer.mozilla.org/en-US/docs/Web/API/WebCodecs_API)
— specifically a global `VideoEncoder` that supports H.264. This means:

- **Supported**: current Chrome, Edge, and other Chromium-based browsers.
- **Not supported**: browsers without a `VideoEncoder` global (e.g. Firefox
  and Safari do not ship WebCodecs support at the time of writing).

When WebCodecs is unavailable, `render()` throws before doing any work,
with this exact message:

```
zoetrope: WebCodecs is required in this browser (see README#browser-support)
```

Live preview via `mount()` does not need WebCodecs and works in any modern
browser — only the MP4 export path (`render()`) is gated on it. There is no
polyfill or fallback path in v1 (see the design spec's YAGNI fence, §9);
native/CLI export (see the CLI demo above) has no such requirement.

## Document format

zoetrope's scene document format — time model, scenes/elements/animations,
canonical serialization, and the architecture that renders it — is
specified in full in
[`docs/superpowers/specs/2026-08-26-zoetrope-design.md`](docs/superpowers/specs/2026-08-26-zoetrope-design.md).
That document is the binding source of truth; this README only summarizes
enough to get the two demos running.

## Relationship to mysteryshopper

The browser demo's tape adapter (`packages/demo-tape`) consumes
mysteryshopper's tape format v1 (`actions.jsonl` + `step-NN.jpg`). zoetrope
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
