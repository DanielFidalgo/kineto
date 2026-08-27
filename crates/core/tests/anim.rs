use zoetrope_core::{anim::*, doc::*};

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
