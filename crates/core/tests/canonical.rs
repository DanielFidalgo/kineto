mod common;
use zoetrope_core::*;

/// Builds the spec §3.6 example via the Rust builder surface.
/// Task 18's TS golden test builds the SAME doc and byte-compares
/// against testdata/canonical/example.json.
fn example_doc() -> Document {
    let mut d = Document::new(1280, 800).with_fps(30);
    d.add_asset("f01", Asset::image("step-01.jpg"));
    d.add_asset("f02", Asset::image("step-02.jpg"));
    d.add_asset("mono", Asset::font("JetBrainsMono-Regular.ttf"));
    d.push_scene(
        Scene::new("step-1", seconds(0.9))
            .with_element(Element::image("f01", [0.0, 0.0, 1280.0, 800.0]))
            .with_element(Element::rect([0.0, 740.0, 1280.0, 60.0], "#0A0A0AE6"))
            .with_element(
                Element::text(
                    "Landing on the page. Big cookie banner, ominous.",
                    "mono",
                    24.0,
                    "#D4D4D4",
                    [40.0, 756.0],
                )
                .with_max_w(1200.0)
                .with_animation(Track::new(
                    Prop::Opacity,
                    vec![
                        Key::num(0, 0.0),
                        Key::num(ms(200), 1.0).with_ease(Ease::OutCubic),
                    ],
                )),
            ),
    );
    d.push_scene(
        Scene::new("step-2", seconds(0.9))
            .with_transition(Transition::crossfade(ms(150)))
            .with_element(Element::image("f02", [0.0, 0.0, 1280.0, 800.0])),
    );
    d
}

#[test]
fn canonical_bytes_match_golden() {
    common::assert_golden(
        "testdata/canonical/example.json",
        example_doc().canonical_json().as_bytes(),
    );
}

#[test]
#[allow(clippy::bool_comparison)]
fn canonical_omits_defaults_and_ints() {
    let j = example_doc().canonical_json();
    assert!(!j.contains("\"bg\"")); // #000000 default omitted
    assert!(!j.contains("\"translate\"")); // untouched common fields omitted
    assert!(j.contains("\"duration\":635040000")); // seconds(0.9) exact
    assert!(j.contains("\"scale\"") == false);
    assert!(j.contains("1280")); // Scalar 1280.0 → integer, no ".0"
    assert!(!j.contains("1280.0"));
}

#[test]
fn roundtrip_via_serde() {
    let d = example_doc();
    let d2: Document = serde_json::from_str(&d.canonical_json()).unwrap();
    assert_eq!(d.canonical_json(), d2.canonical_json());
}

#[test]
fn time_sugar_is_exact() {
    assert_eq!(seconds(0.9), 635_040_000);
    assert_eq!(ms(150), 105_840_000);
    assert_eq!(frames(27, 30), 27 * 23_520_000);
}
