//! Native golden gate over the golden corpus (`crates/core/src/corpus.rs`,
//! spec §6): renders every corpus doc at its pinned ticks and checks the
//! sha256 of each frame against `testdata/golden/hashes.json`.
//!
//! Regenerate with `UPDATE_GOLDEN=1 cargo test -p kineto-core --test
//! golden -- --test-threads=1` — `assert_hash_entry` read-modifies-writes
//! the shared hashes file, which races under parallel test execution.

mod common;

use kineto_core::corpus::{corpus, corpus_load_assets};
use kineto_core::{Document, Engine};

/// Guards against an accidentally-empty corpus (also the RED starting point
/// for this test file, before the six docs existed).
#[test]
fn corpus_is_not_empty() {
    assert!(!corpus().is_empty(), "no corpus docs");
}

/// Every corpus doc must be buildable via the Rust builders AND survive a
/// round trip through the canonical-JSON loading path (`Document::from_json`
/// — the same validation wasm, the CLI, and every other consumer go
/// through). Catches builder-level mistakes (e.g. a bad color literal, an
/// unknown asset id, a transition longer than its scene) that only the
/// semantic-validation pass would surface.
#[test]
fn corpus_docs_round_trip_through_json() {
    for c in corpus() {
        let json = c.doc.canonical_json();
        Document::from_json(&json).unwrap_or_else(|e| {
            panic!(
                "corpus doc '{}' failed to validate via from_json: {e}",
                c.name
            )
        });
    }
}

/// Render every corpus doc, at every pinned tick, on the native engine, and
/// check the frame's sha256 against `testdata/golden/hashes.json` (key
/// `"{doc.name}@{tick}"`). Also writes a debug PNG per tick to
/// `target/debug-goldens/` (or `$CARGO_TARGET_DIR/debug-goldens/`), pass or
/// fail, so a human can eyeball what actually rendered.
#[test]
fn native_corpus_matches_goldens() {
    for c in corpus() {
        let mut engine = Engine::new(c.doc.clone(), corpus_load_assets(&c.doc)).unwrap();
        let (w, h) = (engine.width(), engine.height());
        for t in &c.ticks {
            let frame = engine.render(*t).to_vec();
            let key = format!("{}@{}", c.name, t);
            common::write_debug_png(&key, w, h, &frame);
            common::assert_hash_entry(&key, &common::sha256_hex(&frame));
        }
    }
}
