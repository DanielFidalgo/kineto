# zoetrope-mcp

An MCP (Model Context Protocol) server that exposes the native zoetrope
engine to agents over stdio. It's a fourth invocation surface alongside the
existing native CLI (`zoetrope-cast`) and the two authoring SDKs (Rust, TS)
— same deterministic engine, same canonical document format, just reachable
by an MCP client instead of a shell or a build script.

It speaks JSON-RPC 2.0 over stdio (the MCP `transport-io` transport; there
is no HTTP transport). It exposes three tools —
`render_document`, `render_asciicast`, `render_storyboard` — plus read-only
resources for the document JSON Schema and the golden example corpus.

## Prerequisite: ffmpeg

Every render tool shells out to `ffmpeg` to mux the rendered frame sequence
into an `.mp4`. If `ffmpeg` is not on `PATH`, the server preflights this and
returns a clear tool error rather than silently writing frames without a
video — do not skip installing it.

- macOS: `brew install ffmpeg`
- Debian/Ubuntu: `sudo apt-get install -y ffmpeg`
- Windows: see <https://ffmpeg.org/download.html>

## Build

There is no published package yet — this project does not publish to
crates.io or npm until a public name is chosen (the `zoetrope` codename is
already taken on crates.io). You must build the binary from source:

```sh
cargo build -p zoetrope-mcp --release
```

This produces `target/release/zoetrope-mcp`.

## Client configuration

Point your MCP client at the built binary. For example, in a client that
reads a JSON config of `mcpServers`:

```json
{
  "mcpServers": {
    "zoetrope": {
      "command": "/absolute/path/to/zoetrope/target/release/zoetrope-mcp"
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

### `render_document`

Renders a canonical zoetrope scene document to an MP4. Provide exactly one
of `document` (inline JSON string) or `documentPath`. `out` is required
unless `validateOnly` is true. `previewFrames` (default 5, capped at 12)
returns evenly spaced frames as inline images so the caller can check the
result without opening the file.

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
first image's dimensions if omitted (provide both or neither).

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

- `zoetrope://schema/document` — the JSON Schema for the canonical document
  format accepted by `render_document`.
- `zoetrope://corpus/<name>` — worked example documents from the golden
  corpus, covering every element type, easing, crossfade, wrap, and group
  nesting.

Use `resources/list` to enumerate them.

## Design

For the full design rationale (why preflighting ffmpeg rather than
propagating the CLI's `Ok(false)` contract, the schema hand-write, resource
shape, etc.), see
[`docs/superpowers/specs/2026-08-27-zoetrope-mcp-design.md`](../../docs/superpowers/specs/2026-08-27-zoetrope-mcp-design.md).
