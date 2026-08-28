//! Mechanical checks on what a document will draw at a given tick.
//!
//! Deliberately a *linter*, not a scene dump. A full description of every
//! element costs more context than the preview image it would accompany
//! (~800-1200 tokens against ~390 for a 720px PNG), and hands the caller an
//! analysis job. Reporting only what looks wrong costs tens of tokens and
//! hands back the conclusion.
//!
//! Nothing here rasterizes. These are the defects that are decidable from
//! geometry, resolved animation and colour arithmetic — which is also why
//! they are decidable *reliably*, where looking at a picture is not: a
//! 1.04:1 contrast ratio is unmissable to arithmetic and easy to miss by eye.

use kineto_core::anim::resolve_common;
use kineto_core::raster::base_bbox;
use kineto_core::{AssetStore, Document, Element};
use serde::Serialize;

/// Opacity at or below which an element contributes nothing visible.
const INVISIBLE_OPACITY: f64 = 0.01;

/// Contrast ratio below which text is reported as effectively invisible.
///
/// Deliberately well under WCAG AA (4.5:1 body, 3:1 large): the job here is
/// catching text that cannot be read *at all*, not grading design. Muted
/// captions are a legitimate choice and must not be flagged.
const MIN_CONTRAST: f64 = 2.0;

/// Whether an element can ever be visible within its scene.
///
/// An opacity track overrides the static value, so the track's keys are the
/// question when one is present.
fn never_visible(el: &Element) -> bool {
    use kineto_core::doc::{KeyValue, Prop};

    let common = el.common();
    match common.animations.iter().find(|t| t.prop == Prop::Opacity) {
        Some(track) => track.keys.iter().all(|k| match &k.v {
            KeyValue::Num(n) => n.0 <= INVISIBLE_OPACITY,
            // An opacity track keyed with vectors is rejected by validation
            // long before here; treat it as visible rather than guess.
            KeyValue::Vec2(_) => false,
        }),
        None => common
            .opacity
            .map(|o| o.0 <= INVISIBLE_OPACITY)
            .unwrap_or(false),
    }
}

/// WCAG relative luminance.
fn luminance(c: &kineto_core::Color) -> f64 {
    let (r, g, b, _) = c.rgba8();
    let f = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b)
}

/// WCAG contrast ratio; 1.0 means identical, 21.0 is black on white.
fn contrast(a: &kineto_core::Color, b: &kineto_core::Color) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub scene: String,
    /// Index of the element within its scene, so the caller can find it.
    pub element: usize,
    pub kind: &'static str,
    pub detail: String,
}

/// Report everything mechanically wrong with what `doc` draws at `tick`.
///
/// An empty result is the common case and the cheap one — "nothing wrong at
/// 2500 ms" is a handful of tokens.
pub fn analyze(doc: &Document, assets: &mut AssetStore, tick: i64) -> Vec<Issue> {
    let mut issues = Vec::new();
    let starts = kineto_core::timeline::scene_starts(doc);
    let (cw, ch) = (doc.size.w as f32, doc.size.h as f32);

    for (si, scene) in doc.scenes.iter().enumerate() {
        let start = starts[si];
        // Only scenes actually on screen at this tick can contribute.
        if tick < start || tick >= start + scene.duration {
            continue;
        }
        let local = tick - start;
        for (ei, el) in scene.elements.iter().enumerate() {
            check_element(doc, assets, el, local, cw, ch, &scene.id, ei, &mut issues);
        }
    }
    issues
}

