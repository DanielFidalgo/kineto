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
        let result = resolve_assets(&doc, dir.path());
        if let Err(err) = result {
            let msg = err.to_string();
            assert!(msg.contains("missing.png"), "message was: {msg}");
            assert!(msg.contains("'i'"), "message was: {msg}");
        } else {
            panic!("expected error but got success");
        }
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
