#!/usr/bin/env python3
"""Compose a Kineto scene spec from a repository's own git history.

    scripts/changelog-spec.py --title "Kineto 0.1.5" > spec.json
    kineto --scenes spec.json -o release.mp4

Release notes are a list of strings, and the commits already are that list.
Reading them beats maintaining a second one that drifts from the first.

Deliberately a *generator*, not part of Kineto: turning a list of strings into
a composed video is the tool's job, and reading git is not. The output is an
ordinary scene spec, so it can be edited, checked, or thrown away.

Works on any repository. Conventional-commit subjects are preferred when
present, because their prefixes say which changes were user-visible; where a
repository does not use them, every subject is a candidate and merges and
version bumps are dropped instead.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys

CONVENTIONAL = re.compile(r"(?P<type>[a-z]+)(?:\([^)]*\))?!?: (?P<text>.+)")

# Types whose changes a reader of release notes cares about. Everything else
# is real work that does not belong on a title card.
USER_VISIBLE = {"feat", "fix", "perf"}

# Subjects that are never interesting, in any repository.
NOISE = re.compile(
    r"^(merge\b|revert\b|bump\b|release\b|v?\d+\.\d+\.\d+$|wip\b)",
    re.IGNORECASE,
)


def subjects(rng: str, repo: str | None) -> list[str]:
    cmd = ["git"]
    if repo:
        cmd += ["-C", repo]
    cmd += ["log", "--no-merges", "--pretty=%s"]
    if rng:
        cmd.append(rng)
    out = subprocess.run(cmd, capture_output=True, text=True, check=True)
    return out.stdout.splitlines()


def default_range(repo: str | None) -> str:
    """Everything since the previous tag, or all history if there is none."""
    cmd = ["git"] + (["-C", repo] if repo else []) + ["describe", "--tags", "--abbrev=0", "HEAD^"]
    try:
        prev = subprocess.run(cmd, capture_output=True, text=True, check=True).stdout.strip()
        return f"{prev}..HEAD"
    except subprocess.CalledProcessError:
        return ""


def points(lines: list[str], limit: int, max_len: int) -> list[str]:
    """The user-visible changes, in order, deduplicated.

    Two passes: conventional-commit types first, and if a repository does not
    use them, anything that is not obvious noise. A generator that only worked
    on projects sharing this one's conventions would be a demo of this project
    rather than of the tool.
    """
    # Whether the *repository* uses conventional commits, not whether this
    # range happened to contain a user-visible one. A project that uses them
    # and shipped only chores should say so, rather than listing the chores as
    # though they were features.
    uses_conventional = any(CONVENTIONAL.fullmatch(l.strip()) for l in lines if l.strip())

    def collect(strict: bool) -> list[str]:
        seen: set[str] = set()
        found: list[str] = []
        for line in lines:
            line = line.strip()
            if not line or NOISE.match(line):
                continue
            m = CONVENTIONAL.fullmatch(line)
            if strict:
                if not m or m.group("type") not in USER_VISIBLE:
                    continue
                text = m.group("text")
            else:
                text = m.group("text") if m else line
            # Long lines stop being scannable, which is all a release video is
            # good for. Truncating mid-sentence reads worse than dropping it.
            if len(text) > max_len or text.lower() in seen:
                continue
            seen.add(text.lower())
            found.append(text)
            if len(found) == limit:
                break
        return found

    return collect(strict=True) if uses_conventional else collect(strict=False)


def spec(args: argparse.Namespace, items: list[str]) -> dict:
    scenes: list[dict] = [
        {"kind": "title", "text": args.title, "subtitle": args.subtitle}
        if args.subtitle
        else {"kind": "title", "text": args.title}
    ]
    scenes.append({"kind": "points", "heading": args.heading, "items": items})
    if args.install:
        scenes.append({"kind": "code", "heading": "Install", "items": args.install})
    return {
        "theme": args.theme,
        "width": args.width,
        "height": args.height,
        "scenes": scenes,
    }


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--title", required=True, help='e.g. "Kineto 0.1.5"')
    p.add_argument("--subtitle", default=None)
    p.add_argument("--heading", default="What changed")
    p.add_argument("--range", dest="rng", default=None,
                   help="git range; defaults to everything since the previous tag")
    p.add_argument("--repo", default=None, help="path to the repository")
    p.add_argument("--theme", default="midnight", choices=["midnight", "paper"])
    p.add_argument("--width", type=int, default=1280)
    p.add_argument("--height", type=int, default=720)
    p.add_argument("--max-points", type=int, default=4)
    p.add_argument("--max-length", type=int, default=58)
    p.add_argument("--install", action="append", default=None,
                   help="a line for a closing Install scene; repeatable")
    p.add_argument("-o", "--out", default=None, help="write here instead of stdout")
    args = p.parse_args()

    rng = args.rng if args.rng is not None else default_range(args.repo)
    items = points(subjects(rng, args.repo), args.max_points, args.max_length)
    if not items:
        # An honest card beats an empty list the builder would reject.
        items = ["maintenance and internal changes"]

    text = json.dumps(spec(args, items), indent=2) + "\n"
    if args.out:
        with open(args.out, "w") as fh:
            fh.write(text)
        print(f"wrote {args.out} — {len(items)} change(s) from {rng or 'all history'}",
              file=sys.stderr)
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
