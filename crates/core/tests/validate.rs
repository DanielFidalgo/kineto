mod common;
use kineto_core::*;

/// Loads testdata/canonical/example.json as a mutable `serde_json::Value`
/// so each test can perform targeted key/value surgery before feeding the
/// (now invalid) JSON back through `Document::from_json`.
fn example_value() -> serde_json::Value {
    let s = std::fs::read_to_string(common::repo("testdata/canonical/example.json")).unwrap();
    serde_json::from_str(&s).unwrap()
}

fn full_value() -> serde_json::Value {
    let s = std::fs::read_to_string(common::repo("testdata/canonical/example-full.json")).unwrap();
    serde_json::from_str(&s).unwrap()
}

fn load(v: &serde_json::Value) -> Result<Document, DocError> {
    Document::from_json(&v.to_string())
}

fn err(v: &serde_json::Value) -> DocError {
    load(v).expect_err("expected validation to fail")
}

// ---- happy path ----

#[test]
fn loads_example_json() {
    let v = example_value();
    let doc = load(&v).expect("example.json should load");
    assert_eq!(doc.v, 1);
    assert_eq!(doc.scenes.len(), 2);
}

#[test]
fn loads_example_full_json() {
    let v = full_value();
    let doc = load(&v).expect("example-full.json should load");
    assert_eq!(doc.v, 1);
    assert_eq!(doc.scenes.len(), 1);
}

// ---- unknown-field walk ----

#[test]
fn unknown_top_level_field() {
    let mut v = example_value();
    v.as_object_mut()
        .unwrap()
        .insert("foo".into(), serde_json::json!("bar"));
    assert_eq!(
        err(&v),
        DocError::UnknownField {
            ctx: "document".into(),
            field: "foo".into(),
        }
    );
}

#[test]
fn unknown_element_field() {
    let mut v = example_value();
    v["scenes"][0]["elements"][1]["blur"] = serde_json::json!(1);
    assert_eq!(
        err(&v),
        DocError::UnknownField {
            ctx: "element".into(),
            field: "blur".into(),
        }
    );
}

#[test]
fn unknown_animatable_prop() {
    let mut v = example_value();
    v["scenes"][0]["elements"][2]["animations"][0]["prop"] = serde_json::json!("blur");
    assert_eq!(
        err(&v),
        DocError::UnknownField {
            ctx: "track.prop".into(),
            field: "blur".into(),
        }
    );
}

#[test]
fn unknown_element_type() {
    let mut v = example_value();
    v["scenes"][0]["elements"][1]["type"] = serde_json::json!("blur");
    assert_eq!(
        err(&v),
        DocError::UnknownField {
            ctx: "element.type".into(),
            field: "blur".into(),
        }
    );
}

// ---- version / timebase ----

#[test]
fn bad_version() {
    let mut v = example_value();
    v["v"] = serde_json::json!(2);
    assert_eq!(err(&v), DocError::Version(2));
}

#[test]
fn bad_timebase() {
    let mut v = example_value();
    v["timebase"] = serde_json::json!(1000);
    assert_eq!(err(&v), DocError::Timebase(1000));
}

// ---- ids ----

#[test]
fn bad_scene_id() {
    let mut v = example_value();
    v["scenes"][0]["id"] = serde_json::json!("bad id!");
    assert_eq!(err(&v), DocError::BadId("bad id!".into()));
}

#[test]
fn bad_asset_id() {
    let mut v = example_value();
    let assets = v["assets"].as_object_mut().unwrap();
    let f01 = assets.remove("f01").unwrap();
    assets.insert("bad id!".into(), f01);
    assert_eq!(err(&v), DocError::BadId("bad id!".into()));
}

#[test]
fn duplicate_scene_id() {
    let mut v = example_value();
    v["scenes"][1]["id"] = serde_json::json!("step-1");
    assert_eq!(err(&v), DocError::DuplicateSceneId("step-1".into()));
}

// ---- durations / transitions ----

#[test]
fn non_positive_duration() {
    let mut v = example_value();
    v["scenes"][0]["duration"] = serde_json::json!(0);
    assert_eq!(err(&v), DocError::NonPositiveDuration("step-1".into()));
}

#[test]
fn transition_on_first_scene() {
    let mut v = example_value();
    v["scenes"][0]["transition"] =
        serde_json::json!({"type": "crossfade", "duration": 105_840_000i64});
    assert_eq!(err(&v), DocError::TransitionOnFirstScene);
}

#[test]
fn transition_too_long() {
    let mut v = example_value();
    // Two 150ms scenes with a 200ms crossfade between them: 200ms > min(150ms, 150ms).
    v["scenes"][0]["duration"] = serde_json::json!(105_840_000i64); // ms(150)
    v["scenes"][1]["duration"] = serde_json::json!(105_840_000i64); // ms(150)
    v["scenes"][1]["transition"]["duration"] = serde_json::json!(141_120_000i64); // ms(200)
    assert_eq!(err(&v), DocError::TransitionTooLong("step-2".into()));
}

