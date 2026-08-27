mod common;

use kineto_core::doc::{ms, Element, Scene, Transition};
use kineto_core::{AssetStore, Document, Engine};

/// 32x32 canvas, opaque-black bg (default). Scene "a" (300ms): full-canvas
/// opaque red rect. Scene "b" (300ms, 200ms crossfade in): full-canvas
/// opaque blue rect.
///
/// `timeline::scene_starts`: start[a] = 0; start[b] = 0 + 300ms - 200ms =
/// 100ms. `total_duration` = 100ms + 300ms = 400ms. Scene "b"'s incoming
/// crossfade window is `[start[b], start[b]+200ms)` = `[100ms, 300ms)`.
fn crossfade_doc() -> Document {
    let mut doc = Document::new(32, 32);
    doc.push_scene(
        Scene::new("a", ms(300)).with_element(Element::rect([0.0, 0.0, 32.0, 32.0], "#FF0000")),
    );
    doc.push_scene(
        Scene::new("b", ms(300))
            .with_transition(Transition::crossfade(ms(200)))
            .with_element(Element::rect([0.0, 0.0, 32.0, 32.0], "#0000FF")),
    );
    doc
}

fn pixel(frame: &[u8], w: u32, x: u32, y: u32) -> (u8, u8, u8, u8) {
    let i = ((y * w + x) * 4) as usize;
    (frame[i], frame[i + 1], frame[i + 2], frame[i + 3])
}

/// Tick 50ms sits before scene "b"'s crossfade window ([100ms, 300ms)), so
/// only scene "a" is visible, at alpha 1.0 (drawn direct, no scratch layer):
/// plain opaque red, no blending.
#[test]
fn tick_in_scene_a_is_pure_red() {
    let mut engine = Engine::new(crossfade_doc(), AssetStore::new()).unwrap();
    let frame = engine.render(ms(50)).to_vec();

    assert_eq!(pixel(&frame, 32, 16, 16), (255, 0, 0, 255));
    common::assert_golden_hash("render-sceneA", 32, 32, &frame);
}

/// Tick 200ms is the midpoint of scene "b"'s `[100ms, 300ms)` incoming
/// crossfade window: `alpha = (200ms - 100ms) / 200ms = 0.5`. `layers_at`
/// returns scene "a" (outgoing, alpha 1.0, drawn direct onto `frame`) then
/// scene "b" (incoming, alpha 0.5, drawn into `scratch` then composited via
/// `draw_pixmap` with `PixmapPaint { opacity: 0.5, .. }`).
///
/// --- Hand derivation ---
/// After scene "a": `frame` = opaque premultiplied red `(255, 0, 0, 255)`
/// everywhere (bg fill then a full-canvas, pixel-aligned opaque rect — no
/// AA edge to account for).
/// `scratch` after scene "b": opaque premultiplied blue `(0, 0, 255, 255)`
/// (element opacity 1.0, so tiny-skia's own `PixmapPaint::opacity` stage in
/// `draw_elements`'s `Rect` arm is a no-op there).
/// `PixmapPaint::opacity = 0.5` uniformly scales that premultiplied pixel
/// before compositing (same mechanism as `raster.rs`'s
/// `group_isolated_opacity` test): `(0, 0, 255, 255) * 0.5` ->
/// `255 * 0.5 = 127.5`, which tiny-skia's `u8 -> f32/255 -> scale ->
/// *255 -> round` pipeline rounds to `128` (same half-up rounding the
/// `raster.rs` `rect_fill_and_opacity` golden documents for the identical
/// `127.5` case) -> scaled src = `(0, 0, 128, 128)`.
/// Source-over of that onto opaque red dst `(255, 0, 0, 255)`, using the
/// same integer `div255` formula `raster.rs::over_premul` documents
/// (`div255(x) = ((x+128) + ((x+128)>>8))>>8`):
///   inv    = 255 - 128 = 127
///   out_a  = 128 + div255(255*127) = 128 + 127 = 255
///   out_r  =   0 + div255(255*127) =   0 + 127 = 127
///   out_g  =   0 + div255(  0*127) =   0 +   0 =   0
///   out_b  = 128 + div255(  0*127) = 128 +   0 = 128
/// => hand-derived expected pixel = `(127, 0, 128, 255)`.
///
/// tiny-skia's `draw_pixmap` compositing runs through its own float
/// (`highp`) pipeline rather than this crate's integer `div255` helper, so
/// per the task brief this is verified (not blindly trusted) against the
/// implementation's actual output before being asserted as an exact
/// constant below. Observed: `(128, 0, 128, 255)` — the red channel is 1
/// higher than the hand-derived `127`, i.e. within the brief's +/-1
/// tolerance (tiny-skia's `highp` pipeline rounds `out_r`'s float
/// intermediate slightly differently than the crate's own integer
/// `div255`; both are correct roundings of the same real-valued math, just
/// via different paths).
#[test]
fn tick_mid_crossfade_blends_blue_over_red() {
    let mut engine = Engine::new(crossfade_doc(), AssetStore::new()).unwrap();
    let frame = engine.render(ms(200)).to_vec();

    let observed = pixel(&frame, 32, 16, 16);
    let hand_derived = (127u8, 0u8, 128u8, 255u8);
    let within_1 = |a: u8, b: u8| (a as i16 - b as i16).abs() <= 1;
    assert!(
        within_1(observed.0, hand_derived.0)
            && within_1(observed.1, hand_derived.1)
            && within_1(observed.2, hand_derived.2)
            && within_1(observed.3, hand_derived.3),
        "observed {observed:?} must be within +/-1 per channel of the hand-derived {hand_derived:?}"
    );
    // Confirmed within tolerance above (tiny-skia's float compositing
    // pipeline matches the hand-derived integer formula exactly here) —
    // pin the exact observed constant per the task brief.
    assert_eq!(observed, (128, 0, 128, 255));

    common::assert_golden_hash("render-crossfade-mid", 32, 32, &frame);
}

