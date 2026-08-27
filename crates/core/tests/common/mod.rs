use std::path::PathBuf;
pub fn repo(p: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(p)
}
/// Compare `actual` against a checked-in golden file.
/// Run with UPDATE_GOLDEN=1 to (re)write the golden instead.
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
