//! `crates/wasm`: the wasm-bindgen shim around `kineto_core::Engine`.
//!
//! Thin delegation only — no rendering logic lives here. `packages/sdk`
//! (`engine.ts`, Task 17) binds to exactly the `WasmEngine` surface below;
//! do not change its shape without updating that binding.
//!
//! Ticks cross the JS boundary as `f64`, never `i64`/BigInt: wasm-bindgen
//! maps `i64` to a JS `BigInt`, which is needless friction for callers that
//! just want to pass a number. Every legal tick fits in `f64`'s 53-bit
//! exact-integer range, so the cast is lossless in practice.
//!
//! `default-features = false` on the `kineto-core` dependency is load
//! bearing: it excludes the `bundled-fonts` feature (Inter/JetBrains Mono
//! `include_bytes!`), keeping those fonts out of the wasm binary entirely
//! (size budget, spec §8). The JS host supplies font bytes via `add_asset`
//! instead.

use kineto_core::render::unpremultiply;
use kineto_core::{AssetStore, DocError, Document, Engine};
use std::fmt;
use wasm_bindgen::prelude::*;

/// Everything that can go wrong in the shim's own plumbing, on top of
/// `kineto_core::DocError` (bad/unvalidated documents, decode failures).
/// Kept as a plain Rust error (not `JsError`) so the fallible logic below
/// stays natively unit-testable: constructing a `JsError`/`JsValue` calls
/// into wasm-bindgen's JS-imported glue, which panics with "cannot call
/// wasm-bindgen imported functions on non-wasm targets" under a normal
/// `cargo test` — there's no JS engine to call into. Each `#[wasm_bindgen]`
/// method converts a `ShimError` to `JsError` only at the actual boundary
/// (`map_err(JsError::from)`), so native tests exercise every error branch
/// by calling the `*_inner` methods directly and asserting on `ShimError`,
/// never touching `JsError`.
#[derive(Debug)]
enum ShimError {
    Doc(DocError),
    AlreadyReady,
    NotReady,
}

impl fmt::Display for ShimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShimError::Doc(e) => write!(f, "{e}"),
            ShimError::AlreadyReady => {
                write!(f, "WasmEngine::ready: already ready (or new() failed)")
            }
            ShimError::NotReady => write!(f, "WasmEngine: called before ready()"),
        }
    }
}

impl std::error::Error for ShimError {}

impl From<DocError> for ShimError {
    fn from(e: DocError) -> Self {
        ShimError::Doc(e)
    }
}

/// A `Document` in progress toward a renderable `Engine`: parsed and
/// validated (`from_json`), accumulating staged asset bytes (`add_asset`),
/// until `ready()` decodes/loads them and constructs the `Engine`.
///
/// `doc` is consumed by `ready()` (`Option::take`); `inner` is populated by
/// it. Every other method requires `inner` to be `Some` — callers driving
/// the intended `new -> add_asset* -> ready -> render*` lifecycle never hit
/// the panics guarding that invariant.
#[wasm_bindgen]
pub struct WasmEngine {
    inner: Option<Engine>,
    doc: Option<Document>,
    staged: AssetStore,
}

#[wasm_bindgen]
impl WasmEngine {
    /// Parse and validate `doc_json` (`Document::from_json`). Does not load
    /// assets or construct the renderer yet — call `add_asset` for every
    /// asset the document references, then `ready()`.
    #[wasm_bindgen(constructor)]
    pub fn new(doc_json: &str) -> Result<WasmEngine, JsError> {
        Self::new_inner(doc_json).map_err(JsError::from)
    }

    /// Stage raw bytes for a doc asset id. Must be called for every asset
    /// id the document references before `ready()`.
    pub fn add_asset(&mut self, id: &str, bytes: &[u8]) {
        self.staged.add_bytes(id, bytes.to_vec());
    }

    /// Decode/load staged assets and construct the `Engine`. Consumes the
    /// parsed `Document`; calling `ready()` a second time errors rather
    /// than silently no-op-ing.
    pub fn ready(&mut self) -> Result<(), JsError> {
        self.ready_inner().map_err(JsError::from)
    }

    pub fn width(&self) -> u32 {
        self.engine().width()
    }

    pub fn height(&self) -> u32 {
        self.engine().height()
    }

