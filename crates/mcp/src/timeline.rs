//! Scene spans over a document's timeline: where each scene actually starts,
//! and how much wall time transitions consume.
//!
//! Kept here rather than derived from an `Engine` because `Engine::new`
//! consumes its `Document` and exposes no accessor — the tools hold the
//! document just before that, which is the cheapest place to measure it.

use kineto_core::Document;
use serde::Serialize;

use crate::render::round_ms;

/// Where one scene sits on the timeline.
///
/// Tick fields stay off the wire: they are what the server does arithmetic
/// with, while a caller navigates in milliseconds.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneSpan {
    pub id: String,
    pub start_ms: i64,
    pub duration_ms: i64,
    #[serde(skip)]
    pub start_tick: i64,
    #[serde(skip)]
    pub duration_ticks: i64,
    /// How far this scene's incoming crossfade pulls it back into the
    /// previous one. Zero for a cut.
    #[serde(skip)]
    pub incoming_overlap_ticks: i64,
}

impl SceneSpan {
    /// The moment that reliably shows *this* scene.
    ///
    /// Not the start: a crossfaded scene is at alpha 0 at its own start tick,
    /// so a frame there is mostly the scene before it.
    pub fn midpoint_tick(&self) -> i64 {
        self.start_tick + self.duration_ticks / 2
    }

