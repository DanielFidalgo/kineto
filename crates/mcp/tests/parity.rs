//! The server's own loading path must produce byte-identical pixels to the
//! committed corpus goldens. If this fails while `crates/core`'s golden test
//! passes, the bug is in this crate's document loading or asset resolution.

use std::collections::BTreeMap;
use std::path::PathBuf;

use sha2::{Digest, Sha256};
use zoetrope_mcp::source::{load_document, resolve_assets};

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

    // The number is knowable, so assert it exactly: the six corpus entries
    // contribute 18 `name@tick` keys between them. `> 0` would still pass if
    // a key-format drift silently skipped 17 of the 18.
    let expected: usize = zoetrope_core::corpus::corpus()
        .iter()
        .map(|e| e.ticks.len())
        .sum();
    assert_eq!(expected, 18, "corpus tick count changed");
    assert_eq!(
        checked, expected,
        "not every corpus tick was checked against a golden — the key format \
         has drifted"
    );
}
