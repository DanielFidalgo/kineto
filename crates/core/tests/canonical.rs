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

#[test]
#[should_panic(expected = "unsupported fps")]
fn frames_panics_on_non_divisor_fps() {
    frames(1, 11);
}

/// Exercises drift-prone serialization surfaces not touched by
/// `example_doc()`: a `group` element, all four `Common` transform fields,
/// a `Vec2`-keyed translate track, non-default `Align` variants, and
/// fractional `Scalar` values (the `serialize_f64` branch).
fn example_full_doc() -> Document {
    let mut d = Document::new(640, 360);
    d.add_asset("f01", Asset::image("frame.png"));
    d.add_asset("mono", Asset::font("JetBrainsMono-Regular.ttf"));
    let group = Element::group(
        [10.0, 20.0],
        vec![
            Element::rect([0.0, 0.0, 100.0, 100.0], "#FFFFFF").with_opacity(0.1),
            Element::text("Centered", "mono", 16.0, "#FFFFFF", [0.0, 0.0])
                .with_align(Align::Center),
            Element::text("Right", "mono", 16.0, "#FFFFFF", [0.0, 0.0]).with_align(Align::Right),
        ],
    )
    .with_translate([10.0, 20.0])
    .with_scale(0.5)
    .with_rotation(12.5)
    .with_opacity(0.5)
    .with_animation(Track::new(
        Prop::Translate,
        vec![
            Key::vec2(0, [0.0, 0.0]),
            Key::vec2(ms(500), [100.0, 50.0]).with_ease(Ease::InOutCubic),
        ],
    ));

    d.push_scene(Scene::new("scene-1", seconds(1.0)).with_element(group));
    d
}

#[test]
fn canonical_bytes_match_golden_full() {
    common::assert_golden(
        "testdata/canonical/example-full.json",
        example_full_doc().canonical_json().as_bytes(),
    );
}

#[test]
fn canonical_full_covers_drift_prone_surfaces() {
    let j = example_full_doc().canonical_json();
    assert!(j.contains("\"scale\":0.5"));
    assert!(j.contains("\"rotation\":12.5"));
    assert!(j.contains("\"opacity\":0.5"));
    assert!(j.contains("\"opacity\":0.1"));
    assert!(j.contains("\"translate\":[10,20]"));
    assert!(j.contains("\"type\":\"group\""));
    assert!(j.contains("\"prop\":\"translate\""));
    assert!(j.contains("\"align\":\"center\""));
    assert!(j.contains("\"align\":\"right\""));
}
