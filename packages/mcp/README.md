# kineto-mcp

**Give your coding agent a camera.**

[Kineto](https://github.com/DanielFidalgo/kineto) is a video *compiler*, not a
screen recorder. You describe a scene as a JSON document; it compiles to an
MP4. Deterministic, no browser, no display, no render farm.

This package ships Kineto's **MCP server**, so an agent can render, inspect and
correct video on its own.

## Use it

```sh
claude mcp add --scope user kineto npx kineto-mcp
```

That is the whole install — `npx` fetches the binary for your platform on
first use. Then, in any session:

> Turn these screenshots into a 20-second clip with captions.

> Render a release video from the last ten commits.

> Explain this architecture as a diagram, then make it move.

Prefer a binary on your PATH? `cargo install kineto`, or take an archive from
[Releases](https://github.com/DanielFidalgo/kineto/releases).

> Use `--scope user`, not the default. Project scope registers the server for
> one directory only, which is a confusing way to discover that your other
> sessions cannot see it.

## What the agent gets

Tools to render a document, an asciinema recording, or a storyboard; to
preview single frames as images so it can *see* what it made; and to check a
document for the mistakes that are invisible in JSON and obvious on screen —
text too small to read, colours with no contrast, elements off-canvas.

Reading a frame back is what closes the loop. An agent that can look at its
own output stops producing slide decks.

## Requirements

Node 18+. **ffmpeg** on `PATH` for encoding — frames render without it.

Prebuilt binaries cover macOS and Linux on arm64 and x64. On anything else,
`cargo install kineto` builds from source.

## License

MIT OR Apache-2.0