#[allow(clippy::too_many_arguments)]
fn check_element(
    doc: &Document,
    assets: &mut AssetStore,
    el: &Element,
    local: i64,
    cw: f32,
    ch: f32,
    scene: &str,
    index: usize,
    out: &mut Vec<Issue>,
) {
    let mut push = |kind: &'static str, detail: String| {
        out.push(Issue {
            scene: scene.to_string(),
            element: index,
            kind,
            detail,
        })
    };

    let resolved = resolve_common(el.common(), local);
    if resolved.opacity <= INVISIBLE_OPACITY {
        // Reported only when the element is invisible for its *whole* scene.
        // Keying an element transparent at some moments is a technique, not a
        // defect — a flipbook holds one frame visible and the rest at zero,
        // and flagging the sampled instant made a 48-frame sequence emit 47
        // issues per moment, burying every real finding.
        if never_visible(el) {
            push(
                "fullyTransparent",
                "opacity never rises above zero anywhere in this scene — the \
                 element can never be seen"
                    .into(),
            );
        }
        // Either way nothing else about an invisible element is meaningful.
        return;
    }

    // Text's own geometry needs real layout: `base_bbox` gives it a
    // zero-size placeholder, which would make every bounds test vacuous.
    let base = match el {
        Element::Text {
            text,
            font,
            size_px,
            pos,
            max_w,
            align,
            ..
        } => {
            let family = assets.family(font).to_string();
            let layout = kineto_core::layout_text(
                assets.font_system(),
                &family,
                text,
                size_px.0 as f32,
                max_w.map(|w| w.0 as f32),
                *align,
            );
            kineto_core::BBox {
                x: pos[0].0 as f32,
                y: pos[1].0 as f32,
                w: layout.width,
                h: layout.height,
            }
        }
        _ => base_bbox(el),
    };

    if let Element::Text { color, .. } = el {
        let ratio = contrast(color, &doc.bg);
        if ratio < MIN_CONTRAST {
            push(
                "lowContrast",
                format!(
                    "text '{}' against background '{}' is {ratio:.2}:1 — effectively invisible",
                    color.0, doc.bg.0
                ),
            );
        }
    }

    let degenerate = match el {
        // tiny-skia's `Rect::from_xywh` returns None for either dimension at
        // zero, and the raster arm skips it — nothing is drawn at all.
        Element::Rect { .. } | Element::Image { .. } => base.w <= 0.0 || base.h <= 0.0,
        // A zero-height path is a horizontal line, which strokes perfectly
        // well; only a path collapsed to a single point draws nothing.
        Element::Path { .. } => base.w <= 0.0 && base.h <= 0.0,
        // A group's box is the union of its children, each checked on its
        // own; text is covered by the overflow rule below.
        Element::Text { .. } | Element::Group { .. } => false,
    };
    if degenerate {
        push(
            "zeroSize",
            "geometry collapses to nothing — the element cannot be drawn".into(),
        );
        return;
    }

    // Transformed corners, so rotation and scale are accounted for rather
    // than assumed away.
    let m = kineto_core::element_matrix(&base, &resolved);
    let corners = [
        (base.x, base.y),
        (base.x + base.w, base.y),
        (base.x + base.w, base.y + base.h),
        (base.x, base.y + base.h),
    ]
    .map(|(x, y)| {
        let mut p = [tiny_skia::Point::from_xy(x, y)];
        m.map_points(&mut p);
        (p[0].x, p[0].y)
    });

    let (min_x, max_x) = corners.iter().fold((f32::MAX, f32::MIN), |(lo, hi), c| {
        (lo.min(c.0), hi.max(c.0))
    });
    let (min_y, max_y) = corners.iter().fold((f32::MAX, f32::MIN), |(lo, hi), c| {
        (lo.min(c.1), hi.max(c.1))
    });

    if max_x <= 0.0 || min_x >= cw || max_y <= 0.0 || min_y >= ch {
        push(
            "offCanvas",
            format!(
                "bounds ({min_x:.0},{min_y:.0})-({max_x:.0},{max_y:.0}) are entirely \
                 outside the {cw:.0}x{ch:.0} canvas"
            ),
        );
        return;
    }

    // Partial overflow is reported for text only. A rect or image running off
    // the edge is usually a deliberate full-bleed; text running off the edge
    // is almost always a mistake.
    if matches!(el, Element::Text { .. })
        && (min_x < 0.0 || min_y < 0.0 || max_x > cw || max_y > ch)
    {
        push(
            "textOverflow",
            format!(
                "laid-out text spans ({min_x:.0},{min_y:.0})-({max_x:.0},{max_y:.0}), \
                 past the {cw:.0}x{ch:.0} canvas"
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kineto_core::doc::{Key, Prop, Track, TIMEBASE};
    use kineto_core::{Element, Scene};

    fn doc_with(elements: Vec<Element>) -> Document {
        let mut d = Document::new(200, 100);
        let mut scene = Scene::new("s", TIMEBASE);
        for e in elements {
            scene = scene.with_element(e);
        }
        d.push_scene(scene);
        d
    }

    fn kinds(doc: &Document, tick: i64) -> Vec<&'static str> {
        let mut assets = AssetStore::new();
        analyze(doc, &mut assets, tick)
            .into_iter()
            .map(|i| i.kind)
            .collect()
    }

    #[test]
    fn a_clean_document_reports_nothing() {
        // The control: every rule below must be able to stay silent, or a
        // checker that flagged everything would pass the whole group.
        let doc = doc_with(vec![Element::rect([10.0, 10.0, 50.0, 50.0], "#FF9900")]);
        assert_eq!(kinds(&doc, 0), Vec::<&str>::new());
    }

    #[test]
    fn an_element_animated_off_the_canvas_is_reported() {
        // Exactly the defect that shipped in a title card earlier: a rule
        // translating 900px across a 200px canvas is gone long before the
        // scene ends, and nothing about the document says so.
        let doc = doc_with(vec![Element::rect([10.0, 40.0, 60.0, 20.0], "#FF9900")
            .with_animation(Track::new(
                Prop::Translate,
                vec![Key::vec2(0, [0.0, 0.0]), Key::vec2(TIMEBASE, [900.0, 0.0])],
            ))]);

        assert_eq!(kinds(&doc, 0), Vec::<&str>::new(), "on canvas at t=0");
        assert_eq!(kinds(&doc, TIMEBASE - 1), vec!["offCanvas"]);
    }

    #[test]
    fn an_element_fading_out_is_not_reported_at_the_end_of_its_fade() {
        // Originally asserted the opposite, which was wrong: a fade-out is
        // how a scene clears itself before a crossfade, and flagging its tail
        // reports the technique rather than a defect.
        let doc = doc_with(vec![Element::rect([10.0, 10.0, 50.0, 50.0], "#FF9900")
            .with_animation(Track::new(
                Prop::Opacity,
                vec![Key::num(0, 1.0), Key::num(TIMEBASE, 0.0)],
            ))]);

        assert_eq!(kinds(&doc, 0), Vec::<&str>::new(), "visible at t=0");
        assert_eq!(kinds(&doc, TIMEBASE - 1), Vec::<&str>::new(), "mid-fade");
    }

    #[test]
    fn an_element_keyed_invisible_only_some_of_the_time_is_not_reported() {
        // A flipbook holds one frame visible and the rest transparent; that
        // is the technique, not a defect. Reporting the sampled instant made
        // a 48-frame sequence emit 47 issues per moment — noise that buries
        // the real findings. Only an element that is invisible for its whole
        // scene is worth reporting.
        let doc = doc_with(vec![Element::rect([10.0, 10.0, 50.0, 50.0], "#FF9900")
            .with_animation(Track::new(
                Prop::Opacity,
                vec![
                    Key::num(0, 0.0),
                    Key::num(TIMEBASE / 4, 1.0),
                    Key::num(TIMEBASE / 2, 0.0),
                ],
            ))]);

        // Invisible at t=0 and at the midpoint, but visible in between.
        assert_eq!(kinds(&doc, 0), Vec::<&str>::new());
        assert_eq!(kinds(&doc, TIMEBASE / 2), Vec::<&str>::new());
    }

    #[test]
    fn an_element_invisible_for_its_whole_scene_is_still_reported() {
        // The control: the rule must not have been defanged into silence.
        let doc = doc_with(vec![
            Element::rect([10.0, 10.0, 50.0, 50.0], "#FF9900").with_opacity(0.0)
        ]);
        assert_eq!(kinds(&doc, 0), vec!["fullyTransparent"]);
    }

    #[test]
    fn degenerate_geometry_is_reported() {
        let doc = doc_with(vec![Element::rect([10.0, 10.0, 0.0, 40.0], "#FF9900")]);
        assert_eq!(kinds(&doc, 0), vec!["zeroSize"]);
    }

    /// A document with Inter loaded under asset id "body".
    fn text_doc(color: &str, bg: &str, size: f64, pos: [f64; 2], max_w: Option<f64>) -> Document {
        let mut d = Document::new(200, 100).with_bg(bg);
        d.add_asset("body", kineto_core::Asset::font("kineto:inter"));
        let mut el = Element::text("Hamburgefons", "body", size, color, pos);
        if let Some(w) = max_w {
            el = el.with_max_w(w);
        }
        d.push_scene(Scene::new("s", TIMEBASE).with_element(el));
        d
    }

    fn text_kinds(doc: &Document, tick: i64) -> Vec<&'static str> {
        let mut assets = AssetStore::new();
        assets.add_bytes(
            "body",
            kineto_core::resolve_reserved_src("kineto:inter")
                .unwrap()
                .to_vec(),
        );
        assets.prepare(doc).unwrap();
        analyze(doc, &mut assets, tick)
            .into_iter()
            .map(|i| i.kind)
            .collect()
    }

    #[test]
    fn legible_text_reports_nothing() {
        // Control for both text rules below.
        let doc = text_doc("#F2F5F7", "#0D1419", 16.0, [10.0, 20.0], None);
        assert_eq!(text_kinds(&doc, 0), Vec::<&str>::new());
    }

    #[test]
    fn text_that_cannot_be_seen_against_the_background_is_reported() {
        // The defect that motivated this whole tool: #131b24 on #101820 is a
        // structurally perfect document that renders an invisible smudge. No
        // validator can catch it; arithmetic catches it every time.
        let doc = text_doc("#131b24", "#101820", 16.0, [10.0, 20.0], None);
        assert_eq!(text_kinds(&doc, 0), vec!["lowContrast"]);
    }

    #[test]
    fn text_running_past_the_canvas_edge_is_reported() {
        // Also seen for real: a code block laid out past the bottom of the
        // canvas. Needs true layout — base_bbox gives text a zero-size
        // placeholder, so geometry alone cannot see it.
        let doc = text_doc("#F2F5F7", "#0D1419", 40.0, [10.0, 20.0], None);
        assert_eq!(text_kinds(&doc, 0), vec!["textOverflow"]);
    }

    #[test]
    fn an_issue_names_the_scene_and_element_index() {
        // "something is wrong" is not actionable; the caller has to be able
        // to find it in a document with twenty scenes.
        let doc = doc_with(vec![
            Element::rect([10.0, 10.0, 50.0, 50.0], "#FF9900"),
            Element::rect([10.0, 10.0, 0.0, 40.0], "#FF9900"),
        ]);
        let mut assets = AssetStore::new();
        let issues = analyze(&doc, &mut assets, 0);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].scene, "s");
        assert_eq!(issues[0].element, 1);
    }
}