    fn end_tick(&self) -> i64 {
        self.start_tick + self.duration_ticks
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineSummary {
    /// What the scene durations add up to.
    pub nominal_ms: i64,
    /// What the document actually lasts.
    pub actual_ms: i64,
    /// The difference, which crossfades consumed. Nonzero here means a
    /// document is shorter than its author probably intended.
    pub transition_overlap_ms: i64,
    pub scenes: Vec<SceneSpan>,
}

impl TimelineSummary {
    pub fn find(&self, id: &str) -> Option<&SceneSpan> {
        self.scenes.iter().find(|s| s.id == id)
    }

    pub fn ids(&self) -> Vec<&str> {
        self.scenes.iter().map(|s| s.id.as_str()).collect()
    }

    /// The scene a viewer would say they are looking at, at `tick`.
    ///
    /// Inside a crossfade two scenes are on screen at once; the incoming one
    /// only wins once it is at least half faded in, so attribution matches
    /// what dominates the frame rather than what is technically topmost.
    pub fn scene_at(&self, tick: i64) -> Option<&SceneSpan> {
        let visible: Vec<&SceneSpan> = self
            .scenes
            .iter()
            .filter(|s| tick >= s.start_tick && tick < s.end_tick())
            .collect();
        match visible.as_slice() {
            [] => None,
            [only] => Some(only),
            [prev, cur, ..] => {
                let into = tick - cur.start_tick;
                if cur.incoming_overlap_ticks > 0 && into * 2 < cur.incoming_overlap_ticks {
                    Some(prev)
                } else {
                    Some(cur)
                }
            }
        }
    }
}

/// Measure a document's timeline.
pub fn summary(doc: &Document) -> TimelineSummary {
    let starts = kineto_core::timeline::scene_starts(doc);
    let mut scenes = Vec::with_capacity(doc.scenes.len());

    for (i, scene) in doc.scenes.iter().enumerate() {
        // Derived rather than read off the transition so it stays correct if
        // a future transition kind overlaps by some other rule.
        let incoming_overlap_ticks = if i == 0 {
            0
        } else {
            let prev_end = starts[i - 1] + doc.scenes[i - 1].duration;
            (prev_end - starts[i]).max(0)
        };
        scenes.push(SceneSpan {
            id: scene.id.clone(),
            start_ms: round_ms(starts[i]),
            duration_ms: round_ms(scene.duration),
            start_tick: starts[i],
            duration_ticks: scene.duration,
            incoming_overlap_ticks,
        });
    }

    let nominal_ms = round_ms(doc.scenes.iter().map(|s| s.duration).sum());
    let actual_ms = round_ms(kineto_core::timeline::total_duration(doc));

    TimelineSummary {
        nominal_ms,
        actual_ms,
        // Taken as the difference so the three numbers always reconcile,
        // rather than summing separately-rounded overlaps.
        transition_overlap_ms: nominal_ms - actual_ms,
        scenes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kineto_core::doc::{Transition, TIMEBASE};
    use kineto_core::{Document, Element, Scene};

    /// Three one-second scenes joined by two third-of-a-second crossfades.
    /// Nominal length is 3s; the overlaps make the real length 2.333s.
    fn crossfaded_doc() -> Document {
        let mut doc = Document::new(320, 180);
        for i in 0..3 {
            let mut scene = Scene::new(&format!("s{i}"), TIMEBASE)
                .with_element(Element::rect([0.0, 0.0, 320.0, 180.0], "#3366FF"));
            if i > 0 {
                scene = scene.with_transition(Transition::Crossfade {
                    duration: TIMEBASE / 3,
                });
            }
            doc.push_scene(scene);
        }
        doc
    }

    #[test]
    fn a_crossfaded_document_is_shorter_than_the_sum_of_its_scenes() {
        // The trap this block exists to surface: authoring 3 x 1s scenes and
        // expecting a 3s video. Two crossfades eat 2/3 of a second, and
        // nothing in the document says so.
        let s = summary(&crossfaded_doc());
        assert_eq!(s.nominal_ms, 3_000);
        assert_eq!(s.actual_ms, 2_333);
        assert_eq!(s.transition_overlap_ms, 667);
    }

    #[test]
    fn scene_starts_account_for_the_overlap_they_are_pulled_back_by() {
        // Scene 1 does not start at 1000ms — its crossfade pulls it back into
        // scene 0 by a third of a second. Summing durations gets this wrong,
        // which is exactly why callers should not have to.
        let s = summary(&crossfaded_doc());
        let starts: Vec<i64> = s.scenes.iter().map(|x| x.start_ms).collect();
        assert_eq!(starts, vec![0, 667, 1_333]);
        assert_eq!(s.scenes[0].id, "s0");
    }

    #[test]
    fn a_document_with_cuts_only_is_exactly_the_sum_of_its_scenes() {
        // The control for the test above: with no crossfades, nominal and
        // actual must agree, so a bug that always subtracted an overlap
        // cannot hide.
        let mut doc = Document::new(320, 180);
        for i in 0..3 {
            doc.push_scene(Scene::new(&format!("s{i}"), TIMEBASE));
        }
        let s = summary(&doc);
        assert_eq!(s.nominal_ms, 3_000);
        assert_eq!(s.actual_ms, 3_000);
        assert_eq!(s.transition_overlap_ms, 0);
    }

    #[test]
    fn a_scene_is_addressed_at_its_midpoint_not_its_start() {
        // A crossfaded scene is at alpha 0 at its own start tick, so a preview
        // there shows the *previous* scene. The midpoint is the first moment
        // that reliably shows the scene the caller asked for.
        let s = summary(&crossfaded_doc());
        let span = s.find("s1").expect("scene s1 exists");
        assert_eq!(span.midpoint_tick(), span.start_tick + TIMEBASE / 2);
    }

    #[test]
    fn an_unknown_scene_id_is_not_found() {
        let s = summary(&crossfaded_doc());
        assert!(s.find("nope").is_none());
        assert_eq!(s.ids(), vec!["s0", "s1", "s2"]);
    }

    #[test]
    fn the_scene_visible_at_a_tick_is_the_one_that_dominates_it() {
        // Inside a crossfade both scenes are on screen. Attribution must name
        // the one the viewer actually sees, so the incoming scene only wins
        // once it is at least half faded in.
        let s = summary(&crossfaded_doc());
        let one = s.find("s1").unwrap();
        let just_in = one.start_tick + 1;
        let past_half = one.start_tick + TIMEBASE / 3;
        assert_eq!(s.scene_at(just_in).unwrap().id, "s0");
        assert_eq!(s.scene_at(past_half).unwrap().id, "s1");
    }
}
