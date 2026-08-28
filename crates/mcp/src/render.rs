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
    /// Scene spans and the nominal-vs-actual length gap. Absent only where a
    /// caller had no document to measure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline: Option<crate::timeline::TimelineSummary>,
}

impl RenderOutcome {
    pub fn with_timeline(mut self, timeline: crate::timeline::TimelineSummary) -> Self {
        self.timeline = Some(timeline);
        self
    }
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

/// Flicks per millisecond. `705600000 / 1000` is exact, which is why the
/// preview surface addresses time in whole milliseconds: the conversion to
/// ticks needs no floating point and cannot round. Seconds-as-float would
/// have put rounding on the input path of a renderer whose entire premise is
/// byte-determinism.
pub const TICKS_PER_MS: i64 = kineto_core::doc::TIMEBASE / 1000;

/// Ticks to the nearest whole millisecond.
///
/// Nearest, not truncated, because these numbers are handed back to a caller
/// who may feed them in again. Frame 29 at 30 fps sits at 966.67 ms;
/// truncating reports 966, and 966 ms resolves to frame *28* — the reply
/// would name a moment that is not the one it showed.
pub fn round_ms(ticks: i64) -> i64 {
    if ticks >= 0 {
        (ticks + TICKS_PER_MS / 2) / TICKS_PER_MS
    } else {
        (ticks - TICKS_PER_MS / 2) / TICKS_PER_MS
    }
}

/// Resolve a millisecond offset to the index of the frame containing it.
///
/// Returns a frame *index*, not a raw tick, for the reason
/// `preview_frame_indices` does: only the exact ticks `export_frames` lands on
/// are byte-comparable to exported PNGs. Rendering an arbitrary tick between
/// two frames would produce pixels no export ever writes, quietly voiding the
/// one property that makes a preview evidence of anything.
///
/// Offsets past the end clamp to the last frame — the caller sees what they
/// actually got in the reported metadata.
pub fn frame_for_ms(ms: i64, fps: i64, frame_count: u64) -> Result<u64, ToolError> {
    if ms < 0 {
        return Err(ToolError::Invalid(format!(
            "`atMs` offsets must not be negative, got {ms}"
        )));
    }
    if frame_count == 0 {
        return Err(ToolError::Invalid(
            "this document has no frames to preview: its total duration is zero".into(),
        ));
    }
    let step = if fps > 0 {
        kineto_core::doc::TIMEBASE / fps
    } else {
        0
    };
    if step <= 0 {
        return Err(ToolError::Fps(fps));
    }
    // Checked: `ms * 705_600` overflows i64 above ~1.3e13 ms. Wrapping would
    // produce a negative tick and silently resolve to frame 0 — a confident
    // wrong answer, which is worse than a refusal.
    let tick = ms.checked_mul(TICKS_PER_MS).ok_or_else(|| {
        ToolError::Invalid(format!(
            "`atMs` offset {ms} is too large to be a time within any document"
        ))
    })?;
    Ok(frame_for_tick(tick, step, frame_count))
}

/// The index of the frame containing `tick`, clamped to the last frame.
///
/// Split out so scene addressing can resolve a midpoint tick directly rather
/// than converting it to milliseconds first — that round trip would round
/// twice and could land a frame off.
fn frame_for_tick(tick: i64, step: i64, frame_count: u64) -> u64 {
    ((tick.max(0) / step) as u64).min(frame_count - 1)
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
        timeline: None,
    }
}

/// One requested moment and the frame it resolved to.
///
/// `requested_ms` is echoed back deliberately: clamping and frame snapping
/// both move the answer, and a caller comparing what it asked for against
/// `actual_ms` is how it learns that 5000 ms in a one-second document is the
/// last frame rather than a failure.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSample {
    /// Present when the moment came from `atMs`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_ms: Option<i64>,
    /// Present when the moment came from `atScenes`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_scene: Option<String>,
    pub frame_index: u64,
    pub tick: i64,
    pub actual_ms: i64,
    /// The scene dominating this frame — not necessarily the one requested,
    /// which is the point: it is how a caller discovers it is looking at a
    /// crossfade rather than the scene it had in mind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene_local_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewOutcome {
    pub width: u32,
    pub height: u32,
    pub fps: i64,
    pub frame_count: u64,
    pub duration_ticks: i64,
    pub duration_seconds: f64,
    pub timeline: crate::timeline::TimelineSummary,
    pub samples: Vec<PreviewSample>,
}

/// One requested moment resolved onto the frame grid: the frame index, its
/// tick, and which of `atMs`/`atScenes` asked for it.
pub struct Moment {
    pub frame_index: u64,
    pub tick: i64,
    pub requested_ms: Option<i64>,
    pub requested_scene: Option<String>,
}

