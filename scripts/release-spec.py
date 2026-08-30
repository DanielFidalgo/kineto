#!/usr/bin/env python3
"""Compose the scene spec for a release video from the tag's own commits.

The release page should say what changed, and the commits already do. Reading
them beats maintaining a second list that drifts from the first.

    scripts/release-spec.py 0.1.4 > spec.json

Kept as a file rather than inline in the workflow so it can be run — a heredoc
inside a YAML `run:` block is indentation-fragile and impossible to test
without pushing a tag.
"""

import json
import re
import subprocess
import sys

# Longer than this and a line stops being scannable at a glance, which is the
# only thing a release video is good for.
MAX_POINT = 58
MAX_POINTS = 4


def subjects(version: str) -> list[str]:
    """Commit subjects introduced by this tag."""
    tag = f"v{version}"
    try:
        prev = subprocess.run(
            ["git", "describe", "--tags", "--abbrev=0", f"{tag}^"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
        rng = f"{prev}..{tag}"
    except subprocess.CalledProcessError:
        # The first release has nothing before it.
        rng = tag
    out = subprocess.run(
        ["git", "log", "--pretty=%s", rng], capture_output=True, text=True, check=True
    )
    return out.stdout.splitlines()


def points(lines: list[str]) -> list[str]:
    """User-visible changes, in order, deduplicated."""
    seen: set[str] = set()
    found: list[str] = []
    for line in lines:
        m = re.match(r"(feat|fix)(?:\([^)]*\))?: (.+)", line.strip())
        if not m:
            continue
        text = m.group(2)
        if text.lower() in seen or len(text) > MAX_POINT:
            continue
        seen.add(text.lower())
        found.append(text)
        if len(found) == MAX_POINTS:
            break
    # A release with no feat or fix is a real release; say something true
    # rather than emitting an empty list the builder would reject.
    return found or ["maintenance and internal changes"]


def spec(version: str, items: list[str]) -> dict:
    return {
        "theme": "midnight",
        "width": 1280,
        "height": 720,
        "scenes": [
            {
                "kind": "title",
                "text": f"Kineto {version}",
                "subtitle": "video as a build artifact",
            },
            {"kind": "points", "heading": "What changed", "items": items},
            {
                "kind": "code",
                "heading": "Install",
                "items": ["cargo install kineto", "npx kineto-mcp"],
            },
        ],
    }


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: release-spec.py <version>", file=sys.stderr)
        return 1
    version = sys.argv[1].lstrip("v")
    print(json.dumps(spec(version, points(subjects(version))), indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
