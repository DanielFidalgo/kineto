//! `reveal` — entrance motion in one line.
//!
//! An agent writing a document by hand emits static text, and the reason is
//! economic rather than expressive. Fading five lines in sequence means five
//! elements each carrying two keyframe tracks, plus arithmetic in Flicks to
//! stagger them. That is roughly 600 tokens and a tick calculation which is
//! easy to get wrong, so a model working to a budget writes a slide deck.
//!
//! `"reveal": { "at": 300, "kind": "fadeUp" }` is about 20 tokens, in
//! milliseconds, and cannot be off by a factor of 705,600.
//!
//! This is sugar expanded here, in the tooling layer, before the document ever
//! reaches the engine — `kineto-core` neither knows nor could know about it.
//! That keeps the format opinion-free and the engine's input canonical, which
//! is the line this project draws between the two.

use std::borrow::Cow;

use serde_json::{json, Map, Value};

use crate::error::ToolError;

/// Default entrance duration. Long enough to read as motion, short enough that
/// several in sequence do not feel slow.
const DEFAULT_MS: i64 = 400;

/// How far a sliding entrance travels, in canvas units.
const DEFAULT_DISTANCE: f64 = 24.0;

/// What `popIn` starts at. Small enough to notice, large enough to stay legible.
const POP_FROM: f64 = 0.8;

pub const KINDS: &[&str] = &[
    "fadeIn",
    "fadeUp",
    "fadeDown",
    "slideLeft",
    "slideRight",
    "popIn",
];

/// Expands every `reveal` in a document, leaving everything else alone.
///
/// Returns the input untouched when there is no `reveal` anywhere, so a
/// document that does not use this cannot be perturbed by it — the canonical
/// bytes are the same ones the engine would have seen.
pub fn expand(json: &str) -> Result<Cow<'_, str>, ToolError> {
    if !json.contains("\"reveal\"") {
        return Ok(Cow::Borrowed(json));
    }
    let mut doc: Value = serde_json::from_str(json)
        .map_err(|e| ToolError::DocumentSource(format!("document is not valid JSON: {e}")))?;

    if let Some(scenes) = doc.get_mut("scenes").and_then(Value::as_array_mut) {
        for scene in scenes.iter_mut() {
            if let Some(elements) = scene.get_mut("elements").and_then(Value::as_array_mut) {
                expand_elements(elements)?;
            }
        }
    }
    serde_json::to_string(&doc)
        .map(Cow::Owned)
        .map_err(|e| ToolError::DocumentSource(format!("re-serialising document: {e}")))
}

fn expand_elements(elements: &mut [Value]) -> Result<(), ToolError> {
    for element in elements.iter_mut() {
        // Groups first: a reveal on a group animates the group as a whole,
        // and its children may carry their own.
        if let Some(children) = element.get_mut("children").and_then(Value::as_array_mut) {
            expand_elements(children)?;
        }
        let Some(object) = element.as_object_mut() else {
            continue;
        };
        if object.contains_key("reveal") {
            expand_one(object)?;
        }
    }
    Ok(())
}

/// Reads a number field, rejecting a wrong type rather than silently defaulting
/// — a typo that quietly does nothing is worse than an error.
fn number(spec: &Map<String, Value>, key: &str, default: f64) -> Result<f64, ToolError> {
    match spec.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v.as_f64().ok_or_else(|| {
            ToolError::DocumentSource(format!("reveal.{key} must be a number, got {v}"))
        }),
    }
}

fn expand_one(element: &mut Map<String, Value>) -> Result<(), ToolError> {
    let reveal = element.remove("reveal").expect("checked by caller");
    let spec = reveal.as_object().ok_or_else(|| {
        ToolError::DocumentSource(format!("reveal must be an object, got {reveal}"))
    })?;

    let kind = spec
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::DocumentSource("reveal.kind is required".into()))?
        .to_string();
    if !KINDS.contains(&kind.as_str()) {
        return Err(ToolError::DocumentSource(format!(
            "unknown reveal kind '{kind}': use one of {}",
            KINDS.join(", ")
        )));
    }

    let at_ms = number(spec, "at", 0.0)? as i64;
    let ms = number(spec, "ms", DEFAULT_MS as f64)? as i64;
    if ms <= 0 {
        return Err(ToolError::DocumentSource(format!(
            "reveal.ms must be positive, got {ms}"
        )));
    }
    if at_ms < 0 {
        return Err(ToolError::DocumentSource(format!(
            "reveal.at must not be negative, got {at_ms}"
        )));
    }
    let distance = number(spec, "distance", DEFAULT_DISTANCE)?;

    let start = at_ms * crate::render::TICKS_PER_MS;
    let end = (at_ms + ms) * crate::render::TICKS_PER_MS;

    // Relative to whatever the element already is, never replacing it. An
    // element positioned by `translate` must still land there.
    let base_translate = match element.get("translate") {
        Some(v) => {
            let a = v.as_array().filter(|a| a.len() == 2).ok_or_else(|| {
                ToolError::DocumentSource(format!("translate must be a pair, got {v}"))
            })?;
            [a[0].as_f64().unwrap_or(0.0), a[1].as_f64().unwrap_or(0.0)]
        }
        None => [0.0, 0.0],
    };
    let base_scale = element.get("scale").and_then(Value::as_f64).unwrap_or(1.0);
    let base_opacity = element
        .get("opacity")
        .and_then(Value::as_f64)
        .unwrap_or(1.0);

    // The ease belongs on the key being *entered* (see `anim::ease`), so it
    // goes on the second key of each pair, never the first.
    let ease = if kind == "popIn" {
        "outBack"
    } else {
        "outCubic"
    };

    let mut tracks: Vec<Value> = vec![json!({
        "prop": "opacity",
        "keys": [
            { "t": start, "v": 0.0 },
            { "t": end, "v": base_opacity, "ease": ease },
        ]
    })];

    let offset = match kind.as_str() {
        "fadeUp" => Some([0.0, distance]),
        "fadeDown" => Some([0.0, -distance]),
        // Named for the direction of travel: `slideLeft` enters from the right
        // and moves left.
        "slideLeft" => Some([distance, 0.0]),
        "slideRight" => Some([-distance, 0.0]),
        _ => None,
    };
    if let Some([dx, dy]) = offset {
        tracks.push(json!({
            "prop": "translate",
            "keys": [
                { "t": start, "v": [base_translate[0] + dx, base_translate[1] + dy] },
                { "t": end, "v": base_translate, "ease": ease },
            ]
        }));
    }
    if kind == "popIn" {
        tracks.push(json!({
            "prop": "scale",
            "keys": [
                { "t": start, "v": base_scale * POP_FROM },
                { "t": end, "v": base_scale, "ease": ease },
            ]
        }));
    }

    // Refusing to merge is deliberate. Silently combining a reveal with a
    // hand-written track on the same property produces motion nobody asked
    // for, and the author cannot see why.
    let existing = element
        .entry("animations")
        .or_insert_with(|| Value::Array(vec![]));
    let existing = existing
        .as_array_mut()
        .ok_or_else(|| ToolError::DocumentSource("animations must be an array".into()))?;
    for track in &tracks {
        let prop = track["prop"].as_str().unwrap_or_default();
        if existing
            .iter()
            .any(|t| t.get("prop").and_then(Value::as_str) == Some(prop))
        {
            return Err(ToolError::DocumentSource(format!(
                "element has both a reveal '{kind}' and its own '{prop}' animation; \
                 the reveal would fight it. Remove one."
            )));
        }
    }
    existing.extend(tracks);
    Ok(())
}
