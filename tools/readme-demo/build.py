#!/usr/bin/env python3
"""Build a README hero demo: a document turning into the pixels it compiles to.

The point of the artifact is the anti-rot property. The JSON shown on the left
and the frames shown on the right are produced from *one* source in *one* run,
so they cannot disagree — re-running this after the renderer changes yields a
video that is still telling the truth. A hand-recorded screencast cannot make
that claim, which is the whole pitch.

Usage:
    python3 tools/readme-demo/build.py            # build + render
    python3 tools/readme-demo/build.py --preview  # build + preview only

Configure it for another project by editing PRODUCT below; nothing else is
Kineto-specific.
"""
import argparse
import base64
import json
import os
import subprocess
import sys

TB = 705_600_000
MS = TB // 1000

MCP_BIN = os.environ.get(
    "KINETO_MCP", os.path.expanduser("~/.cargo-target/release/kineto-mcp"))
REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OUT = os.path.join(REPO, "out")
FRAMES = os.path.join(OUT, "readme-frames")

# ---- everything project-specific lives here ---------------------------------
PRODUCT = {
    "name": "Kineto",
    "tagline": "video as a build artifact",
    "claim_head": "Deterministic.\nNo browser. No display.",
    "claim_body": "The same document always produces the same frames.",
    "close": "github.com/danielfidalgo/kineto",
}

W, H = 1280, 720
BG = "#0D1419"
FG = "#F2F5F7"
DIM = "#8FA3B0"
CODE = "#7C93A3"
ACCENT = "#FF9900"

XFADE_MS = 220
FLIPBOOK_N = 8

# The document the demo is *about*. Deliberately small enough that its full
# JSON fits on screen — showing a truncated document would undercut the point.
SAMPLE = {
    "v": 1,
    "timebase": TB,
    "size": {"w": 480, "h": 270},
    "bg": "#0D1419",
    "scenes": [{
        "id": "hello",
        "duration": TB,
        "elements": [{
            "type": "rect",
            "rect": [40, 105, 60, 60],
            "fill": "#FF9900",
            "animations": [{
                "prop": "translate",
                "keys": [
                    {"t": 0, "v": [0, 0]},
                    {"t": TB, "v": [320, 0], "ease": "inOutCubic"},
                ],
            }],
        }],
    }],
}


