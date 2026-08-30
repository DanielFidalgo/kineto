//! `reveal` expansion.
//!
//! The expansion tests check the emitted JSON, but JSON that looks right and
//! animates nothing would pass all of them. So the last test renders through
//! the real engine and compares pixels: that is the one that would catch a
//! reveal which expands beautifully and does not move.

use kineto::motion::expand;
use serde_json::Value;

/// A document with one centred line, optionally carrying `extra` fields.
fn doc_with(extra: &str) -> String {
    format!(
        r##"{{
      "v": 1, "timebase": 705600000, "defaultFps": 30,
      "size": {{ "w": 200, "h": 100 }}, "bg": "#101820",
      "assets": {{ "body": {{ "type": "font", "src": "kineto:inter" }} }},
      "scenes": [{{
        "id": "s", "duration": 705600000,
        "elements": [
          {{ "type": "text", "text": "hello", "font": "body", "sizePx": 28,
             "color": "#F2F5F7", "pos": [20, 40]{extra} }}
        ]
      }}]
    }}"##
    )
}

fn expanded(extra: &str) -> Value {
    let src = doc_with(extra);
    let out = expand(&src).expect("expansion succeeds");
    serde_json::from_str(&out).expect("valid JSON")
}

fn tracks(v: &Value) -> &Vec<Value> {
    v["scenes"][0]["elements"][0]["animations"]
        .as_array()
        .expect("animations array")
}

fn track<'a>(v: &'a Value, prop: &str) -> &'a Value {
    tracks(v)
        .iter()
        .find(|t| t["prop"] == prop)
        .unwrap_or_else(|| panic!("no {prop} track in {:#}", v["scenes"][0]["elements"][0]))
}

#[test]
fn a_document_without_reveal_is_returned_byte_for_byte() {
    // The guarantee that makes this safe to run on every load: a document not
    // using the sugar cannot be perturbed by its existence.
    let src = doc_with("");
    assert_eq!(expand(&src).unwrap(), src);
}

