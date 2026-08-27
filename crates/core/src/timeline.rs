//! Timeline evaluation: scene start ticks, durations, and layering for rendering.

use crate::doc::{Document, Transition};

/// A layer at a given tick: which scene, local tick within that scene, and blending alpha.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub scene: usize,
    pub local: i64,
    pub alpha: f64,
}

/// Compute the global start tick for each scene.
///
/// Formulas:
/// - start[0] = 0
/// - start[i] = start[i-1] + dur[i-1] - overlap[i]
///   where overlap[i] is the crossfade duration of scene i (0 for cut)
pub fn scene_starts(doc: &Document) -> Vec<i64> {
    let mut starts = Vec::new();
    let mut current_start = 0i64;

    for (i, scene) in doc.scenes.iter().enumerate() {
        starts.push(current_start);

        // Compute overlap for the *next* scene (if it exists)
        let overlap_next = if i + 1 < doc.scenes.len() {
            match doc.scenes[i + 1].transition {
                Some(Transition::Crossfade { duration }) => duration,
                None => 0,
            }
        } else {
            0
        };

        // Next start = current_start + current_duration - overlap_next
        current_start += scene.duration - overlap_next;
    }

    starts
}

/// Compute the total duration of the document in ticks.
///
/// Formula: total = start.last() + dur.last()
pub fn total_duration(doc: &Document) -> i64 {
    if doc.scenes.is_empty() {
        return 0;
    }

    let starts = scene_starts(doc);
    let last_scene = &doc.scenes[doc.scenes.len() - 1];
    starts[starts.len() - 1] + last_scene.duration
}

/// Retrieve the layers visible at a given tick.
///
/// Returns empty if tick < 0 or tick >= total_duration.
/// Otherwise returns 1 or 2 layers (outgoing first if 2):
/// - Every scene with start <= tick < start+duration
/// - Alpha = 1.0 normally; inside incoming crossfade window [start[i], start[i]+overlap[i]),
///   alpha = (tick - start[i]) / overlap[i].
pub fn layer_at(doc: &Document, tick: i64) -> Vec<Layer> {
    let total = total_duration(doc);
    if tick < 0 || tick >= total {
        return vec![];
    }

    let starts = scene_starts(doc);
    let mut result = Vec::new();

    for (i, scene) in doc.scenes.iter().enumerate() {
        let start = starts[i];
        let end = start + scene.duration;

        // Check if this scene is visible at this tick
        if tick >= start && tick < end {
            // Determine if this scene is in its own incoming crossfade window
            let incoming_overlap = match scene.transition {
                Some(Transition::Crossfade { duration }) => duration,
                None => 0,
            };

            let crossfade_start = start;
            let crossfade_end = start + incoming_overlap;

            let alpha = if incoming_overlap > 0 && tick >= crossfade_start && tick < crossfade_end {
                (tick - crossfade_start) as f64 / incoming_overlap as f64
            } else {
                1.0
            };

            let local = tick - start;
            result.push(Layer {
                scene: i,
                local,
                alpha,
            });
        }
    }

    result
}