/// Resolve `atMs` + `atScenes` onto the frame grid.
///
/// Shared by `preview_document` and `check_document` so the two tools cannot
/// disagree about which frame a given moment names.
pub fn resolve_moments(
    total_ticks: i64,
    fps: i64,
    timeline: &crate::timeline::TimelineSummary,
    at_ms: &[i64],
    at_scenes: &[String],
) -> Result<Vec<Moment>, ToolError> {
    if at_ms.is_empty() && at_scenes.is_empty() {
        return Err(ToolError::Invalid(
            "provide at least one of `atMs` (moments in milliseconds) or \
             `atScenes` (scene ids, each previewed at its midpoint)"
                .into(),
        ));
    }
    let asked = at_ms.len() + at_scenes.len();
    if asked > PREVIEW_MAX_COUNT {
        return Err(ToolError::Invalid(format!(
            "{asked} moments were named but at most {PREVIEW_MAX_COUNT} may be \
             handled per call; ask for fewer rather than being given a \
             silently truncated answer"
        )));
    }

    let total = frames_for(total_ticks, fps);
    if total == 0 {
        return Err(ToolError::Invalid(
            "this document has no frames: its total duration is zero".into(),
        ));
    }
    let step = kineto_core::doc::TIMEBASE / fps;
    let mut out = Vec::with_capacity(asked);

    for &ms in at_ms {
        let frame_index = frame_for_ms(ms, fps, total)?;
        out.push(Moment {
            frame_index,
            tick: frame_index as i64 * step,
            requested_ms: Some(ms),
            requested_scene: None,
        });
    }
    for id in at_scenes {
        let span = timeline.find(id).ok_or_else(|| {
            ToolError::Invalid(format!(
                "no scene with id '{id}' — this document's scenes are: {}",
                timeline.ids().join(", ")
            ))
        })?;
        // Resolved from the midpoint *tick*, not via milliseconds: routing a
        // tick through ms and back would round twice and can land a frame off.
        let frame_index = frame_for_tick(span.midpoint_tick(), step, total);
        out.push(Moment {
            frame_index,
            tick: frame_index as i64 * step,
            requested_ms: None,
            requested_scene: Some(id.clone()),
        });
    }
    Ok(out)
}

/// Resolve requested millisecond offsets into the metadata to report and the
/// distinct frame indices to encode.
///
/// The two differ on purpose: several moments can land on one frame, and that
/// frame is rasterized and encoded once, while every moment the caller asked
/// about still appears in `samples`.
pub fn resolve_preview(
    engine: &Engine,
    fps: i64,
    timeline: &crate::timeline::TimelineSummary,
    at_ms: &[i64],
    at_scenes: &[String],
) -> Result<(PreviewOutcome, Vec<u64>), ToolError> {
    let moments = resolve_moments(engine.total_duration(), fps, timeline, at_ms, at_scenes)?;

    let total = frame_count(engine, fps);
    let mut samples: Vec<PreviewSample> = Vec::with_capacity(moments.len());
    let mut frames: Vec<u64> = Vec::new();

    for m in moments {
        let tick = engine.tick_for_frame(m.frame_index as i64, fps);
        if !frames.contains(&m.frame_index) {
            frames.push(m.frame_index);
        }
        let dominant = timeline.scene_at(tick);
        samples.push(PreviewSample {
            requested_ms: m.requested_ms,
            requested_scene: m.requested_scene,
            frame_index: m.frame_index,
            tick,
            actual_ms: round_ms(tick),
            scene_id: dominant.map(|s| s.id.clone()),
            scene_local_ms: dominant.map(|s| round_ms(tick - s.start_tick)),
        });
    }

    let ticks = engine.total_duration();
    Ok((
        PreviewOutcome {
            width: engine.width(),
            height: engine.height(),
            fps,
            frame_count: total,
            duration_ticks: ticks,
            duration_seconds: ticks as f64 / kineto_core::doc::TIMEBASE as f64,
            timeline: timeline.clone(),
            samples,
        },
        frames,
    ))
}

/// Render `count` evenly spaced frames as base64-encoded PNGs.
pub fn sample_frames(
    engine: &mut Engine,
    fps: i64,
    count: usize,
) -> Result<Vec<String>, ToolError> {
    let total = frame_count(engine, fps);
    encode_frames(engine, fps, &preview_frame_indices(total, count))
}

