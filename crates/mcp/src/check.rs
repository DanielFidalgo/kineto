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
    _doc: &Document,
    _assets: &mut AssetStore,
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
        push(
            "fullyTransparent",
            format!(
                "resolved opacity is {:.3} — the element draws nothing here",
                resolved.opacity
            ),
        );
        // Everything else about an invisible element is moot.
        return;
    }

    let base = base_bbox(el);
    let degenerate = match el {
        // tiny-skia's `Rect::from_xywh` returns None for either dimension at
        // zero, and the raster arm skips it — nothing is drawn at all.
        Element::Rect { .. } | Element::Image { .. } => base.w <= 0.0 || base.h <= 0.0,
        // A zero-height path is a horizontal line, which strokes perfectly
        // well; only a path collapsed to a single point draws nothing.
        Element::Path { .. } => base.w <= 0.0 && base.h <= 0.0,
        // Text's base box is a placeholder (raster.rs::base_bbox), and a
        // group's is the union of its children, checked on their own.
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

    if matches!(el, Element::Text { .. }) {
        // base_bbox gives text a zero-size placeholder, so its bounds here
        // would be meaningless. Text geometry needs real layout (next cycle).
        return;
    }
    if max_x <= 0.0 || min_x >= cw || max_y <= 0.0 || min_y >= ch {
        push(
            "offCanvas",
            format!(
                "bounds ({min_x:.0},{min_y:.0})-({max_x:.0},{max_y:.0}) are entirely \
                 outside the {cw:.0}x{ch:.0} canvas"
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
    fn an_element_faded_to_nothing_is_reported() {
        let doc = doc_with(vec![Element::rect([10.0, 10.0, 50.0, 50.0], "#FF9900")
            .with_animation(Track::new(
                Prop::Opacity,
                vec![Key::num(0, 1.0), Key::num(TIMEBASE, 0.0)],
            ))]);

        assert_eq!(kinds(&doc, 0), Vec::<&str>::new(), "visible at t=0");
        assert_eq!(kinds(&doc, TIMEBASE - 1), vec!["fullyTransparent"]);
    }

    #[test]
    fn degenerate_geometry_is_reported() {
        let doc = doc_with(vec![Element::rect([10.0, 10.0, 0.0, 40.0], "#FF9900")]);
        assert_eq!(kinds(&doc, 0), vec!["zeroSize"]);
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
