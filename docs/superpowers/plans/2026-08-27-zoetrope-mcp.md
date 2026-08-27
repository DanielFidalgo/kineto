# Zoetrope MCP Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose the existing headless native zoetrope engine to agents as a
stdio MCP server with three render tools and two read-only resource families.

**Architecture:** A new leaf binary crate `crates/mcp` depends on
`zoetrope-core` and `zoetrope-asciicast` and speaks MCP over stdio via the
official `rmcp` SDK. It adds no dependency to any existing crate and does not
enter the wasm build graph. Internally it is four small modules — error
mapping, document/asset loading, render+preview, storyboard document
building — with a thin tool layer on top. Every render path funnels through
one function so behavior cannot drift between tools.

**Tech Stack:** Rust 1.97, `rmcp` 3.1.4 (stdio transport, `schemars`-derived
tool schemas), tokio (io only), `zoetrope-core` (tiny-skia + cosmic-text),
ffmpeg via shell-out for muxing.

**Spec:** `docs/superpowers/specs/2026-08-27-zoetrope-mcp-design.md`

## Global Constraints

These apply to every task. They are copied verbatim from the spec and the
repo guide; do not restate or renegotiate them per task.

- **Determinism is law.** Pure `(doc, tick) → pixels`. No system fonts — assets
  only. No network fetching, ever. No fast-math-style codegen flags.
- **Time is `i64` ticks at `TIMEBASE = 705_600_000`/s.** fps is an export hint.
  An fps value is only legal if `TIMEBASE % fps == 0`.
- **`crates/mcp` is a leaf crate.** Nothing in `crates/core`, `crates/wasm`, or
  `adapters/asciicast` may gain a dependency on it. The single permitted edit
  outside the new crate is the `Theme` field-type widening in Task 5, which
  adds no dependency.
- **`rmcp` is pinned to `3.1.4` with `default-features = false`** and exactly
  the features `["server", "macros", "transport-io", "base64"]`. Do not enable
  `client`, `reqwest`, or any `transport-streamable-http-*` feature.
- **Missing ffmpeg is a loud error, never a silent PNG-sequence fallback.**
  `zoetrope_core::export::mux_with_ffmpeg` returns `Ok(false)` both when ffmpeg
  is absent and when it ran and failed; the server must distinguish these and
  surface both as errors.
- **Tool failures are `Ok(CallToolResult::error(...))`, not `Err(ErrorData)`.**
  rmcp's own docs are explicit: MCP clients render protocol errors opaquely, so
  an `Err` message never reaches the caller. Reserve `Err(ErrorData)` for
  requests the server cannot route at all.
- **License headers/manifest fields:** `license = "MIT OR Apache-2.0"` via
  `license.workspace = true`, `edition.workspace = true`.
- **No brand strings in code.** The codename `zoetrope` is what exists today;
  the published name is decided at brand time.
- **TDD, conventional commits, commit after every green test cycle.**
- **Comment non-obvious `default-features` choices in the manifest**, matching
  the existing convention in `crates/core/Cargo.toml`.

## File Structure

| File | Responsibility |
|---|---|
| `crates/mcp/Cargo.toml` | Manifest, pinned rmcp feature set |
| `crates/mcp/src/main.rs` | Binary entry: build server, serve stdio, wait |
| `crates/mcp/src/lib.rs` | `ZoetropeServer` struct, `ServerHandler` impl, module wiring |
| `crates/mcp/src/error.rs` | `ToolError` and its conversion into `CallToolResult` |
| `crates/mcp/src/source.rs` | Document loading and asset resolution from the filesystem |
| `crates/mcp/src/render.rs` | ffmpeg preflight, mux pipeline, frame sampling |
| `crates/mcp/src/storyboard.rs` | Builds a `Document` from an ordered image list |
| `crates/mcp/src/tools.rs` | The three `#[tool]` functions and their parameter structs |
| `crates/mcp/src/resources.rs` | Schema and corpus resource listing/reading |
| `crates/mcp/tests/harness.rs` | Shared stdio JSON-RPC test driver |
| `crates/mcp/tests/protocol.rs` | Handshake, `tools/list`, `resources/*` transcripts |
| `crates/mcp/tests/tools.rs` | End-to-end tool call tests |
| `crates/mcp/tests/parity.rs` | Corpus rendered through the server's loading path vs committed golden hashes |

**A note on "golden transcripts".** The protocol tests assert *specific fields*
of each response frame, not whole-frame byte equality. Whole-frame goldens
would break on every rmcp bump — new optional fields like `ttlMs` and
`cacheScope` appear routinely — for no detection benefit. What we actually
need to catch is drift in *our* tool names, descriptions, and derived input
schemas, so that is what gets asserted exactly.

---

### Task 1: Scaffold the crate and get a stdio handshake

**Files:**
- Create: `crates/mcp/Cargo.toml`
- Create: `crates/mcp/src/lib.rs`
- Create: `crates/mcp/src/main.rs`
- Create: `crates/mcp/tests/harness.rs`
- Create: `crates/mcp/tests/protocol.rs`
- Modify: `Cargo.toml` (workspace `members`)

**Interfaces:**
- Consumes: nothing.
- Produces: `zoetrope_mcp::ZoetropeServer` with `ZoetropeServer::new() -> Self`
  and `impl ServerHandler for ZoetropeServer`. Binary name `zoetrope-mcp`.
  Test harness `harness::Server` with `start()`, `send(&Value)`,
  `recv() -> Value`, `initialize() -> Value`, `request(&Value) -> Value`.

This task deliberately implements `ServerHandler` **by hand** with only
`get_info`, and declares no tool capability. The `#[tool_router]` /
`#[tool_handler]` macros arrive in Task 4 alongside the first real tool, so we
never have to reason about whether an empty tool router is legal.

- [ ] **Step 1: Add the crate to the workspace**

Modify the `members` line in the root `Cargo.toml`:

```toml
members = ["crates/core", "crates/wasm", "crates/mcp", "adapters/asciicast"]
```

- [ ] **Step 2: Write the manifest**

Create `crates/mcp/Cargo.toml`:

```toml
[package]
name = "zoetrope-mcp"
version = "0.1.0"
edition.workspace = true
license.workspace = true

[[bin]]
name = "zoetrope-mcp"
path = "src/main.rs"

[dependencies]
# default-features off: upstream defaults would be acceptable, but pinning the
# set explicitly keeps the heavy optional tree (reqwest, hyper, oauth2,
# jsonwebtoken, process-wrap) permanently off. `base64` is NOT optional to us:
# MCP inline image content is base64-encoded and every render tool returns
# sampled frames as images. `transport-io` is the stdio transport; no HTTP
# transport is ever enabled (spec §9).
rmcp = { version = "3.1.4", default-features = false, features = [
    "server",
    "macros",
    "transport-io",
    "base64",
] }
image = { version = "0.25.10", default-features = false, features = ["png", "jpeg"] }
schemars = "1.0"
serde = { version = "1.0.229", features = ["derive"] }
serde_json = "1.0.151"
tempfile = "3.27.0"
thiserror = "2.0.20"
# `rt` (not `rt-multi-thread`): a stdio server is I/O-bound with one peer, so
# the current-thread runtime is sufficient and keeps the dependency surface
# smaller.
tokio = { version = "1", features = ["rt", "macros", "io-std"] }
zoetrope-asciicast = { path = "../../adapters/asciicast" }
zoetrope-core = { path = "../core" }
```

- [ ] **Step 3: Write the test harness**

Create `crates/mcp/tests/harness.rs`. This is a shared module, not a test file
with its own tests — it is included by the real test files via `mod harness;`.

```rust
//! Minimal stdio JSON-RPC driver for the MCP server binary.
//!
//! Deliberately does not use rmcp's client: the `client` feature is disabled
//! (Global Constraints), and driving the wire format directly is also what we
//! want to be testing.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

pub struct Server {
    child: Child,
    reader: BufReader<ChildStdout>,
    next_id: i64,
}

impl Server {
    pub fn start() -> Server {
        let mut child = Command::new(env!("CARGO_BIN_EXE_zoetrope-mcp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn zoetrope-mcp");
        let reader = BufReader::new(child.stdout.take().expect("stdout piped"));
        Server { child, reader, next_id: 1 }
    }

    pub fn send(&mut self, msg: &Value) {
        let stdin = self.child.stdin.as_mut().expect("stdin piped");
        writeln!(stdin, "{}", serde_json::to_string(msg).expect("serialize"))
            .expect("write to server");
        stdin.flush().expect("flush");
    }

    pub fn recv(&mut self) -> Value {
        let mut line = String::new();
        let n = self.reader.read_line(&mut line).expect("read from server");
        assert!(n > 0, "server closed stdout before responding");
        serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("server emitted non-JSON line {line:?}: {e}"))
    }

    /// Send a request with an auto-assigned id and read exactly one response.
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        self.recv()
    }

    /// Perform the MCP handshake and return the `initialize` response.
    pub fn initialize(&mut self) -> Value {
        let resp = self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "zoetrope-mcp-test", "version": "0" },
            }),
        );
        self.send(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        }));
        resp
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
```

- [ ] **Step 4: Write the failing test**

Create `crates/mcp/tests/protocol.rs`:

```rust
mod harness;

use harness::Server;

#[test]
fn initialize_returns_server_info() {
    let mut server = Server::start();
    let resp = server.initialize();

    let result = resp.get("result").expect("initialize returned an error");
    assert_eq!(result["serverInfo"]["name"], "zoetrope-mcp");
    // Asserted as "present", not as an exact string: the negotiated version
    // is rmcp's to choose and will move with SDK upgrades.
    assert!(
        result.get("protocolVersion").is_some(),
        "no protocolVersion in {result}"
    );
}
```

- [ ] **Step 5: Run the test to verify it fails**

Run: `cargo test -p zoetrope-mcp --test protocol`
Expected: FAIL — the crate has no `src/` yet, so this is a compile error
(`couldn't read crates/mcp/src/main.rs`).

- [ ] **Step 6: Write the server skeleton**

Create `crates/mcp/src/lib.rs`:

```rust
//! MCP server exposing the native zoetrope engine over stdio.

use rmcp::ServerHandler;
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};

#[derive(Clone, Default)]
pub struct ZoetropeServer {}

impl ZoetropeServer {
    pub fn new() -> Self {
        ZoetropeServer {}
    }
}

/// Built via `ServerInfo::new` + field assignment rather than a struct
/// literal: several rmcp model types are `#[non_exhaustive]`, so a literal
/// would not compile from outside the crate, and the protocol version is
/// rmcp's to choose.
fn server_info(capabilities: ServerCapabilities) -> ServerInfo {
    let mut info = ServerInfo::new(capabilities);
    info.server_info = Implementation {
        name: "zoetrope-mcp".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        ..Implementation::default()
    };
    info.instructions = Some(
        "Renders zoetrope scene documents to MP4. Deterministic: the same \
         document always produces the same bytes. Requires ffmpeg on PATH."
            .into(),
    );
    info
}