    /// Total document duration in ticks, as `f64` (see module docs).
    pub fn duration_ticks(&self) -> f64 {
        self.engine().total_duration() as f64
    }

    /// The tick for export frame number `n` at rate `fps`.
    pub fn tick_for_frame(&self, n: f64, fps: f64) -> f64 {
        self.engine().tick_for_frame(n as i64, fps as i64) as f64
    }

    /// Render `tick` into the engine's internal frame buffer. Read the
    /// result via `frame_ptr`/`frame_len` (zero-copy), `frame_copy`
    /// (premultiplied), or `frame_unpremultiplied` (straight alpha).
    pub fn render(&mut self, tick: f64) -> Result<(), JsError> {
        self.render_inner(tick as i64).map_err(JsError::from)
    }

    /// Pointer into wasm linear memory at the start of the current frame
    /// buffer (premultiplied RGBA8, `frame_len()` bytes). Valid until the
    /// next `render()` call or any allocation that could move/grow memory;
    /// callers must read it out (e.g. via a `Uint8Array` view) before then.
    pub fn frame_ptr(&self) -> *const u8 {
        self.engine().frame_data().as_ptr()
    }

    pub fn frame_len(&self) -> usize {
        self.engine().frame_data().len()
    }

    /// Owned copy of the current frame buffer, premultiplied RGBA8. The
    /// parity gate (Task 16) hashes this.
    pub fn frame_copy(&self) -> Vec<u8> {
        self.engine().frame_data().to_vec()
    }

    /// Owned copy of the current frame buffer, unpremultiplied (straight
    /// alpha) RGBA8 — the layout WebCodecs' `VideoFrame` wants for the
    /// `RGBA` format.
    pub fn frame_unpremultiplied(&mut self) -> Vec<u8> {
        let mut buf = self.engine().frame_data().to_vec();
        unpremultiply(&mut buf);
        buf
    }
}

impl WasmEngine {
    fn engine(&self) -> &Engine {
        self.inner
            .as_ref()
            .expect("WasmEngine: call ready() before this method")
    }

    /// Fallible logic behind the constructor, plain-`Result` so it's
    /// natively unit-testable (see `ShimError` docs above).
    fn new_inner(doc_json: &str) -> Result<WasmEngine, ShimError> {
        let doc = Document::from_json(doc_json)?;
        Ok(WasmEngine {
            inner: None,
            doc: Some(doc),
            staged: AssetStore::new(),
        })
    }

    /// Fallible logic behind `ready()`.
    fn ready_inner(&mut self) -> Result<(), ShimError> {
        let doc = self.doc.take().ok_or(ShimError::AlreadyReady)?;
        let staged = std::mem::take(&mut self.staged);
        self.inner = Some(Engine::new(doc, staged)?);
        Ok(())
    }

    /// Fallible logic behind `render()`.
    fn render_inner(&mut self, tick: i64) -> Result<(), ShimError> {
        let engine = self.inner.as_mut().ok_or(ShimError::NotReady)?;
        engine.render(tick);
        Ok(())
    }
}

/// Corpus exports for parity builds (Task 16): re-exposes
/// `kineto_core::corpus` across the wasm boundary without pulling any
/// extra dependency into the binding (asset (id, src) pairs are encoded as
/// `"id\tsrc"` strings rather than a struct/tuple type).
#[cfg(feature = "corpus")]
mod corpus_bindings {
    use kineto_core::corpus::corpus;
    use kineto_core::{Asset, Document};
    use wasm_bindgen::prelude::*;

