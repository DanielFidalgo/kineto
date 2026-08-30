# Kineto — common tasks.
#   just            list recipes
#   just check      everything CI checks, locally
#   just install    build and register the MCP server with Claude Code

# Honours CARGO_TARGET_DIR, which is easy to have set and easy to forget.
cargo_target := env_var_or_default("CARGO_TARGET_DIR", justfile_directory() / "target")
built_mcp := cargo_target / "release/kineto-mcp"
kineto := cargo_target / "release/kineto"
install_dir := env_var("HOME") / ".local/bin"

default:
    @just --list --unsorted

# ---------------------------------------------------------------- the MCP ---

# Build the release binaries: the MCP server and the `kineto` CLI.
build:
    cargo build -p kineto --release --bins

# Alias kept because the README's manual install mentions it.
build-mcp: build

# Build it, copy it somewhere stable, and register it with Claude Code.
#
# Copied out of the cargo target directory on purpose: `cargo clean` would
# otherwise delete a binary every other project's MCP config points at.
# Build, install to ~/.local/bin, and register with Claude Code.
install: build
    mkdir -p "{{install_dir}}"
    cp "{{built_mcp}}" "{{install_dir}}/kineto-mcp"
    claude mcp add --scope user kineto "{{install_dir}}/kineto-mcp"
    @echo "registered. start a new session and ask for a video."

# Remove the registration (leaves the binary in place).
uninstall:
    claude mcp remove --scope user kineto

# Print the path to give any other MCP client.
mcp-path: build
    @echo "{{built_mcp}}"

# ------------------------------------------------------------------ checks ---

# Everything CI runs, in the same order.
# Everything CI runs, so a push is not the first place a failure shows up.
#
# `typecheck` is here because it once was not: `vite build` never type-checks,
# so a TypeScript error can pass a local build, pass the tests, and fail only
# in CI's web job.
check: fmt-check lint test typecheck scripts parity
    @echo "all green"

# The changelog generator, which composes every release video.
scripts:
    python3 scripts/test_changelog_spec.py

# tsc across the TypeScript packages.
typecheck:
    npm -ws run typecheck --if-present

test:
    cargo test --workspace

lint:
    cargo clippy --workspace --all-targets -- -D warnings

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all --check

# The byte-identical gate: native aarch64/x86_64 against wasm+simd128.
# Must run from the repo root — run.mjs resolves paths against the cwd.
# The byte-identical gate: native against wasm+simd128.
parity: wasm-corpus
    cargo run -p kineto-core --bin dump-parity --features parity
    node tests/parity/run.mjs

# Regenerate golden hashes after an intentional renderer change. Read the
# diff before committing: a golden that moves without a reason is the bug.
# Regenerate golden hashes after an intentional renderer change.
golden:
    UPDATE_GOLDEN=1 cargo test -p kineto-core -- --test-threads=1

# ------------------------------------------------------------------- wasm ---

# Release wasm build. Run from crates/wasm so .cargo/config.toml applies and
# simd128 is actually enabled — without it the browser renders ~4x slower.
# Release wasm build, with simd128 enabled.
wasm:
    cd crates/wasm && wasm-pack build . --target web --release

# Same, plus the corpus feature the parity gate needs.
wasm-corpus:
    cd crates/wasm && wasm-pack build . --target web --release -- --features corpus

# ---------------------------------------------------------------- packages ---

ts-test: wasm
    npm ci
    npm -ws run typecheck --if-present
    npm -ws run test --if-present

# The flagship browser demo on http://localhost:5200
demo: wasm
    npm -w @kineto/demo-tape run dev

# --------------------------------------------------------------- rendering ---

# Turn an asciinema recording into frames plus an out.mp4, headlessly.
#   just cast adapters/asciicast/tests/fixture.cast out/demo
# Turn an asciinema recording into frames plus an out.mp4.
cast input dir:
    cargo run -q -p kineto-asciicast --bin kineto-cast -- {{input}} -o {{dir}}

