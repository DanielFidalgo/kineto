---
name: making-videos-with-kineto
description: Use when a user wants a video, animation, animated explainer, demo clip, release video, animated chart, or an MP4 or animated WebP for a README, docs, pull request or social post — especially about code, systems, data or a terminal session.
---

# Making videos with Kineto

Kineto compiles a JSON scene document into an MP4 or animated WebP. The
`kineto` MCP server renders it locally and deterministically: no screen
recording, no browser, no API key.

## Setup (once)

If no `kineto` tools are available, install the MCP server:

- Claude Code: `claude mcp add --scope user kineto npx kineto-mcp`
- Other clients: an MCP server with command `npx`, args `["-y", "kineto-mcp"]`

`npx` downloads a prebuilt binary from npm on first run; tell the user before
running it. MP4 and WebP output need `ffmpeg` on PATH; checks and previews do
not. Restart the client so the tools load.

## Pick the starting tool

| You have | Call |
|---|---|
| Text-led points, code, a quote | `build_scenes` |
| Numbers | `build_chart` (line, area, bar) |
| A release | `build_changelog`, with `range` set explicitly (`v0.1.0..v0.2.0`) |
| A terminal recording (`.cast`) | `render_asciicast` |
| Screenshots | `render_storyboard` |
| Your own work, as it happens | `session_append`, then `compile_session` |
| Anything else | write a document yourself (below) |

`build_*` and `compile_session` write a document and render nothing. Their
output is an ordinary document: edit it like your own.

## Writing a document yourself

Read `kineto://schema/document` and two or three `kineto://example/*`
documents first. Field names are not guessable.

Every scene has all five:

1. **A shot type unlike the scene before it**: statement, split, cards,
   reveal, flow, metric or steps — one example document each.
2. **Something that moves on purpose.** Use
   `"reveal": {"kind": "fadeUp", "at": 300}` instead of keyframes. Kinds:
   `fadeIn`, `fadeUp`, `fadeDown`, `slideLeft`, `slideRight`, `popIn`,
   `drawOn`. `at` and `ms` are milliseconds. `drawOn` makes a stroked `path`
   draw itself — use it for every arrow and connector.
3. **Curves where a relationship is drawn**: `path.d` takes SVG path data
   (`M L C Q A Z`).
4. **Depth on at least one surface**: a gradient `fill`, a `radius`, or a
   `shadow`.
5. **Content that fills the frame**: centred, or spanning the width. A scene
   with an empty half is unfinished.

Across the sequence, a label-plus-heading header appears on at most every
other scene. Statement, metric and flow scenes say it with their content: the
sentence, the number, the drawing.

Scene `duration` is in ticks: seconds × 705,600,000. Only `reveal` uses
milliseconds.

## Check before you claim it is done

1. `check_document` — fix every issue it reports.
2. `preview_document` at moments mid-motion and settled. Look at every image
   and answer: does this scene look different from the last one? Is something
   entering or drawing? Is any half of the canvas empty? Does any text touch
   or overlap another element?
   Anything you can see in a preview gets fixed and previewed again before
   rendering. Overlapping or clipped text is a defect, never "cosmetic".
3. `render_document` once. Look at the frames it returns before telling the
   user it is done, and say you judged from sampled frames.

## Common mistakes

| Mistake | Fix |
|---|---|
| Every scene is label, heading, diagram underneath | Alternate shot types; no heading on statement and metric scenes |
| Dozens of opacity keyframes | `reveal` |
| Arrows that simply appear | `drawOn` |
| A bar chart built from rects | `build_chart` |
| Guessed field names (`kind`, `w`, `h`) | Read the schema |
| `.webp` for anything long | WebP costs ~28× an MP4; keep it for short loops |
