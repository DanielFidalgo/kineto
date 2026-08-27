//! Load-time validation: `Document::from_json` is THE loading entrypoint for
//! wasm, CLI, and every future consumer. Pipeline:
//!   1. parse `serde_json::Value`
//!   2. unknown-field walk (per-context whitelists) over the raw `Value`
//!   3. `serde_json::from_value::<Document>` (typed decode)
//!   4. semantic checks over the typed `Document`
use crate::color::Color;
use crate::doc::{Asset, Common, Document, Element, KeyValue, Prop, Scene, Track, Transition};
use serde_json::{Map, Value};
use std::collections::HashSet;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error)]
pub enum DocError {
    #[error("unsupported document version {0} (expected 1)")]
    Version(u64),
    #[error("unsupported timebase {0} (expected 705600000)")]
    Timebase(i64),
    #[error("unknown field '{field}' in {ctx}")]
    UnknownField { ctx: String, field: String },
    #[error("invalid id '{0}': must match [A-Za-z0-9_-]{{1,64}}")]
    BadId(String),
    #[error("duplicate scene id '{0}'")]
    DuplicateSceneId(String),
    #[error("scene '{0}' has non-positive duration")]
    NonPositiveDuration(String),
    #[error("transition not allowed on the first scene")]
    TransitionOnFirstScene,
    #[error("transition into scene '{0}' is longer than the shorter adjacent scene")]
    TransitionTooLong(String),
    #[error("unknown asset id '{0}'")]
    UnknownAssetId(String),
    #[error("asset '{id}' is not a {expected}")]
    AssetTypeMismatch { id: String, expected: &'static str },
    #[error("invalid color '{0}'")]
    BadColor(String),
    #[error("keyframe times not strictly increasing in track '{0}'")]
    KeysNotIncreasing(String),
    #[error("key value arity mismatch in track '{0}'")]
    KeyArity(String),
    #[error("track '{0}' has no keys")]
    EmptyTrack(String),
    #[error("opacity out of range [0,1]: {0}")]
    OpacityRange(String),
    #[error("{0}")]
    Json(String),
}

impl Document {
    /// THE loading entrypoint (wasm, CLI, tests, everything): parse →
    /// unknown-field walk → typed decode → semantic checks.
    pub fn from_json(s: &str) -> Result<Document, DocError> {
        let value: Value = serde_json::from_str(s).map_err(|e| DocError::Json(e.to_string()))?;
        walk_document(&value)?;
        let doc: Document =
            serde_json::from_value(value).map_err(|e| DocError::Json(e.to_string()))?;
        validate_semantics(&doc)?;
        Ok(doc)
    }
}

// ---- unknown-field walk (over the raw `Value` tree) ----

const DOC_KEYS: &[&str] = &[
    "v",
    "timebase",
    "defaultFps",
    "size",
    "bg",
    "assets",
    "scenes",
];
const SIZE_KEYS: &[&str] = &["w", "h"];
const ASSET_KEYS: &[&str] = &["type", "src"];
const SCENE_KEYS: &[&str] = &["id", "transition", "duration", "elements"];
const TRANSITION_KEYS: &[&str] = &["type", "duration"];
const TRACK_KEYS: &[&str] = &["prop", "keys"];
const KEY_KEYS: &[&str] = &["t", "v", "ease"];
const PROP_VALUES: &[&str] = &["translate", "scale", "rotation", "opacity"];

const COMMON_KEYS: &[&str] = &["translate", "scale", "rotation", "opacity", "animations"];
const IMAGE_KEYS: &[&str] = &["type", "asset", "rect"];
const TEXT_KEYS: &[&str] = &[
    "type", "text", "font", "sizePx", "color", "pos", "maxW", "align",
];
const RECT_KEYS: &[&str] = &["type", "rect", "fill"];
const GROUP_KEYS: &[&str] = &["type", "origin", "children"];

fn check_keys(obj: &Map<String, Value>, allowed: &[&[&str]], ctx: &str) -> Result<(), DocError> {
    for k in obj.keys() {
        if !allowed.iter().any(|set| set.contains(&k.as_str())) {
            return Err(DocError::UnknownField {
                ctx: ctx.to_string(),
                field: k.clone(),
            });
        }
    }
    Ok(())
}

fn walk_document(v: &Value) -> Result<(), DocError> {
    let Some(obj) = v.as_object() else {
        return Ok(());
    };
    check_keys(obj, &[DOC_KEYS], "document")?;
    if let Some(size) = obj.get("size").and_then(Value::as_object) {
        check_keys(size, &[SIZE_KEYS], "size")?;
    }
    if let Some(assets) = obj.get("assets").and_then(Value::as_object) {
        for asset in assets.values() {
            if let Some(aobj) = asset.as_object() {
                check_keys(aobj, &[ASSET_KEYS], "asset")?;
            }
        }
    }
    if let Some(scenes) = obj.get("scenes").and_then(Value::as_array) {
        for scene in scenes {
            walk_scene(scene)?;
        }
    }
    Ok(())
}

fn walk_scene(v: &Value) -> Result<(), DocError> {
    let Some(obj) = v.as_object() else {
        return Ok(());
    };
    check_keys(obj, &[SCENE_KEYS], "scene")?;
    if let Some(t) = obj.get("transition").and_then(Value::as_object) {
        check_keys(t, &[TRANSITION_KEYS], "transition")?;
    }
    if let Some(elements) = obj.get("elements").and_then(Value::as_array) {
        for el in elements {
            walk_element(el)?;
        }
    }
    Ok(())
}

fn walk_element(v: &Value) -> Result<(), DocError> {
    let Some(obj) = v.as_object() else {
        return Ok(());
    };
    let ty = obj.get("type").and_then(Value::as_str);
    match ty {
        Some("image") => check_keys(obj, &[IMAGE_KEYS, COMMON_KEYS], "element")?,
        Some("text") => check_keys(obj, &[TEXT_KEYS, COMMON_KEYS], "element")?,
        Some("rect") => check_keys(obj, &[RECT_KEYS, COMMON_KEYS], "element")?,
        Some("group") => check_keys(obj, &[GROUP_KEYS, COMMON_KEYS], "element")?,
        Some(other) => {
            return Err(DocError::UnknownField {
                ctx: "element.type".to_string(),
                field: other.to_string(),
            });
        }
        None => return Ok(()),
    }
    walk_animations(obj)?;
    if ty == Some("group") {
        if let Some(children) = obj.get("children").and_then(Value::as_array) {
            for child in children {
                walk_element(child)?;
            }
        }
    }
    Ok(())
}

fn walk_animations(obj: &Map<String, Value>) -> Result<(), DocError> {
    let Some(tracks) = obj.get("animations").and_then(Value::as_array) else {
        return Ok(());
    };
    for track in tracks {
        let Some(tobj) = track.as_object() else {
            continue;
        };
        check_keys(tobj, &[TRACK_KEYS], "track")?;
        if let Some(prop) = tobj.get("prop").and_then(Value::as_str) {
            if !PROP_VALUES.contains(&prop) {
                return Err(DocError::UnknownField {
                    ctx: "track.prop".to_string(),
                    field: prop.to_string(),
                });
            }
        }
        if let Some(keys) = tobj.get("keys").and_then(Value::as_array) {
            for key in keys {
                if let Some(kobj) = key.as_object() {
                    check_keys(kobj, &[KEY_KEYS], "key")?;
                }
            }
        }
    }
    Ok(())
}

/// Re-run semantic validation on an already-typed `Document`, independent of
/// `from_json`'s parse -> unknown-field-walk -> decode pipeline. Public so
/// callers that build a `Document` some other way (the Rust SDK builder in
/// `doc.rs`, or `Engine::new` re-checking defensively) can still get the
/// same semantic guarantees `from_json` gives JSON-sourced documents.
pub fn check(doc: &Document) -> Result<(), DocError> {
    validate_semantics(doc)
}

// ---- semantic checks (over the typed `Document`) ----

fn valid_id(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn prop_name(p: Prop) -> &'static str {
    match p {
        Prop::Translate => "translate",
        Prop::Scale => "scale",
        Prop::Rotation => "rotation",
        Prop::Opacity => "opacity",
    }
}

fn validate_semantics(doc: &Document) -> Result<(), DocError> {
    if doc.v != 1 {
        return Err(DocError::Version(doc.v as u64));
    }
    if doc.timebase != crate::doc::TIMEBASE {
        return Err(DocError::Timebase(doc.timebase));
    }
    for id in doc.assets.keys() {
        if !valid_id(id) {
            return Err(DocError::BadId(id.clone()));
        }
    }

    let mut seen = HashSet::new();
    for (i, scene) in doc.scenes.iter().enumerate() {
        validate_scene(doc, scene, i)?;
        if !seen.insert(scene.id.clone()) {
            return Err(DocError::DuplicateSceneId(scene.id.clone()));
        }
    }
    Ok(())
}

fn validate_scene(doc: &Document, scene: &Scene, index: usize) -> Result<(), DocError> {
    if !valid_id(&scene.id) {
        return Err(DocError::BadId(scene.id.clone()));
    }
    if scene.duration <= 0 {
        return Err(DocError::NonPositiveDuration(scene.id.clone()));
    }
    if let Some(Transition::Crossfade { duration }) = &scene.transition {
        if index == 0 {
            return Err(DocError::TransitionOnFirstScene);
        }
        let prev = &doc.scenes[index - 1];
        if *duration <= 0 || *duration > prev.duration.min(scene.duration) {
            return Err(DocError::TransitionTooLong(scene.id.clone()));
        }
    }
    validate_elements(&scene.elements, doc)
}

fn validate_elements(elements: &[Element], doc: &Document) -> Result<(), DocError> {
    for el in elements {
        validate_element(el, doc)?;
    }
    Ok(())
}

fn validate_element(el: &Element, doc: &Document) -> Result<(), DocError> {
    match el {
        Element::Image { asset, common, .. } => {
            check_asset(doc, asset, "image")?;
            validate_common(common)?;
        }
        Element::Text {
            font,
            color,
            common,
            ..
        } => {
            check_asset(doc, font, "font")?;
            if !Color::parse_ok(&color.0) {
                return Err(DocError::BadColor(color.0.clone()));
            }
            validate_common(common)?;
        }
        Element::Rect { fill, common, .. } => {
            if !Color::parse_ok(&fill.0) {
                return Err(DocError::BadColor(fill.0.clone()));
            }
            validate_common(common)?;
        }
        Element::Group {
            children, common, ..
        } => {
            validate_common(common)?;
            validate_elements(children, doc)?;
        }
    }
    Ok(())
}

fn check_asset(doc: &Document, id: &str, expected: &'static str) -> Result<(), DocError> {
    match doc.assets.get(id) {
        None => Err(DocError::UnknownAssetId(id.to_string())),
        Some(Asset::Image { .. }) if expected == "image" => Ok(()),
        Some(Asset::Font { .. }) if expected == "font" => Ok(()),
        Some(_) => Err(DocError::AssetTypeMismatch {
            id: id.to_string(),
            expected,
        }),
    }
}

fn validate_common(common: &Common) -> Result<(), DocError> {
    if let Some(op) = common.opacity {
        if !(0.0..=1.0).contains(&op.0) {
            return Err(DocError::OpacityRange(op.0.to_string()));
        }
    }
    for track in &common.animations {
        validate_track(track)?;
    }
    Ok(())
}

fn validate_track(track: &Track) -> Result<(), DocError> {
    let name = prop_name(track.prop);
    if track.keys.is_empty() {
        return Err(DocError::EmptyTrack(name.to_string()));
    }
    let mut last_t: Option<i64> = None;
    for key in &track.keys {
        if let Some(prev) = last_t {
            if key.t <= prev {
                return Err(DocError::KeysNotIncreasing(name.to_string()));
            }
        }
        last_t = Some(key.t);

        match (track.prop, &key.v) {
            (Prop::Translate, KeyValue::Vec2(_)) => {}
            (Prop::Translate, KeyValue::Num(_)) => {
                return Err(DocError::KeyArity(name.to_string()))
            }
            (_, KeyValue::Num(v)) => {
                if track.prop == Prop::Opacity && !(0.0..=1.0).contains(&v.0) {
                    return Err(DocError::OpacityRange(v.0.to_string()));
                }
            }
            (_, KeyValue::Vec2(_)) => return Err(DocError::KeyArity(name.to_string())),
        }
    }
    Ok(())
}
