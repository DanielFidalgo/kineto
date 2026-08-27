mod common;

use zoetrope_core::doc::{Align, Asset, Common, Element};
use zoetrope_core::raster::{element_matrix, BBox, Renderer};
use zoetrope_core::{anim::Resolved, resolve_reserved_src, AssetStore, Document};

/// Build an `AssetStore` with Inter loaded under asset id `"body"`, prepared
/// against a `w x h` document (mirrors `tests/text.rs`'s `inter_store`).
fn inter_store(w: u32, h: u32) -> AssetStore {
    let mut doc = Document::new(w, h);
    doc.add_asset("body", Asset::font("zoetrope:inter"));
    let mut assets = AssetStore::new();
    assets.add_bytes(
        "body",
        resolve_reserved_src("zoetrope:inter").unwrap().to_vec(),
    );
    assets.prepare(&doc).unwrap();
    assets
}

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

    common::assert_golden_hash("raster-rect-opacity", pm.width(), pm.height(), pm.data());
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

    common::assert_golden_hash(
        "raster-rect-rotation-45",
        pm.width(),
        pm.height(),
        pm.data(),
    );
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

/// Draw grad.png (8×8) stretched into rect [8,8,32,32] on 64×64 canvas.
/// grad.png pixel (x,y) = (x*32, y*32, 128, 255) and is fully opaque.
/// Probe pixel (10,10): should NOT be background (0,0,0,255) since it falls
/// inside the stretched image rect [8,8,32,32].
#[test]
fn image_stretch() {
    let mut pm = blank_pixmap(64, 64, (0, 0, 0, 255));
    let el = Element::image("grad", [8.0, 8.0, 32.0, 32.0]);
    let mut renderer = Renderer::new();
    let mut assets = AssetStore::new();
    assets.add_bytes(
        "grad",
        std::fs::read(common::repo("testdata/assets/grad.png")).unwrap(),
    );
    let mut doc = zoetrope_core::Document::new(64, 64);
    doc.add_asset("grad", zoetrope_core::doc::Asset::image("grad.png"));
    assets.prepare(&doc).unwrap();

    renderer.draw_elements(&mut pm.as_mut(), &[el], &mut assets, 0, (0.0, 0.0));

    let px = pm.pixel(10, 10).unwrap();
    // Pixel (10,10) should not be background; it falls within the stretched image rect.
    assert_ne!(
        (px.red(), px.green(), px.blue(), px.alpha()),
        (0, 0, 0, 255),
        "pixel (10,10) should not be background"
    );

    common::assert_golden_hash("raster-image-stretch", pm.width(), pm.height(), pm.data());
}

/// Draw grad.png stretched into [8,8,32,32], rotated 30deg, opacity 0.5 on 64×64 canvas.
#[test]
fn image_rotation_opacity() {
    let mut pm = blank_pixmap(64, 64, (0, 0, 0, 255));
    let el = Element::image("grad", [8.0, 8.0, 32.0, 32.0])
        .with_rotation(30.0)
        .with_opacity(0.5);
    let mut renderer = Renderer::new();
    let mut assets = AssetStore::new();
    assets.add_bytes(
        "grad",
        std::fs::read(common::repo("testdata/assets/grad.png")).unwrap(),
    );
    let mut doc = zoetrope_core::Document::new(64, 64);
    doc.add_asset("grad", zoetrope_core::doc::Asset::image("grad.png"));
    assets.prepare(&doc).unwrap();

    renderer.draw_elements(&mut pm.as_mut(), &[el], &mut assets, 0, (0.0, 0.0));

    common::assert_golden_hash("raster-image-rot", pm.width(), pm.height(), pm.data());
}

