//! An append-only journal of what an agent did, and its projection into a
//! document.
//!
//! The journal is the record; the document is one rendering of it. Recompiling
//! the same journal later with a different projection yields a different video
//! from the same truth, which is the whole reason the beats are stored as
//! semantic events rather than as scenes.
//!
//! Wall-clock lives here, in the log, where it belongs — a log has timestamps.
//! `compile` is a pure function of the beats, so the engine's `(doc, tick) →
//! pixels` contract is untouched and re-rendering an old journal reproduces
//! its pixels exactly.

use std::io::Write;
use std::path::Path;

use kineto_core::doc::{Ease, Key, Prop, Track, TIMEBASE};
use kineto_core::{Asset, Document, Element, Scene};
use serde::{Deserialize, Serialize};

use crate::error::ToolError;

/// Reading speed used to size a beat's scene. The same number
/// `check::SCAN_WPM` lints against, so a compiled session cannot flash past —
/// the thing that decides pacing is the thing that would complain about it.
const SCAN_WPM: f64 = 300.0;
const BEAT_MS: f64 = 500.0;
/// No beat is worth less than this, however few words it carries.
const MIN_BEAT_MS: i64 = 1800;

const W: u32 = 1280;
const H: u32 = 720;
const BG: &str = "#0D1419";
const FG: &str = "#F2F5F7";
const DIM: &str = "#8FA3B0";
const RAIL: &str = "#22303b";

/// One thing that happened.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Beat {
    /// Milliseconds since the epoch the caller is using. Only differences
    /// matter; the first beat defines zero.
    pub at_ms: i64,
    /// `task` | `step` | `result` | `note` | `error`. Chosen by the caller,
    /// rendered by the projection — an agent says what happened and never how
    /// it looks.
    pub kind: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

fn accent_for(kind: &str) -> &'static str {
    match kind {
        "task" => "#FF9900",
        "step" => "#4ECDC4",
        "result" => "#C77DFF",
        "error" => "#FF5C5C",
        _ => "#8FA3B0",
    }
}

/// Append one beat. Creates the journal if absent, never rewrites what is
/// already there.
pub fn append(path: &Path, beat: &Beat) -> Result<usize, ToolError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::Io {
                context: "creating journal directory",
                path: parent.display().to_string(),
                source: e,
            })?;
        }
    }
    let line = serde_json::to_string(beat)
        .map_err(|e| ToolError::Invalid(format!("beat is not serialisable: {e}")))?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| ToolError::Io {
            context: "opening journal",
            path: path.display().to_string(),
            source: e,
        })?;
    writeln!(f, "{line}").map_err(|e| ToolError::Io {
        context: "appending to journal",
        path: path.display().to_string(),
        source: e,
    })?;
    Ok(read(path)?.len())
}

/// Every beat in the journal, in the order it was written.
pub fn read(path: &Path) -> Result<Vec<Beat>, ToolError> {
    let text = std::fs::read_to_string(path).map_err(|e| ToolError::Io {
        context: "reading journal",
        path: path.display().to_string(),
        source: e,
    })?;
    let mut beats = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        beats.push(serde_json::from_str(line).map_err(|e| {
            ToolError::Invalid(format!("journal line {} is not a beat: {e}", i + 1))
        })?);
    }
    Ok(beats)
}

/// How long a scene carrying `words` words should last, in milliseconds.
///
/// Derived rather than fixed, so a one-line note does not linger and a dense
/// result is not snatched away. The count must be of everything the scene
/// *renders* — badge, elapsed stamp and progress caption included — because
/// that is what `check::tooFast` counts. Sizing from the beat's own text alone
/// under-counted by about six words and every compiled scene tripped the lint.
fn scene_duration_ms(words: usize) -> i64 {
    let needed = (words as f64 / SCAN_WPM * 60_000.0 + BEAT_MS).ceil() as i64;
    needed.max(MIN_BEAT_MS)
}

fn count_words(s: &str) -> usize {
    s.split_whitespace().count()
}

/// `+1m 32s` — elapsed since the first beat.
fn elapsed_label(ms: i64) -> String {
    let s = ms / 1000;
    if s < 60 {
        format!("+{s}s")
    } else {
        format!("+{}m {:02}s", s / 60, s % 60)
    }
}

