#!/usr/bin/env python3
"""Tests for the changelog generator.

Run with `python3 scripts/test_changelog_spec.py`; CI runs it too.

The load-bearing behaviour is the fallback. A generator that only worked on
repositories following this project's commit conventions would demonstrate
this project rather than the tool, so the interesting cases are the ones with
no conventional commits at all.
"""

import importlib.util
import json
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("changelog_spec", HERE / "changelog-spec.py")
mod = importlib.util.module_from_spec(spec)
assert spec.loader
spec.loader.exec_module(mod)

failures: list[str] = []


def check(name: str, got, want) -> None:
    if got == want:
        print(f"  ok   {name}")
    else:
        failures.append(f"{name}\n       got:  {got!r}\n       want: {want!r}")
        print(f"  FAIL {name}")


def repo_with(subjects: list[str]) -> str:
    d = tempfile.mkdtemp()
    run = lambda *a: subprocess.run(["git", "-C", d, *a], check=True, capture_output=True)
    run("init", "-q", ".")
    run("config", "user.email", "t@t.t")
    run("config", "user.name", "T")
    for s in subjects:
        (Path(d) / "log.txt").write_text(s + "\n")
        run("add", "-A")
        run("commit", "-q", "-m", s)
    return d


# Conventional commits: only user-visible types, newest first.
check(
    "conventional keeps feat/fix/perf and drops the rest",
    mod.points(
        ["feat: a thing", "chore: tidy", "fix: a crash", "docs: words", "perf: faster"],
        limit=4,
        max_len=58,
    ),
    ["a thing", "a crash", "faster"],
)

# The fallback: no conventional commits anywhere.
check(
    "falls back to plain subjects when no conventional ones exist",
    mod.points(["Add dark mode", "Fix a crash"], limit=4, max_len=58),
    ["Add dark mode", "Fix a crash"],
)

check(
    "noise is dropped in the fallback",
    mod.points(
        ["Merge branch 'x'", "Bump version to 2.0.0", "Add dark mode", "Revert something"],
        limit=4,
        max_len=58,
    ),
    ["Add dark mode"],
)

check(
    "duplicates collapse",
    mod.points(["Add dark mode", "add dark MODE"], limit=4, max_len=58),
    ["Add dark mode"],
)

check(
    "overlong subjects are dropped rather than truncated mid-sentence",
    mod.points(["x" * 80, "Short one"], limit=4, max_len=58),
    ["Short one"],
)

check("the limit is honoured", len(mod.points([f"feat: n{i}" for i in range(9)], 3, 58)), 3)

# A repository that has conventional commits but none user-visible must not
# fall through to chores; it should say nothing rather than something wrong.
check(
    "chores-only yields nothing, not the chores",
    mod.points(["chore: tidy", "docs: words"], limit=4, max_len=58),
    [],
)

# End to end against a real repository with no conventions.
d = repo_with(["Initial commit", "Add dark mode toggle", "Merge branch 'x'", "Fix crash"])
got = mod.points(mod.subjects("", d), 4, 58)
check("reads a real repository", got, ["Fix crash", "Add dark mode toggle", "Initial commit"])

if failures:
    print("\n" + "\n".join(failures))
    sys.exit(1)
print("changelog-spec: all ok")