/// "Zoetrope" in Inter 24px white at [8,8] on a 256x64 opaque-black canvas.
/// No pixel-exact hand math here (glyph coverage is font-dependent) — pin
/// exact output via the golden hash, and sanity-check via a coarse probe
/// that a plausible amount of the canvas actually got painted white-ish.
#[test]
fn text_render() {
    let mut pm = blank_pixmap(256, 64, (0, 0, 0, 255));
    let el = Element::text("Zoetrope", "body", 24.0, "#FFFFFF", [8.0, 8.0]);
    let mut renderer = Renderer::new();
    let mut assets = inter_store(256, 64);

    renderer.draw_elements(&mut pm.as_mut(), &[el], &mut assets, 0, (0.0, 0.0));

    let non_bg = pm
        .data()
        .chunks_exact(4)
        .filter(|px| px != &[0u8, 0, 0, 255])
        .count();
    assert!(
        non_bg >= 200,
        "expected at least 200 non-background pixels, got {non_bg}"
    );

    common::assert_golden_hash("raster-text", pm.width(), pm.height(), pm.data());
}

/// Wrapped (`max_w`), center-aligned, 10deg-rotated text on a 256x128
/// opaque-black canvas — exercises the wrap/align path through `layout_text`
/// plus `element_matrix` rotation about the (wrapped) text box's own center.
#[test]
fn text_wrapped_rotated() {
    let mut pm = blank_pixmap(256, 128, (0, 0, 0, 255));
    let el = Element::text(
        "Deterministic video, twice.",
        "body",
        24.0,
        "#FFFFFF",
        [8.0, 8.0],
    )
    .with_max_w(120.0)
    .with_align(Align::Center)
    .with_rotation(10.0);
    let mut renderer = Renderer::new();
    let mut assets = inter_store(256, 128);

    renderer.draw_elements(&mut pm.as_mut(), &[el], &mut assets, 0, (0.0, 0.0));

    common::assert_golden_hash(
        "raster-text-wrapped-rot",
        pm.width(),
        pm.height(),
        pm.data(),
    );
}

/// Regression pin for the alignment-box pivot fix: cosmic-text positions
/// glyphs (per `align`) inside a box of width `max_w` when it's set, not
/// inside a box tightly fit to the ink (`layout.width`). Right-align a
/// short string ("Hi") inside a `max_w` much wider than its own ink, then
/// rotate — before the fix, the rotation/scale pivot (`element_matrix`'s
/// box center) was computed from the ink-only box, so it sat well left of
/// where the glyphs actually render (right-aligned against `max_w`);
/// rotating about the wrong center visibly swings the glyphs to a
/// different place than rotating about the true alignment-box center.
#[test]
fn text_right_aligned_wide_max_w_rotated() {
    let mut pm = blank_pixmap(256, 64, (0, 0, 0, 255));
    let el = Element::text("Hi", "body", 24.0, "#FFFFFF", [8.0, 8.0])
        .with_max_w(200.0)
        .with_align(Align::Right)
        .with_rotation(15.0);
    let mut renderer = Renderer::new();
    let mut assets = inter_store(256, 64);

    renderer.draw_elements(&mut pm.as_mut(), &[el], &mut assets, 0, (0.0, 0.0));

    common::assert_golden_hash("raster-text-right-rot", pm.width(), pm.height(), pm.data());
}