// ---- assets ----

#[test]
fn unknown_asset_id() {
    let mut v = example_value();
    v["scenes"][0]["elements"][0]["asset"] = serde_json::json!("missing");
    assert_eq!(err(&v), DocError::UnknownAssetId("missing".into()));
}

#[test]
fn asset_type_mismatch() {
    let mut v = example_value();
    // "mono" text element's font now points at the "f01" image asset.
    v["scenes"][0]["elements"][2]["font"] = serde_json::json!("f01");
    assert_eq!(
        err(&v),
        DocError::AssetTypeMismatch {
            id: "f01".into(),
            expected: "font",
        }
    );
}

// ---- colors ----

#[test]
fn bad_color() {
    let mut v = example_value();
    v["scenes"][0]["elements"][1]["fill"] = serde_json::json!("#12345");
    assert_eq!(err(&v), DocError::BadColor("#12345".into()));
}

#[test]
fn bad_bg_color() {
    let mut v = example_value();
    v["bg"] = serde_json::json!("junk");
    assert_eq!(err(&v), DocError::BadColor("junk".into()));
}

// ---- keyframe tracks ----

#[test]
fn keys_not_increasing() {
    let mut v = example_value();
    v["scenes"][0]["elements"][2]["animations"][0]["keys"][1]["t"] = serde_json::json!(0);
    assert_eq!(err(&v), DocError::KeysNotIncreasing("opacity".into()));
}

#[test]
fn key_arity_mismatch() {
    let mut v = example_value();
    // Retag the opacity track (Num-valued keys) as translate, which requires Vec2.
    v["scenes"][0]["elements"][2]["animations"][0]["prop"] = serde_json::json!("translate");
    assert_eq!(err(&v), DocError::KeyArity("translate".into()));
}

#[test]
fn empty_track() {
    let mut v = example_value();
    v["scenes"][0]["elements"][2]["animations"][0]["keys"] = serde_json::json!([]);
    assert_eq!(err(&v), DocError::EmptyTrack("opacity".into()));
}

// ---- opacity range ----

#[test]
fn static_opacity_out_of_range() {
    let mut v = example_value();
    v["scenes"][0]["elements"][1]["opacity"] = serde_json::json!(1.5);
    assert_eq!(err(&v), DocError::OpacityRange("1.5".into()));
}

#[test]
fn key_opacity_out_of_range() {
    let mut v = example_value();
    v["scenes"][0]["elements"][2]["animations"][0]["keys"][1]["v"] = serde_json::json!(1.5);
    assert_eq!(err(&v), DocError::OpacityRange("1.5".into()));
}

// ---- path element ----

fn path_doc(path: serde_json::Value) -> Result<Document, DocError> {
    let v = serde_json::json!({
        "v": 1, "timebase": 705600000, "size": { "w": 100, "h": 100 },
        "scenes": [{ "id": "s", "duration": 705600000, "elements": [path] }]
    });
    Document::from_json(&v.to_string())
}

#[test]
fn a_valid_path_is_accepted() {
    // Control for every rejection below: without this, a validator that
    // refused all paths would pass the whole group.
    assert!(path_doc(serde_json::json!({
        "type": "path",
        "points": [[0, 0], [50, 50]],
        "stroke": "#FF9900",
        "strokeWidth": 2
    }))
    .is_ok());
}

#[test]
fn a_path_needs_at_least_two_points() {
    // One point has no segment to stroke and no area to fill; it would
    // render nothing at all.
    for pts in [serde_json::json!([]), serde_json::json!([[10, 10]])] {
        let err = path_doc(serde_json::json!({
            "type": "path", "points": pts, "stroke": "#FFFFFF", "strokeWidth": 1
        }))
        .expect_err("expected rejection");
        assert!(matches!(err, DocError::PathTooFewPoints(_)), "got {err:?}");
    }
}

#[test]
fn a_path_with_neither_stroke_nor_fill_is_rejected() {
    // It would parse, validate, render nothing, and look like a renderer
    // bug rather than an authoring mistake.
    let err = path_doc(serde_json::json!({
        "type": "path", "points": [[0, 0], [10, 10]]
    }))
    .expect_err("expected rejection");
    assert!(matches!(err, DocError::PathNotPainted), "got {err:?}");
}

#[test]
fn a_path_stroke_width_must_be_positive() {
    for w in [0, -3] {
        let err = path_doc(serde_json::json!({
            "type": "path", "points": [[0, 0], [10, 10]],
            "stroke": "#FFFFFF", "strokeWidth": w
        }))
        .expect_err("expected rejection");
        assert!(matches!(err, DocError::PathStrokeWidth(_)), "got {err:?}");
    }
}

