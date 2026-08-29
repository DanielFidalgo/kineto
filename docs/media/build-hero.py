#!/usr/bin/env python3
"""Build the README hero video and the short inline loop.

    python3 docs/media/build-hero.py           # 27s hero  -> docs/media/kineto-hero.mp4
    python3 docs/media/build-hero.py --loop    # 8s  loop  -> docs/media/kineto-loop.webp

Made of the primitives it advertises: gradients, corner radius, shadows,
clip windows and overshoot easings. Checked with `check_document` before it
renders, which is why the copy is as short as it is — the first draft ran at
479 wpm and the linter refused it.

Requires the MCP server built (`just build-mcp`) and ffmpeg on PATH.
"""
import importlib.util, json, os, subprocess, sys

TB = 705_600_000; MS = TB // 1000
W, H = 1280, 720
BG   = "#0B1116"
FG   = "#F4F7F9"
DIM  = "#8FA3B0"
CODE = "#7C93A3"
AMBER  = "#FF9F45"
TEAL   = "#4ECDC4"
VIOLET = "#C77DFF"
PANEL  = "#16212a"
EDGE   = "#2b3d4a"

XFADE = 260
LOOP = "--loop" in sys.argv
TARGET = 8_000 if LOOP else 27_000
REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OUT = os.path.join(REPO, "out")
MEDIA = os.path.join(REPO, "docs", "media")

spec = importlib.util.spec_from_file_location("kmcp", "/tmp/kmcp.py")
kmcp = importlib.util.module_from_spec(spec); spec.loader.exec_module(kmcp)
_target = os.environ.get("CARGO_TARGET_DIR", os.path.join(REPO, "target"))
kmcp.BIN = os.path.join(_target, "release", "kineto-mcp")


def txt(t, f, s, c, pos, anims, mw=None):
    e = {"type": "text", "text": t, "font": f, "sizePx": s, "color": c,
         "pos": list(pos), "animations": anims}
    if mw: e["maxW"] = mw
    return e


def fade(hold, o0, o1, v0=0.0):
    k = [{"t": 0, "v": v0}]
    if v0 == 0.0:
        k.append({"t": hold * MS, "v": 1.0, "ease": "outCubic"})
    k += [{"t": o0, "v": 1.0}, {"t": o1, "v": 0.0, "ease": "outCubic"}]
    return {"prop": "opacity", "keys": k}


def rise(delay, dist=34.0):
    """Enter from below and overshoot slightly — the thing that reads as alive."""
    return {"prop": "translate", "keys": [
        {"t": 0, "v": [0, dist]},
        {"t": delay * MS, "v": [0, dist]},
        {"t": (delay + 620) * MS, "v": [0, 0], "ease": "outBack"}]}


def wash(a, b):
    """A full-canvas gradient so a shot does not sit on the flat background."""
    return {"type": "path", "closed": True,
            "points": [[0, 0], [W, 0], [W, H], [0, H]],
            "fill": {"type": "linear", "from": [0, 0], "to": [1, 1],
                     "stops": [{"at": 0, "color": a}, {"at": 1, "color": b}]}}


