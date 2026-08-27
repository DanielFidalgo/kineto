//! The single path from an `Engine` to an MP4 plus previews. Every tool
//! funnels through here so ffmpeg handling and preview behavior cannot drift
//! between tools.

use std::path::Path;

use base64::prelude::{Engine as _, BASE64_STANDARD};
use kineto_core::export::{export_frames, ffmpeg_available, mux_with_ffmpeg};
use kineto_core::Engine;
use serde::Serialize;

use crate::error::ToolError;

/// Previews are downscaled above this edge length to bound context cost.
/// Above it they are no longer byte-comparable to exported frames and must
/// never be used as parity evidence (spec §6).
pub const PREVIEW_MAX_EDGE: u32 = 720;

/// Hard cap on sampled frames per call, regardless of what the caller asks.
pub const PREVIEW_MAX_COUNT: usize = 12;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderOutcome {
    /// Absent — not empty — when nothing was written (spec §6:
    /// `validate_only` returns the metadata block with no `out`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub out: Option<String>,
    pub width: u32,
    pub height: u32,
    pub fps: i64,
    pub frame_count: u64,
    pub duration_ticks: i64,
    pub duration_seconds: f64,
}

/// Evenly spaced frame indices from the first frame to the last, inclusive.
///
/// Returns frame *indices* rather than ticks so previews are exactly the
/// frames `export_frames` writes — that is what lets a preview be compared
/// byte-for-byte against an exported frame.
pub fn preview_frame_indices(frame_count: u64, count: usize) -> Vec<u64> {
    if count == 0 || frame_count == 0 {
        return Vec::new();
    }
    let count = count.min(PREVIEW_MAX_COUNT).min(frame_count as usize);
    if count == 1 {
        return vec![0];
    }
    let last = frame_count - 1;
    (0..count)
        .map(|i| (i as u64 * last) / (count as u64 - 1))
        .collect()
}

/// How many frames this engine will emit at `fps`.
pub fn frame_count(engine: &Engine, fps: i64) -> u64 {
    frames_for(engine.total_duration(), fps)
}

/// Closed form of `export_frames`'s loop.
///
/// That loop emits a frame for every `n` where
/// `tick_for_frame(n, fps) = n * (TIMEBASE / fps) < total_duration`, so the
/// count is `ceil(total_duration / step)`. Counting it by *iterating* is an
/// O(frames) spin with no cancellation point: a year-long document spent
/// seconds doing nothing but incrementing, on the `validateOnly` path that
/// renders nothing at all.
///
/// Computed in `u64`: signed `div_ceil` is not stable, and doing the ceiling
/// by hand as `(total + step - 1) / step` would overflow near `i64::MAX`.
pub fn frames_for(total_duration: i64, fps: i64) -> u64 {
    if total_duration <= 0 || fps <= 0 {
        return 0;
    }
    let step = kineto_core::doc::TIMEBASE / fps;
    if step <= 0 {
        return 0;
    }
    (total_duration as u64).div_ceil(step as u64)
}

pub fn describe(engine: &Engine, fps: i64) -> RenderOutcome {
    let ticks = engine.total_duration();
    RenderOutcome {
        out: None,
        width: engine.width(),
        height: engine.height(),
        fps,
        frame_count: frame_count(engine, fps),
        duration_ticks: ticks,
        duration_seconds: ticks as f64 / kineto_core::doc::TIMEBASE as f64,
    }
}

/// Render `count` evenly spaced frames as base64-encoded PNGs.
pub fn sample_frames(
    engine: &mut Engine,
    fps: i64,
    count: usize,
) -> Result<Vec<String>, ToolError> {
    let total = frame_count(engine, fps);
    let mut out = Vec::new();
    for index in preview_frame_indices(total, count) {
        let tick = engine.tick_for_frame(index as i64, fps);
        let mut rgba = engine.render(tick).to_vec();
        kineto_core::render::unpremultiply(&mut rgba);

        let (w, h) = (engine.width(), engine.height());
        let img =
            image::RgbaImage::from_raw(w, h, rgba).expect("engine frame buffer is always w*h*4");

        let img = if w.max(h) > PREVIEW_MAX_EDGE {
            let scale = PREVIEW_MAX_EDGE as f64 / w.max(h) as f64;
            let (nw, nh) = (
                ((w as f64 * scale).round() as u32).max(1),
                ((h as f64 * scale).round() as u32).max(1),
            );
            // Triangle is chosen for determinism, not quality: previews above
            // the cap are explicitly not parity evidence, but the server must
            // still produce identical bytes for identical input.
            image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle)
        } else {
            img
        };

        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|e| ToolError::Invalid(format!("preview PNG encode failed: {e}")))?;

        out.push(BASE64_STANDARD.encode(&png));
    }
    Ok(out)
}

