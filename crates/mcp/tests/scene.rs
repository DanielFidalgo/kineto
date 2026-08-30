//! The themed scene vocabulary.
//!
//! The load-bearing test here is `every_kind_passes_our_own_linter`. A
//! vocabulary whose whole justification is "the output looks composed" has to
//! be held to the standard the project already ships: `check_document`
//! catches unreadable sizes, colours without contrast, elements off canvas and
//! text that overflows. Emitting a document our own linter complains about
//! would be worse than emitting nothing.

use kineto::{check, scene, source, theme};

fn spec(kind: &str) -> scene::SceneSpec {
    scene::SceneSpec {
        kind: kind.to_string(),
        text: Some("Video as a build artifact".into()),
        subtitle: Some("Deterministic, no browser, no display".into()),
        heading: Some("What changed in this release".into()),
        items: vec![
            "reveal — entrance motion in one line".into(),
            "provenance on every published package".into(),
            "npx kineto-mcp, nothing to install".into(),
        ],
        attribution: Some("the README".into()),
        seconds: None,
    }
}

fn document(kind: &str, theme_name: &str, w: u32, h: u32) -> String {
    scene::build_document(theme_name, w, h, &[spec(kind)]).expect("builds")
}

/// Loads through the real path, so `reveal` is expanded exactly as it would be
/// for a rendered document.
fn load(json: &str) -> kineto_core::doc::Document {
    let (doc, _) = source::load_document(Some(json), None).expect("document loads");
    doc
}

#[test]
fn every_kind_passes_our_own_linter() {
    for kind in scene::KINDS {
        for theme_name in theme::NAMES {
            // 1280x720 is the smallest size anyone would plausibly use, and
            // therefore where a type scale expressed as ratios is most likely
            // to fall below the legibility floor.
            for (w, h) in [(1920u32, 1080u32), (1280, 720)] {
                let json = document(kind, theme_name, w, h);
                let doc = load(&json);
                let mut assets =
                    source::resolve_assets(&doc, std::path::Path::new(".")).expect("assets");
                // The contrast and overflow rules measure real shaped text.
                assets.prepare(&doc).expect("assets prepare");

                let mut issues = check::analyze_document(&doc);
                let starts = kineto_core::timeline::scene_starts(&doc);
                for (i, sc) in doc.scenes.iter().enumerate() {
                    // Halfway through, by which point every entrance has
                    // finished — checking at tick 0 would flag elements that
                    // are legitimately still invisible.
                    issues.extend(check::analyze(
                        &doc,
                        &mut assets,
                        starts[i] + sc.duration / 2,
                    ));
                }
                assert!(
                    issues.is_empty(),
                    "{kind}/{theme_name} at {w}x{h} is not clean: {:#?}",
                    issues
                );
            }
        }
    }
}

#[test]
fn the_type_scale_follows_the_canvas() {
    // The point of deriving from the canvas: the same design at two sizes,
    // rather than absolute numbers that only work at one.
    let big = theme::Theme::resolve("midnight", 1920.0, 1080.0).unwrap();
    let small = theme::Theme::resolve("midnight", 960.0, 540.0).unwrap();
    // Sizes are rounded to whole pixels, so doubling the canvas doubles them
    // to within that rounding rather than exactly: 540 * 0.075 rounds to 41,
    // and 41 * 2 is 82 where 1080 * 0.075 is 81.
    assert!((big.title_px - small.title_px * 2.0).abs() <= 1.0);
    assert!((big.margin - small.margin * 2.0).abs() <= 1.0);
    // And the hierarchy holds regardless of size.
    assert!(small.title_px > small.heading_px);
    assert!(small.heading_px > small.body_px);
    assert!(small.body_px > small.caption_px);
}

#[test]
fn points_are_staggered_rather_than_arriving_together() {
    // Simultaneous arrival is a slide. Sequence is what makes it a video, and
    // it is the entire reason this vocabulary emits motion at all.
    let json = document("points", "midnight", 1920, 1080);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let ats: Vec<i64> = v["scenes"][0]["elements"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["reveal"]["at"].as_i64())
        .collect();
    assert!(
        ats.len() >= 4,
        "expected several revealed elements: {ats:?}"
    );
    assert!(
        ats.windows(2).any(|w| w[1] > w[0]),
        "nothing is staggered: {ats:?}"
    );
    assert!(
        ats.iter().max().unwrap() - ats.iter().min().unwrap() >= 200,
        "the stagger is too tight to read as sequence: {ats:?}"
    );
}

#[test]
fn a_long_line_is_wrapped_rather_than_run_off_the_canvas() {
    let long = "a".repeat(400);
    let json = scene::build_document(
        "midnight",
        1920,
        1080,
        &[scene::SceneSpec {
            kind: "points".into(),
            text: None,
            subtitle: None,
            heading: None,
            items: vec![long],
            attribution: None,
            seconds: None,
        }],
    )
    .expect("builds");
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let text_el = v["scenes"][0]["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["type"] == "text")
        .expect("a text element");
    assert!(text_el.get("maxW").is_some(), "no maxW on an overlong line");
}

#[test]
fn the_scenes_actually_animate() {
    // Every assertion above inspects JSON, and would pass against a document
    // whose reveals expanded to nothing.
    let json = document("points", "midnight", 1280, 720);
    let doc = load(&json);
    let assets = source::resolve_assets(&doc, std::path::Path::new(".")).expect("assets");
    let mut engine = kineto_core::Engine::new(doc, assets).expect("engine");

    let ms = |n: i64| n * 705_600;
    let early = engine.render(ms(80)).to_vec();
    let mid = engine.render(ms(400)).to_vec();
    let settled = engine.render(ms(1600)).to_vec();

    assert_ne!(
        early, mid,
        "nothing changed while the entrances were running"
    );
    assert_ne!(mid, settled, "the scene had already settled mid-entrance");
}

#[test]
fn an_unknown_theme_is_refused_and_lists_the_real_ones() {
    let err = scene::build_document("neon", 1920, 1080, &[spec("title")]).expect_err("must fail");
    assert!(err.to_string().contains("midnight"), "{err}");
}

#[test]
fn a_kind_missing_its_content_says_what_it_needs() {
    let empty = scene::SceneSpec {
        kind: "title".into(),
        text: None,
        subtitle: None,
        heading: None,
        items: vec![],
        attribution: None,
        seconds: None,
    };
    let err = scene::build_document("midnight", 1920, 1080, &[empty]).expect_err("must fail");
    assert!(err.to_string().contains("text"), "{err}");
}