/// Colored, *translucent* text: `#FF880080` (straight RGBA
/// `(255, 136, 0, 128)`) exercises `blit_mask`'s tint-then-premultiply step
/// in a way opaque white text (color alpha 255) cannot. At alpha 255,
/// premultiplying is the identity (`div255(c*255) == c`), so a bug that
/// tinted by the mask *without* premultiplying by the color's own alpha
/// would produce byte-identical output to the correct code for every other
/// test in this file — including `text_render`'s golden. It can't hide
/// here.
///
/// Two elements share one *fully transparent* canvas (so the canvas itself
/// stands in for the isolated per-element "layer" described in
/// `raster.rs`'s `draw_elements` comment — nothing else is drawn to
/// contaminate it):
/// - `probe`: a single large "I" at the element's default opacity (1.0).
///   Big enough (64px) to have an interior run of pixels at full mask
///   coverage (swash mask byte 255) away from any antialiased edge. Hand-
///   probed for exact bytes below.
/// - `pinned`: `"Zoetrope"` at element opacity 0.5, same tint color —
///   exercises the opacity-scaled composite path (tiny-skia's own
///   `PixmapPaint::opacity` / `highp` float raster pipeline) for
///   golden-hash coverage. Deliberately *not* hand-derived byte-for-byte:
///   getting tiny-skia's internal float pipeline bit-exact by hand (u8 ->
///   f32 `/255.0` -> opacity-scale -> `SourceOver` -> `*255.0` -> round)
///   is re-implementing someone else's floating-point code path from
///   memory, not verifying ours — that's what the golden hash is for.
///   `probe`'s opacity-1.0 case is the one that isolates *our* math
///   cleanly (see derivation below) and is worth hand-verifying.
#[test]
fn text_tinted() {
    // Fully transparent background — see the module comment above.
    let mut pm = blank_pixmap(256, 220, (0, 0, 0, 0));
    let probe = Element::text("I", "body", 64.0, "#FF880080", [8.0, 8.0]);
    let pinned =
        Element::text("Zoetrope", "body", 24.0, "#FF880080", [8.0, 120.0]).with_opacity(0.5);
    let mut renderer = Renderer::new();
    let mut assets = inter_store(256, 220);

    renderer.draw_elements(
        &mut pm.as_mut(),
        &[probe, pinned],
        &mut assets,
        0,
        (0.0, 0.0),
    );

    // --- Exact-probe derivation ---
    // color "#FF880080" -> straight RGBA (cr=255, cg=136, cb=0, ca=128).
    // At a full-coverage pixel of `probe`'s "I" (mask byte m=255), inside
    // `blit_mask` (this crate's own code, `crates/core/src/raster.rs`):
    //   src_a = div255(m * ca)     = div255(255*128) = 128
    //   src_r = div255(cr * src_a) = div255(255*128) = 128
    //   src_g = div255(cg * src_a) = div255(136*128) = 68
    //   src_b = div255(cb * src_a) = div255(0)        = 0
    // `over_premul` against a transparent destination (0,0,0,0) is the
    // identity (every `dst * inv` term is `0 * anything = 0`), so
    // `blit_mask` writes exactly (128, 68, 0, 128) into the layer at that
    // pixel.
    // `probe`'s element opacity is 1.0 (default), so tiny-skia's
    // `PixmapPaint::opacity` stage (`Pattern::push_stages`'s
    // `if self.opacity != NormalizedF32::ONE { ... Scale1Float }`) is
    // skipped entirely for it — confirmed by reading
    // `tiny-skia-0.12.0/src/shaders/pattern.rs`. With the canvas itself
    // transparent, `draw_pixmap`'s `SourceOver` composite of that layer
    // pixel onto (0,0,0,0) is then a lossless copy: `mad(dst, inv(a), src)
    // == mad(0.0, _, src) == src` in tiny-skia's `highp` pipeline
    // (`pipeline/highp.rs::source_over_rgba`), and the one remaining
    // round-trip — `u8 -> (as f32 / 255.0) -> (* 255.0) -> round_int()` in
    // `load_8888`/`store_8888` — is exact for every integer 0..=255 (this
    // is *why* `1.0 / 255.0` and not `1.0 / 256.0` is used as the
    // normalization factor throughout tiny-skia: it's the property that
    // makes plain copies bit-exact). So the final canvas byte at that
    // pixel is (128, 68, 0, 128), exactly.
    //
    // Scan for the pixel rather than compute its (x,y) from font metrics
    // (font-hinting-dependent, and not what this test is about): full mask
    // coverage (src_a=128) is strictly higher than anything `pinned` can
    // produce (its src_a is `ca=128` scaled down again by `opacity=0.5`
    // inside tiny-skia's own pipeline, capping it well under 128), so on
    // this deterministic render the single highest-alpha pixel on the
    // whole canvas is unambiguously one of `probe`'s full-coverage pixels.
    let data = pm.data();
    let (mut best_idx, mut best_alpha) = (0usize, 0u8);
    for (i, px) in data.chunks_exact(4).enumerate() {
        if px[3] > best_alpha {
            best_alpha = px[3];
            best_idx = i;
        }
    }
    let probe_px = &data[best_idx * 4..best_idx * 4 + 4];
    assert_eq!(
        probe_px,
        [128u8, 68, 0, 128],
        "full-coverage tinted-glyph pixel should be exactly the hand-derived premultiplied bytes"
    );

    common::assert_golden_hash("raster-text-tinted", pm.width(), pm.height(), pm.data());
}

