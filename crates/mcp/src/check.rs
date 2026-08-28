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
    /// Absent for rules that are a property of the whole document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    /// Index of the element within its scene, so the caller can find it.
    /// Absent for rules that are a property of the whole scene.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element: Option<usize>,
    pub kind: &'static str,
    /// `"correctness"` — the document does not draw what it says it does, and
    /// no amount of taste makes that acceptable. `"design"` — it draws
    /// correctly but breaks a rule of thumb that is checkable as arithmetic.
    /// Separated so a caller can block on one and merely report the other.
    pub category: &'static str,
    pub detail: String,
}

/// Words per minute assumed for on-screen text.
///
/// Faster than prose reading (~250) because captions and labels are scanned,
/// not read. Deliberately generous: the rule exists to catch text that flashes
/// past, not to grade pacing.
const SCAN_WPM: f64 = 300.0;

/// Extra time a viewer needs to notice a scene changed at all.
const SCENE_BEAT_MS: f64 = 500.0;

/// Smallest text height, as a fraction of canvas height, that survives being
/// watched on a phone. 1.6% is ~17px at 1080p and ~11.5px at 720p.
///
/// Relative rather than absolute because the video is scaled to the viewport:
/// 10px at 720p and 15px at 1080p are the same problem.
const MIN_TEXT_FRACTION: f64 = 0.016;

/// Fraction of text-only scenes above which a document is a slide deck.
const DECK_TEXT_ONLY_FRACTION: f64 = 0.7;
/// Below this many scenes there is no pattern to judge.
const DECK_MIN_SCENES: usize = 4;

/// Most words that can be taken in from a single screen at once.
///
/// 40 rather than something tighter because a *scanned list* is not a
/// paragraph: the long-standing "6x6" slide rule is already ~36 words, and a
/// rule that flags legitimate layouts is worse than no rule. This is meant to
/// catch a wall of text, which `tooFast` will usually flag as well.
const MAX_WORDS_ON_SCREEN: usize = 40;

/// Rules about the document as a whole, independent of any tick.
///
/// Reported once by the caller rather than per moment, which is why they are
/// not part of `analyze`.
pub fn analyze_document(doc: &Document) -> Vec<Issue> {
    let mut out = Vec::new();
    let n = doc.scenes.len();
    if n < DECK_MIN_SCENES {
        return out;
    }
    let text_only = doc
        .scenes
        .iter()
        .filter(|s| {
            !s.elements.is_empty() && s.elements.iter().all(|e| matches!(e, Element::Text { .. }))
        })
        .count();
    if text_only as f64 / n as f64 > DECK_TEXT_ONLY_FRACTION {
        out.push(Issue {
            scene: None,
            element: None,
            kind: "deckShaped",
            category: "design",
            detail: format!(
                "{text_only} of {n} scenes contain nothing but text — this is a \
                 slide deck, not a video. Show the structure: a path between \
                 two things, a bar for a quantity, an image of the thing itself"
            ),
        });
    }
    out
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
        check_scene(scene, ch, &mut issues);
    }
    issues
}