/// Render every frame and mux to `out`.
///
/// Preflights ffmpeg *before* rendering a single frame: without this, a caller
/// with no ffmpeg pays the full render cost and then fails.
pub fn render_to_mp4(engine: &mut Engine, fps: i64, out: &str) -> Result<RenderOutcome, ToolError> {
    if !ffmpeg_available() {
        return Err(ToolError::FfmpegMissing);
    }

    let out_path = Path::new(out);
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::Io {
                context: "creating output directory",
                path: parent.display().to_string(),
                source: e,
            })?;
        }
    }

    let frames_dir = tempfile::tempdir().map_err(|e| ToolError::Io {
        context: "creating temporary frame directory",
        path: "<temp>".into(),
        source: e,
    })?;

    let count = export_frames(engine, fps, frames_dir.path()).map_err(|e| ToolError::Io {
        context: "writing frames",
        path: frames_dir.path().display().to_string(),
        source: e,
    })?;

    // `Ok(false)` here can no longer mean "ffmpeg absent" — we checked above —
    // so it means ffmpeg ran and exited nonzero.
    let muxed = mux_with_ffmpeg(frames_dir.path(), fps, out_path).map_err(|e| ToolError::Io {
        context: "running ffmpeg",
        path: out.to_string(),
        source: e,
    })?;
    if !muxed {
        return Err(ToolError::MuxFailed(out.to_string()));
    }

    let ticks = engine.total_duration();
    Ok(RenderOutcome {
        out: Some(out.to_string()),
        width: engine.width(),
        height: engine.height(),
        fps,
        frame_count: count,
        duration_ticks: ticks,
        duration_seconds: ticks as f64 / kineto_core::doc::TIMEBASE as f64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_indices_span_first_to_last() {
        assert_eq!(preview_frame_indices(100, 5), vec![0, 24, 49, 74, 99]);
    }

    #[test]
    fn preview_indices_handle_single_sample() {
        assert_eq!(preview_frame_indices(100, 1), vec![0]);
    }

    #[test]
    fn preview_indices_are_empty_when_disabled() {
        assert_eq!(preview_frame_indices(100, 0), Vec::<u64>::new());
    }

    #[test]
    fn preview_indices_never_exceed_frame_count() {
        // Asking for more samples than there are frames must not duplicate or
        // run past the end.
        let idx = preview_frame_indices(3, 5);
        assert_eq!(idx, vec![0, 1, 2]);
    }

    #[test]
    fn preview_indices_are_capped() {
        assert_eq!(preview_frame_indices(1000, 99).len(), PREVIEW_MAX_COUNT);
    }

    #[test]
    fn sampled_frames_match_exported_frames_below_the_downscale_cap() {
        // This is what makes the spec's byte-identity claim testable. A 320x180
        // document is under PREVIEW_MAX_EDGE, so no resampling happens and the
        // preview PNG must be byte-identical to the exported one.
        //
        // Three samples, not one: with a single sample only frame 0 is ever
        // compared, and an index-mapping bug that only bites away from the
        // first frame would pass.
        use kineto_core::export::export_frames;

        let mut engine = small_engine();
        let dir = tempfile::tempdir().unwrap();
        let exported_count = export_frames(&mut engine, 30, dir.path()).unwrap();

        let mut engine = small_engine();
        let previews = sample_frames(&mut engine, 30, 3).unwrap();
        assert_eq!(previews.len(), 3);

        let indices = preview_frame_indices(exported_count, 3);
        assert_eq!(
            indices,
            vec![0, 14, 29],
            "first, middle and last of a 30-frame document"
        );

        for (preview, index) in previews.iter().zip(&indices) {
            let exported = std::fs::read(dir.path().join(format!("frame-{index:05}.png"))).unwrap();
            assert_eq!(
                base64_decode(preview),
                exported,
                "preview for frame {index} is not byte-identical to the exported frame"
            );
        }
    }

    #[test]
    fn frame_count_matches_the_export_loop() {
        // The closed form replaces `export_frames`'s predicate
        // (`tick_for_frame(n, fps) < total_duration`). This asserts the two
        // agree, including at exact multiples and one tick either side.
        fn by_loop(total: i64, fps: i64) -> u64 {
            let step = kineto_core::doc::TIMEBASE / fps;
            let mut n = 0u64;
            while (n as i64) * step < total {
                n += 1;
            }
            n
        }

        for fps in [1, 24, 25, 30, 50, 60, 1000] {
            let step = kineto_core::doc::TIMEBASE / fps;
            for total in [
                0,
                1,
                step - 1,
                step,
                step + 1,
                2 * step - 1,
                2 * step,
                2 * step + 1,
                97 * step + 3,
                kineto_core::doc::TIMEBASE,
            ] {
                assert_eq!(
                    frames_for(total, fps),
                    by_loop(total, fps),
                    "closed form disagrees with the loop at total={total}, fps={fps}"
                );
            }
        }
    }

    #[test]
    fn frame_count_is_closed_form_not_a_spin() {
        // A one-second document at 30 fps used to be counted by incrementing
        // through every frame; at a year-long duration that measured 2.9s of
        // pure counting. This asserts the answer for the largest legal
        // duration there is: only a closed form can produce it at all.
        let total = i64::MAX;
        let step = kineto_core::doc::TIMEBASE / 30;
        let expected = (total as u64).div_ceil(step as u64);
        assert_eq!(frames_for(total, 30), expected);
        assert_eq!(frames_for(total, 30), 392_150_171_635);
    }

    #[test]
    fn describe_omits_the_output_path() {
        // Spec §6: `validate_only` returns the metadata block with no `out`.
        let engine = small_engine();
        let outcome = describe(&engine, 30);
        assert_eq!(outcome.out, None);
        assert_eq!(outcome.frame_count, 30);

        let json = serde_json::to_value(&outcome).unwrap();
        assert!(
            json.get("out").is_none(),
            "`out` must be absent, not empty: {json}"
        );
    }

    /// A 320x180 one-second document: small, deterministic, no assets.
    fn small_engine() -> kineto_core::Engine {
        use kineto_core::{Document, Element, Scene};
        let mut doc = Document::new(320, 180);
        doc.push_scene(
            Scene::new("s", kineto_core::doc::TIMEBASE)
                .with_element(Element::rect([0.0, 0.0, 320.0, 180.0], "#3366FF")),
        );
        kineto_core::Engine::new(doc, kineto_core::AssetStore::new()).unwrap()
    }

    fn base64_decode(s: &str) -> Vec<u8> {
        BASE64_STANDARD.decode(s).expect("valid base64")
    }
}
