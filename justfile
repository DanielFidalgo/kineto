# Kineto — common tasks.
#   just            list recipes
#   just check      everything CI checks, locally
#   just install    build and register the MCP server with Claude Code

# Honours CARGO_TARGET_DIR, which is easy to have set and easy to forget.
cargo_target := env_var_or_default("CARGO_TARGET_DIR", justfile_directory() / "target")
built_mcp := cargo_target / "release/kineto-mcp"
install_dir := env_var("HOME") / ".local/bin"

default:
    @just --list --unsorted

# ---------------------------------------------------------------- the MCP ---

# Build the MCP server (release).
build-mcp:
    cargo build -p kineto-mcp --release

# Build it, copy it somewhere stable, and register it with Claude Code.
#
# Copied out of the cargo target directory on purpose: `cargo clean` would
# otherwise delete a binary every other project's MCP config points at.
install: build-mcp
    mkdir -p "{{install_dir}}"
    cp "{{built_mcp}}" "{{install_dir}}/kineto-mcp"
    claude mcp add --scope user kineto "{{install_dir}}/kineto-mcp"
    @echo "registered. start a new session and ask for a video."

# Remove the registration (leaves the binary in place).
uninstall:
    claude mcp remove --scope user kineto

# Print the path to give any other MCP client.
mcp-path: build-mcp
    @echo "{{built_mcp}}"

# ------------------------------------------------------------------ checks ---

# Everything CI runs, in the same order.
check: fmt-check lint test parity
    @echo "all green"

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
parity: wasm-corpus
    cargo run -p kineto-core --bin dump-parity --features parity
    node tests/parity/run.mjs

# Regenerate golden hashes after an intentional renderer change. Read the
# diff before committing: a golden that moves without a reason is the bug.
golden:
    UPDATE_GOLDEN=1 cargo test -p kineto-core -- --test-threads=1

# ------------------------------------------------------------------- wasm ---

# Release wasm build. Run from crates/wasm so .cargo/config.toml applies and
# simd128 is actually enabled — without it the browser renders ~4x slower.
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
cast input dir:
    cargo run -q -p kineto-asciicast --bin kineto-cast -- {{input}} -o {{dir}}

# Rebuild the README hero video and the inline loop from source.
media: build-mcp
    python3 docs/media/build-hero.py
    python3 docs/media/build-hero.py --loop
    @echo "wrote docs/media/"

# --------------------------------------------------------------------- dev ---

# Delete build output. Keeps the golden hashes, which are source.
clean:
    cargo clean
    rm -rf out crates/wasm/pkg
