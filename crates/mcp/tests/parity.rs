//! The server's own loading path must produce byte-identical pixels to the
//! committed corpus goldens. If this fails while `crates/core`'s golden test
//! passes, the bug is in this crate's document loading or asset resolution.

use std::collections::BTreeMap;
use std::path::PathBuf;

use kineto_mcp::render::{resolve_preview, TICKS_PER_MS};
use kineto_mcp::source::{load_document, resolve_assets};
use kineto_mcp::timeline::summary;
use sha2::{Digest, Sha256};

fn repo(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
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

    for entry in kineto_core::corpus::corpus() {
        // Round-trip through canonical JSON so the server's parser is what
        // builds the document, exactly as it would for a real tool call.
        let json = entry.doc.canonical_json();
        let (doc, _) = load_document(Some(&json), None).expect("corpus doc parses");
        let assets = resolve_assets(&doc, &assets_dir).expect("corpus assets resolve");
        let mut engine = kineto_core::Engine::new(doc, assets).expect("engine builds");

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

    // The number is knowable, so assert it exactly: the ten corpus entries
    // contribute 26 `name@tick` keys between them. `> 0` would still pass if
    // a key-format drift silently skipped 25 of the 26.
    let expected: usize = kineto_core::corpus::corpus()
        .iter()
        .map(|e| e.ticks.len())
        .sum();
    assert_eq!(expected, 26, "corpus tick count changed");
    assert_eq!(
        checked, expected,
        "not every corpus tick was checked against a golden — the key format \
         has drifted"
    );
}

/// Asking for a moment in milliseconds must land on exactly the tick the
/// committed goldens pin — and those pixels must be the golden pixels.
///
/// This is what stops the millisecond surface from becoming a second, looser
/// source of truth: `preview_document` addresses time differently from
/// `render_document`, and without this it could drift a frame and still look
/// entirely plausible.
///
/// Runs at 100 fps (a 10 ms frame grid) because every whole-millisecond
/// corpus tick is a multiple of 10 ms and so falls exactly on a boundary.
/// The two `dur - 1` ticks are not whole milliseconds and are skipped: no
/// millisecond names them, which is a property of the corpus, not a gap.
#[test]
fn a_millisecond_request_resolves_to_the_exact_tick_the_goldens_pin() {
    const FPS: i64 = 100;

    let goldens: BTreeMap<String, String> = serde_json::from_str(
        &std::fs::read_to_string(repo("testdata/golden/hashes.json"))
            .expect("read testdata/golden/hashes.json"),
    )
    .expect("parse golden hashes");

    let assets_dir = repo("testdata/assets");
    let mut checked = 0usize;

    for entry in kineto_core::corpus::corpus() {
        let json = entry.doc.canonical_json();
        let (doc, _) = load_document(Some(&json), None).expect("corpus doc parses");
        let assets = resolve_assets(&doc, &assets_dir).expect("corpus assets resolve");
        let timeline = summary(&doc);
        let mut engine = kineto_core::Engine::new(doc, assets).expect("engine builds");

        for tick in &entry.ticks {
            let key = format!("{}@{}", entry.name, tick);
            let Some(expected) = goldens.get(&key) else {
                continue;
            };
            if tick % TICKS_PER_MS != 0 {
                continue;
            }
            let ms = tick / TICKS_PER_MS;

            let (outcome, frames) = resolve_preview(&engine, FPS, &timeline, &[ms], &[])
                .expect("corpus moment resolves");
            assert_eq!(frames.len(), 1);
            assert_eq!(
                outcome.samples[0].tick, *tick,
                "{ms} ms resolved to tick {} but the golden {key} is at {tick}",
                outcome.samples[0].tick
            );

            let actual: String = Sha256::digest(engine.render(outcome.samples[0].tick))
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_eq!(&actual, expected, "pixels at {ms} ms differ from {key}");
            checked += 1;
        }
    }

    // 24 of the 26 golden ticks are whole milliseconds; the other two are
    // `dur - 1`. Asserted exactly so that a key-format or alignment drift
    // that silently skipped most of them cannot pass.
    assert_eq!(
        checked, 24,
        "expected 24 whole-millisecond corpus ticks to be checked"
    );
}