def build(dur):
    S = []
    def tail(d):
        o1 = (d - XFADE) * MS
        return o1 - 380 * MS, o1
    def scene(i, els, sid):
        s = {"id": sid, "duration": dur[i] * MS, "elements": els}
        if i > 0:
            s["transition"] = {"type": "crossfade", "duration": XFADE * MS}
        S.append(s)

    # 0 — open
    d = dur[0]; o0, o1 = tail(d)
    scene(0, [
        wash("#132433", BG),
        txt("Kineto", "inter", 92, FG, [110, 262], [fade(380, o0, o1), rise(120, 26)]),
        txt("video as a build artifact", "mono", 25, DIM, [116, 392],
            [fade(760, o0, o1)]),
        {"type": "path", "points": [[112, 462], [352, 462]], "stroke": AMBER,
         "strokeWidth": 6, "cap": "round", "animations": [
             {"prop": "translate", "keys": [
                 {"t": 0, "v": [-300, 0]},
                 {"t": 820 * MS, "v": [0, 0], "ease": "outCubic"}]},
             fade(0, o0, o1, 1.0)]},
    ], "open")

    # 1 — the claim, full bleed
    d = dur[1]; o0, o1 = tail(d)
    scene(1, [
        wash("#0B1116", "#101d27"),
        txt("You write a document.\nIt compiles to a video.", "inter", 62, FG,
            [110, 258], [fade(420, o0, o1)], mw=1060),
        txt("the same way source compiles to a binary", "mono", 21, AMBER,
            [116, 434], [fade(900, o0, o1)]),
    ], "claim")

    # 2 — split: the document, and what it becomes
    d = dur[2]; o0, o1 = tail(d)
    code = ('{\n  "size": { "w": 1280, "h": 720 },\n'
            '  "scenes": [{\n    "duration": 705600000,\n'
            '    "elements": [ ... ]\n  }]\n}')
    scene(2, [
        txt("THE DOCUMENT", "mono", 15, AMBER, [110, 150], [fade(280, o0, o1)]),
        txt(code, "mono", 17, CODE, [110, 200], [fade(520, o0, o1)], mw=470),
        # arrow across
        {"type": "path", "points": [[600, 360], [660, 360]], "stroke": AMBER,
         "strokeWidth": 3, "cap": "round", "animations": [fade(900, o0, o1)]},
        {"type": "path", "closed": True, "fill": AMBER,
         "points": [[676, 360], [660, 353], [660, 367]],
         "animations": [fade(900, o0, o1)]},
        # the panel it becomes
        {"type": "rect", "rect": [720, 190, 450, 340],
         "fill": {"type": "linear", "from": [0, 0], "to": [1, 1],
                  "stops": [{"at": 0, "color": "#1d3040"},
                            {"at": 1, "color": "#141f29"}]},
         "radius": 18,
         "shadow": {"color": "#00000073", "blur": 26, "dy": 14},
         "animations": [fade(1000, o0, o1), rise(1000, 28)]},
        {"type": "rect", "rect": [760, 250, 300, 10], "fill": TEAL, "radius": 5,
         "animations": [fade(1250, o0, o1)]},
        {"type": "rect", "rect": [760, 288, 190, 10], "fill": "#2c3d4a", "radius": 5,
         "animations": [fade(1330, o0, o1)]},
        {"type": "rect", "rect": [760, 326, 250, 10], "fill": "#2c3d4a", "radius": 5,
         "animations": [fade(1410, o0, o1)]},
        {"type": "rect", "rect": [760, 420, 120, 60], "fill": VIOLET, "radius": 12,
         "animations": [fade(1500, o0, o1)]},
    ], "split")

    # 3 — three properties, arriving in sequence
    d = dur[3]; o0, o1 = tail(d)
    els = [txt("WHY", "mono", 15, AMBER, [110, 150],
               [fade(280, o0, o1)]),
           txt("No browser. No display.", "inter", 46, FG, [110, 186],
               [fade(420, o0, o1)])]
    cards = [("deterministic", "same document,\nsame pixels", AMBER),
             ("headless", "renders in CI", TEAL),
             ("inspectable", "read it like code", VIOLET)]
    for i, (name, sub, col) in enumerate(cards):
        x = 110.0 + i * 356
        dl = 620 + i * 200
        els += [
            {"type": "rect", "rect": [x, 320, 320, 210], "fill": PANEL, "radius": 16,
             "shadow": {"color": "#00000059", "blur": 20, "dy": 10},
             "animations": [fade(dl, o0, o1), rise(dl)]},
            txt(name, "mono", 20, FG, [x + 26, 356], [fade(dl + 140, o0, o1)]),
            {"type": "path", "points": [[x + 26, 396], [x + 96, 396]],
             "stroke": col, "strokeWidth": 4, "cap": "round",
             "animations": [fade(dl + 200, o0, o1)]},
            txt(sub, "mono", 16, DIM, [x + 26, 424], [fade(dl + 260, o0, o1)], mw=270),
        ]
    scene(3, els, "cards")

    # 4 — the number, revealed behind a window
    d = dur[4]; o0, o1 = tail(d)
    bx, by, bw, bh = 110.0, 430.0, 1060.0, 26.0
    scene(4, [
        txt("PROVEN, NOT CLAIMED", "mono", 15, AMBER, [110, 150], [fade(280, o0, o1)]),
        txt("27 / 27", "inter", 122, FG, [110, 200], [fade(420, o0, o1)]),
        txt("frames byte-identical, native and WebAssembly",
            "mono", 21, DIM, [116, 366], [fade(780, o0, o1)]),
        {"type": "rect", "rect": [bx, by, bw, bh], "fill": "#16212a", "radius": 13,
         "animations": [fade(900, o0, o1)]},
        # slides in behind a fixed window: the bar fills
        {"type": "rect", "rect": [bx, by, bw, bh], "radius": 13,
         "fill": {"type": "linear", "from": [0, 0], "to": [1, 0],
                  "stops": [{"at": 0, "color": AMBER}, {"at": 1, "color": VIOLET}]},
         "clip": {"rect": [bx, by, bw, bh], "radius": 13},
         "animations": [
             {"prop": "translate", "keys": [
                 {"t": 0, "v": [-bw, 0]},
                 {"t": 1000 * MS, "v": [-bw, 0]},
                 {"t": 2100 * MS, "v": [0, 0], "ease": "outCubic"}]},
             fade(0, o0, o1, 1.0)]},
        txt("checked on every build", "mono", 16, CODE, [116, 484],
            [fade(1800, o0, o1)]),
    ], "proof")

    # 5 — close
    d = dur[5]; o0, o1 = tail(d)
    scene(5, [
        wash("#152838", BG),
        txt("Kineto", "inter", 76, FG, [110, 268], [fade(360, o0, o1), rise(120, 22)]),
        txt("video as a build artifact", "mono", 23, AMBER, [116, 378],
            [fade(720, o0, o1)]),
        txt("Rust · WebAssembly · MIT OR Apache-2.0", "mono", 17, DIM,
            [116, 428], [fade(1000, o0, o1)]),
        {"type": "path", "points": [[112, 486], [292, 486]], "stroke": AMBER,
         "strokeWidth": 5, "cap": "round", "animations": [fade(1200, o0, o1)]},
    ], "close")

    return {"v": 1, "timebase": TB, "defaultFps": 30, "size": {"w": W, "h": H},
            "bg": BG,
            "assets": {"inter": {"type": "font", "src": "kineto:inter"},
                       "mono": {"type": "font", "src": "kineto:jetbrains-mono"}},
            "scenes": S}


