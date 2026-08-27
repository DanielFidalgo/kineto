//! Native half of the byte-parity gate (Task 16, spec §1/§6): renders every
//! `(corpus doc, tick)` pair natively and writes their sha256 hashes to
//! `target/parity/native-hashes.json`. `tests/parity/run.mjs` (the wasm half)
//! renders the same pairs in wasm and diffs against this file — parity means
//! every hash matches.
//!
//! Requires `--features parity` (pulls in `sha2`, which the rest of the
//! crate has no reason to depend on). Must run from the repo root — writes
//! to `target/parity/`, a path relative to `CARGO_MANIFEST_DIR/../..`, not
//! to the process's cwd, so it's cwd-independent (unlike `run.mjs`, which
//! *does* require repo-root cwd — see that file's header comment).
//!
//! Run: `cargo run -p kineto-core --bin dump-parity --features parity`

use kineto_core::corpus::{corpus, corpus_load_assets};
use kineto_core::Engine;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn main() {
    let mut hashes: BTreeMap<String, String> = BTreeMap::new();

    for c in corpus() {
        let mut engine = Engine::new(c.doc.clone(), corpus_load_assets(&c.doc))
            .unwrap_or_else(|e| panic!("dump-parity: corpus doc '{}' failed: {e}", c.name));
        for t in &c.ticks {
            let frame = engine.render(*t).to_vec();
            let key = format!("{}@{}", c.name, t);
            hashes.insert(key, sha256_hex(&frame));
        }
    }

    let out_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/parity");
    std::fs::create_dir_all(&out_dir)
        .unwrap_or_else(|e| panic!("dump-parity: failed creating {}: {e}", out_dir.display()));
    let out_path = out_dir.join("native-hashes.json");
    let json = serde_json::to_string_pretty(&hashes).expect("dump-parity: serialize hashes");
    std::fs::write(&out_path, format!("{json}\n"))
        .unwrap_or_else(|e| panic!("dump-parity: failed writing {}: {e}", out_path.display()));

    println!(
        "dump-parity: wrote {} hashes to {}",
        hashes.len(),
        out_path.display()
    );
}