#[test]
fn milliseconds_become_flicks_exactly() {
    // The arithmetic this feature exists to remove from the author.
    let v = expanded(r##", "reveal": { "at": 300, "kind": "fadeIn", "ms": 400 }"##);
    let keys = &track(&v, "opacity")["keys"];
    assert_eq!(keys[0]["t"], 300 * 705_600i64);
    assert_eq!(keys[1]["t"], 700 * 705_600i64);
}

#[test]
fn fade_up_travels_and_lands_where_the_element_was_placed() {
    let v = expanded(r##", "reveal": { "at": 0, "kind": "fadeUp" }"##);
    let keys = &track(&v, "translate")["keys"];
    assert_eq!(keys[0]["v"], serde_json::json!([0.0, 24.0]));
    assert_eq!(keys[1]["v"], serde_json::json!([0.0, 0.0]));
}

#[test]
fn a_reveal_is_relative_to_an_existing_transform() {
    // The element is already offset; the entrance must land on that offset,
    // not on the origin, or adding a reveal silently moves the element.
    let v = expanded(r##", "translate": [10, 5], "reveal": { "at": 0, "kind": "fadeUp" }"##);
    let keys = &track(&v, "translate")["keys"];
    assert_eq!(keys[0]["v"], serde_json::json!([10.0, 29.0]));
    assert_eq!(keys[1]["v"], serde_json::json!([10.0, 5.0]));
}

#[test]
fn opacity_returns_to_the_elements_own_value_not_to_one() {
    let v = expanded(r##", "opacity": 0.5, "reveal": { "at": 0, "kind": "fadeIn" }"##);
    let keys = &track(&v, "opacity")["keys"];
    assert_eq!(keys[0]["v"], 0.0);
    assert_eq!(keys[1]["v"], 0.5);
}

#[test]
fn the_ease_sits_on_the_key_being_entered() {
    // `anim::ease` applies the easing of the key being entered, so an ease on
    // the first key would do nothing at all.
    let v = expanded(r##", "reveal": { "at": 0, "kind": "fadeIn" }"##);
    let keys = &track(&v, "opacity")["keys"];
    assert!(keys[0].get("ease").is_none(), "ease on the wrong key");
    assert_eq!(keys[1]["ease"], "outCubic");
}

#[test]
fn pop_in_scales_and_overshoots() {
    let v = expanded(r##", "reveal": { "at": 0, "kind": "popIn" }"##);
    let keys = &track(&v, "scale")["keys"];
    assert_eq!(keys[0]["v"], 0.8);
    assert_eq!(keys[1]["v"], 1.0);
    assert_eq!(keys[1]["ease"], "outBack");
}

#[test]
fn a_reveal_inside_a_group_is_expanded_too() {
    let src = r##"{
      "v": 1, "timebase": 705600000, "defaultFps": 30,
      "size": { "w": 100, "h": 100 }, "bg": "#000000",
      "assets": {},
      "scenes": [{ "id": "s", "duration": 705600000, "elements": [
        { "type": "group", "origin": [0, 0], "children": [
          { "type": "rect", "rect": [0,0,10,10], "fill": "#ffffff",
            "reveal": { "at": 100, "kind": "fadeIn" } }
        ] }
      ] }]
    }"##;
    let v: Value = serde_json::from_str(&expand(src).unwrap()).unwrap();
    let child = &v["scenes"][0]["elements"][0]["children"][0];
    assert!(child.get("reveal").is_none(), "reveal survived expansion");
    assert_eq!(child["animations"][0]["prop"], "opacity");
}

#[test]
fn an_unknown_kind_is_refused_and_lists_the_real_ones() {
    let err = expand(&doc_with(r##", "reveal": { "at": 0, "kind": "explode" }"##))
        .expect_err("unknown kind must fail");
    let msg = err.to_string();
    assert!(msg.contains("explode"), "{msg}");
    assert!(msg.contains("fadeUp"), "does not say what is valid: {msg}");
}

#[test]
fn a_reveal_fighting_a_hand_written_track_is_refused() {
    // Silently merging produces motion nobody wrote and cannot debug.
    let err = expand(&doc_with(
        r##", "reveal": { "at": 0, "kind": "fadeIn" },
           "animations": [{ "prop": "opacity",
                            "keys": [{"t":0,"v":1},{"t":100,"v":0}] }]"##,
    ))
    .expect_err("conflicting opacity must fail");
    assert!(err.to_string().contains("opacity"), "{}", err);
}

#[test]
fn a_mistyped_field_is_an_error_rather_than_a_silent_default() {
    let err = expand(&doc_with(
        r##", "reveal": { "at": "soon", "kind": "fadeIn" }"##,
    ))
    .expect_err("a string `at` must fail");
    assert!(err.to_string().contains("at"), "{}", err);
}

#[test]
fn the_expansion_actually_animates() {
    // Everything above could pass while the reveal rendered as a no-op. This
    // renders the real engine at three ticks: hidden at the start, mid-fade in
    // between, fully arrived at the end. Comparing frames is what proves the
    // emitted keyframes mean anything.
    let src = doc_with(r##", "reveal": { "at": 0, "kind": "fadeUp", "ms": 400 }"##);
    let expanded = expand(&src).expect("expansion succeeds");

    let doc = kineto_core::doc::Document::from_json(&expanded).expect("valid document");
    let assets =
        kineto::source::resolve_assets(&doc, std::path::Path::new(".")).expect("assets resolve");
    let mut engine = kineto_core::Engine::new(doc, assets).expect("engine");

    let ms = |n: i64| n * 705_600;
    let start = engine.render(ms(0)).to_vec();
    let middle = engine.render(ms(200)).to_vec();
    let arrived = engine.render(ms(400)).to_vec();

    // A blank frame of the same background, for comparison.
    let blank_src = doc_with(r##", "opacity": 0"##);
    let blank_doc = kineto_core::doc::Document::from_json(&blank_src).expect("valid");
    let blank_assets =
        kineto::source::resolve_assets(&blank_doc, std::path::Path::new(".")).expect("assets");
    let mut blank_engine = kineto_core::Engine::new(blank_doc, blank_assets).expect("engine");
    let blank = blank_engine.render(0).to_vec();

    assert_eq!(
        start, blank,
        "the element is visible before its reveal begins"
    );
    assert_ne!(middle, blank, "nothing had appeared mid-reveal");
    assert_ne!(middle, arrived, "the reveal was already finished mid-way");
    assert_ne!(arrived, blank, "nothing is on screen after the reveal");
}
