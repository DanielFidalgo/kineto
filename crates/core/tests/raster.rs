mod common;

use zoetrope_core::doc::{Common, Element};
use zoetrope_core::raster::{element_matrix, BBox, Renderer};
use zoetrope_core::{anim::Resolved, AssetStore};

fn blank_pixmap(w: u32, h: u32, bg: (u8, u8, u8, u8)) -> tiny_skia::Pixmap {
    let mut pm = tiny_skia::Pixmap::new(w, h).unwrap();
    // bg is opaque in both tests below, so straight == premultiplied here.
    pm.fill(tiny_skia::Color::from_rgba8(bg.0, bg.1, bg.2, bg.3));
    pm
}

/// 64x64 canvas, bg #000000 (opaque black); rect [8,8,48,48] fill #FF0000,
/// opacity 0.5. Center pixel (32,32) is deep inside the rect (no AA edge),
/// so its exact premultiplied value can be derived by hand:
///
/// Source-over compositing, in premultiplied space, of straight red
/// (255,0,0) at alpha 0.5 over an opaque black destination (0,0,0,255):
///   src_premul   = (255*0.5, 0*0.5, 0*0.5, 0.5)   = (127.5, 0, 0, 0.5)
///   dst_premul   = (0, 0, 0, 1.0)                  (opaque black)
///   out = src_premul + dst_premul * (1 - src_a)
///       = (127.5 + 0*0.5, 0, 0, 0.5 + 1.0*0.5)
///       = (127.5, 0, 0, 1.0)
/// 127.5 rounds to 128 (tiny-skia's f32->u8 conversion rounds to nearest,
/// and 127.5 is equidistant between 127/128 with .round() going up).
/// Final alpha is fully opaque (255) because the destination was opaque, so
/// green/blue stay 0 and alpha stays 255 throughout.
#[test]
fn rect_fill_and_opacity() {
    let mut pm = blank_pixmap(64, 64, (0, 0, 0, 255));
    let el = Element::Rect {
        rect: [8.0.into(), 8.0.into(), 48.0.into(), 48.0.into()],
        fill: "#FF0000".into(),
        common: Common {
            opacity: Some(0.5.into()),
            ..Common::default()
        },
    };
    let mut renderer = Renderer::new();
    let mut assets = AssetStore::new();
    renderer.draw_elements(&mut pm.as_mut(), &[el], &mut assets, 0, (0.0, 0.0));

    let px = pm.pixel(32, 32).unwrap();
    assert_eq!(px.red(), 128);
    assert_eq!(px.green(), 0);
    assert_eq!(px.blue(), 0);
    assert_eq!(px.alpha(), 255);

    common::assert_golden_hash("raster-rect-opacity", pm.data());
}

/// Same rect, rotated 45deg about its own center. A 45deg-rotated 48x48
/// square's half-diagonal (48/sqrt(2) ≈ 33.9) is larger than half its side
/// (24), so its corners swing further from the box center than the
/// unrotated square's corners do — corner pixel (9,9), just inside the
/// unrotated rect's top-left corner, ends up outside the rotated shape and
/// stays background.
#[test]
fn rect_rotation_45deg() {
    let mut pm = blank_pixmap(64, 64, (0, 0, 0, 255));
    let el = Element::Rect {
        rect: [8.0.into(), 8.0.into(), 48.0.into(), 48.0.into()],
        fill: "#FF0000".into(),
        common: Common {
            rotation: Some(45.0.into()),
            ..Common::default()
        },
    };
    let mut renderer = Renderer::new();
    let mut assets = AssetStore::new();
    renderer.draw_elements(&mut pm.as_mut(), &[el], &mut assets, 0, (0.0, 0.0));

    let px = pm.pixel(9, 9).unwrap();
    assert_eq!(
        (px.red(), px.green(), px.blue(), px.alpha()),
        (0, 0, 0, 255)
    );

    common::assert_golden_hash("raster-rect-rotation-45", pm.data());
}

/// `element_matrix` rotates about the box center, not its top-left corner:
/// a 10x10 box's right-mid point (10,5) rotated 90deg about its center
/// (5,5) lands on the box's bottom-mid point (5,10). This is the arbiter
/// for the `post_*` chaining order in `element_matrix` — tiny-skia's
/// `post_*` builders post-concat (apply after what's already composed), so
/// chaining `from_translate(-center) -> post_scale -> post_rotate ->
/// post_translate(center + translate)` applies -center first, center+translate
/// last, matching `M = T(translate)·T(center)·R(rotation)·S(scale)·T(-center)`
/// read right-to-left as applied to a point.
#[test]
fn element_matrix_rotates_about_center() {
    let b = BBox {
        x: 0.0,
        y: 0.0,
        w: 10.0,
        h: 10.0,
    };
    let r = Resolved {
        translate: (0.0, 0.0),
        scale: 1.0,
        rotation: 90.0,
        opacity: 1.0,
    };
    let m = element_matrix(&b, &r);
    let mut p = [tiny_skia::Point::from_xy(10.0, 5.0)];
    m.map_points(&mut p);
    assert!((p[0].x - 5.0).abs() < 1e-4 && (p[0].y - 10.0).abs() < 1e-4);
}