/// Rules about a scene as a whole rather than any one element.
fn check_scene(scene: &kineto_core::Scene, _ch: f32, out: &mut Vec<Issue>) {
    let words: usize = scene
        .elements
        .iter()
        .filter_map(|e| match e {
            Element::Text { text, .. } => Some(text.split_whitespace().count()),
            _ => None,
        })
        .sum();
    if words == 0 {
        return;
    }

    let duration_ms = scene.duration as f64 / (kineto_core::doc::TIMEBASE as f64 / 1000.0);
    let needed = words as f64 / SCAN_WPM * 60_000.0 + SCENE_BEAT_MS;
    if duration_ms < needed {
        out.push(Issue {
            scene: Some(scene.id.clone()),
            element: None,
            kind: "tooFast",
            category: "design",
            detail: format!(
                "{words} words in {duration_ms:.0} ms — about {:.0} wpm; \
                 needs roughly {needed:.0} ms to be readable",
                words as f64 / (duration_ms / 60_000.0)
            ),
        });
    }
    if words > MAX_WORDS_ON_SCREEN {
        out.push(Issue {
            scene: Some(scene.id.clone()),
            element: None,
            kind: "tooDense",
            category: "design",
            detail: format!(
                "{words} words on screen at once; more than {MAX_WORDS_ON_SCREEN} \
                 is more than a viewer can take in"
            ),
        });
    }
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
    let mut push = |kind: &'static str, category: &'static str, detail: String| {
        out.push(Issue {
            scene: Some(scene.to_string()),
            element: Some(index),
            kind,
            category,
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
                "correctness",
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

    if let Element::Text { size_px, .. } = el {
        let fraction = size_px.0 / ch as f64;
        if fraction < MIN_TEXT_FRACTION {
            push(
                "tooSmall",
                "design",
                format!(
                    "{}px is {:.2}% of canvas height; under {:.1}% is unreadable \
                     once the video is scaled to a phone",
                    size_px.0,
                    fraction * 100.0,
                    MIN_TEXT_FRACTION * 100.0
                ),
            );
        }
    }

    if let Element::Text { color, .. } = el {
        let ratio = contrast(color, &doc.bg);
        if ratio < MIN_CONTRAST {
            push(
                "lowContrast",
                "correctness",
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
            "correctness",
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
            "correctness",
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
            "correctness",
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

    // ---- document-level rules ----

    fn deck(n: usize, with_structure: usize) -> Document {
        let mut d = Document::new(1280, 720).with_bg("#0D1419");
        d.add_asset("body", kineto_core::Asset::font("kineto:inter"));
        for i in 0..n {
            let mut sc = Scene::new(&format!("s{i}"), TIMEBASE).with_element(Element::text(
                "a line of narration here",
                "body",
                40.0,
                "#F2F5F7",
                [80.0, 300.0],
            ));
            if i < with_structure {
                sc = sc.with_element(
                    Element::path(vec![[80.0, 400.0], [400.0, 400.0]]).with_stroke("#FF9900", 4.0),
                );
            }
            d.push_scene(sc);
        }
        d
    }

    fn doc_kinds(doc: &Document) -> Vec<&'static str> {
        analyze_document(doc).into_iter().map(|i| i.kind).collect()
    }

    #[test]
    fn a_document_of_nothing_but_text_is_reported_as_a_deck() {
        // The failure mode an agent falls into by default: handed a schema,
        // it emits the lowest-energy thing that validates, which is centred
        // prose on slides. Correct, lint-clean, and lifeless.
        assert_eq!(doc_kinds(&deck(8, 0)), vec!["deckShaped"]);
    }

    #[test]
    fn a_document_with_visual_structure_is_not_reported() {
        // Control: the rule must be able to stay silent, or it is just noise.
        assert_eq!(doc_kinds(&deck(8, 8)), Vec::<&str>::new());
    }

    #[test]
    fn a_title_card_does_not_make_a_document_a_deck() {
        // Judged over the document, not per scene: a text-only title or close
        // is normal, and flagging it would train callers to ignore the rule.
        assert_eq!(doc_kinds(&deck(8, 6)), Vec::<&str>::new());
    }

    #[test]
    fn a_very_short_document_is_never_called_a_deck() {
        // Two text scenes is a card, not a deck. Too small a sample to judge.
        assert_eq!(doc_kinds(&deck(2, 0)), Vec::<&str>::new());
    }

    #[test]
    fn a_document_level_issue_names_neither_scene_nor_element() {
        let issues = analyze_document(&deck(8, 0));
        assert_eq!(issues[0].scene, None);
        assert_eq!(issues[0].element, None);
        assert_eq!(issues[0].category, "design");
    }

    // ---- design rules ----

    fn text_scene(words: &str, size: f64, duration_ms: i64) -> Document {
        let mut d = Document::new(1280, 720).with_bg("#0D1419");
        d.add_asset("body", kineto_core::Asset::font("kineto:inter"));
        d.push_scene(
            Scene::new("s", duration_ms * (TIMEBASE / 1000)).with_element(
                Element::text(words, "body", size, "#F2F5F7", [80.0, 300.0]).with_max_w(1000.0),
            ),
        );
        d
    }

    #[test]
    fn comfortable_text_reports_nothing() {
        // Control for all three design rules: 8 words, 28px, 5 seconds.
        let doc = text_scene("eight short words is a comfortable amount here", 28.0, 5000);
        assert_eq!(text_kinds(&doc, 0), Vec::<&str>::new());
    }

    #[test]
    fn text_too_small_to_read_is_reported() {
        // Judged as a fraction of canvas height, not absolute pixels: the
        // video is watched scaled, mostly on a phone, so 10px at 720p and
        // 15px at 1080p are the same problem.
        let doc = text_scene("a few words", 9.0, 5000);
        assert_eq!(text_kinds(&doc, 0), vec!["tooSmall"]);
    }

    #[test]
    fn a_scene_too_short_to_read_is_reported() {
        // The commonest amateur mistake in explainer video, and pure
        // arithmetic: 40 words cannot be read in 900ms.
        let words = "one two three four five six seven eight nine ten \
                     eleven twelve thirteen fourteen fifteen sixteen seventeen \
                     eighteen nineteen twenty twentyone twentytwo twentythree \
                     twentyfour twentyfive twentysix twentyseven twentyeight \
                     twentynine thirty thirtyone thirtytwo thirtythree \
                     thirtyfour thirtyfive thirtysix thirtyseven thirtyeight \
                     thirtynine forty";
        let doc = text_scene(words, 28.0, 900);
        let kinds = text_kinds(&doc, 0);
        assert!(kinds.contains(&"tooFast"), "got {kinds:?}");
    }

    #[test]
    fn too_much_text_at_once_is_reported() {
        // Enough time to read it, but too much on screen to take in.
        let words = "one two three four five six seven eight nine ten eleven \
                     twelve thirteen fourteen fifteen sixteen seventeen eighteen \
                     nineteen twenty a b c d e f g h i j k l m n o p q r s t u v \
                     w x y z";
        let doc = text_scene(words, 28.0, 30_000);
        let kinds = text_kinds(&doc, 0);
        assert!(kinds.contains(&"tooDense"), "got {kinds:?}");
        assert!(!kinds.contains(&"tooFast"), "30s is ample time: {kinds:?}");
    }

    #[test]
    fn issues_carry_a_category_so_callers_can_separate_taste_from_defects() {
        // A correctness failure blocks; a design failure advises. A caller
        // that cannot tell them apart has to treat both the same.
        let doc = text_scene("a few words", 9.0, 5000);
        let mut assets = AssetStore::new();
        assets.add_bytes(
            "body",
            kineto_core::resolve_reserved_src("kineto:inter")
                .unwrap()
                .to_vec(),
        );
        assets.prepare(&doc).unwrap();
        let issues = analyze(&doc, &mut assets, 0);
        assert_eq!(issues[0].category, "design");

        let bad = text_doc("#131b24", "#101820", 30.0, [10.0, 20.0], None);
        let mut a2 = AssetStore::new();
        a2.add_bytes(
            "body",
            kineto_core::resolve_reserved_src("kineto:inter")
                .unwrap()
                .to_vec(),
        );
        a2.prepare(&bad).unwrap();
        assert_eq!(analyze(&bad, &mut a2, 0)[0].category, "correctness");
    }

    #[test]
    fn a_scene_level_issue_has_no_element_index() {
        // tooFast is a property of the scene, not of any one element.
        let doc = text_scene(
            "one two three four five six seven eight nine ten",
            28.0,
            300,
        );
        let mut assets = AssetStore::new();
        assets.add_bytes(
            "body",
            kineto_core::resolve_reserved_src("kineto:inter")
                .unwrap()
                .to_vec(),
        );
        assets.prepare(&doc).unwrap();
        let issues = analyze(&doc, &mut assets, 0);
        let fast = issues
            .iter()
            .find(|i| i.kind == "tooFast")
            .expect("tooFast");
        assert_eq!(fast.element, None);
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
        assert_eq!(issues[0].scene.as_deref(), Some("s"));
        assert_eq!(issues[0].element, Some(1));
    }
}