/// Render the named frame indices as base64-encoded PNGs, in the order given.
///
/// Takes indices rather than ticks so both callers — even sampling and an
/// explicit `atMs` request — go through the same rasterize/downscale/encode
/// path, and so every image returned is a frame `export_frames` would write.
pub fn encode_frames(
    engine: &mut Engine,
    fps: i64,
    frames: &[u64],
) -> Result<Vec<String>, ToolError> {
    let mut out = Vec::new();
    for &index in frames {
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
        timeline: None,
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
        //
        // The fixture animates for the same reason. Against a static document
        // every frame is identical, so comparing preview 14 to exported frame
        // 14 succeeds even if the code fetched frame 3 — the assertion could
        // not fail on the bug it exists to catch.
        use kineto_core::export::export_frames;

        let mut engine = animated_engine();
        let dir = tempfile::tempdir().unwrap();
        let exported_count = export_frames(&mut engine, 30, dir.path()).unwrap();

        let mut engine = animated_engine();
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

    #[test]
    fn a_millisecond_is_a_whole_number_of_ticks() {
        // The reason the tool surface takes milliseconds rather than seconds:
        // 705600000 / 1000 is exact, so the conversion needs no float and
        // cannot round. If this ever stops holding, `frame_for_ms` must change
        // rather than silently lose precision.
        assert_eq!(TICKS_PER_MS, 705_600);
        assert_eq!(TICKS_PER_MS * 1000, kineto_core::doc::TIMEBASE);
    }

    #[test]
    fn a_millisecond_offset_resolves_to_the_frame_containing_it() {
        // 30 fps: one frame every 23_520_000 ticks (33.333ms). 50ms lands
        // inside frame 1, not on a boundary — a truncating-to-zero or
        // rounding-up bug would both show here.
        assert_eq!(frame_for_ms(0, 30, 30).unwrap(), 0);
        assert_eq!(frame_for_ms(50, 30, 30).unwrap(), 1);
        assert_eq!(frame_for_ms(100, 30, 30).unwrap(), 3);
    }

    #[test]
    fn a_millisecond_offset_past_the_end_clamps_to_the_last_frame() {
        // A one-second document at 30 fps has frames 0..=29. Asking for 5s is
        // answered with the last frame; the caller learns what they actually
        // got from the reported metadata rather than from an error.
        assert_eq!(frame_for_ms(5_000, 30, 30).unwrap(), 29);
    }

    #[test]
    fn a_negative_millisecond_offset_is_an_error() {
        assert!(frame_for_ms(-1, 30, 30).is_err());
    }

    #[test]
    fn an_absurd_millisecond_offset_is_an_error_not_an_overflow() {
        // ms * 705_600 overflows i64 above ~1.3e13 ms. Unchecked, this wraps
        // to a negative tick and resolves to frame 0 — a wrong answer rather
        // than a refusal.
        assert!(frame_for_ms(i64::MAX, 30, 30).is_err());
    }

    #[test]
    fn resolving_a_frame_in_an_empty_document_is_an_error() {
        // Nothing to look at: a zero-duration document has no frames, and
        // clamping to `frame_count - 1` would underflow.
        assert!(frame_for_ms(0, 30, 0).is_err());
    }

    #[test]
    fn resolving_moments_reports_the_frame_the_caller_actually_gets() {
        // The caller asks in milliseconds; what closes the loop is being told
        // which frame that became. 5000ms is past the end of a one-second
        // document and clamps, which the caller can only detect from the
        // reported values.
        let engine = animated_engine();
        let (outcome, frames) =
            resolve_preview(&engine, 30, &animated_tl(), &[0, 50, 5_000], &[]).unwrap();

        assert_eq!(frames, vec![0, 1, 29]);
        assert_eq!(outcome.frame_count, 30);
        let got: Vec<(i64, u64, i64)> = outcome
            .samples
            .iter()
            .map(|s| (s.requested_ms.unwrap(), s.frame_index, s.actual_ms))
            .collect();
        assert_eq!(got, vec![(0, 0, 0), (50, 1, 33), (5_000, 29, 967)]);
    }

    #[test]
    fn moments_landing_on_one_frame_are_encoded_once_but_reported_each() {
        // 0ms and 10ms are both inside frame 0 at 30 fps. Encoding that frame
        // twice would spend a second base64 PNG of context to say nothing, but
        // dropping the caller's second question would be worse — it stays in
        // `samples`.
        let engine = animated_engine();
        let (outcome, frames) =
            resolve_preview(&engine, 30, &animated_tl(), &[0, 10, 50], &[]).unwrap();

        assert_eq!(frames, vec![0, 1], "frame 0 must be encoded once");
        assert_eq!(outcome.samples.len(), 3, "every requested moment reported");
        assert_eq!(outcome.samples[1].requested_ms, Some(10));
        assert_eq!(outcome.samples[1].frame_index, 0);
    }

    #[test]
    fn a_reported_moment_resolves_back_to_the_same_frame() {
        // `actualMs` is only useful if it round-trips: a caller that reads
        // "you got 966 ms" and asks for 966 ms must land on the frame it was
        // just looking at. Truncating the tick instead of rounding it reports
        // a moment that belongs to the *previous* frame — 966 ms is inside
        // frame 28, not frame 29.
        let engine = animated_engine();
        let (first, _) = resolve_preview(&engine, 30, &animated_tl(), &[999], &[]).unwrap();
        let reported = first.samples[0].actual_ms;

        let (again, _) = resolve_preview(&engine, 30, &animated_tl(), &[reported], &[]).unwrap();
        assert_eq!(
            again.samples[0].frame_index, first.samples[0].frame_index,
            "reported {reported} ms resolved to a different frame than it named"
        );
    }

    #[test]
    fn asking_for_no_moments_is_an_error() {
        let engine = animated_engine();
        assert!(resolve_preview(&engine, 30, &animated_tl(), &[], &[]).is_err());
    }

    #[test]
    fn more_moments_than_the_cap_is_a_rejection_not_a_silent_truncation() {
        // Quietly dropping frames someone explicitly named sends them off to
        // reason about images they never received.
        let engine = animated_engine();
        let many: Vec<i64> = (0..=PREVIEW_MAX_COUNT as i64).collect();
        let msg = resolve_preview(&engine, 30, &animated_tl(), &many, &[])
            .unwrap_err()
            .to_string();
        assert!(msg.contains("12"), "the error must name the cap: {msg}");
    }

    #[test]
    fn explicitly_requested_frames_are_byte_identical_to_exported_frames() {
        // The control for the whole preview premise: a caller names a moment
        // and receives exactly the frame the exporter would write for it.
        //
        // Frames 7 and 23 are deliberately not indices `preview_frame_indices`
        // would ever choose, so this fails if explicit selection silently
        // falls back to even spacing.
        use kineto_core::export::export_frames;

        let mut engine = animated_engine();
        let dir = tempfile::tempdir().unwrap();
        export_frames(&mut engine, 30, dir.path()).unwrap();

        let mut engine = animated_engine();
        let encoded = encode_frames(&mut engine, 30, &[7, 23]).unwrap();
        assert_eq!(encoded.len(), 2);

        for (png, index) in encoded.iter().zip([7u64, 23]) {
            let exported = std::fs::read(dir.path().join(format!("frame-{index:05}.png"))).unwrap();
            assert_eq!(
                base64_decode(png),
                exported,
                "encoded frame {index} is not the frame the exporter wrote"
            );
        }
    }

    #[test]
    fn the_animated_fixture_actually_differs_between_frames() {
        // Guards every byte-identity test above: against a static document
        // all frames are identical, so a wrong-index bug would compare equal
        // and pass. This asserts the fixture can actually catch one.
        let mut engine = animated_engine();
        let frames = encode_frames(&mut engine, 30, &[0, 7, 23, 29]).unwrap();
        for (i, a) in frames.iter().enumerate() {
            for b in &frames[i + 1..] {
                assert_ne!(a, b, "fixture frames must differ or the goldens are blind");
            }
        }
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

    /// `small_engine` with a square sliding across it, so no two frames share
    /// pixels. Byte-identity assertions against a static document cannot fail
    /// on a wrong frame index; against this one they can.
    fn animated_engine() -> kineto_core::Engine {
        kineto_core::Engine::new(animated_doc(), kineto_core::AssetStore::new()).unwrap()
    }

    /// The timeline of `animated_doc`, for calls that need scene spans.
    fn animated_tl() -> crate::timeline::TimelineSummary {
        crate::timeline::summary(&animated_doc())
    }

    fn animated_doc() -> kineto_core::Document {
        use kineto_core::doc::{Key, Prop, Track};
        use kineto_core::{Document, Element, Scene};
        let mut doc = Document::new(320, 180);
        doc.push_scene(
            Scene::new("s", kineto_core::doc::TIMEBASE)
                .with_element(Element::rect([0.0, 0.0, 320.0, 180.0], "#3366FF"))
                .with_element(
                    Element::rect([0.0, 60.0, 40.0, 60.0], "#FF9900").with_animation(Track::new(
                        Prop::Translate,
                        vec![
                            Key::vec2(0, [0.0, 0.0]),
                            Key::vec2(kineto_core::doc::TIMEBASE, [280.0, 0.0]),
                        ],
                    )),
                ),
        );
        doc
    }

    fn base64_decode(s: &str) -> Vec<u8> {
        BASE64_STANDARD.decode(s).expect("valid base64")
    }
}