impl ServerHandler for ZoetropeServer {
    fn get_info(&self) -> ServerInfo {
        server_info(ServerCapabilities::builder().build())
    }
}
```

Create `crates/mcp/src/main.rs`:

```rust
use rmcp::ServiceExt;
use zoetrope_mcp::ZoetropeServer;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service = ZoetropeServer::new().serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
```

- [ ] **Step 7: Run the test to verify it passes**

Run: `cargo test -p zoetrope-mcp --test protocol`
Expected: PASS

- [ ] **Step 8: Verify the leaf-crate constraint holds**

Run: `cargo tree -p zoetrope-core --invert -p zoetrope-mcp`
Expected: no output / no path — `zoetrope-core` must not depend on the server.

Run: `cargo build -p zoetrope-wasm --target wasm32-unknown-unknown`
Expected: success, unchanged. The new crate must not enter the wasm graph.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml Cargo.lock crates/mcp
git commit -m "feat(mcp): scaffold stdio MCP server crate with initialize handshake"
```

---

### Task 2: Error type and document/asset loading

**Files:**
- Create: `crates/mcp/src/error.rs`
- Create: `crates/mcp/src/source.rs`
- Modify: `crates/mcp/src/lib.rs` (add `pub mod error; pub mod source;`)

**Interfaces:**
- Consumes: `zoetrope_mcp::ZoetropeServer` (Task 1).
- Produces:
  - `error::ToolError` (enum, `thiserror`) with `ToolError::into_result(self) -> rmcp::model::CallToolResult`.
  - `source::load_document(document: Option<&str>, document_path: Option<&str>) -> Result<(Document, PathBuf), ToolError>` — returns the document and the directory asset paths resolve against.
  - `source::resolve_assets(doc: &Document, base_dir: &Path) -> Result<AssetStore, ToolError>`.
  - `source::check_fps(fps: i64) -> Result<(), ToolError>`.

- [ ] **Step 1: Write the failing tests**

Create `crates/mcp/src/source.rs` with only the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_neither_document_nor_path() {
        let err = load_document(None, None).unwrap_err();
        assert!(matches!(err, ToolError::DocumentSource(_)));
    }

    #[test]
    fn rejects_both_document_and_path() {
        let err = load_document(Some("{}"), Some("/tmp/x.json")).unwrap_err();
        assert!(matches!(err, ToolError::DocumentSource(_)));
    }

    #[test]
    fn inline_document_base_dir_is_cwd() {
        let doc = zoetrope_core::Document::new(16, 16).canonical_json();
        let (parsed, base) = load_document(Some(&doc), None).unwrap();
        assert_eq!(parsed.size.w, 16);
        assert_eq!(base, std::env::current_dir().unwrap());
    }

    #[test]
    fn path_document_base_dir_is_its_parent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        std::fs::write(&path, zoetrope_core::Document::new(8, 8).canonical_json()).unwrap();
        let (_, base) = load_document(None, Some(path.to_str().unwrap())).unwrap();
        assert_eq!(base, dir.path());
    }

    #[test]
    fn invalid_document_surfaces_doc_error() {
        let err = load_document(Some(r#"{"v":99}"#), None).unwrap_err();
        assert!(matches!(err, ToolError::Document(_)));
    }

    #[test]
    fn resolves_reserved_font_src_without_touching_disk() {
        let mut doc = zoetrope_core::Document::new(8, 8);
        doc.add_asset("f", zoetrope_core::Asset::font("zoetrope:jetbrains-mono"));
        let store = resolve_assets(&doc, std::path::Path::new("/nonexistent")).unwrap();
        // `prepare` is what actually decodes; getting here without an I/O error
        // proves the reserved src never hit the filesystem.
        drop(store);
    }

    #[test]
    fn missing_asset_file_names_the_resolved_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = zoetrope_core::Document::new(8, 8);
        doc.add_asset("i", zoetrope_core::Asset::image("missing.png"));
        let err = resolve_assets(&doc, dir.path()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("missing.png"), "message was: {msg}");
        assert!(msg.contains("'i'"), "message was: {msg}");
    }

    #[test]
    fn rejects_fps_that_does_not_divide_the_timebase() {
        // TIMEBASE factors as 2^9 * 3^2 * 5^5 * 7^2, so a legal fps is any
        // product of those primes within those exponents. 11 has a prime
        // factor the timebase lacks; 27 is 3^3, which overruns its exponent.
        assert!(check_fps(30).is_ok());
        assert!(check_fps(24).is_ok());
        assert!(check_fps(0).is_err());
        assert!(check_fps(-1).is_err());
        assert!(check_fps(11).is_err());
        assert!(check_fps(27).is_err());
    }
}
```

Add `tempfile` to `[dev-dependencies]` is not needed — it is already a real
dependency from Task 1.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p zoetrope-mcp --lib`
Expected: FAIL — compile error, `load_document` / `resolve_assets` /
`check_fps` / `ToolError` not found.

- [ ] **Step 3: Write the error type**

Create `crates/mcp/src/error.rs`:

```rust
//! Every failure the tools can produce, and its mapping onto MCP.
//!
//! All of these are *tool-level* errors: the request was well-formed and
//! routed correctly, and the caller needs to read the message to fix their
//! input. Per rmcp's own guidance, that means `Ok(CallToolResult::error(..))`
//! — an `Err(ErrorData)` is rendered opaquely by MCP clients and the message
//! never reaches the model.

use rmcp::model::{CallToolResult, ContentBlock};
use zoetrope_core::DocError;

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("{0}")]
    DocumentSource(String),

    #[error("invalid document: {0}")]
    Document(#[from] DocError),

    #[error("asset '{id}': {source} (resolved to {path})")]
    Asset {
        id: String,
        path: String,
        source: std::io::Error,
    },

    #[error("{context}: {source} ({path})")]
    Io {
        context: &'static str,
        path: String,
        source: std::io::Error,
    },

    #[error(
        "unsupported fps {0}: fps must be positive and divide the timebase \
         705600000 exactly (e.g. 24, 25, 30, 50, 60)"
    )]
    Fps(i64),

    #[error(
        "ffmpeg was not found on PATH. zoetrope renders frames itself but \
         relies on ffmpeg to encode and mux MP4. Install it (macOS: \
         `brew install ffmpeg`; Debian/Ubuntu: `apt install ffmpeg`) and retry."
    )]
    FfmpegMissing,

    #[error(
        "ffmpeg is installed but failed while muxing to {0}. Its stderr was \
         inherited by this server's stderr; check the host logs for the cause."
    )]
    MuxFailed(String),

    #[error("{0}")]
    Invalid(String),
}

impl ToolError {
    pub fn into_result(self) -> CallToolResult {
        CallToolResult::error(vec![ContentBlock::text(self.to_string())])
    }
}
```

- [ ] **Step 4: Write the loader**

Prepend to `crates/mcp/src/source.rs` (above the existing test module):

```rust
//! Turning tool parameters into a validated `Document` plus a populated
//! `AssetStore`, resolving asset `src` values against the filesystem.

use std::path::{Path, PathBuf};

use zoetrope_core::assets::AssetStore;
use zoetrope_core::doc::TIMEBASE;
use zoetrope_core::{Asset, Document};

use crate::error::ToolError;

/// Load a document from exactly one of `document` (inline canonical JSON) or
/// `document_path`. Returns the parsed document and the directory that asset
/// `src` values resolve against.
pub fn load_document(
    document: Option<&str>,
    document_path: Option<&str>,
) -> Result<(Document, PathBuf), ToolError> {
    match (document, document_path) {
        (Some(_), Some(_)) => Err(ToolError::DocumentSource(
            "provide exactly one of `document` or `document_path`, not both".into(),
        )),
        (None, None) => Err(ToolError::DocumentSource(
            "provide exactly one of `document` (inline canonical JSON) or \
             `document_path`"
                .into(),
        )),
        (Some(json), None) => {
            let doc = Document::from_json(json)?;
            let base = std::env::current_dir().map_err(|e| ToolError::Io {
                context: "reading current directory",
                path: ".".into(),
                source: e,
            })?;
            Ok((doc, base))
        }
        (None, Some(path)) => {
            let path = Path::new(path);
            let json = std::fs::read_to_string(path).map_err(|e| ToolError::Io {
                context: "reading document",
                path: path.display().to_string(),
                source: e,
            })?;
            let doc = Document::from_json(&json)?;
            let base = path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            Ok((doc, base))
        }
    }
}

/// Stage bytes for every asset the document references.
///
/// Reserved font srcs (`zoetrope:inter`, `zoetrope:jetbrains-mono`) come from
/// the bytes bundled into `zoetrope-core`; everything else is a filesystem
/// path resolved against `base_dir`. Absolute srcs are used as-is. There is
/// no network fetching — a document whose pixels depend on a URL would not be
/// reproducible.
pub fn resolve_assets(doc: &Document, base_dir: &Path) -> Result<AssetStore, ToolError> {
    let mut store = AssetStore::new();
    for (id, asset) in &doc.assets {
        let src = match asset {
            Asset::Image { src } | Asset::Font { src } => src,
        };

        if let Some(bytes) = zoetrope_core::resolve_reserved_src(src) {
            store.add_bytes(id, bytes.to_vec());
            continue;
        }

        let path = {
            let p = Path::new(src);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                base_dir.join(p)
            }
        };
        let bytes = std::fs::read(&path).map_err(|e| ToolError::Asset {
            id: id.clone(),
            path: path.display().to_string(),
            source: e,
        })?;
        store.add_bytes(id, bytes);
    }
    Ok(store)
}

/// `Engine::tick_for_frame` asserts this; we check it first so bad caller
/// input is a readable tool error rather than a panic that kills the server.
pub fn check_fps(fps: i64) -> Result<(), ToolError> {
    if fps <= 0 || TIMEBASE % fps != 0 {
        return Err(ToolError::Fps(fps));
    }
    Ok(())
}
```

Note: `resolve_reserved_src` is re-exported from the crate root only under the
`bundled-fonts` feature, which is on by default and which this crate relies on.
If the import fails, use the full path
`zoetrope_core::assets::resolve_reserved_src`.

- [ ] **Step 5: Wire the modules**

Add to `crates/mcp/src/lib.rs`, above the `ZoetropeServer` definition:

```rust
pub mod error;
pub mod source;
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p zoetrope-mcp --lib`
Expected: PASS (8 tests)

- [ ] **Step 7: Commit**

```bash
git add crates/mcp
git commit -m "feat(mcp): document loading, filesystem asset resolution, error mapping"
```

---

### Task 3: Render pipeline and frame sampling

