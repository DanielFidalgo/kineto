use std::collections::BTreeMap;
use std::path::PathBuf;
pub fn repo(p: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(p)
}
/// Compare `actual` against a checked-in golden file.
/// Run with UPDATE_GOLDEN=1 to (re)write the golden instead.
/// Not every test binary that includes this shared `mod common` uses this
/// helper (each integration test file compiles its own copy of `common`).
#[allow(dead_code)]
pub fn assert_golden(rel_path: &str, actual: &[u8]) {
    let path = repo(rel_path);
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read(&path)
        .unwrap_or_else(|_| panic!("missing golden {rel_path}; run with UPDATE_GOLDEN=1"));
    assert_eq!(expected, actual, "golden mismatch: {rel_path}");
}

/// Compare the sha256 hex digest of a raster buffer (e.g.
/// `pixmap.data()`, premultiplied RGBA) against a checked-in entry in
/// `testdata/golden/hashes.json` (a `{name: hex}` object, keys sorted).
/// Run with UPDATE_GOLDEN=1 to insert/rewrite the entry for `name`.
///
/// A raster is a lot of bytes to check into git for every golden — hashing
/// keeps the repo small while still pinning exact pixel output. As a sighted
/// complement, this also (unconditionally, pass or fail, golden or not)
/// dumps the buffer as a PNG to `<target>/debug-goldens/<name>.png` so a
/// human can eyeball what actually rendered.
///
/// `w`/`h` are required (not inferred from `rgba.len()`): reconstructing
/// dimensions via `sqrt(len/4)` silently mis-writes any non-square buffer
/// whose pixel count happens to be a perfect square (e.g. a 256x64 canvas
/// is 16384px = 128², so it would get written out as 128x128 garbage with
/// no error). Callers must pass the real dimensions of `rgba`.
#[allow(dead_code)]
pub fn assert_golden_hash(name: &str, w: u32, h: u32, rgba: &[u8]) {
    assert_eq!(
        rgba.len(),
        (w * h * 4) as usize,
        "assert_golden_hash: rgba.len() ({}) doesn't match w*h*4 ({}x{}x4)",
        rgba.len(),
        w,
        h
    );
    let hex_hash = sha256_hex(rgba);
    write_debug_png(name, w, h, rgba);

    let hashes_path = repo("testdata/golden/hashes.json");
    let mut hashes = read_hashes(&hashes_path);

    if std::env::var("UPDATE_GOLDEN").is_ok() {
        hashes.insert(name.to_string(), hex_hash);
        std::fs::create_dir_all(hashes_path.parent().unwrap()).unwrap();
        // BTreeMap serializes in key order, so the file stays sorted.
        let json = serde_json::to_string_pretty(&hashes).unwrap();
        std::fs::write(&hashes_path, format!("{json}\n")).unwrap();
        return;
    }

    let expected = hashes
        .get(name)
        .unwrap_or_else(|| panic!("missing golden hash for '{name}'; run with UPDATE_GOLDEN=1"));
    assert_eq!(*expected, hex_hash, "golden hash mismatch: {name}");
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn read_hashes(path: &std::path::Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// `target/debug-goldens/<name>.png`, honoring `CARGO_TARGET_DIR` if set
/// (this workspace's dev machines may override it away from `<repo>/target`).
fn debug_goldens_dir() -> PathBuf {
    match std::env::var_os("CARGO_TARGET_DIR") {
        Some(dir) => PathBuf::from(dir).join("debug-goldens"),
        None => repo("target/debug-goldens"),
    }
}

fn write_debug_png(name: &str, w: u32, h: u32, rgba: &[u8]) {
    let img = image::RgbaImage::from_raw(w, h, rgba.to_vec())
        .expect("RGBA buffer size mismatch building debug PNG");
    let dir = debug_goldens_dir();
    std::fs::create_dir_all(&dir).unwrap();
    img.save(dir.join(format!("{name}.png"))).unwrap();
}