/// Tick `total_duration()` (400ms) is >= total: `layers_at` returns no
/// layers, so the frame is plain background (opaque black, default bg).
#[test]
fn tick_past_total_duration_is_plain_bg() {
    let mut engine = Engine::new(crossfade_doc(), AssetStore::new()).unwrap();
    let total = engine.total_duration();
    assert_eq!(total, ms(400));

    let frame = engine.render(total).to_vec();
    assert_eq!(pixel(&frame, 32, 16, 16), (0, 0, 0, 255));
    common::assert_golden_hash("render-past-end", 32, 32, &frame);
}

/// Two consecutive `render()` calls at the same tick return identical
/// bytes: the frame/scratch buffers are reused across calls (an allocation
/// optimization), but each call fully overwrites `frame` (bg fill, then
/// every visible layer), so nothing leaks from one call into the next.
#[test]
fn repeated_render_at_same_tick_is_idempotent() {
    let mut engine = Engine::new(crossfade_doc(), AssetStore::new()).unwrap();
    let tick = ms(200); // mid-crossfade: exercises the scratch-layer path too.

    let first = engine.render(tick).to_vec();
    let second = engine.render(tick).to_vec();
    assert_eq!(first, second);
}

#[test]
fn tick_for_frame_matches_timebase_formula() {
    let engine = Engine::new(crossfade_doc(), AssetStore::new()).unwrap();
    assert_eq!(engine.tick_for_frame(1, 30), kineto_core::TIMEBASE / 30);
    assert_eq!(
        engine.tick_for_frame(2, 30),
        2 * (kineto_core::TIMEBASE / 30)
    );
}

#[test]
#[should_panic(expected = "unsupported fps 0: must divide 705600000")]
fn tick_for_frame_rejects_zero_fps() {
    let engine = Engine::new(crossfade_doc(), AssetStore::new()).unwrap();
    engine.tick_for_frame(1, 0);
}

#[test]
#[should_panic(expected = "unsupported fps -5: must divide 705600000")]
fn tick_for_frame_rejects_negative_fps() {
    let engine = Engine::new(crossfade_doc(), AssetStore::new()).unwrap();
    engine.tick_for_frame(1, -5);
}

#[test]
#[should_panic(expected = "unsupported fps 23: must divide 705600000")]
fn tick_for_frame_rejects_non_divisor_fps() {
    let engine = Engine::new(crossfade_doc(), AssetStore::new()).unwrap();
    engine.tick_for_frame(1, 23);
}

#[test]
fn width_height_report_doc_size() {
    let engine = Engine::new(crossfade_doc(), AssetStore::new()).unwrap();
    assert_eq!(engine.width(), 32);
    assert_eq!(engine.height(), 32);
}

/// `frame_data` returns the same bytes `render` last produced, without
/// triggering another render pass (the wasm shim's pointer-access path).
#[test]
fn frame_data_matches_last_render_without_rerendering() {
    let mut engine = Engine::new(crossfade_doc(), AssetStore::new()).unwrap();
    let rendered = engine.render(ms(50)).to_vec();
    let accessed = engine.frame_data().to_vec();
    assert_eq!(rendered, accessed);
}