#[test]
fn a_path_with_a_bad_colour_is_rejected() {
    for key in ["stroke", "fill"] {
        let mut el = serde_json::json!({
            "type": "path", "points": [[0, 0], [10, 10]],
            "stroke": "#FFFFFF", "strokeWidth": 1
        });
        el[key] = serde_json::json!("not-a-colour");
        let err = path_doc(el).expect_err("expected rejection");
        assert!(matches!(err, DocError::BadColor(_)), "{key}: got {err:?}");
    }
}

#[test]
fn a_path_may_be_fill_only() {
    // A closed filled shape with no stroke is the arrowhead case, and must
    // not be caught by the not-painted rule.
    assert!(path_doc(serde_json::json!({
        "type": "path",
        "points": [[0, 0], [10, 5], [0, 10]],
        "closed": true,
        "fill": "#FF9900"
    }))
    .is_ok());
}

// ---- gradients ----

fn grad_doc(fill: serde_json::Value) -> Result<Document, DocError> {
    let v = serde_json::json!({
        "v": 1, "timebase": 705600000, "size": { "w": 100, "h": 100 },
        "scenes": [{ "id": "s", "duration": 705600000, "elements": [
            { "type": "rect", "rect": [0, 0, 100, 100], "fill": fill }
        ]}]
    });
    Document::from_json(&v.to_string())
}

fn linear(stops: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "type": "linear", "from": [0, 0], "to": [1, 1], "stops": stops })
}

#[test]
fn a_valid_gradient_is_accepted() {
    // Control: without it, a validator rejecting all gradients would pass
    // every rejection test below.
    assert!(grad_doc(linear(serde_json::json!([
        { "at": 0, "color": "#FF9900" }, { "at": 1, "color": "#4ECDC4" }
    ])))
    .is_ok());
}

#[test]
fn a_gradient_needs_at_least_two_stops() {
    let err = grad_doc(linear(serde_json::json!([{ "at": 0, "color": "#FF9900" }])))
        .expect_err("expected rejection");
    assert!(matches!(err, DocError::GradientStops(1)), "got {err:?}");
}

#[test]
fn gradient_stops_must_increase() {
    // Out of order renders something the author did not describe, so it is
    // rejected rather than sorted behind their back.
    let err = grad_doc(linear(serde_json::json!([
        { "at": 0.8, "color": "#FF9900" }, { "at": 0.2, "color": "#4ECDC4" }
    ])))
    .expect_err("expected rejection");
    assert!(matches!(err, DocError::GradientStopOrder(_)), "got {err:?}");
}

#[test]
fn gradient_stops_must_lie_between_zero_and_one() {
    for bad in [-0.1, 1.5] {
        let err = grad_doc(linear(serde_json::json!([
            { "at": bad, "color": "#FF9900" }, { "at": 1, "color": "#4ECDC4" }
        ])))
        .expect_err("expected rejection");
        assert!(matches!(err, DocError::GradientStopOrder(_)), "got {err:?}");
    }
}

#[test]
fn a_radial_gradient_needs_a_positive_radius() {
    for r in [0, -1] {
        let err = grad_doc(serde_json::json!({
            "type": "radial", "center": [0.5, 0.5], "radius": r,
            "stops": [{ "at": 0, "color": "#FFFFFF" }, { "at": 1, "color": "#000000" }]
        }))
        .expect_err("expected rejection");
        assert!(matches!(err, DocError::GradientRadius(_)), "got {err:?}");
    }
}

#[test]
fn a_gradient_stop_colour_is_validated() {
    let err = grad_doc(linear(serde_json::json!([
        { "at": 0, "color": "not-a-colour" }, { "at": 1, "color": "#4ECDC4" }
    ])))
    .expect_err("expected rejection");
    assert!(matches!(err, DocError::BadColor(_)), "got {err:?}");
}

#[test]
fn an_unknown_key_inside_a_gradient_is_rejected() {
    // serde ignores unknown keys in the gradient object, so without the walk
    // a typo'd axis would be silently dropped and render as a default.
    let err = grad_doc(serde_json::json!({
        "type": "linear", "from": [0, 0], "two": [1, 1],
        "stops": [{ "at": 0, "color": "#FF9900" }, { "at": 1, "color": "#4ECDC4" }]
    }))
    .expect_err("expected rejection");
    assert!(matches!(err, DocError::UnknownField { .. }), "got {err:?}");
}

#[test]
fn an_unknown_key_inside_a_gradient_stop_is_rejected() {
    let err = grad_doc(linear(serde_json::json!([
        { "at": 0, "color": "#FF9900", "colour": "#fff" },
        { "at": 1, "color": "#4ECDC4" }
    ])))
    .expect_err("expected rejection");
    assert!(matches!(err, DocError::UnknownField { .. }), "got {err:?}");
}