**Files:**
- Create: `crates/mcp/src/render.rs`
- Create: `crates/mcp/tests/parity.rs`
- Modify: `crates/mcp/src/lib.rs` (add `pub mod render;`)
- Modify: `crates/mcp/Cargo.toml` (add `sha2` dev-dependency)

**Interfaces:**
- Consumes: `error::ToolError`, `source::check_fps` (Task 2).
- Produces:
  - `render::RenderOutcome { out: String, width: u32, height: u32, fps: i64, frame_count: u64, duration_ticks: i64, duration_seconds: f64 }` — `Serialize`.
  - `render::PREVIEW_MAX_EDGE: u32 = 720`, `render::PREVIEW_MAX_COUNT: usize = 12`.
  - `render::preview_frame_indices(frame_count: u64, count: usize) -> Vec<u64>` — evenly spaced frame *indices*, so previews are exactly the frames `export_frames` writes.
  - `render::frame_count(engine: &Engine, fps: i64) -> u64`.
  - `render::sample_frames(engine: &mut Engine, fps: i64, count: usize) -> Result<Vec<String>, ToolError>` — base64 PNGs.
  - `render::render_to_mp4(engine: &mut Engine, fps: i64, out: &str) -> Result<RenderOutcome, ToolError>`.
  - `render::describe(engine: &Engine, fps: i64) -> RenderOutcome` — metadata only, `out` empty; used by `validate_only`.

- [ ] **Step 1: Write the failing tests**

Create `crates/mcp/src/render.rs` with only the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_indices_span_first_to_last() {
        assert_eq!(preview_frame_indices(100, 5), vec![0, 24, 49, 74, 99]);
    }

    #[test]
    fn preview_indices_handle_single_sample() {
        assert_eq!(preview_frame_indices(100, 1), vec![0]);
    }

    #[test]
    fn preview_indices_are_empty_when_disabled() {
        assert_eq!(preview_frame_indices(100, 0), Vec::<u64>::new());
    }

    #[test]
    fn preview_indices_never_exceed_frame_count() {
        // Asking for more samples than there are frames must not duplicate or
        // run past the end.
        let idx = preview_frame_indices(3, 5);
        assert_eq!(idx, vec![0, 1, 2]);
    }

    #[test]
    fn preview_indices_are_capped() {
        assert_eq!(preview_frame_indices(1000, 99).len(), PREVIEW_MAX_COUNT);
    }

    #[test]
    fn sampled_frames_match_exported_frames_below_the_downscale_cap() {
        // This is what makes the spec's byte-identity claim testable. A 320x180
        // document is under PREVIEW_MAX_EDGE, so no resampling happens and the
        // preview PNG must be byte-identical to the exported one.
        use zoetrope_core::export::export_frames;

        let mut engine = small_engine();
        let dir = tempfile::tempdir().unwrap();
        export_frames(&mut engine, 30, dir.path()).unwrap();
        let exported = std::fs::read(dir.path().join("frame-00000.png")).unwrap();

        let mut engine = small_engine();
        let previews = sample_frames(&mut engine, 30, 1).unwrap();
        let decoded = base64_decode(&previews[0]);

        assert_eq!(decoded, exported);
    }

    /// A 320x180 one-second document: small, deterministic, no assets.
    fn small_engine() -> zoetrope_core::Engine {
        use zoetrope_core::{Document, Element, Scene};
        let mut doc = Document::new(320, 180);
        doc.push_scene(
            Scene::new("s", zoetrope_core::doc::TIMEBASE)
                .with_element(Element::rect([0.0, 0.0, 320.0, 180.0], "#3366FF")),
        );
        zoetrope_core::Engine::new(doc, zoetrope_core::AssetStore::new()).unwrap()
    }

    fn base64_decode(s: &str) -> Vec<u8> {
        use rmcp::base64::Engine as _;
        rmcp::base64::engine::general_purpose::STANDARD
            .decode(s)
            .expect("valid base64")
    }
}
```

If `rmcp` does not re-export `base64`, add `base64 = "0.22"` to
`[dev-dependencies]` in `crates/mcp/Cargo.toml` and use that in the test
helper instead. Do not add it as a real dependency — the server's own
encoding goes through rmcp.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p zoetrope-mcp --lib render`
Expected: FAIL — compile error, `preview_frame_indices` / `sample_frames` /
`PREVIEW_MAX_COUNT` not found.

- [ ] **Step 3: Write the render module**

Prepend to `crates/mcp/src/render.rs`:

```rust
//! The single path from an `Engine` to an MP4 plus previews. Every tool
//! funnels through here so ffmpeg handling and preview behavior cannot drift
//! between tools.

use std::path::Path;

use rmcp::base64::Engine as _;
use serde::Serialize;
use zoetrope_core::export::{export_frames, ffmpeg_available, mux_with_ffmpeg};
use zoetrope_core::Engine;

use crate::error::ToolError;

/// Previews are downscaled above this edge length to bound context cost.
/// Above it they are no longer byte-comparable to exported frames and must
/// never be used as parity evidence (spec §6).
pub const PREVIEW_MAX_EDGE: u32 = 720;

/// Hard cap on sampled frames per call, regardless of what the caller asks.
pub const PREVIEW_MAX_COUNT: usize = 12;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderOutcome {
    pub out: String,
    pub width: u32,
    pub height: u32,
    pub fps: i64,
    pub frame_count: u64,
    pub duration_ticks: i64,
    pub duration_seconds: f64,
}

/// Evenly spaced frame indices from the first frame to the last, inclusive.
///
/// Returns frame *indices* rather than ticks so previews are exactly the
/// frames `export_frames` writes — that is what lets a preview be compared
/// byte-for-byte against an exported frame.
pub fn preview_frame_indices(frame_count: u64, count: usize) -> Vec<u64> {
    if count == 0 || frame_count == 0 {
        return Vec::new();
    }
    let count = count.min(PREVIEW_MAX_COUNT).min(frame_count as usize);
    if count == 1 {
        return vec![0];
    }
    let last = frame_count - 1;
    (0..count)
        .map(|i| (i as u64 * last) / (count as u64 - 1))
        .collect()
}

/// How many frames this engine will emit at `fps`.
pub fn frame_count(engine: &Engine, fps: i64) -> u64 {
    let total = engine.total_duration();
    let mut n = 0u64;
    while engine.tick_for_frame(n as i64, fps) < total {
        n += 1;
    }
    n
}

pub fn describe(engine: &Engine, fps: i64) -> RenderOutcome {
    let ticks = engine.total_duration();
    RenderOutcome {
        out: String::new(),
        width: engine.width(),
        height: engine.height(),
        fps,
        frame_count: frame_count(engine, fps),
        duration_ticks: ticks,
        duration_seconds: ticks as f64 / zoetrope_core::doc::TIMEBASE as f64,
    }
}

/// Render `count` evenly spaced frames as base64-encoded PNGs.
pub fn sample_frames(
    engine: &mut Engine,
    fps: i64,
    count: usize,
) -> Result<Vec<String>, ToolError> {
    let total = frame_count(engine, fps);
    let mut out = Vec::new();
    for index in preview_frame_indices(total, count) {
        let tick = engine.tick_for_frame(index as i64, fps);
        let mut rgba = engine.render(tick).to_vec();
        zoetrope_core::render::unpremultiply(&mut rgba);

        let (w, h) = (engine.width(), engine.height());
        let img = image::RgbaImage::from_raw(w, h, rgba)
            .expect("engine frame buffer is always w*h*4");

        let img = if w.max(h) > PREVIEW_MAX_EDGE {
            let scale = PREVIEW_MAX_EDGE as f64 / w.max(h) as f64;
            let (nw, nh) = (
                ((w as f64 * scale).round() as u32).max(1),
                ((h as f64 * scale).round() as u32).max(1),
            );
            // Triangle is chosen for determinism, not quality: previews above
            // the cap are explicitly not parity evidence, but the server must
            // still produce identical bytes for identical input.
            image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle)
        } else {
            img
        };

        let mut png = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut png),
            image::ImageFormat::Png,
        )
        .map_err(|e| ToolError::Invalid(format!("preview PNG encode failed: {e}")))?;

        out.push(rmcp::base64::engine::general_purpose::STANDARD.encode(&png));
    }
    Ok(out)
}

/// Render every frame and mux to `out`.
///
/// Preflights ffmpeg *before* rendering a single frame: without this, a caller
/// with no ffmpeg pays the full render cost and then fails.
pub fn render_to_mp4(
    engine: &mut Engine,
    fps: i64,
    out: &str,
) -> Result<RenderOutcome, ToolError> {
    if !ffmpeg_available() {
        return Err(ToolError::FfmpegMissing);
    }

    let out_path = Path::new(out);
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::Io {
                context: "creating output directory",
                path: parent.display().to_string(),
                source: e,
            })?;
        }
    }

    let frames_dir = tempfile::tempdir().map_err(|e| ToolError::Io {
        context: "creating temporary frame directory",
        path: "<temp>".into(),
        source: e,
    })?;

    let count = export_frames(engine, fps, frames_dir.path()).map_err(|e| ToolError::Io {
        context: "writing frames",
        path: frames_dir.path().display().to_string(),
        source: e,
    })?;

    // `Ok(false)` here can no longer mean "ffmpeg absent" — we checked above —
    // so it means ffmpeg ran and exited nonzero.
    let muxed = mux_with_ffmpeg(frames_dir.path(), fps, out_path).map_err(|e| ToolError::Io {
        context: "running ffmpeg",
        path: out.to_string(),
        source: e,
    })?;
    if !muxed {
        return Err(ToolError::MuxFailed(out.to_string()));
    }

    let ticks = engine.total_duration();
    Ok(RenderOutcome {
        out: out.to_string(),
        width: engine.width(),
        height: engine.height(),
        fps,
        frame_count: count,
        duration_ticks: ticks,
        duration_seconds: ticks as f64 / zoetrope_core::doc::TIMEBASE as f64,
    })
}
```

- [ ] **Step 4: Wire the module**

Add `pub mod render;` to `crates/mcp/src/lib.rs`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p zoetrope-mcp --lib render`
Expected: PASS (6 tests)

- [ ] **Step 6: Tie the server's loading path to the committed parity goldens**

This is spec §10's requirement that the MCP surface not become a second source
of truth. `testdata/golden/hashes.json` maps `"{corpus_name}@{tick}"` to the
sha256 of the raw rendered frame, and `crates/core/tests/golden.rs` already
covers core's own path. What is *not* yet covered is this crate's document
loading and asset resolution — so the test drives a corpus document through
`source::load_document` + `source::resolve_assets` (not
`corpus_load_assets`) and checks the resulting pixels against the same
goldens.

Add to `crates/mcp/Cargo.toml`:

```toml
[dev-dependencies]
sha2 = "0.11.0"
```

Create `crates/mcp/tests/parity.rs`:

```rust
//! The server's own loading path must produce byte-identical pixels to the
//! committed corpus goldens. If this fails while `crates/core`'s golden test
//! passes, the bug is in this crate's document loading or asset resolution.