/// Group at origin `[10,10]` containing two overlapping 50%-opacity red
/// rects, group `opacity: 0.5`, on a 64x64 opaque-black canvas. Proves
/// isolated compositing (spec §3.3): group opacity is applied ONCE to the
/// whole composited subtree, not pushed into each child.
///
/// --- Derivation (same style as `rect_fill_and_opacity` above: tiny-skia's
/// float pipeline is `u8 -> normalized f32 (/255) -> composite -> *255 ->
/// round`, with a *real* intermediate u8-rounding step wherever a `Pixmap`
/// actually stores bytes — the isolation layer is a real `Pixmap`, so that
/// happens once after both children are drawn into it) ---
///
/// Child A: rect local `[0,0,30,30]`, child B: rect local `[10,10,30,30]`,
/// both fill `#FF0000` opacity 0.5. Group origin `[10,10]` shifts both to
/// global A=`[10,10,30,30]`, B=`[20,20,30,30]`; their overlap covers global
/// `(20,20)`-`(40,40)`. Probe `(30,30)` sits >=10px from every edge of both
/// rects (anti-aliasing only touches ~1px at edges), so it's an exact
/// interior pixel for both — no AA blending to account for.
///
/// Each rect's own alpha byte: `round(255 * 0.5) = round(127.5) = 128`
/// (`.round()` rounds half away from zero) — straight `(255,0,0,128)`.
///
/// Isolated path — both children painted onto a transparent layer, full
/// strength (their own opacity only; group opacity NOT applied to them):
///   - A onto transparent `(0,0,0,0)`: source-over of anything over fully
///     transparent is just the src converted to premultiplied, so the
///     layer becomes premultiplied `(128,0,0,128)` exactly
///     (`255 * 128/255 = 128`, no rounding loss).
///   - B (same straight color/alpha) over that: for a *straight-red*
///     pixel, the premultiplied red channel always equals its own alpha,
///     so `out_r = out_a = src_a + dst_a*(1-src_a)
///     = 128/255 + (128/255)*(127/255) = 28544/65025`
///     -> `round(255 * 28544/65025) = round(191.735...) = 192`.
///   - Layer now stores premultiplied `(192,0,0,192)` at the probe pixel.
///
/// Then the WHOLE layer is composited *once* with the group's own opacity
/// (0.5) via `PixmapPaint::opacity` (a uniform scale of the premultiplied
/// pixel before the final source-over onto the opaque black canvas):
///   - scaled = `(192,0,0,192) * 0.5 = (96,0,0,96)` in premultiplied
///     0..255 terms (exact: `192*0.5 = 96`, no rounding).
///   - Over opaque black: `out_r = 96 + 0*(...) = 96`; `out_a` normalizes
///     to fully opaque (255) since the destination was opaque (same
///     reasoning as `rect_fill_and_opacity` above).
///
/// => isolated probe pixel = `(96, 0, 0, 255)`.
///
/// Contrast with what per-child opacity multiplication (pushing the
/// group's 0.5 into each child instead of isolating) would give: each
/// child's effective opacity becomes `0.5*0.5=0.25` (alpha byte
/// `round(255*0.25)=64`), composited directly (no isolation layer) at the
/// same global rects. Same derivation shape as above with `128`
/// replaced by `64` throughout gives `round(255 * (64/255 +
/// (64/255)*(191/255))) = round(255 * 28544/65025) `... — rather than
/// re-deriving by hand a second time, this is asserted directly below by
/// rendering that exact scenario and pinning its value, then asserting it
/// differs from the isolated one.
#[test]
fn group_isolated_opacity() {
    let mut pm = blank_pixmap(64, 64, (0, 0, 0, 255));
    let group = Element::group(
        [10.0, 10.0],
        vec![
            Element::rect([0.0, 0.0, 30.0, 30.0], "#FF0000").with_opacity(0.5),
            Element::rect([10.0, 10.0, 30.0, 30.0], "#FF0000").with_opacity(0.5),
        ],
    )
    .with_opacity(0.5);
    let mut renderer = Renderer::new();
    let mut assets = AssetStore::new();
    renderer.draw_elements(&mut pm.as_mut(), &[group], &mut assets, 0, (0.0, 0.0));

    let px = pm.pixel(30, 30).unwrap();
    assert_eq!(
        (px.red(), px.green(), px.blue(), px.alpha()),
        (96, 0, 0, 255),
        "isolated compositing: group opacity applied once to the whole subtree (hand-derived above)"
    );

    // Isolation probe: same overlap pixel, but rendered as if group
    // opacity had (incorrectly) been pushed into each child instead of
    // applied once to the isolated layer — child opacity 0.5 * group
    // opacity 0.5 = 0.25 each, drawn directly (no isolation layer, no
    // group) at the same global rect positions.
    let mut naive = blank_pixmap(64, 64, (0, 0, 0, 255));
    let naive_children = [
        Element::rect([10.0, 10.0, 30.0, 30.0], "#FF0000").with_opacity(0.25),
        Element::rect([20.0, 20.0, 30.0, 30.0], "#FF0000").with_opacity(0.25),
    ];
    renderer.draw_elements(
        &mut naive.as_mut(),
        &naive_children,
        &mut assets,
        0,
        (0.0, 0.0),
    );
    let naive_px = naive.pixel(30, 30).unwrap();
    assert_eq!(
        (
            naive_px.red(),
            naive_px.green(),
            naive_px.blue(),
            naive_px.alpha()
        ),
        (112, 0, 0, 255),
        "sanity pin: per-child opacity multiplication would double-fade the overlap"
    );
    assert_ne!(
        (px.red(), px.green(), px.blue(), px.alpha()),
        (
            naive_px.red(),
            naive_px.green(),
            naive_px.blue(),
            naive_px.alpha()
        ),
        "isolated group compositing must NOT match per-child opacity multiplication"
    );

    common::assert_golden_hash("raster-group-isolated", pm.width(), pm.height(), pm.data());
}

