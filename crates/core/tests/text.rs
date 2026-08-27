mod common;

use zoetrope_core::doc::{Align, Asset};
use zoetrope_core::{layout_text, resolve_reserved_src, AssetStore, Document};

/// Build an `AssetStore` with Inter loaded under asset id `"body"`, and
/// return the store alongside the resolved family name for that asset.
fn inter_store() -> (AssetStore, String) {
    let mut d = Document::new(64, 64);
    d.add_asset("body", Asset::font("zoetrope:inter"));
    let mut store = AssetStore::new();
    store.add_bytes(
        "body",
        resolve_reserved_src("zoetrope:inter").unwrap().to_vec(),
    );
    store.prepare(&d).unwrap();
    let family = store.family("body").to_string();
    (store, family)
}

#[derive(serde::Serialize)]
struct GoldenSummary {
    width: f32,
    height: f32,
    glyph_count: usize,
    first_x: i32,
    first_y: i32,
    last_x: i32,
    last_y: i32,
}

#[test]
fn wraps_and_matches_golden() {
    let (mut store, family) = inter_store();
    let layout = layout_text(
        store.font_system(),
        &family,
        "Deterministic video, twice.",
        24.0,
        Some(200.0),
        Align::Left,
    );

    assert!(
        layout.glyphs.len() >= 2,
        "expected at least first/last glyphs"
    );
    let first = layout.glyphs.first().unwrap();
    let last = layout.glyphs.last().unwrap();
    let summary = GoldenSummary {
        width: layout.width,
        height: layout.height,
        glyph_count: layout.glyphs.len(),
        first_x: first.x,
        first_y: first.y,
        last_x: last.x,
        last_y: last.y,
    };
    let actual = serde_json::to_string(&summary).unwrap();
    common::assert_golden("testdata/golden/text-layout.json", actual.as_bytes());

    // Wrapping at max_w: 200.0 must actually force more than one line.
    assert!(layout.height > 24.0 * 1.3, "expected a forced wrap");
}

#[test]
fn center_align_shifts_first_glyph_right_of_left_align() {
    let (mut store, family) = inter_store();
    let text = "Hi";
    let left = layout_text(
        store.font_system(),
        &family,
        text,
        24.0,
        Some(200.0),
        Align::Left,
    );
    let center = layout_text(
        store.font_system(),
        &family,
        text,
        24.0,
        Some(200.0),
        Align::Center,
    );

    let left_x = left.glyphs.first().unwrap().x;
    let center_x = center.glyphs.first().unwrap().x;
    assert!(
        center_x > left_x,
        "center align ({center_x}) should shift the first glyph right of left align ({left_x})"
    );
}

#[test]
fn no_max_w_is_single_line_with_locked_line_height() {
    let (mut store, family) = inter_store();
    let layout = layout_text(
        store.font_system(),
        &family,
        "Deterministic video, twice.",
        24.0,
        None,
        Align::Left,
    );

    assert_eq!(layout.height, 24.0_f32 * 1.3);
}