use std::collections::BTreeMap;
use std::path::PathBuf;

use sha2::{Digest, Sha256};
use zoetrope_mcp::source::{load_document, resolve_assets};

fn repo(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel)
}

#[test]
fn corpus_rendered_through_the_server_path_matches_golden_hashes() {
    let goldens: BTreeMap<String, String> = serde_json::from_str(
        &std::fs::read_to_string(repo("testdata/golden/hashes.json"))
            .expect("read testdata/golden/hashes.json"),
    )
    .expect("parse golden hashes");

    let assets_dir = repo("testdata/assets");
    let mut checked = 0usize;

    for entry in zoetrope_core::corpus::corpus() {
        // Round-trip through canonical JSON so the server's parser is what
        // builds the document, exactly as it would for a real tool call.
        let json = entry.doc.canonical_json();
        let (doc, _) = load_document(Some(&json), None).expect("corpus doc parses");
        let assets = resolve_assets(&doc, &assets_dir).expect("corpus assets resolve");
        let mut engine = zoetrope_core::Engine::new(doc, assets).expect("engine builds");

        for tick in &entry.ticks {
            let key = format!("{}@{}", entry.name, tick);
            let Some(expected) = goldens.get(&key) else {
                continue;
            };
            let actual: String = Sha256::digest(engine.render(*tick))
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_eq!(&actual, expected, "frame mismatch at {key}");
            checked += 1;
        }
    }

    assert!(checked > 0, "no golden hashes matched any corpus entry — the key format has drifted");
}
```

Note the `checked > 0` assertion. Without it, a change to the `name@tick` key
format would make every lookup miss, the loop would compare nothing, and the
test would pass green while checking zero frames.

- [ ] **Step 7: Run the parity test**

Run: `cargo test -p zoetrope-mcp --test parity`
Expected: PASS, and it must report a nonzero number of assertions — if it
fails on the `checked > 0` line, fix the key format rather than the assertion.

- [ ] **Step 8: Commit**

```bash
git add crates/mcp
git commit -m "feat(mcp): render pipeline with ffmpeg preflight and frame previews"
```

---

### Task 4: The `render_document` tool

**Files:**
- Create: `crates/mcp/src/tools.rs`
- Modify: `crates/mcp/src/lib.rs` (tool router, capabilities)
- Create: `crates/mcp/tests/tools.rs`
- Modify: `crates/mcp/tests/protocol.rs` (add a `tools/list` assertion)

**Interfaces:**
- Consumes: `source::{load_document, resolve_assets, check_fps}`, `render::{render_to_mp4, sample_frames, describe, RenderOutcome}`, `error::ToolError`.
- Produces:
  - `tools::RenderDocumentParams` (`Deserialize + JsonSchema`).
  - `tools::success(outcome: &RenderOutcome, previews: Vec<String>) -> CallToolResult` — the shared result shape used by all three tools.
  - `ZoetropeServer::render_document` registered as MCP tool `render_document`.

- [ ] **Step 1: Write the failing tests**

Create `crates/mcp/tests/tools.rs`:

```rust
mod harness;

use harness::Server;
use serde_json::json;

/// A 320x180 one-second solid-color document — no assets, renders fast.
fn tiny_doc() -> String {
    json!({
        "v": 1,
        "timebase": 705600000,
        "size": { "w": 320, "h": 180 },
        "scenes": [{
            "id": "s",
            "duration": 705600000,
            "elements": [{
                "type": "rect",
                "rect": [0, 0, 320, 180],
                "fill": "#3366FF"
            }]
        }]
    })
    .to_string()
}

fn call(server: &mut Server, name: &str, args: serde_json::Value) -> serde_json::Value {
    server.request("tools/call", json!({ "name": name, "arguments": args }))
}

#[test]
fn validate_only_returns_metadata_and_no_frames() {
    let mut server = Server::start();
    server.initialize();

    let resp = call(
        &mut server,
        "render_document",
        json!({ "document": tiny_doc(), "validateOnly": true }),
    );

    let result = &resp["result"];
    assert_ne!(result["isError"], json!(true), "unexpected error: {result}");
    let structured = &result["structuredContent"];
    assert_eq!(structured["width"], 320);
    assert_eq!(structured["height"], 180);
    assert_eq!(structured["frameCount"], 30);
    assert_eq!(structured["durationTicks"], 705600000);

    let images = result["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == "image")
        .count();
    assert_eq!(images, 0, "validateOnly must not render previews");
}

#[test]
fn invalid_document_is_a_tool_error_with_a_readable_message() {
    let mut server = Server::start();
    server.initialize();

    // NOTE: the document must be structurally complete. `Document::from_json`
    // runs the unknown-field walk, then the typed decode, and only then
    // `validate_semantics` — which is where the version check lives
    // (crates/core/src/validate.rs:224). A bare `{"v":99}` fails the typed
    // decode on the missing required fields and never reaches it, producing a
    // `DocError::Json` about `timebase` instead.
    let wrong_version = json!({
        "v": 99,
        "timebase": 705600000,
        "size": { "w": 320, "h": 180 },
        "scenes": [{
            "id": "s",
            "duration": 705600000,
            "elements": []
        }]
    })
    .to_string();

    let resp = call(
        &mut server,
        "render_document",
        json!({ "document": wrong_version, "validateOnly": true }),
    );

    let result = &resp["result"];
    assert_eq!(result["isError"], json!(true), "expected a tool error: {result}");
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("unsupported document version"), "message was: {text}");
}

#[test]
fn both_document_and_path_is_a_tool_error() {
    let mut server = Server::start();
    server.initialize();

    let resp = call(
        &mut server,
        "render_document",
        json!({
            "document": tiny_doc(),
            "documentPath": "/tmp/whatever.json",
            "validateOnly": true
        }),
    );

    assert_eq!(resp["result"]["isError"], json!(true));
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("not both"), "message was: {text}");
}

#[test]
fn bad_fps_is_a_tool_error_not_a_panic() {
    let mut server = Server::start();
    server.initialize();

    let resp = call(
        &mut server,
        "render_document",
        // 11 has a prime factor TIMEBASE (2^9 * 3^2 * 5^5 * 7^2) lacks.
        // Note 7 IS legal — it divides the timebase twice over.
        json!({ "document": tiny_doc(), "fps": 11, "validateOnly": true }),
    );

    assert_eq!(resp["result"]["isError"], json!(true));

    // The server must still be alive — a panic would have killed the process.
    let alive = call(
        &mut server,
        "render_document",
        json!({ "document": tiny_doc(), "validateOnly": true }),
    );
    assert_ne!(alive["result"]["isError"], json!(true));
}