/// Nested groups: outer group (origin `[5,5]`, no own transform) wraps an
/// inner group (origin `[10,10]`, rotated 30deg) wraps a single rect
/// (local `[0,0,20,20]`, opaque green). Exercises the recursive
/// `base_bbox` union math for a group-of-a-group (Task 8), and that a
/// group's own `element_matrix` (here the inner one's rotation) pivots
/// about the *group*'s own bbox center — the union of its children's
/// static boxes, shifted by its own origin and the accumulated parent
/// offset — not any single child's box.
#[test]
fn group_nested_rotation() {
    let mut pm = blank_pixmap(64, 64, (0, 0, 0, 255));
    let inner = Element::group(
        [10.0, 10.0],
        vec![Element::rect([0.0, 0.0, 20.0, 20.0], "#00FF00")],
    )
    .with_rotation(30.0);
    let outer = Element::group([5.0, 5.0], vec![inner]);
    let mut renderer = Renderer::new();
    let mut assets = AssetStore::new();
    renderer.draw_elements(&mut pm.as_mut(), &[outer], &mut assets, 0, (0.0, 0.0));

    let non_bg = pm
        .data()
        .chunks_exact(4)
        .filter(|px| px != &[0u8, 0, 0, 255])
        .count();
    assert!(
        non_bg > 0,
        "expected the nested, rotated rect to paint something onto the canvas"
    );

    common::assert_golden_hash("raster-group-nested", pm.width(), pm.height(), pm.data());
}