def main():
    loop = LOOP
    # For the loop cut, size the durations for the two scenes that survive
    # *before* building: the fade keys are derived from each scene's duration,
    # so rewriting it afterwards leaves keyframes out of order.
    n = 2 if loop else 6
    need = TARGET + (n - 1) * XFADE
    base = need // n
    per = [base] * (n - 1) + [need - base * (n - 1)]
    dur = per + [base] * (6 - n)
    doc = build(dur)
    if loop:
        doc["scenes"] = doc["scenes"][:2]
        doc["scenes"][0].pop("transition", None)
    p = os.path.join(OUT, "hero-loop.json" if loop else "hero.json")
    os.makedirs(OUT, exist_ok=True)
    json.dump(doc, open(p, "w"))

    k = kmcp.Kineto()
    tl = k.call("render_document", {"documentPath": p, "validateOnly": True})
    t = tl["structuredContent"]["timeline"]
    print(f"timeline: nominal {t['nominalMs']} -> actual {t['actualMs']}ms "
          f"(target {TARGET}: {t['actualMs'] == TARGET})")
    ids = [s["id"] for s in doc["scenes"]]
    chk = k.call("check_document", {"documentPath": p, "atScenes": ids})
    for line in chk["content"][0]["text"].splitlines():
        print("  ", line)
    if chk["structuredContent"]["issueCount"]:
        raise SystemExit("check_document reported issues; fix them before rendering")

    os.makedirs(MEDIA, exist_ok=True)
    if loop:
        # Rendered at 720p then scaled to the width a README actually shows.
        # Animated WebP has no inter-frame prediction, so pixels are the whole
        # cost: 1280px was 2.5 MB, 960px is 1.6 MB for the same 8 seconds.
        tmp = os.path.join(OUT, "loop.mp4")
        print(k.call("render_document", {"documentPath": p, "out": tmp,
                                         "previewFrames": 0})["content"][0]["text"])
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", tmp,
                        "-vf", "scale=960:-2", "-c:v", "libwebp", "-lossless", "0",
                        "-q:v", "82", "-compression_level", "4", "-preset", "picture",
                        "-loop", "0", os.path.join(MEDIA, "kineto-loop.webp")], check=True)
        print("wrote docs/media/kineto-loop.webp")
    else:
        out = os.path.join(MEDIA, "kineto-hero.mp4")
        print(k.call("render_document", {"documentPath": p, "out": out,
                                         "previewFrames": 0})["content"][0]["text"])
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-ss", "1.6", "-i", out,
                        "-frames:v", "1", "-vf", "scale=960:-2",
                        os.path.join(MEDIA, "kineto-poster.png")], check=True)
        print("wrote docs/media/kineto-poster.png")
    return k, p, ids


if __name__ == "__main__":
    k, p, ids = main(); k.close()
