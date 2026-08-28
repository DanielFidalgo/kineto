use kineto_core::{anim::*, doc::*};

#[test]
fn easing_formulas() {
    assert_eq!(ease(Ease::Linear, 0.25), 0.25);
    assert_eq!(ease(Ease::InCubic, 0.5), 0.125);
    assert_eq!(ease(Ease::OutCubic, 0.5), 0.875);
    assert_eq!(ease(Ease::InOutCubic, 0.25), 0.0625);
    assert_eq!(ease(Ease::InOutCubic, 0.75), 1.0 - 0.0625);
}

#[test]
fn clamps_and_interpolates() {
    let c = Common {
        animations: vec![Track::new(
            Prop::Opacity,
            vec![Key::num(100, 0.0), Key::num(300, 1.0)],
        )],
        ..Default::default()
    };
    assert_eq!(resolve_common(&c, 0).opacity, 0.0); // before first key
    assert_eq!(resolve_common(&c, 200).opacity, 0.5); // linear midpoint
    assert_eq!(resolve_common(&c, 900).opacity, 1.0); // after last key
    assert_eq!(resolve_common(&c, 200).scale, 1.0); // untouched default
}

#[test]
fn translate_track_is_vec2() {
    let c = Common {
        animations: vec![Track::new(
            Prop::Translate,
            vec![Key::vec2(0, [0.0, 0.0]), Key::vec2(100, [10.0, 20.0])],
        )],
        ..Default::default()
    };
    assert_eq!(resolve_common(&c, 50).translate, (5.0, 10.0));
}

#[test]
fn back_easing_overshoots_its_endpoints() {
    // That is the whole point of it: motion that overshoots and settles reads
    // as alive. If these ever stay inside 0..1 the curve has been flattened.
    let below = (1..50)
        .map(|i| ease(Ease::InBack, i as f64 / 100.0))
        .fold(f64::MAX, f64::min);
    let above = (50..100)
        .map(|i| ease(Ease::OutBack, i as f64 / 100.0))
        .fold(f64::MIN, f64::max);
    assert!(below < 0.0, "InBack never dips below zero: {below}");
    assert!(above > 1.0, "OutBack never rises above one: {above}");
}

#[test]
fn every_easing_starts_at_zero_and_ends_at_one() {
    // Expo is the one at risk: 2^-10 is 0.00098, which leaves a visible seam
    // at a keyframe boundary unless the endpoints are special-cased.
    for e in [
        Ease::Linear,
        Ease::InCubic,
        Ease::OutCubic,
        Ease::InOutCubic,
        Ease::InBack,
        Ease::OutBack,
        Ease::InOutBack,
        Ease::InExpo,
        Ease::OutExpo,
        Ease::InOutExpo,
    ] {
        assert_eq!(ease(e, 0.0), 0.0, "{e:?} does not start at 0");
        assert_eq!(ease(e, 1.0), 1.0, "{e:?} does not end at 1");
    }
}

#[test]
fn an_overshooting_opacity_track_is_clamped() {
    // Opacity is the one property that cannot overshoot: it is an alpha, and
    // tiny-skia is handed it directly as a PixmapPaint opacity. Geometry may
    // overshoot freely; this may not.
    use kineto_core::doc::{Common, Key, Prop, Track, TIMEBASE};
    let common = Common {
        animations: vec![Track::new(
            Prop::Opacity,
            vec![
                Key::num(0, 0.0),
                Key::num(TIMEBASE, 1.0).with_ease(Ease::OutBack),
            ],
        )],
        ..Common::default()
    };
    for i in 0..=100 {
        let t = TIMEBASE * i / 100;
        let o = resolve_common(&common, t).opacity;
        assert!(
            (0.0..=1.0).contains(&o),
            "opacity {o} out of range at t={t}"
        );
    }
}
