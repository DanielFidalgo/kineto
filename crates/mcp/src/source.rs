//! Turning tool parameters into a validated `Document` plus a populated
//! `AssetStore`, resolving asset `src` values against the filesystem.

use std::path::{Path, PathBuf};

use kineto_core::assets::AssetStore;
use kineto_core::doc::TIMEBASE;
use kineto_core::{Asset, Document};

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

/// Resolve the export rate: the explicit argument, else the document's own
/// `defaultFps`, else 30 (spec §4.1).
///
/// `crates/core` does not validate `default_fps`, so an unusable one reaches
/// us here. The error says where the number came from — otherwise a caller who
/// passed no `fps` is told off for a value they never sent.
pub fn resolve_fps(explicit: Option<i64>, doc: &Document) -> Result<i64, ToolError> {
    let (fps, from_document) = match explicit {
        Some(fps) => (fps, false),
        None => match doc.default_fps {
            Some(fps) => (i64::from(fps), true),
            None => (crate::tools::default_fps(), false),
        },
    };
    check_fps(fps).map_err(|e| {
        if from_document {
            ToolError::Invalid(format!("the document's `defaultFps` is unusable — {e}"))
        } else {
            e
        }
    })?;
    Ok(fps)
}

/// Stage bytes for every asset the document references.
///
/// Reserved font srcs (`kineto:inter`, `kineto:jetbrains-mono`) come from
/// the bytes bundled into `kineto-core`; everything else is a filesystem
/// path resolved against `base_dir`. Absolute srcs are used as-is. There is
/// no network fetching — a document whose pixels depend on a URL would not be
/// reproducible.
pub fn resolve_assets(doc: &Document, base_dir: &Path) -> Result<AssetStore, ToolError> {
    let mut store = AssetStore::new();
    for (id, asset) in &doc.assets {
        let src = match asset {
            Asset::Image { src } | Asset::Font { src } => src,
        };

        if let Some(bytes) = kineto_core::resolve_reserved_src(src) {
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

/// Largest accepted export rate.
///
/// Dividing the timebase is necessary but not sufficient: `TIMEBASE` divides
/// itself, so `fps = 705600000` passed the divisibility check and asked the
/// server for 705 million frames. 1000 divides the timebase exactly
/// (705600000 / 1000 = 705600) and is far above any real frame rate.
pub const MAX_FPS: i64 = 1000;

/// Largest accepted canvas edge, in pixels.
pub const MAX_CANVAS_EDGE: u32 = 16_384;

/// Largest accepted canvas area, in pixels. 64 Mpx — comfortably above 8K
/// (7680 x 4320 = 33 Mpx).
pub const MAX_CANVAS_PIXELS: u64 = 67_108_864;

/// `Engine::tick_for_frame` asserts divisibility; we check it first so bad
/// caller input is a readable tool error rather than a panic that kills the
/// server. The upper bound is ours: an fps that divides the timebase can
/// still be absurd, and frame count scales linearly with it.
pub fn check_fps(fps: i64) -> Result<(), ToolError> {
    if fps <= 0 || fps > MAX_FPS || TIMEBASE % fps != 0 {
        return Err(ToolError::Fps(fps));
    }
    Ok(())
}

/// Bound the canvas before an `Engine` is built.
///
/// `Engine::new` allocates two full-canvas pixmaps and decodes every
/// referenced asset, and core's `validate_semantics` puts no ceiling on
/// `size` — so an unbounded canvas is unbounded allocation, on the
/// `validateOnly` path advertised as rendering nothing.
pub fn check_canvas_size(w: u32, h: u32) -> Result<(), ToolError> {
    if w > MAX_CANVAS_EDGE || h > MAX_CANVAS_EDGE {
        return Err(ToolError::Invalid(format!(
            "canvas {w}x{h} is too large: each edge must be at most \
             {MAX_CANVAS_EDGE} px"
        )));
    }
    // u64 so the product cannot overflow: u32::MAX squared is ~1.8e19,
    // which fits u64 (max ~1.84e19) but not u32 or i64.
    let pixels = w as u64 * h as u64;
    if pixels > MAX_CANVAS_PIXELS {
        return Err(ToolError::Invalid(format!(
            "canvas {w}x{h} is {pixels} pixels: at most {MAX_CANVAS_PIXELS} \
             pixels are permitted (64 Mpx, above 8K)"
        )));
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
        let doc = kineto_core::Document::new(16, 16).canonical_json();
        let (parsed, base) = load_document(Some(&doc), None).unwrap();
        assert_eq!(parsed.size.w, 16);
        assert_eq!(base, std::env::current_dir().unwrap());
    }

    #[test]
    fn path_document_base_dir_is_its_parent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        std::fs::write(&path, kineto_core::Document::new(8, 8).canonical_json()).unwrap();
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
        let mut doc = kineto_core::Document::new(8, 8);
        doc.add_asset("f", kineto_core::Asset::font("kineto:jetbrains-mono"));
        let store = resolve_assets(&doc, std::path::Path::new("/nonexistent")).unwrap();
        // `prepare` is what actually decodes; getting here without an I/O error
        // proves the reserved src never hit the filesystem.
        drop(store);
    }

    #[test]
    fn missing_asset_file_names_the_resolved_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = kineto_core::Document::new(8, 8);
        doc.add_asset("i", kineto_core::Asset::image("missing.png"));
        let result = resolve_assets(&doc, dir.path());
        if let Err(err) = result {
            let msg = err.to_string();
            assert!(msg.contains("missing.png"), "message was: {msg}");
            assert!(msg.contains("'i'"), "message was: {msg}");
            assert!(
                msg.contains(dir.path().to_str().unwrap()),
                "message must carry the resolved path, not the raw src: {msg}"
            );
        } else {
            panic!("expected error but got success");
        }
    }

    #[test]
    fn resolves_absolute_path_asset_ignoring_base_dir() {
        let asset_dir = tempfile::tempdir().unwrap();
        let base_dir = tempfile::tempdir().unwrap();
        // Write a file to asset_dir and reference it by absolute path
        let asset_path = asset_dir.path().join("image.dat");
        std::fs::write(&asset_path, b"fake image bytes").unwrap();
        let mut doc = kineto_core::Document::new(8, 8);
        // Use the absolute path directly
        doc.add_asset(
            "img",
            kineto_core::Asset::image(asset_path.to_str().unwrap()),
        );
        // Resolve against an unrelated base_dir; the absolute path should be used as-is
        let store = resolve_assets(&doc, base_dir.path()).unwrap();
        // If the asset was staged successfully, we got the file from the absolute path
        // (if base_dir had been used, we'd get a file-not-found error)
        drop(store);
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

    #[test]
    fn rejects_fps_above_the_upper_bound() {
        // Dividing the timebase is not enough: TIMEBASE itself divides it,
        // and was accepted — `{"fps": 705600000}` reported 705600000 frames
        // and would have tried to write that many PNGs.
        assert!(check_fps(MAX_FPS).is_ok(), "1000 divides the timebase");
        assert!(check_fps(1200).is_err(), "1200 divides but is out of range");
        assert!(check_fps(TIMEBASE).is_err());
    }

    #[test]
    fn the_fps_error_names_the_upper_bound() {
        let msg = check_fps(TIMEBASE).unwrap_err().to_string();
        assert!(msg.contains("1000"), "message was: {msg}");
    }

    #[test]
    fn accepts_a_canvas_within_both_limits() {
        assert!(check_canvas_size(320, 180).is_ok());
        assert!(check_canvas_size(7680, 4320).is_ok(), "8K must be allowed");
        assert!(check_canvas_size(MAX_CANVAS_EDGE, 4096).is_ok());
    }

    #[test]
    fn rejects_a_canvas_edge_beyond_the_limit() {
        let msg = check_canvas_size(MAX_CANVAS_EDGE + 1, 8)
            .unwrap_err()
            .to_string();
        assert!(msg.contains("16385"), "must name the actual size: {msg}");
        assert!(msg.contains("16384"), "must name the limit: {msg}");
        assert!(check_canvas_size(8, MAX_CANVAS_EDGE + 1).is_err());
    }

    #[test]
    fn rejects_a_canvas_area_beyond_the_limit() {
        // Both edges legal, product not: 16384 x 16384 is 268 Mpx.
        let msg = check_canvas_size(MAX_CANVAS_EDGE, MAX_CANVAS_EDGE)
            .unwrap_err()
            .to_string();
        assert!(msg.contains("67108864"), "must name the limit: {msg}");
    }

    #[test]
    fn the_canvas_area_check_cannot_overflow() {
        // u32::MAX squared is ~1.8e19, past u64's midpoint but inside it.
        assert!(check_canvas_size(u32::MAX, u32::MAX).is_err());
    }
}