/// Project a journal into a document.
///
/// Pure: the same beats always produce the same document, which is what lets
/// an old journal be re-rendered byte-for-byte.
pub fn compile(beats: &[Beat], title: &str) -> Result<Document, ToolError> {
    if beats.is_empty() {
        return Err(ToolError::Invalid(
            "this journal has no beats — nothing to compile".into(),
        ));
    }

    let mut doc = Document::new(W, H).with_fps(30).with_bg(BG);
    doc.add_asset("inter", Asset::font("kineto:inter"));
    doc.add_asset("mono", Asset::font("kineto:jetbrains-mono"));

    let t0 = beats[0].at_ms;
    let n = beats.len();

    for (i, beat) in beats.iter().enumerate() {
        // Build every string first, so the duration is sized against exactly
        // what the scene will render.
        let kind_label = beat.kind.to_uppercase();
        let elapsed = elapsed_label(beat.at_ms - t0);
        let rail_label = format!("{} / {}   {}", i + 1, n, title);
        let words = count_words(&kind_label)
            + count_words(&elapsed)
            + count_words(&beat.title)
            + beat.detail.as_deref().map(count_words).unwrap_or(0)
            + count_words(&rail_label);

        let dur = scene_duration_ms(words) * (TIMEBASE / 1000);
        // Elements fade in and out within the scene, and scenes hard-cut
        // between: a dip to background, with none of the crossfade overlap
        // arithmetic and no chance of two beats reading on top of each other.
        let out0 = dur - 360 * (TIMEBASE / 1000);
        let fade = |hold_ms: i64| {
            Track::new(
                Prop::Opacity,
                vec![
                    Key::num(0, 0.0),
                    Key::num(hold_ms * (TIMEBASE / 1000), 1.0).with_ease(Ease::OutCubic),
                    Key::num(out0, 1.0),
                    Key::num(dur, 0.0).with_ease(Ease::OutCubic),
                ],
            )
        };

        let accent = accent_for(&beat.kind);
        let mut scene = Scene::new(&format!("b{i:04}"), dur)
            .with_element(
                Element::text(&kind_label, "mono", 16.0, accent, [90.0, 232.0])
                    .with_animation(fade(240)),
            )
            .with_element(
                Element::text(&elapsed, "mono", 16.0, DIM, [1090.0, 232.0])
                    .with_animation(fade(240)),
            )
            .with_element(
                Element::text(&beat.title, "inter", 44.0, FG, [90.0, 272.0])
                    .with_max_w(1100.0)
                    .with_animation(fade(380)),
            );

        if let Some(detail) = &beat.detail {
            scene = scene.with_element(
                Element::text(detail, "mono", 20.0, DIM, [92.0, 392.0])
                    .with_max_w(1090.0)
                    .with_animation(fade(600)),
            );
        }

        // Progress rail: how far through the session this beat sits.
        let done = 90.0 + (1100.0 * (i + 1) as f64 / n as f64);
        scene = scene
            .with_element(
                Element::path(vec![[90.0, 560.0], [1190.0, 560.0]])
                    .with_stroke(RAIL, 3.0)
                    .with_animation(fade(240)),
            )
            .with_element(
                Element::path(vec![[90.0, 560.0], [done, 560.0]])
                    .with_stroke(accent, 3.0)
                    .with_cap(kineto_core::doc::Cap::Round)
                    .with_animation(fade(380)),
            )
            .with_element(
                Element::text(&rail_label, "mono", 15.0, DIM, [90.0, 584.0])
                    .with_animation(fade(500)),
            );

        doc.push_scene(scene);
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beat(at: i64, kind: &str, title: &str, detail: Option<&str>) -> Beat {
        Beat {
            at_ms: at,
            kind: kind.into(),
            title: title.into(),
            detail: detail.map(str::to_string),
            status: None,
        }
    }

    #[test]
    fn appending_preserves_what_was_already_there() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("j.jsonl");
        assert_eq!(append(&p, &beat(0, "task", "first", None)).unwrap(), 1);
        assert_eq!(append(&p, &beat(10, "step", "second", None)).unwrap(), 2);
        let beats = read(&p).unwrap();
        assert_eq!(beats.len(), 2);
        assert_eq!(beats[0].title, "first");
        assert_eq!(beats[1].title, "second");
    }

    #[test]
    fn compiling_the_same_journal_twice_gives_the_same_document() {
        // The property that makes a journal a record rather than a snapshot:
        // it can be re-rendered later and produce identical pixels.
        let beats = vec![
            beat(
                1000,
                "task",
                "Bound image residency",
                Some("1185 MB to 53 MB"),
            ),
            beat(9000, "result", "Parity held", Some("20/20 identical")),
        ];
        let a = compile(&beats, "session").unwrap().canonical_json();
        let b = compile(&beats, "session").unwrap().canonical_json();
        assert_eq!(a, b);
    }

    #[test]
    fn a_wordier_beat_is_given_more_time() {
        // Asserted through `compile`, not the arithmetic alone: the duration
        // has to be sized against what the scene renders, and testing the
        // helper in isolation is exactly how that went wrong the first time.
        let short = compile(&[beat(0, "note", "ok", None)], "s").unwrap();
        let long = compile(
            &[beat(
                0,
                "result",
                "Bounded image residency with a byte budgeted cache",
                Some(
                    "peak resident memory fell from 1185 MB to 53 MB and is now flat \
                     in the number of frames a document references",
                ),
            )],
            "s",
        )
        .unwrap();
        assert!(long.scenes[0].duration > short.scenes[0].duration);
        assert!(
            short.scenes[0].duration >= MIN_BEAT_MS * (TIMEBASE / 1000),
            "no scene may fall below the floor"
        );
    }

    #[test]
    fn every_beat_gets_its_own_scene() {
        let beats: Vec<Beat> = (0..5).map(|i| beat(i * 1000, "step", "x", None)).collect();
        let doc = compile(&beats, "s").unwrap();
        assert_eq!(doc.scenes.len(), 5);
        let ids: std::collections::BTreeSet<&str> =
            doc.scenes.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids.len(), 5, "scene ids must be unique");
    }

    #[test]
    fn an_empty_journal_is_an_error() {
        // Rather than a zero-length video that looks like a rendering bug.
        assert!(compile(&[], "s").is_err());
    }

    #[test]
    fn elapsed_is_shown_relative_to_the_first_beat() {
        assert_eq!(elapsed_label(0), "+0s");
        assert_eq!(elapsed_label(45_000), "+45s");
        assert_eq!(elapsed_label(92_000), "+1m 32s");
    }
}
