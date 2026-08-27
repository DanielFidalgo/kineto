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

        let mut scene = Scene::new(&format!("frame-{i}"), frame.duration_ms * TICKS_PER_MS)
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
            Frame {
                image: write_png(dir.path(), "a.png", 100, 50),
                duration_ms: 500,
                caption: None,
            },
            Frame {
                image: write_png(dir.path(), "b.png", 100, 50),
                duration_ms: 1500,
                caption: None,
            },
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

        assert_eq!(
            doc.scenes[0].elements.len(),
            3,
            "image + caption band + text"
        );
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
