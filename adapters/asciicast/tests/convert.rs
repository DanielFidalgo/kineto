mod common;

use zoetrope_asciicast::{cast_to_document, parse_cast, Theme};
use zoetrope_core::{seconds, Document};

const FIXTURE: &str = include_str!("fixture.cast");

fn convert() -> (Document, Vec<(String, &'static [u8])>) {
    let cast = parse_cast(FIXTURE).expect("fixture should parse");
    cast_to_document(&cast, &Theme::default())
}

#[test]
fn produces_a_document_that_validates() {
    let (doc, _assets) = convert();
    Document::from_json(&doc.canonical_json()).expect("converted document should validate");
}

#[test]
fn one_scene_per_grid_state() {
    let (doc, _assets) = convert();
    // Task 21's fixture coalesces 5 raw events into 4 distinct grid states.
    assert_eq!(doc.scenes.len(), 4);
}

#[test]
fn first_scene_duration_matches_grid_state_gap() {
    let (doc, _assets) = convert();
    // states[0].time_s == 0.0, states[1].time_s == 0.5.
    assert_eq!(doc.scenes[0].duration, seconds(0.5));
}

#[test]
fn doc_size_is_even_by_even() {
    let (doc, _assets) = convert();
    // cols=20, rows=4, Theme::default(): w = ceil(20*12+32) = 272,
    // h = ceil(4*26+32) = 136 -- both already even.
    assert_eq!(doc.size.w, 272);
    assert_eq!(doc.size.h, 136);
    assert_eq!(doc.size.w % 2, 0, "width must be even (yuv420p)");
    assert_eq!(doc.size.h % 2, 0, "height must be even (yuv420p)");
}

#[test]
fn contains_the_green_ok_run() {
    let (doc, _assets) = convert();
    let json = doc.canonical_json();
    assert!(
        json.contains("#4EBF22"),
        "expected the green 'OK' fg color in the converted document"
    );
}

#[test]
fn stages_the_bundled_term_font_asset() {
    let (_doc, assets) = convert();
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].0, "term");
    assert!(!assets[0].1.is_empty());
}

#[test]
fn matches_canonical_golden() {
    let (doc, _assets) = convert();
    common::assert_golden(
        "testdata/golden/asciicast-fixture.json",
        doc.canonical_json().as_bytes(),
    );
}