    fn find(name: &str) -> kineto_core::corpus::CorpusDoc {
        corpus()
            .into_iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("corpus: unknown doc '{name}'"))
    }

    #[wasm_bindgen]
    pub fn corpus_names() -> Vec<String> {
        corpus().into_iter().map(|c| c.name.to_string()).collect()
    }

    #[wasm_bindgen]
    pub fn corpus_doc_json(name: &str) -> String {
        find(name).doc.canonical_json()
    }

    #[wasm_bindgen]
    pub fn corpus_ticks(name: &str) -> Vec<f64> {
        find(name).ticks.into_iter().map(|t| t as f64).collect()
    }

    /// `(id, src)` for every asset `doc_json` references, one `"id\tsrc"`
    /// string per asset. The JS parity harness (Task 16) uses this to know
    /// which corpus asset bytes to fetch and stage via `add_asset` before
    /// calling `ready()`.
    #[wasm_bindgen]
    pub fn corpus_asset_srcs(doc_json: &str) -> Vec<String> {
        let doc = Document::from_json(doc_json).expect("corpus_asset_srcs: invalid doc json");
        doc.assets
            .iter()
            .map(|(id, asset)| {
                let src = match asset {
                    Asset::Image { src } => src,
                    Asset::Font { src } => src,
                };
                format!("{id}\t{src}")
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kineto_core::doc::{ms, Asset, Document as CoreDocument, Scene};

    fn minimal_doc_json() -> String {
        let mut doc = CoreDocument::new(64, 48);
        doc.push_scene(Scene::new("s", ms(100)));
        doc.canonical_json()
    }

    // Error-path assertions below call the private `*_inner` methods
    // directly rather than the public `#[wasm_bindgen]` API: constructing
    // the `Err` variant of a `#[wasm_bindgen]`-exported `Result<_, JsError>`
    // builds a `JsError`, which panics off a real wasm+JS runtime (see the
    // `ShimError` doc comment). The `Ok` paths never touch `JsError`, so
    // those go through the real public API unmodified — full lifecycle
    // coverage of the actual exported methods, error-path coverage of the
    // logic behind them.

    #[test]
    fn new_accepts_valid_json_and_rejects_garbage() {
        assert!(WasmEngine::new(&minimal_doc_json()).is_ok());
        assert!(WasmEngine::new_inner("not json").is_err());
    }

    #[test]
    fn ready_without_assets_then_render_and_frame_copy_roundtrip() {
        let mut engine = WasmEngine::new(&minimal_doc_json()).unwrap();
        engine.ready().unwrap();
        assert_eq!(engine.width(), 64);
        assert_eq!(engine.height(), 48);
        assert_eq!(engine.duration_ticks(), ms(100) as f64);

        engine.render(0.0).unwrap();
        let copy = engine.frame_copy();
        assert_eq!(copy.len(), engine.frame_len());
        assert_eq!(copy.len(), 64 * 48 * 4);
    }

    #[test]
    fn render_before_ready_errors_instead_of_panicking() {
        let mut engine = WasmEngine::new(&minimal_doc_json()).unwrap();
        assert!(engine.render_inner(0).is_err());
    }

    #[test]
    fn ready_called_twice_errors() {
        let mut engine = WasmEngine::new(&minimal_doc_json()).unwrap();
        engine.ready().unwrap();
        assert!(engine.ready_inner().is_err());
    }

    #[test]
    fn add_asset_stages_bytes_consumed_by_ready() {
        // A doc referencing an image asset fails `ready()` if the bytes
        // were never staged (AssetStore::prepare -> UnknownAssetId), and
        // succeeds once `add_asset` stages *some* decodable bytes.
        let mut doc = CoreDocument::new(4, 4);
        doc.add_asset("img", Asset::image("img.png"));
        doc.push_scene(Scene::new("s", ms(100)));
        let json = doc.canonical_json();

        let mut missing = WasmEngine::new(&json).unwrap();
        assert!(missing.ready_inner().is_err());

        let mut staged = WasmEngine::new(&json).unwrap();
        // `grad.png`, the corpus's own tiny gradient fixture — embedded at
        // compile time (works on wasm32 too, unlike `std::fs::read`) so
        // this test has no dependency on the corpus/native-only I/O.
        const GRAD_PNG: &[u8] = include_bytes!("../../../testdata/assets/grad.png");
        staged.add_asset("img", GRAD_PNG);
        assert!(staged.ready().is_ok());
    }

    #[cfg(feature = "corpus")]
    #[test]
    fn corpus_bindings_round_trip() {
        use super::corpus_bindings::*;

        let names = corpus_names();
        assert!(names.contains(&"kitchen-sink".to_string()));

        let json = corpus_doc_json("image-transform");
        let srcs = corpus_asset_srcs(&json);
        // image-transform stages "grad" and "photo" image assets.
        assert!(srcs.iter().any(|s| s == "grad\tgrad.png"));
        assert!(srcs.iter().any(|s| s == "photo\tphoto.jpg"));

        let ticks = corpus_ticks("image-transform");
        assert!(!ticks.is_empty());
    }
}
