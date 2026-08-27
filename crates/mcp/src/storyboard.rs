//! Builds a `Document` from an ordered list of images — the "agent shows its
//! work" path. Deliberately a pure builder over existing primitives: the
//! deferred mysteryshopper tape adapter becomes "parse actions.jsonl, call
//! `build`".

use std::path::Path;

use kineto_core::doc::TIMEBASE;
use kineto_core::{Asset, Document, Element, Scene};

use crate::error::ToolError;

/// Exact: 705_600_000 ticks/second / 1000 ms. No rounding at any duration.
const TICKS_PER_MS: i64 = TIMEBASE / 1000;

/// Longest a single frame may be held, in milliseconds (24 hours).
///
/// A frame longer than a day is a typo, not a request. The real job of this
/// bound is arithmetic: `duration_ms * TICKS_PER_MS` is an `i64` multiply,
/// and `timeline::total_duration` then *sums* the scenes.
pub const MAX_FRAME_DURATION_MS: i64 = 86_400_000;

/// Most frames a single storyboard may contain.
///
/// Paired with [`MAX_FRAME_DURATION_MS`] this caps the summed timeline at
/// 86_400_000 x 705_600 x 10_000 ~= 6.1e17 ticks, comfortably inside
/// `i64::MAX` (~9.2e18).
pub const MAX_FRAMES: usize = 10_000;

pub const CAPTION_FONT_ID: &str = "caption-font";
pub const CAPTION_FONT_SRC: &str = "kineto:jetbrains-mono";

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
    if frames.len() > MAX_FRAMES {
        return Err(ToolError::Invalid(format!(
            "`frames` has {} entries: at most {MAX_FRAMES} are permitted",
            frames.len()
        )));
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
                "frame {i}: durationMs must be positive, got {} (the \
                 permitted range is 1..={MAX_FRAME_DURATION_MS} ms, 24 hours)",
                frame.duration_ms
            )));
        }
        if frame.duration_ms > MAX_FRAME_DURATION_MS {
            return Err(ToolError::Invalid(format!(
                "frame {i}: durationMs {} exceeds the limit of \
                 {MAX_FRAME_DURATION_MS} ms (24 hours); a storyboard may also \
                 have at most {MAX_FRAMES} frames",
                frame.duration_ms
            )));
        }
        // Bounded above, so this cannot fail — but an unchecked multiply here
        // panicked in debug (leaving the request unanswered) and wrapped
        // silently in release, so the fallible form stays.
        let duration_ticks = frame.duration_ms.checked_mul(TICKS_PER_MS).ok_or_else(|| {
            ToolError::Invalid(format!(
                "frame {i}: durationMs {} overflows the tick timebase; the \
                 limit is {MAX_FRAME_DURATION_MS} ms (24 hours)",
                frame.duration_ms
            ))
        })?;

        // Asset ids must match [A-Za-z0-9_-]{1,64} (DocError::BadId), so they
        // are generated rather than derived from user-supplied filenames.
        let asset_id = format!("img-{i}");
        doc.add_asset(&asset_id, Asset::image(&frame.image));

        let mut scene = Scene::new(&format!("frame-{i}"), duration_ticks)
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

        let elements = &doc.scenes[0].elements;
        assert_eq!(elements.len(), 3, "image + caption band + text");
        assert!(
            matches!(elements[0], kineto_core::Element::Image { .. }),
            "element 0 should be the image, got {:?}",
            elements[0]
        );
        assert!(
            matches!(elements[1], kineto_core::Element::Rect { .. }),
            "element 1 should be the caption band, got {:?}",
            elements[1]
        );
        match &elements[2] {
            kineto_core::Element::Text { text, font, .. } => {
                assert_eq!(text, "clicked Checkout");
                assert_eq!(font, CAPTION_FONT_ID);
            }
            other => panic!("element 2 should be the caption text, got {other:?}"),
        }
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
        assert_eq!(
            doc.scenes[0].elements.len(),
            1,
            "no caption means image only"
        );
        assert!(!doc.assets.contains_key(CAPTION_FONT_ID));
    }

    #[test]
    fn rejects_a_duration_that_would_overflow_the_tick_multiply() {
        // Reproduces the reported panic: `duration_ms * TICKS_PER_MS` used to
        // be an unchecked multiply, which panicked in debug (leaving the
        // request unanswered) and silently wrapped in release.
        let dir = tempfile::tempdir().unwrap();
        let frames = vec![Frame {
            image: write_png(dir.path(), "a.png", 32, 32),
            duration_ms: 10_000_000_000_000_000,
            caption: None,
        }];
        let err = build(&frames, None).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("86400000"),
            "the error must name the limit: {msg}"
        );
    }

    #[test]
    fn rejects_a_duration_above_the_24_hour_limit() {
        let dir = tempfile::tempdir().unwrap();
        let frames = vec![Frame {
            image: write_png(dir.path(), "a.png", 32, 32),
            duration_ms: MAX_FRAME_DURATION_MS + 1,
            caption: None,
        }];
        assert!(build(&frames, None).is_err());
    }

    #[test]
    fn accepts_a_duration_exactly_at_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let frames = vec![Frame {
            image: write_png(dir.path(), "a.png", 32, 32),
            duration_ms: MAX_FRAME_DURATION_MS,
            caption: None,
        }];
        let doc = build(&frames, None).unwrap();
        assert_eq!(doc.scenes[0].duration, MAX_FRAME_DURATION_MS * 705_600);
    }

    #[test]
    fn rejects_more_frames_than_the_limit() {
        // The per-frame ceiling alone does not bound the *sum*
        // (`timeline::total_duration` adds them up), so the list length is
        // bounded too. Built without touching disk: an explicit size means
        // `build` never reads the first image, and the length check fires
        // before any asset work.
        let frames: Vec<Frame> = (0..MAX_FRAMES + 1)
            .map(|i| Frame {
                image: format!("frame-{i}.png"),
                duration_ms: 1,
                caption: None,
            })
            .collect();
        let err = build(&frames, Some((32, 32))).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("10000"),
            "the error must name the limit: {msg}"
        );
    }

    #[test]
    fn the_worst_case_permitted_total_duration_fits_in_i64() {
        // The two bounds exist to keep the summed timeline inside i64. If
        // either is ever raised, this is the check that must be re-derived.
        let worst = (MAX_FRAME_DURATION_MS as i128) * (TICKS_PER_MS as i128) * (MAX_FRAMES as i128);
        assert!(worst < i64::MAX as i128, "worst case {worst} overflows i64");
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
        kineto_core::Engine::new(doc, store).expect("engine accepts the built document");
    }
}
