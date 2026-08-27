use zoetrope_core::{
    doc::{Document, Scene, Transition},
    ms,
    timeline::{layer_at, scene_starts, total_duration},
    Color, Size,
};

fn build_test_doc() -> Document {
    // 3 scenes: durations ms(1000)/ms(1000)/ms(500)
    // ms(200) crossfade into scene 2, cut into scene 3
    Document {
        v: 1,
        timebase: 705_600_000,
        default_fps: None,
        size: Size { w: 1920, h: 1080 },
        bg: Color("#000000".into()),
        assets: Default::default(),
        scenes: vec![
            Scene {
                id: "a".to_string(),
                transition: None,
                duration: ms(1000),
                elements: vec![],
            },
            Scene {
                id: "b".to_string(),
                transition: Some(Transition::Crossfade { duration: ms(200) }),
                duration: ms(1000),
                elements: vec![],
            },
            Scene {
                id: "c".to_string(),
                transition: None,
                duration: ms(500),
                elements: vec![],
            },
        ],
    }
}

#[test]
fn test_scene_starts() {
    let doc = build_test_doc();
    let starts = scene_starts(&doc);
    // start[0] = 0
    // start[1] = 0 + ms(1000) - ms(200) = ms(800)
    // start[2] = ms(800) + ms(1000) - 0 = ms(1800)
    assert_eq!(starts, vec![0, ms(800), ms(1800)]);
}

#[test]
fn test_total_duration() {
    let doc = build_test_doc();
    // total = start.last() + dur.last() = ms(1800) + ms(500) = ms(2300)
    assert_eq!(total_duration(&doc), ms(2300));
}

#[test]
fn test_layers_at_before_start() {
    let doc = build_test_doc();
    assert_eq!(layer_at(&doc, -1), vec![]);
}

#[test]
fn test_layers_at_after_end() {
    let doc = build_test_doc();
    assert_eq!(layer_at(&doc, ms(2300)), vec![]);
}

#[test]
fn test_layers_at_scene_0_only() {
    let doc = build_test_doc();
    let layers = layer_at(&doc, ms(100));
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].scene, 0);
    assert_eq!(layers[0].local, ms(100));
    assert_eq!(layers[0].alpha, 1.0);
}

#[test]
fn test_layers_at_crossfade_midpoint() {
    let doc = build_test_doc();
    // At ms(900), scene 0 is at local ms(900), alpha 1.0
    // Scene 1 starts at ms(800), crossfade duration ms(200)
    // ms(900) is in window [ms(800), ms(1000))
    // alpha = (ms(900) - ms(800)) / ms(200) = ms(100) / ms(200) = 0.5
    // local in scene 1 = ms(900) - ms(800) = ms(100)
    let layers = layer_at(&doc, ms(900));
    assert_eq!(layers.len(), 2);
    assert_eq!(layers[0].scene, 0);
    assert_eq!(layers[0].local, ms(900));
    assert_eq!(layers[0].alpha, 1.0);
    assert_eq!(layers[1].scene, 1);
    assert_eq!(layers[1].local, ms(100));
    assert_eq!((layers[1].alpha * 10.0).round(), 5.0); // 0.5 with floating point tolerance
}

#[test]
fn test_layers_at_crossfade_start() {
    let doc = build_test_doc();
    // At ms(800), crossfade just starts
    // alpha = (ms(800) - ms(800)) / ms(200) = 0.0
    let layers = layer_at(&doc, ms(800));
    assert_eq!(layers.len(), 2);
    assert_eq!(layers[0].scene, 0);
    assert_eq!(layers[0].alpha, 1.0);
    assert_eq!(layers[1].scene, 1);
    assert_eq!(layers[1].local, 0);
    assert_eq!(layers[1].alpha, 0.0);
}

#[test]
fn test_layers_at_crossfade_end() {
    let doc = build_test_doc();
    // At ms(1000), crossfade window [ms(800), ms(1000)) ends
    // Scene 1 should be the only layer now
    let layers = layer_at(&doc, ms(1000));
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].scene, 1);
    assert_eq!(layers[0].local, ms(200));
    assert_eq!(layers[0].alpha, 1.0);
}

#[test]
fn test_layers_at_scene_1_middle() {
    let doc = build_test_doc();
    // At ms(1200), scene 1 is in middle, scene 2 hasn't started yet
    let layers = layer_at(&doc, ms(1200));
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].scene, 1);
    assert_eq!(layers[0].local, ms(400));
    assert_eq!(layers[0].alpha, 1.0);
}

#[test]
fn test_layers_at_scene_2_only() {
    let doc = build_test_doc();
    // At ms(1900), scene 2 is the only layer
    // Scene 2 starts at ms(1800), so local = ms(1900) - ms(1800) = ms(100)
    let layers = layer_at(&doc, ms(1900));
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].scene, 2);
    assert_eq!(layers[0].local, ms(100));
    assert_eq!(layers[0].alpha, 1.0);
}