#[test]
fn renders_an_mp4_with_preview_frames() {
    if !zoetrope_core::export::ffmpeg_available() {
        panic!(
            "ffmpeg is required to run this test; CI installs it (see \
             .github/workflows). Install it locally to run the full suite."
        );
    }

    let mut server = Server::start();
    server.initialize();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.mp4");

    let resp = call(
        &mut server,
        "render_document",
        json!({
            "document": tiny_doc(),
            "out": out.to_str().unwrap(),
            "previewFrames": 3
        }),
    );

    let result = &resp["result"];
    assert_ne!(result["isError"], json!(true), "render failed: {result}");
    assert!(out.exists(), "no MP4 at {}", out.display());
    assert!(std::fs::metadata(&out).unwrap().len() > 0);

    let images: Vec<_> = result["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == "image")
        .collect();
    assert_eq!(images.len(), 3);
    assert_eq!(images[0]["mimeType"], "image/png");
}
```

Add to `crates/mcp/Cargo.toml`:

```toml
[dev-dependencies]
zoetrope-core = { path = "../core" }
```

(`zoetrope-core` is already a normal dependency, so this line is only needed if
the integration test cannot see it — it can, so omit unless the build complains.)

Append to `crates/mcp/tests/protocol.rs`:

```rust
#[test]
fn tools_list_advertises_render_document() {
    let mut server = Server::start();
    server.initialize();

    let resp = server.request("tools/list", serde_json::json!({}));
    let tools = resp["result"]["tools"].as_array().expect("tools array");

    let doc_tool = tools
        .iter()
        .find(|t| t["name"] == "render_document")
        .unwrap_or_else(|| panic!("render_document missing from {tools:?}"));

    // The derived input schema is our public contract; assert its shape
    // exactly so schema drift is caught rather than silently shipped.
    let props = &doc_tool["inputSchema"]["properties"];
    for key in ["document", "documentPath", "assetBaseDir", "out", "fps", "validateOnly", "previewFrames"] {
        assert!(props.get(key).is_some(), "missing property {key} in {props}");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p zoetrope-mcp --test tools --test protocol`
Expected: FAIL — `tools/list` returns no tools and `tools/call` returns a
method-not-found error, so every assertion above fails.

- [ ] **Step 3: Write the tool module**

Create `crates/mcp/src/tools.rs`:

```rust
//! The MCP tool surface. Parameter structs derive `JsonSchema` so the wire
//! schema is generated from these types rather than hand-maintained.

use rmcp::model::{CallToolResult, ContentBlock};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::render::RenderOutcome;

fn default_fps() -> i64 {
    30
}
fn default_preview_frames() -> usize {
    5
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenderDocumentParams {
    /// Canonical zoetrope document JSON. Provide exactly one of `document` or
    /// `documentPath`.
    #[serde(default)]
    pub document: Option<String>,

    /// Path to a `.json` document. Provide exactly one of `document` or
    /// `documentPath`.
    #[serde(default)]
    pub document_path: Option<String>,

    /// Directory that image and font `src` values resolve against. Defaults to
    /// the document's own directory, or the working directory for an inline
    /// document.
    #[serde(default)]
    pub asset_base_dir: Option<String>,

    /// Output `.mp4` path. Required unless `validateOnly` is true.
    #[serde(default)]
    pub out: Option<String>,

    /// Frames per second. Must divide 705600000 exactly (24, 25, 30, 50, 60...).
    #[serde(default = "default_fps")]
    pub fps: i64,

    /// Parse and validate the document without rendering anything.
    #[serde(default)]
    pub validate_only: bool,

    /// How many evenly spaced frames to return as inline images, so the caller
    /// can check the result. 0 disables; capped at 12.
    #[serde(default = "default_preview_frames")]
    pub preview_frames: usize,
}

/// The shared success shape for every render tool: a one-line summary, the
/// structured metadata, then the sampled frames.
pub fn success(outcome: &RenderOutcome, previews: Vec<String>) -> CallToolResult {
    let summary = if outcome.out.is_empty() {
        format!(
            "document is valid: {}x{}, {} frames at {} fps ({:.3}s)",
            outcome.width, outcome.height, outcome.frame_count, outcome.fps,
            outcome.duration_seconds
        )
    } else {
        format!(
            "wrote {} ({}x{}, {} frames at {} fps, {:.3}s)",
            outcome.out, outcome.width, outcome.height, outcome.frame_count,
            outcome.fps, outcome.duration_seconds
        )
    };

    let mut content = vec![ContentBlock::text(summary)];
    for png in previews {
        content.push(ContentBlock::image(png, "image/png"));
    }

    let mut result = CallToolResult::success(content);
    result.structured_content = serde_json::to_value(outcome).ok();
    result
}
```

- [ ] **Step 4: Wire the tool into the server**

Rewrite `crates/mcp/src/lib.rs`:

```rust
//! MCP server exposing the native zoetrope engine over stdio.

pub mod error;
pub mod render;
pub mod source;
pub mod tools;

use std::path::PathBuf;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Implementation, ServerCapabilities, ServerInfo};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};

use crate::error::ToolError;
use crate::tools::RenderDocumentParams;

/// Carried forward from Task 1 unchanged — this rewrite must not drop it.
/// Built via `ServerInfo::new` + field assignment rather than a struct
/// literal: several rmcp model types are `#[non_exhaustive]`, so a literal
/// would not compile from outside the crate.
fn server_info(capabilities: ServerCapabilities) -> ServerInfo {
    let mut info = ServerInfo::new(capabilities);
    info.server_info = Implementation {
        name: "zoetrope-mcp".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        ..Implementation::default()
    };
    info.instructions = Some(
        "Renders zoetrope scene documents to MP4. Deterministic: the same \
         document always produces the same bytes. Requires ffmpeg on PATH."
            .into(),
    );
    info
}

#[derive(Clone)]
pub struct ZoetropeServer {
    tool_router: ToolRouter<Self>,
}

impl Default for ZoetropeServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router(router = tool_router)]
impl ZoetropeServer {
    pub fn new() -> Self {
        ZoetropeServer {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "render_document",
        description = "Render a zoetrope scene document to an MP4. Rendering is \
                       deterministic: the same document always produces the same \
                       bytes. Returns the output path, metadata, and sampled \
                       frames as images so you can check the result. Requires \
                       ffmpeg on PATH."
    )]
    pub async fn render_document(
        &self,
        Parameters(params): Parameters<RenderDocumentParams>,
    ) -> CallToolResult {
        match Self::render_document_impl(params) {
            Ok(result) => result,
            Err(e) => e.into_result(),
        }
    }

    fn render_document_impl(params: RenderDocumentParams) -> Result<CallToolResult, ToolError> {
        crate::source::check_fps(params.fps)?;

        let (doc, default_base) = crate::source::load_document(
            params.document.as_deref(),
            params.document_path.as_deref(),
        )?;
        let base = params
            .asset_base_dir
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or(default_base);

        let assets = crate::source::resolve_assets(&doc, &base)?;
        let mut engine = zoetrope_core::Engine::new(doc, assets)?;

        if params.validate_only {
            let outcome = crate::render::describe(&engine, params.fps);
            return Ok(crate::tools::success(&outcome, Vec::new()));
        }

        let out = params.out.ok_or_else(|| {
            ToolError::Invalid("`out` is required unless `validateOnly` is true".into())
        })?;

        let outcome = crate::render::render_to_mp4(&mut engine, params.fps, &out)?;
        let previews =
            crate::render::sample_frames(&mut engine, params.fps, params.preview_frames)?;
        Ok(crate::tools::success(&outcome, previews))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ZoetropeServer {
    fn get_info(&self) -> ServerInfo {
        server_info(ServerCapabilities::builder().enable_tools().build())
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p zoetrope-mcp`
Expected: PASS. The `renders_an_mp4_with_preview_frames` test requires ffmpeg;
install it locally if it fails on that check.

- [ ] **Step 6: Commit**

```bash
git add crates/mcp
git commit -m "feat(mcp): render_document tool with validation, previews, and structured output"
```

---

### Task 5: Widen `Theme` and add the `render_asciicast` tool

**Files:**
- Modify: `adapters/asciicast/src/convert.rs:24-43` (Theme fields), `:66`, `:145`, `:168` (call sites)
- Modify: `crates/mcp/src/tools.rs` (add params struct)
- Modify: `crates/mcp/src/lib.rs` (add the tool)
- Modify: `crates/mcp/tests/tools.rs` (add tests)

**Interfaces:**
- Consumes: everything from Task 4.
- Produces: `tools::RenderAsciicastParams`, `tools::ThemeParams`, MCP tool `render_asciicast`. `zoetrope_asciicast::Theme` gains `String` color fields.

This is the one task that edits a crate other than `crates/mcp`. It adds no
dependency, so the leaf-crate constraint holds.

- [ ] **Step 1: Write the failing tests**

Append to `crates/mcp/tests/tools.rs`:

```rust
/// The smallest valid asciicast v2: a header line then one output event.
fn tiny_cast() -> String {
    let header = json!({ "version": 2, "width": 20, "height": 4 });
    format!("{header}\n[0.0, \"o\", \"hello\"]\n[0.5, \"o\", \" world\"]\n")
}

#[test]
fn asciicast_validates_without_ffmpeg() {
    let mut server = Server::start();
    server.initialize();
    let dir = tempfile::tempdir().unwrap();
    let cast = dir.path().join("demo.cast");
    std::fs::write(&cast, tiny_cast()).unwrap();

    let resp = call(
        &mut server,
        "render_asciicast",
        json!({ "castPath": cast.to_str().unwrap(), "validateOnly": true }),
    );

    let result = &resp["result"];
    assert_ne!(result["isError"], json!(true), "unexpected error: {result}");
    assert!(result["structuredContent"]["frameCount"].as_u64().unwrap() > 0);
}

#[test]
fn asciicast_accepts_theme_overrides() {
    let mut server = Server::start();
    server.initialize();
    let dir = tempfile::tempdir().unwrap();
    let cast = dir.path().join("demo.cast");
    std::fs::write(&cast, tiny_cast()).unwrap();

    let resp = call(
        &mut server,
        "render_asciicast",
        json!({
            "castPath": cast.to_str().unwrap(),
            "validateOnly": true,
            "theme": { "bg": "#101820", "fg": "#F2F2F2", "sizePx": 24 }
        }),
    );

    assert_ne!(resp["result"]["isError"], json!(true), "{}", resp["result"]);
}

#[test]
fn missing_cast_file_names_the_path() {
    let mut server = Server::start();
    server.initialize();

    let resp = call(
        &mut server,
        "render_asciicast",
        json!({ "castPath": "/nonexistent/demo.cast", "validateOnly": true }),
    );

    assert_eq!(resp["result"]["isError"], json!(true));
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("/nonexistent/demo.cast"), "message was: {text}");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p zoetrope-mcp --test tools`
Expected: FAIL — `render_asciicast` is not a known tool.

- [ ] **Step 3: Widen `Theme`**

In `adapters/asciicast/src/convert.rs`, change the struct and its `Default`:

```rust
pub struct Theme {
    pub bg: String,
    pub fg: String,
    pub size_px: f64,
    pub cell_w: f64,
    pub cell_h: f64,
    pub pad: f64,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            bg: "#0A0A0A".to_string(),
            fg: "#D4D4D4".to_string(),
            size_px: 20.0,
            cell_w: 12.0,
            cell_h: 26.0,
            pad: 16.0,
        }
    }
}
```

Then fix the three call sites, which relied on `&'static str`:

- Line ~66: `Document::new(w, h).with_fps(30).with_bg(theme.bg)` →
  `.with_bg(theme.bg.as_str())`
- Line ~145: `.unwrap_or_else(|| theme.fg.to_string())` — unchanged, `String`
  also has `to_string`.
- Line ~168: `Element::rect([x, y, theme.cell_w, theme.cell_h], theme.fg)` →
  `..., theme.fg.as_str())`

Run: `cargo test -p zoetrope-asciicast`
Expected: PASS — this is a pure type widening, so the adapter's existing
tests must still pass unchanged. If any golden bytes move, stop: that means
the change was not neutral.

- [ ] **Step 4: Add the parameter structs**

Append to `crates/mcp/src/tools.rs`:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeParams {
    /// Background color, `#RRGGBB`.
    #[serde(default)]
    pub bg: Option<String>,
    /// Foreground (text) color, `#RRGGBB`.
    #[serde(default)]
    pub fg: Option<String>,
    /// Font size in pixels.
    #[serde(default)]
    pub size_px: Option<f64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenderAsciicastParams {
    /// Path to an asciicast v2 `.cast` file.
    pub cast_path: String,

    /// Output `.mp4` path. Required unless `validateOnly` is true.
    #[serde(default)]
    pub out: Option<String>,

    /// Frames per second. Must divide 705600000 exactly.
    #[serde(default = "default_fps")]
    pub fps: i64,

    /// Terminal colors and font size. Cell metrics are deliberately not
    /// exposed: they are coupled to the bundled monospace font's advance
    /// width, and overriding them produces misaligned output.
    #[serde(default)]
    pub theme: Option<ThemeParams>,

    /// Parse and convert without rendering anything.
    #[serde(default)]
    pub validate_only: bool,

    /// How many evenly spaced frames to return as inline images. 0 disables;
    /// capped at 12.
    #[serde(default = "default_preview_frames")]
    pub preview_frames: usize,
}

impl ThemeParams {
    /// Apply the caller's overrides onto the adapter's defaults.
    pub fn apply(&self, mut theme: zoetrope_asciicast::Theme) -> zoetrope_asciicast::Theme {
        if let Some(bg) = &self.bg {
            theme.bg = bg.clone();
        }
        if let Some(fg) = &self.fg {
            theme.fg = fg.clone();
        }
        if let Some(size) = self.size_px {
            theme.size_px = size;
        }
        theme
    }
}
```

- [ ] **Step 5: Add the tool**

Inside the `#[tool_router(router = tool_router)] impl ZoetropeServer` block in
`crates/mcp/src/lib.rs`, add:

```rust
    #[tool(
        name = "render_asciicast",
        description = "Render an asciicast v2 terminal recording (.cast) to an \
                       MP4. Renders from the event data rather than capturing \
                       pixels, so output is deterministic and faster than \
                       realtime. Returns the output path, metadata, and sampled \
                       frames as images. Requires ffmpeg on PATH."
    )]
    pub async fn render_asciicast(
        &self,
        Parameters(params): Parameters<crate::tools::RenderAsciicastParams>,
    ) -> CallToolResult {
        match Self::render_asciicast_impl(params) {
            Ok(result) => result,
            Err(e) => e.into_result(),
        }
    }

    fn render_asciicast_impl(
        params: crate::tools::RenderAsciicastParams,
    ) -> Result<CallToolResult, ToolError> {
        crate::source::check_fps(params.fps)?;

        let data = std::fs::read_to_string(&params.cast_path).map_err(|e| ToolError::Io {
            context: "reading asciicast",
            path: params.cast_path.clone(),
            source: e,
        })?;
        let cast = zoetrope_asciicast::parse_cast(&data)
            .map_err(|e| ToolError::Invalid(format!("invalid asciicast: {e}")))?;

        let theme = match &params.theme {
            Some(t) => t.apply(zoetrope_asciicast::Theme::default()),
            None => zoetrope_asciicast::Theme::default(),
        };
        let (doc, assets) = zoetrope_asciicast::cast_to_document(&cast, &theme);

        let mut store = zoetrope_core::AssetStore::new();
        for (id, bytes) in assets {
            store.add_bytes(&id, bytes.to_vec());
        }
        let mut engine = zoetrope_core::Engine::new(doc, store)?;

        if params.validate_only {
            let outcome = crate::render::describe(&engine, params.fps);
            return Ok(crate::tools::success(&outcome, Vec::new()));
        }

        let out = params.out.ok_or_else(|| {
            ToolError::Invalid("`out` is required unless `validateOnly` is true".into())
        })?;

        let outcome = crate::render::render_to_mp4(&mut engine, params.fps, &out)?;
        let previews =
            crate::render::sample_frames(&mut engine, params.fps, params.preview_frames)?;
        Ok(crate::tools::success(&outcome, previews))
    }
```

The exact shape of `parse_cast`'s error and of `cast_to_document`'s returned
asset collection is whatever `adapters/asciicast/src/main.rs` already uses —
read that file and mirror it rather than guessing.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p zoetrope-asciicast -p zoetrope-mcp`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add adapters/asciicast crates/mcp
git commit -m "feat(mcp): render_asciicast tool; widen Theme colors to String"
```

---

### Task 6: The `render_storyboard` tool

**Files:**
- Create: `crates/mcp/src/storyboard.rs`
- Modify: `crates/mcp/src/lib.rs` (module + tool)
- Modify: `crates/mcp/src/tools.rs` (params struct)
- Modify: `crates/mcp/tests/tools.rs` (tests)

**Interfaces:**
- Consumes: everything above.
- Produces:
  - `storyboard::Frame { image: String, duration_ms: i64, caption: Option<String> }`.
  - `storyboard::build(frames: &[Frame], size: Option<(u32, u32)>) -> Result<Document, ToolError>` — the built document. Image `src` values are the caller's own paths, so `source::resolve_assets` stages the bytes; no separate path list is needed.
  - `tools::RenderStoryboardParams`, MCP tool `render_storyboard`.

This is the tool that tests the spec's §1 hypothesis, and it is deliberately a
pure `Document` builder so the deferred tape-adapter port reduces to "parse
`actions.jsonl`, call `build`."

- [ ] **Step 1: Write the failing tests**

Create `crates/mcp/src/storyboard.rs` with only its test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn write_png(dir: &std::path::Path, name: &str, w: u32, h: u32) -> String {
        let path = dir.join(name);
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 20, 30, 255]));
        img.save(&path).unwrap();
        path.to_str().unwrap().to_string()
    }

    #[test]
    fn rejects_an_empty_frame_list() {
        assert!(build(&[], None).is_err());
    }

    #[test]
    fn one_scene_per_frame_with_exact_tick_durations() {
        let dir = tempfile::tempdir().unwrap();
        let frames = vec![
            Frame { image: write_png(dir.path(), "a.png", 100, 50), duration_ms: 500, caption: None },
            Frame { image: write_png(dir.path(), "b.png", 100, 50), duration_ms: 1500, caption: None },
        ];
        let doc = build(&frames, None).unwrap();

        assert_eq!(doc.scenes.len(), 2);
        // 705_600 ticks per millisecond, exactly — no rounding.
        assert_eq!(doc.scenes[0].duration, 500 * 705_600);
        assert_eq!(doc.scenes[1].duration, 1500 * 705_600);
        assert_eq!(doc.assets.len(), 2);
    }

    #[test]
    fn size_defaults_to_the_first_image() {
        let dir = tempfile::tempdir().unwrap();
        let frames = vec![Frame {
            image: write_png(dir.path(), "a.png", 640, 360),
            duration_ms: 100,
            caption: None,
        }];
        let doc = build(&frames, None).unwrap();
        assert_eq!((doc.size.w, doc.size.h), (640, 360));
    }

    #[test]
    fn explicit_size_overrides_the_first_image() {
        let dir = tempfile::tempdir().unwrap();
        let frames = vec![Frame {
            image: write_png(dir.path(), "a.png", 640, 360),
            duration_ms: 100,
            caption: None,
        }];
        let doc = build(&frames, Some((320, 180))).unwrap();
        assert_eq!((doc.size.w, doc.size.h), (320, 180));
    }

    #[test]
    fn a_caption_adds_a_text_element_and_the_bundled_font() {
        let dir = tempfile::tempdir().unwrap();
        let frames = vec![Frame {
            image: write_png(dir.path(), "a.png", 320, 180),
            duration_ms: 100,
            caption: Some("clicked Checkout".into()),
        }];
        let doc = build(&frames, None).unwrap();

        assert_eq!(doc.scenes[0].elements.len(), 3, "image + caption band + text");
        assert!(doc.assets.contains_key(CAPTION_FONT_ID));
    }

    #[test]
    fn a_document_with_no_captions_registers_no_font() {
        let dir = tempfile::tempdir().unwrap();
        let frames = vec![Frame {
            image: write_png(dir.path(), "a.png", 320, 180),
            duration_ms: 100,
            caption: None,
        }];
        let doc = build(&frames, None).unwrap();
        assert!(!doc.assets.contains_key(CAPTION_FONT_ID));
    }

    #[test]
    fn rejects_a_nonpositive_duration() {
        let dir = tempfile::tempdir().unwrap();
        let frames = vec![Frame {
            image: write_png(dir.path(), "a.png", 32, 32),
            duration_ms: 0,
            caption: None,
        }];
        assert!(build(&frames, None).is_err());
    }

    #[test]
    fn the_built_document_is_accepted_by_the_engine() {
        let dir = tempfile::tempdir().unwrap();
        let frames = vec![Frame {
            image: write_png(dir.path(), "a.png", 64, 64),
            duration_ms: 200,
            caption: Some("hi".into()),
        }];
        let doc = build(&frames, None).unwrap();
        let base = std::env::current_dir().unwrap();
        let store = crate::source::resolve_assets(&doc, &base).unwrap();
        zoetrope_core::Engine::new(doc, store).expect("engine accepts the built document");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p zoetrope-mcp --lib storyboard`
Expected: FAIL — compile error, `build` / `Frame` / `CAPTION_FONT_ID` not found.

- [ ] **Step 3: Write the builder**

Prepend to `crates/mcp/src/storyboard.rs`:

```rust
//! Builds a `Document` from an ordered list of images — the "agent shows its
//! work" path. Deliberately a pure builder over existing primitives: the
//! deferred mysteryshopper tape adapter becomes "parse actions.jsonl, call
//! `build`".

use std::path::Path;

use zoetrope_core::doc::TIMEBASE;
use zoetrope_core::{Asset, Document, Element, Scene};

use crate::error::ToolError;

/// Exact: 705_600_000 ticks/second / 1000 ms. No rounding at any duration.
const TICKS_PER_MS: i64 = TIMEBASE / 1000;

pub const CAPTION_FONT_ID: &str = "caption-font";
pub const CAPTION_FONT_SRC: &str = "zoetrope:jetbrains-mono";

const CAPTION_BAND_H: f64 = 56.0;
const CAPTION_SIZE_PX: f64 = 22.0;
const CAPTION_PAD: f64 = 16.0;

#[derive(Debug, Clone)]
pub struct Frame {
    pub image: String,
    pub duration_ms: i64,
    pub caption: Option<String>,
}

/// Build the document. Image `src` values are the caller's own paths, so
/// `source::resolve_assets` stages their bytes exactly as it does for a
/// hand-authored document — there is no second asset path here to keep in
/// sync.
pub fn build(frames: &[Frame], size: Option<(u32, u32)>) -> Result<Document, ToolError> {
    if frames.is_empty() {
        return Err(ToolError::Invalid("`frames` must not be empty".into()));
    }

    let (w, h) = match size {
        Some(wh) => wh,
        None => image_size(Path::new(&frames[0].image))?,
    };

    let mut doc = Document::new(w, h);
    let has_captions = frames.iter().any(|f| f.caption.is_some());

    if has_captions {
        doc.add_asset(CAPTION_FONT_ID, Asset::font(CAPTION_FONT_SRC));
    }

    for (i, frame) in frames.iter().enumerate() {
        if frame.duration_ms <= 0 {
            return Err(ToolError::Invalid(format!(
                "frame {i}: durationMs must be positive, got {}",
                frame.duration_ms
            )));
        }

        // Asset ids must match [A-Za-z0-9_-]{1,64} (DocError::BadId), so they
        // are generated rather than derived from user-supplied filenames.
        let asset_id = format!("img-{i}");
        doc.add_asset(&asset_id, Asset::image(&frame.image));

        let mut scene = Scene::new(
            &format!("frame-{i}"),
            frame.duration_ms * TICKS_PER_MS,
        )
        .with_element(Element::image(&asset_id, [0.0, 0.0, w as f64, h as f64]));

        if let Some(caption) = &frame.caption {
            let band_y = h as f64 - CAPTION_BAND_H;
            scene = scene
                .with_element(Element::rect(
                    [0.0, band_y, w as f64, CAPTION_BAND_H],
                    "#000000",
                ))
                .with_element(Element::text(
                    caption,
                    CAPTION_FONT_ID,
                    CAPTION_SIZE_PX,
                    "#FFFFFF",
                    [CAPTION_PAD, band_y + CAPTION_PAD],
                ));
        }

        doc.push_scene(scene);
    }

    Ok(doc)
}

/// Read only the header of the image to get its dimensions.
fn image_size(path: &Path) -> Result<(u32, u32), ToolError> {
    let reader = image::ImageReader::open(path)
        .map_err(|e| ToolError::Io {
            context: "reading storyboard image",
            path: path.display().to_string(),
            source: e,
        })?
        .with_guessed_format()
        .map_err(|e| ToolError::Io {
            context: "reading storyboard image",
            path: path.display().to_string(),
            source: e,
        })?;
    reader
        .into_dimensions()
        .map_err(|e| ToolError::Invalid(format!("{}: {e}", path.display())))
}
```

- [ ] **Step 4: Add the params struct and the tool**

Append to `crates/mcp/src/tools.rs`:

```rust
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoryboardFrameParams {
    /// Path to a PNG or JPEG image.
    pub image: String,
    /// How long this frame is held, in milliseconds. Must be positive.
    pub duration_ms: i64,
    /// Optional caption, drawn in a band across the bottom of the frame.
    #[serde(default)]
    pub caption: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenderStoryboardParams {
    /// Ordered frames. Must not be empty.
    pub frames: Vec<StoryboardFrameParams>,

    /// Output `.mp4` path. Required unless `validateOnly` is true.
    #[serde(default)]
    pub out: Option<String>,

    /// Frames per second. Must divide 705600000 exactly.
    #[serde(default = "default_fps")]
    pub fps: i64,

    /// Canvas width in pixels. Defaults to the first image's width.
    #[serde(default)]
    pub width: Option<u32>,

    /// Canvas height in pixels. Defaults to the first image's height.
    #[serde(default)]
    pub height: Option<u32>,

    /// Build and validate without rendering anything.
    #[serde(default)]
    pub validate_only: bool,

    /// How many evenly spaced frames to return as inline images. 0 disables;
    /// capped at 12.
    #[serde(default = "default_preview_frames")]
    pub preview_frames: usize,
}
```

Inside the `#[tool_router]` impl in `crates/mcp/src/lib.rs`, add:

```rust
    #[tool(
        name = "render_storyboard",
        description = "Render an ordered list of images into an MP4, each held \
                       for a given duration with an optional caption. Use this \
                       to turn a sequence of screenshots into a watchable clip \
                       — for example, showing the steps of a browser run. \
                       Requires ffmpeg on PATH."
    )]
    pub async fn render_storyboard(
        &self,
        Parameters(params): Parameters<crate::tools::RenderStoryboardParams>,
    ) -> CallToolResult {
        match Self::render_storyboard_impl(params) {
            Ok(result) => result,
            Err(e) => e.into_result(),
        }
    }

    fn render_storyboard_impl(
        params: crate::tools::RenderStoryboardParams,
    ) -> Result<CallToolResult, ToolError> {
        crate::source::check_fps(params.fps)?;

        let frames: Vec<crate::storyboard::Frame> = params
            .frames
            .iter()
            .map(|f| crate::storyboard::Frame {
                image: f.image.clone(),
                duration_ms: f.duration_ms,
                caption: f.caption.clone(),
            })
            .collect();

        let size = match (params.width, params.height) {
            (Some(w), Some(h)) => Some((w, h)),
            (None, None) => None,
            _ => {
                return Err(ToolError::Invalid(
                    "provide both `width` and `height`, or neither".into(),
                ));
            }
        };

        let doc = crate::storyboard::build(&frames, size)?;

        // Storyboard image srcs are the caller's own paths — absolute, or
        // relative to the server's working directory. `resolve_assets`
        // handles both.
        let base = std::env::current_dir().map_err(|e| ToolError::Io {
            context: "reading current directory",
            path: ".".into(),
            source: e,
        })?;
        let assets = crate::source::resolve_assets(&doc, &base)?;
        let mut engine = zoetrope_core::Engine::new(doc, assets)?;

        if params.validate_only {
            let outcome = crate::render::describe(&engine, params.fps);
            return Ok(crate::tools::success(&outcome, Vec::new()));
        }

        let out = params.out.ok_or_else(|| {
            ToolError::Invalid("`out` is required unless `validateOnly` is true".into())
        })?;

        let outcome = crate::render::render_to_mp4(&mut engine, params.fps, &out)?;
        let previews =
            crate::render::sample_frames(&mut engine, params.fps, params.preview_frames)?;
        Ok(crate::tools::success(&outcome, previews))
    }
```

Add `pub mod storyboard;` to `crates/mcp/src/lib.rs`.

- [ ] **Step 5: Add the integration test**

Append to `crates/mcp/tests/tools.rs`:

```rust
#[test]
fn storyboard_validates_from_image_paths() {
    let mut server = Server::start();
    server.initialize();
    let dir = tempfile::tempdir().unwrap();

    let mut frames = Vec::new();
    for name in ["a.png", "b.png"] {
        let path = dir.path().join(name);
        image::RgbaImage::from_pixel(160, 90, image::Rgba([40, 40, 40, 255]))
            .save(&path)
            .unwrap();
        frames.push(json!({
            "image": path.to_str().unwrap(),
            "durationMs": 500,
            "caption": format!("step {name}")
        }));
    }

    let resp = call(
        &mut server,
        "render_storyboard",
        json!({ "frames": frames, "validateOnly": true }),
    );

    let result = &resp["result"];
    assert_ne!(result["isError"], json!(true), "unexpected error: {result}");
    assert_eq!(result["structuredContent"]["width"], 160);
    // 1000ms total at 30fps
    assert_eq!(result["structuredContent"]["frameCount"], 30);
}

#[test]
fn storyboard_rejects_an_empty_frame_list() {
    let mut server = Server::start();
    server.initialize();

    let resp = call(
        &mut server,
        "render_storyboard",
        json!({ "frames": [], "validateOnly": true }),
    );

    assert_eq!(resp["result"]["isError"], json!(true));
}
```

Add `image` to `crates/mcp/[dev-dependencies]` only if the integration test
cannot see the normal dependency — it can, so this should not be needed.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p zoetrope-mcp`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add crates/mcp
git commit -m "feat(mcp): render_storyboard tool for image-sequence reporting"
```

---

### Task 7: Schema and corpus resources

**Files:**
- Create: `crates/mcp/src/resources.rs`
- Modify: `crates/mcp/src/lib.rs` (capabilities + `list_resources` / `read_resource`)
- Modify: `crates/mcp/tests/protocol.rs` (tests)

**Interfaces:**
- Consumes: everything above.
- Produces: `resources::list() -> Vec<Resource>`, `resources::read(uri: &str) -> Option<String>`.

URIs: `zoetrope://schema/document` for the derived JSON Schema, and
`zoetrope://corpus/<name>` for each entry from `zoetrope_core::corpus::corpus()`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/mcp/tests/protocol.rs`:

```rust
#[test]
fn resources_list_includes_the_schema_and_the_corpus() {
    let mut server = Server::start();
    server.initialize();

    let resp = server.request("resources/list", serde_json::json!({}));
    let resources = resp["result"]["resources"].as_array().expect("resources array");

    assert!(
        resources.iter().any(|r| r["uri"] == "zoetrope://schema/document"),
        "schema resource missing from {resources:?}"
    );
    assert!(
        resources.iter().filter(|r| r["uri"]
            .as_str()
            .is_some_and(|u| u.starts_with("zoetrope://corpus/")))
            .count()
            > 0,
        "no corpus resources in {resources:?}"
    );
    assert!(resources.iter().all(|r| r["mimeType"] == "application/json"));
}

#[test]
fn reading_a_corpus_resource_returns_a_renderable_document() {
    let mut server = Server::start();
    server.initialize();

    let list = server.request("resources/list", serde_json::json!({}));
    let uri = list["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|r| {
            r["uri"]
                .as_str()
                .filter(|u| u.starts_with("zoetrope://corpus/"))
                .map(str::to_string)
        })
        .expect("a corpus resource");

    let resp = server.request("resources/read", serde_json::json!({ "uri": uri }));
    let text = resp["result"]["contents"][0]["text"].as_str().expect("text contents");

    // The examples we hand a model must actually be valid documents.
    zoetrope_core::Document::from_json(text).expect("corpus resource is a valid document");
}

#[test]
fn reading_an_unknown_uri_is_an_error() {
    let mut server = Server::start();
    server.initialize();

    let resp = server.request(
        "resources/read",
        serde_json::json!({ "uri": "zoetrope://corpus/does-not-exist" }),
    );
    assert!(resp.get("error").is_some(), "expected a JSON-RPC error, got {resp}");
}
```

Note this last test *is* a case for `Err(ErrorData)` rather than
`CallToolResult::error` — `resources/read` has no tool-result envelope, so an
unroutable URI is genuinely a protocol error.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p zoetrope-mcp --test protocol`
Expected: FAIL — `resources/list` returns an empty list and `resources/read`
returns method-not-found.

- [ ] **Step 3: Write the resources module**

Create `crates/mcp/src/resources.rs`:

```rust
//! Read-only resources: the document schema and the golden corpus.
//!
//! A model authoring a document gets worked examples rather than having to
//! infer structure from a bare schema — the corpus already covers every
//! element type, every easing, crossfade, wrap, and group nesting.

use rmcp::model::Resource;

pub const SCHEMA_URI: &str = "zoetrope://schema/document";
const CORPUS_PREFIX: &str = "zoetrope://corpus/";

pub fn list() -> Vec<Resource> {
    let mut out = vec![
        Resource::new(SCHEMA_URI, "document-schema")
            .with_title("Zoetrope document JSON Schema")
            .with_description(
                "JSON Schema for the canonical scene document accepted by \
                 render_document.",
            )
            .with_mime_type("application/json"),
    ];

    for entry in zoetrope_core::corpus::corpus() {
        out.push(
            Resource::new(format!("{CORPUS_PREFIX}{}", entry.name), entry.name)
                .with_title(format!("Example document: {}", entry.name))
                .with_description(
                    "A worked example from the golden corpus. Valid, renderable, \
                     and byte-stable.",
                )
                .with_mime_type("application/json"),
        );
    }
    out
}

pub fn read(uri: &str) -> Option<String> {
    if uri == SCHEMA_URI {
        return Some(DOCUMENT_SCHEMA.to_string());
    }
    let name = uri.strip_prefix(CORPUS_PREFIX)?;
    zoetrope_core::corpus::corpus()
        .into_iter()
        .find(|e| e.name == name)
        .map(|e| e.doc.canonical_json())
}

/// Hand-written rather than derived.
///
/// `schemars::schema_for!` would require `zoetrope_core::Document` to derive
/// `JsonSchema`, which would put `schemars` into `crates/core` and break the
/// leaf-crate constraint. The format is frozen at `v: 1`, so a literal is
/// stable — and it can carry better descriptions than a derived schema would.
pub const DOCUMENT_SCHEMA: &str = r##"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Zoetrope document",
  "description": "A complete, serializable description of a video. Time is in integer ticks at 705600000 ticks/second; fps is only an export hint.",
  "type": "object",
  "required": ["v", "timebase", "size", "scenes"],
  "additionalProperties": false,
  "properties": {
    "v": { "const": 1, "description": "Document format version. Always 1." },
    "timebase": { "const": 705600000, "description": "Ticks per second. Always 705600000 (Flicks)." },
    "defaultFps": { "type": "integer", "minimum": 1, "description": "Export hint. Must divide 705600000 exactly." },
    "size": {
      "type": "object",
      "required": ["w", "h"],
      "additionalProperties": false,
      "properties": {
        "w": { "type": "integer", "minimum": 1 },
        "h": { "type": "integer", "minimum": 1 }
      }
    },
    "bg": { "$ref": "#/$defs/color", "description": "Canvas background. Defaults to #000000." },
    "assets": {
      "type": "object",
      "description": "Asset id -> asset. Ids must match [A-Za-z0-9_-]{1,64}.",
      "additionalProperties": { "$ref": "#/$defs/asset" }
    },
    "scenes": {
      "type": "array",
      "minItems": 1,
      "items": { "$ref": "#/$defs/scene" }
    }
  },
  "$defs": {
    "color": { "type": "string", "pattern": "^#[0-9A-Fa-f]{6}$" },
    "asset": {
      "oneOf": [
        {
          "type": "object",
          "required": ["type", "src"],
          "additionalProperties": false,
          "properties": {
            "type": { "const": "image" },
            "src": { "type": "string", "description": "Path to a PNG or JPEG, resolved against assetBaseDir." }
          }
        },
        {
          "type": "object",
          "required": ["type", "src"],
          "additionalProperties": false,
          "properties": {
            "type": { "const": "font" },
            "src": {
              "type": "string",
              "description": "Path to a TTF/OTF, or a reserved src for a bundled font: 'zoetrope:inter' or 'zoetrope:jetbrains-mono'. There are no system fonts."
            }
          }
        }
      ]
    },
    "scene": {
      "type": "object",
      "required": ["id", "duration", "elements"],
      "additionalProperties": false,
      "properties": {
        "id": { "type": "string", "pattern": "^[A-Za-z0-9_-]{1,64}$" },
        "duration": { "type": "integer", "minimum": 1, "description": "Scene length in ticks." },
        "transition": {
          "type": "object",
          "required": ["type", "duration"],
          "additionalProperties": false,
          "description": "Transition INTO this scene. Not allowed on the first scene, and must not exceed the shorter of the two adjacent scenes.",
          "properties": {
            "type": { "const": "crossfade" },
            "duration": { "type": "integer", "minimum": 1 }
          }
        },
        "elements": { "type": "array", "items": { "$ref": "#/$defs/element" } }
      }
    },
    "element": {
      "oneOf": [
        {
          "type": "object",
          "required": ["type", "asset", "rect"],
          "properties": {
            "type": { "const": "image" },
            "asset": { "type": "string" },
            "rect": { "$ref": "#/$defs/rect" }
          },
          "$ref": "#/$defs/commonProps"
        },
        {
          "type": "object",
          "required": ["type", "text", "font", "sizePx", "color", "pos"],
          "properties": {
            "type": { "const": "text" },
            "text": { "type": "string" },
            "font": { "type": "string", "description": "A font asset id." },
            "sizePx": { "type": "number", "exclusiveMinimum": 0 },
            "color": { "$ref": "#/$defs/color" },
            "pos": { "$ref": "#/$defs/vec2" },
            "maxW": { "type": "number", "exclusiveMinimum": 0, "description": "Wrap width in pixels." },
            "align": { "enum": ["left", "center", "right"] }
          },
          "$ref": "#/$defs/commonProps"
        },
        {
          "type": "object",
          "required": ["type", "rect", "fill"],
          "properties": {
            "type": { "const": "rect" },
            "rect": { "$ref": "#/$defs/rect" },
            "fill": { "$ref": "#/$defs/color" }
          },
          "$ref": "#/$defs/commonProps"
        },
        {
          "type": "object",
          "required": ["type", "origin", "children"],
          "properties": {
            "type": { "const": "group" },
            "origin": { "$ref": "#/$defs/vec2" },
            "children": { "type": "array", "items": { "$ref": "#/$defs/element" } }
          },
          "$ref": "#/$defs/commonProps"
        }
      ]
    },
    "rect": { "type": "array", "minItems": 4, "maxItems": 4, "items": { "type": "number" }, "description": "[x, y, w, h]" },
    "vec2": { "type": "array", "minItems": 2, "maxItems": 2, "items": { "type": "number" }, "description": "[x, y]" },
    "commonProps": {
      "description": "Every element accepts these. Base geometry is static; only these four properties animate.",
      "properties": {
        "translate": { "$ref": "#/$defs/vec2" },
        "scale": { "type": "number" },
        "rotation": { "type": "number", "description": "Degrees." },
        "opacity": { "type": "number", "minimum": 0, "maximum": 1 },
        "animations": { "type": "array", "items": { "$ref": "#/$defs/track" } }
      }
    },
    "track": {
      "type": "object",
      "required": ["prop", "keys"],
      "additionalProperties": false,
      "properties": {
        "prop": { "enum": ["translate", "scale", "rotation", "opacity"] },
        "keys": {
          "type": "array",
          "minItems": 1,
          "description": "Keyframes, strictly increasing in t.",
          "items": {
            "type": "object",
            "required": ["t", "v"],
            "additionalProperties": false,
            "properties": {
              "t": { "type": "integer", "description": "Time in ticks, relative to the scene." },
              "v": {
                "description": "A number, except for 'translate', which takes [x, y].",
                "oneOf": [{ "type": "number" }, { "$ref": "#/$defs/vec2" }]
              },
              "ease": { "enum": ["linear", "inCubic", "outCubic", "inOutCubic"] }
            }
          }
        }
      }
    }
  }
}"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_literal_is_valid_json() {
        serde_json::from_str::<serde_json::Value>(DOCUMENT_SCHEMA)
            .expect("DOCUMENT_SCHEMA must parse");
    }

    #[test]
    fn every_corpus_entry_is_listed_and_readable() {
        for entry in zoetrope_core::corpus::corpus() {
            let uri = format!("zoetrope://corpus/{}", entry.name);
            let text = read(&uri).unwrap_or_else(|| panic!("{uri} not readable"));
            zoetrope_core::Document::from_json(&text)
                .unwrap_or_else(|e| panic!("{uri} is not a valid document: {e}"));
        }
        assert_eq!(list().len(), zoetrope_core::corpus::corpus().len() + 1);
    }
}
```

- [ ] **Step 4: Wire resources into the handler**

In `crates/mcp/src/lib.rs`, change the capabilities line in `get_info`:

```rust
capabilities: ServerCapabilities::builder()
    .enable_tools()
    .enable_resources()
    .build(),
```

And add these two methods inside the `#[tool_handler] impl ServerHandler`
block:

```rust
    async fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListResourcesResult, rmcp::ErrorData> {
        Ok(rmcp::model::ListResourcesResult::with_all_items(
            crate::resources::list(),
        ))
    }

    async fn read_resource(
        &self,
        request: rmcp::model::ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResult, rmcp::ErrorData> {
        let uri = request.uri.clone();
        let text = crate::resources::read(&uri).ok_or_else(|| {
            rmcp::ErrorData::resource_not_found(
                format!("unknown resource: {uri}"),
                None,
            )
        })?;
        Ok(rmcp::model::ReadResourceResult::new(vec![
            rmcp::model::ResourceContents::TextResourceContents {
                uri,
                mime_type: Some("application/json".into()),
                text,
                meta: None,
            },
        ]))
    }
```

If `ErrorData::resource_not_found` does not exist in 3.1.4, use
`ErrorData::invalid_params(...)`. Add `pub mod resources;` to `lib.rs`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p zoetrope-mcp`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/mcp
git commit -m "feat(mcp): expose document schema and golden corpus as resources"
```

---

### Task 8: CI, docs, and final verification

**Files:**
- Modify: `.github/workflows/*.yml` (the `rust` job)
- Modify: `README.md`
- Create: `crates/mcp/README.md`

**Interfaces:**
- Consumes: the finished server.
- Produces: no code interfaces.

- [ ] **Step 1: Read the existing workflow**

Run: `ls .github/workflows && cat .github/workflows/*.yml`

Identify the `rust` job. Do not restructure it; add to it.

- [ ] **Step 2: Install ffmpeg in the `rust` job**

Add this step to the `rust` job, before the test step:

```yaml
      # The MCP server's mux integration test performs a real ffmpeg run. A
      # silently-skipped mux test is exactly how the "ffmpeg missing returns
      # Ok(false)" bug would ship, so CI must actually execute it.
      - name: Install ffmpeg
        run: sudo apt-get update && sudo apt-get install -y ffmpeg
```

Confirm the job's test step covers the whole workspace (`cargo test
--workspace` or equivalent). If it names crates explicitly, add
`-p zoetrope-mcp`.

Do **not** touch the `wasm-parity` or `web` jobs: `crates/mcp` is native-only
and is not in the wasm build graph.

- [ ] **Step 3: Write the crate README**

Create `crates/mcp/README.md` covering: what the server is, the ffmpeg
prerequisite, how to build (`cargo build -p zoetrope-mcp --release`), an
example MCP client config block pointing at the built binary, and one worked
example call for each of the three tools. State plainly that there is no
published package yet and the binary must be built from source.

- [ ] **Step 4: Add a section to the root README**

Add a short "MCP server" section to `README.md` linking to
`crates/mcp/README.md` and to the design spec. Keep it consistent with how the
existing surfaces (CLI, SDKs) are described — this is a fourth surface, not a
new headline.

- [ ] **Step 5: Full verification**

Run each of these and confirm the actual output before claiming completion:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p zoetrope-wasm --target wasm32-unknown-unknown
cargo tree -p zoetrope-core --invert -p zoetrope-mcp
```

Expected: fmt clean; clippy clean; all tests pass; wasm builds; the `cargo
tree` invocation shows no dependency path from core to mcp.

- [ ] **Step 6: Manually drive the server once**

Build it and pipe a real handshake through it, to confirm it behaves outside
the test harness:

```bash
cargo build -p zoetrope-mcp
printf '%s\n%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"manual","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | ./target/debug/zoetrope-mcp
```

Expected: two JSON-RPC response lines, the second listing all three tools.

- [ ] **Step 7: Commit**

```bash
git add .github README.md crates/mcp/README.md
git commit -m "ci(mcp): install ffmpeg for the mux test; document the MCP server"
```

---

## Notes for the executor

**Where this plan will most likely be wrong.** The rmcp API details above were
read from the vendored 3.1.4 source, but three spots are worth verifying
rather than trusting:

1. `ServerInfo`'s exact field set in `get_info` — if construction fails,
   build from `Default::default()` and assign fields.
2. `ErrorData::resource_not_found` — may not exist; `invalid_params` is the
   fallback.
3. Whether `rmcp` re-exports `base64` for the test helper.

None of these change the design; if one differs, adapt locally and keep going.

**The `schemars::schema_for!(Document)` problem in Task 7 is real, not
hypothetical.** `zoetrope_core::Document` does not derive `JsonSchema`, and
adding the derive would push `schemars` into `crates/core` and break the
leaf-crate constraint. Hand-write the schema. Do not add the derive.

**Do not "fix" `mux_with_ffmpeg` in `crates/core`.** Its `Ok(false)` contract
is correct for the CLI. The server compensates by preflighting; that
asymmetry is deliberate and documented in spec §7.