class Mcp:
    """Minimal stdio MCP client."""

    def __init__(self, binary=MCP_BIN):
        if not os.path.exists(binary):
            sys.exit(f"kineto-mcp not found at {binary}\n"
                     f"build it: cargo build -p kineto-mcp --release")
        self.p = subprocess.Popen(
            [binary], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, text=True)
        self._id = 0
        self._rpc("initialize", {
            "protocolVersion": "2025-06-18", "capabilities": {},
            "clientInfo": {"name": "readme-demo", "version": "1"}})
        self.p.stdin.write(json.dumps(
            {"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
        self.p.stdin.flush()

    def _rpc(self, method, params):
        self._id += 1
        self.p.stdin.write(json.dumps({
            "jsonrpc": "2.0", "id": self._id,
            "method": method, "params": params}) + "\n")
        self.p.stdin.flush()
        return json.loads(self.p.stdout.readline())

    def call(self, name, args):
        r = self._rpc("tools/call", {"name": name, "arguments": args})
        res = r["result"]
        if res.get("isError"):
            raise RuntimeError(f"{name}: {res['content'][0]['text']}")
        return res

    def close(self):
        self.p.kill()


def render_sample_frames(mcp):
    """Render the sample document's frames via the MCP preview tool.

    Preview is exact here rather than approximate: the sample canvas is 480px,
    under the server's 720px downscale cap, so each PNG is byte-identical to
    the frame an export would write.
    """
    os.makedirs(FRAMES, exist_ok=True)
    sample_path = os.path.join(OUT, "readme-sample.json")
    with open(sample_path, "w") as f:
        json.dump(SAMPLE, f, indent=2)

    step = 1000 // FLIPBOOK_N
    at_ms = [i * step for i in range(FLIPBOOK_N)]
    res = mcp.call("preview_document", {
        "documentPath": sample_path, "atMs": at_ms, "fps": 30})

    paths, n = [], 0
    for c in res["content"]:
        if c["type"] == "image":
            p = os.path.join(FRAMES, f"f{n:02d}.png")
            with open(p, "wb") as f:
                f.write(base64.b64decode(c["data"]))
            paths.append(p)
            n += 1
    return paths, res["structuredContent"]["samples"], sample_path


def fade(hold_ms, out_start, out_end, start_v=0.0):
    """Opacity: up by hold_ms, held, then cleared before the crossfade opens.

    The engine ramps only the incoming layer during a crossfade, so an
    outgoing scene stays fully opaque; without clearing it here two scenes
    overlap at full strength. outCubic because anim.rs applies the easing of
    the key being entered — inCubic would keep the outgoing text legible right
    through the transition.
    """
    keys = [{"t": 0, "v": start_v}]
    if start_v == 0.0:
        keys.append({"t": hold_ms * MS, "v": 1.0, "ease": "outCubic"})
    keys.append({"t": out_start, "v": 1.0})
    keys.append({"t": out_end, "v": 0.0, "ease": "outCubic"})
    return {"prop": "opacity", "keys": keys}


def window(t0, t1):
    """Opacity keys making an element visible only during [t0, t1).

    One-tick ramps, so the switch is a hard cut at 1/705600000 s.
    """
    keys = []
    if t0 <= 0:
        # Starting at 0 must not emit a t=0 ramp pair — the validator requires
        # strictly increasing keyframe times.
        keys.append({"t": 0, "v": 1.0})
    else:
        keys.extend([{"t": 0, "v": 0.0},
                     {"t": t0 - 1, "v": 0.0},
                     {"t": t0, "v": 1.0}])
    keys.extend([{"t": t1 - 1, "v": 1.0}, {"t": t1, "v": 0.0}])
    return {"prop": "opacity", "keys": keys}


def pretty(obj, indent=0):
    """JSON with short arrays and small objects kept inline.

    json.dumps(indent=2) explodes `"rect": [40, 105, 60, 60]` into six lines,
    which pushes the sample document off the bottom of the canvas. Formatting
    it here rather than hand-writing a display string keeps the one property
    the demo exists to show: what is printed is what was rendered.
    """
    pad = "  " * indent
    if isinstance(obj, list):
        if all(not isinstance(x, (dict, list)) for x in obj):
            return "[" + ", ".join(json.dumps(x) for x in obj) + "]"
        inner = ",\n".join(pad + "  " + pretty(x, indent + 1) for x in obj)
        return "[\n" + inner + "\n" + pad + "]"
    if isinstance(obj, dict):
        def flat(v):
            # A keyframe is `{ "t": .., "v": [x, y] }` — treating its vector
            # value as "complex" would expand every key onto four lines.
            return not isinstance(v, dict) and (
                not isinstance(v, list)
                or all(not isinstance(x, (dict, list)) for x in v))

        scalars = all(flat(v) for v in obj.values())
        if scalars and len(obj) <= 3:
            return "{ " + ", ".join(
                f'"{k}": {json.dumps(v)}' for k, v in obj.items()) + " }"
        inner = ",\n".join(
            f'{pad}  "{k}": {pretty(v, indent + 1)}' for k, v in obj.items())
        return "{\n" + inner + "\n" + pad + "}"
    return json.dumps(obj)


def text(t, font, size, color, pos, anims, max_w=None):
    e = {"type": "text", "text": t, "font": font, "sizePx": size,
         "color": color, "pos": list(pos), "animations": anims}
    if max_w:
        e["maxW"] = max_w
    return e


def build(frame_paths, samples, sample_path):
    scenes = []

    def tail(dur_ms):
        out_end = (dur_ms - XFADE_MS) * MS
        return out_end - 380 * MS, out_end

    # --- s0: title ----------------------------------------------------------
    d = 3200
    o0, o1 = tail(d)
    scenes.append({"id": "title", "duration": d * MS, "elements": [
        text(PRODUCT["name"], "inter", 76, FG, (100, 280), [fade(420, o0, o1)]),
        text(PRODUCT["tagline"], "mono", 24, DIM, (104, 386), [fade(760, o0, o1)]),
        {"type": "rect", "rect": [100, 452, 220, 5], "fill": ACCENT,
         "animations": [
             {"prop": "translate", "keys": [
                 {"t": 0, "v": [-280, 0]},
                 {"t": 700 * MS, "v": [0, 0], "ease": "outCubic"}]},
             fade(0, o0, o1, start_v=1.0)]},
    ]})

    # --- s1: the document becoming pixels -----------------------------------
    d = 9200
    o0, o1 = tail(d)
    els = [
        text("THE DOCUMENT", "mono", 16, ACCENT, (80, 96), [fade(300, o0, o1)]),
        text(pretty(SAMPLE), "mono", 13.5, CODE,
             (80, 150), [fade(520, o0, o1)], max_w=610),
        text("compiles to", "mono", 15, DIM, (628, 350), [fade(900, o0, o1)]),
        {"type": "rect", "rect": [628, 380, 44, 3], "fill": ACCENT,
         "animations": [fade(0, o0, o1, start_v=1.0)]},
        text("THE PIXELS", "mono", 16, ACCENT, (742, 96), [fade(300, o0, o1)]),
        # Border + inner panel: the sample renders on a near-black background,
        # so without a boundary the frame is indistinguishable from the canvas
        # and the right half looks empty.
        {"type": "rect", "rect": [740, 188, 484, 274], "fill": "#243542",
         "animations": [fade(300, o0, o1)]},
        {"type": "rect", "rect": [742, 190, 480, 270], "fill": "#0D1419",
         "animations": [fade(300, o0, o1)]},
    ]

    # Flipbook: each rendered frame held in turn, in the same rect.
    play_from, play_to = 1500 * MS, (d - 900) * MS
    span = (play_to - play_from) // len(frame_paths)
    for i, _ in enumerate(frame_paths):
        t0 = play_from + i * span
        t1 = t0 + span
        els.append({"type": "image", "asset": f"f{i:02d}",
                    "rect": [742, 190, 480, 270], "animations": [window(t0, t1)]})
        els.append(text(f"frame {samples[i]['frameIndex']}   tick {samples[i]['tick']}",
                        "mono", 14, DIM, (742, 484), [window(t0, t1)]))

    scenes.append({"id": "compile", "duration": d * MS,
                   "transition": {"type": "crossfade", "duration": XFADE_MS * MS},
                   "elements": els})

    # --- s2: the claim ------------------------------------------------------
    d = 4200
    o0, o1 = tail(d)
    scenes.append({"id": "claim", "duration": d * MS,
                   "transition": {"type": "crossfade", "duration": XFADE_MS * MS},
                   "elements": [
        text(PRODUCT["claim_head"], "inter", 54, FG, (100, 250),
             [fade(420, o0, o1)], max_w=1000),
        text(PRODUCT["claim_body"], "mono", 21, DIM, (104, 424),
             [fade(800, o0, o1)], max_w=1000),
        {"type": "rect", "rect": [100, 492, 200, 5], "fill": ACCENT,
         "animations": [fade(0, o0, o1, start_v=1.0)]},
    ]})

    # --- s3: close ----------------------------------------------------------
    d = 3400
    o0, o1 = tail(d)
    scenes.append({"id": "close", "duration": d * MS,
                   "transition": {"type": "crossfade", "duration": XFADE_MS * MS},
                   "elements": [
        text(PRODUCT["name"], "inter", 62, FG, (100, 300), [fade(400, o0, o1)]),
        text(PRODUCT["close"], "mono", 20, ACCENT, (104, 392), [fade(760, o0, o1)]),
    ]})

    assets = {"inter": {"type": "font", "src": "kineto:inter"},
              "mono": {"type": "font", "src": "kineto:jetbrains-mono"}}
    for i, p in enumerate(frame_paths):
        assets[f"f{i:02d}"] = {"type": "image", "src": p}

    return {"v": 1, "timebase": TB, "defaultFps": 30,
            "size": {"w": W, "h": H}, "bg": BG,
            "assets": assets, "scenes": scenes}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--preview", action="store_true",
                    help="build and preview scenes; do not render an MP4")
    args = ap.parse_args()

    os.makedirs(OUT, exist_ok=True)
    mcp = Mcp()
    try:
        frames, samples, sample_path = render_sample_frames(mcp)
        print(f"rendered {len(frames)} sample frames -> {FRAMES}")

        doc = build(frames, samples, sample_path)
        doc_path = os.path.join(OUT, "readme-demo.json")
        with open(doc_path, "w") as f:
            json.dump(doc, f, indent=2)

        info = mcp.call("render_document",
                        {"documentPath": doc_path, "validateOnly": True})
        tl = info["structuredContent"]["timeline"]
        print(f"{len(doc['scenes'])} scenes | nominal {tl['nominalMs']}ms "
              f"-> actual {tl['actualMs']}ms ({tl['transitionOverlapMs']}ms in crossfades)")

        if args.preview:
            res = mcp.call("preview_document", {
                "documentPath": doc_path,
                "atScenes": [s["id"] for s in doc["scenes"]]})
            for c in res["content"]:
                if c["type"] == "text":
                    print(" ", c["text"])
            n = 0
            for c in res["content"]:
                if c["type"] == "image":
                    with open(f"/tmp/readme-scene-{n}.png", "wb") as f:
                        f.write(base64.b64decode(c["data"]))
                    n += 1
            print(f"wrote {n} scene previews to /tmp/readme-scene-*.png")
            return

        out_mp4 = os.path.join(OUT, "readme-demo.mp4")
        res = mcp.call("render_document",
                       {"documentPath": doc_path, "out": out_mp4, "previewFrames": 0})
        print(res["content"][0]["text"])
    finally:
        mcp.close()


if __name__ == "__main__":
    main()