# Regenerate the playground's generated examples.
#
# themed-scenes and chart come from the real builders rather than being
# hand-written, so an example a visitor copies cannot drift from what the
# tools actually emit. hello.json and motion.json are hand-authored on
# purpose: they are the ones people read and edit, and generated output is
# too verbose to learn from.
examples: build
    KINETO_MCP="{{cargo_target}}/release/kineto-mcp" node scripts/playground-examples.mjs
    for f in packages/playground/public/examples/*.json; do "{{kineto}}" "$f" --check; done

# Rebuild the README video, the inline loop and the poster.
#
# docs/media/hero.json IS the source — the thing this project claims you
# author. Check it, then render it, three ways. No other tool is involved:
# the loop is scaled by the engine, and the poster is a frame the engine
# writes. Both used to be ffmpeg calls standing in for gaps that are now
# closed.
# Rebuild the README video, loop, poster and social card.
media: build
    "{{kineto}}" docs/media/hero.json --check
    "{{kineto}}" docs/media/hero.json -o docs/media/kineto-hero.mp4
    "{{kineto}}" docs/media/hero.json -o docs/media/kineto-poster.png --at 1600 --width 960
    "{{kineto}}" docs/media/hero-loop.json -o docs/media/kineto-loop.webp --width 960
    "{{kineto}}" docs/media/social.json --check
    "{{kineto}}" docs/media/social.json -o docs/media/social.png --at 0
    @echo "wrote docs/media/ — upload social.png at Settings → Social preview"

# ---------------------------------------------------------------- releasing ---

# Cut a release: set the version, commit, tag, push. The tag is the trigger —
# .github/workflows/release.yml builds and publishes from it.
#
#   just release 0.2.0
#
# Refuses on a dirty tree or a tag that already exists, because both produce a
# release whose contents nobody can reconstruct from the tag.
# Cut a release: check, set version, commit, tag.
release version:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -n "$(git status --porcelain)" ]; then
        echo "error: working tree is dirty" >&2; exit 1
    fi
    if git rev-parse "v{{version}}" >/dev/null 2>&1; then
        echo "error: tag v{{version}} already exists" >&2; exit 1
    fi
    just check
    # awk, not `sed -i -E '0,/re/'`: that address form is a GNU extension, and
    # BSD sed on macOS exits 0 having changed nothing — which would tag a
    # version the manifest does not carry. CI would refuse it, but only after
    # a push.
    awk -v v='{{version}}' 'BEGIN{d=0} /^version = "/ && !d {sub(/"[^"]*"/, "\"" v "\""); d=1} {print}' \
        Cargo.toml > Cargo.toml.new
    grep -q '^version = "{{version}}"' Cargo.toml.new || { rm -f Cargo.toml.new; echo "error: version rewrite did not take" >&2; exit 1; }
    mv Cargo.toml.new Cargo.toml
    # [workspace.dependencies] entries carry their own version for crates.io.
    # The pass above only touches a line *starting* with `version = `, so these
    # need a second one -- and cargo will not complain if they are left behind
    # on a patch bump, since ^0.1.0 matches 0.1.1.
    awk -v v='{{version}}' '/path = "/ { gsub(/version = "[^"]*"/, "version = \"" v "\"") } {print}' \
        Cargo.toml > Cargo.toml.new
    mv Cargo.toml.new Cargo.toml
    cargo update -w --quiet          # refresh Cargo.lock's own version entries
    # The npm wrapper carries the same version in five places and pins its
    # platform packages exactly, so it moves in lockstep with the crates.
    node scripts/bump-npm-version.mjs '{{version}}'
    # tests/manifest.rs is the authority on every version in the repo agreeing
    # -- cargo path deps and the npm wrapper's five. It runs after *every*
    # rewrite, never between them. Ordered before the npm bump it failed each
    # release on precisely the version it was about to set, which it did.
    cargo test -p kineto --test manifest
    git add Cargo.toml Cargo.lock packages/mcp/package.json
    # Re-releasing the version already in the manifest is the normal case for
    # the very first tag, and produces no diff. `git commit` would exit 1 and
    # take the tag down with it.
    if git diff --cached --quiet; then
        echo "manifest already at {{version}} — tagging this commit"
    else
        git commit -m "chore: release v{{version}}"
    fi
    git tag -a "v{{version}}" -m "v{{version}}"
    # Plain echo, not `@echo`: in a shebang recipe every line is script text,
    # so `@` is not just's line-prefix here — bash looks for a command called
    # `@echo` and exits 127, after the tag has already been made.
    echo "tagged v{{version}} — push with: git push && git push --tags"

# What version is this?
version:
    @grep -m1 '^version' Cargo.toml | cut -d'"' -f2

# --------------------------------------------------------------------- dev ---

# Delete build output. Keeps the golden hashes, which are source.
clean:
    cargo clean
    rm -rf out crates/wasm/pkg
