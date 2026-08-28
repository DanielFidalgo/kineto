mod common;

use kineto_core::{doc::Asset, resolve_reserved_src, AssetStore, Document};

/// Regenerates the checked-in image fixtures. Run with
/// `UPDATE_GOLDEN=1 cargo test -p kineto-core --test assets` to rewrite
/// them; otherwise this test is a no-op so the fixtures stay pinned.
#[test]
fn generate_fixtures() {
    if std::env::var("UPDATE_GOLDEN").is_err() {
        return;
    }

    // grad.png: 8x8, pixel (x,y) = (x*32, y*32, 128, 255).
    let mut grad = image::RgbaImage::new(8, 8);
    for y in 0..8u32 {
        for x in 0..8u32 {
            grad.put_pixel(
                x,
                y,
                image::Rgba([(x * 32) as u8, (y * 32) as u8, 128, 255]),
            );
        }
    }
    let grad_path = common::repo("testdata/assets/grad.png");
    std::fs::create_dir_all(grad_path.parent().unwrap()).unwrap();
    grad.save(&grad_path).unwrap();

    // photo.jpg: 16x16 solid (200, 60, 60), quality 90.
    let mut photo = image::RgbImage::new(16, 16);
    for px in photo.pixels_mut() {
        *px = image::Rgb([200, 60, 60]);
    }
    let photo_path = common::repo("testdata/assets/photo.jpg");
    let file = std::fs::File::create(&photo_path).unwrap();
    let mut encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(std::io::BufWriter::new(file), 90);
    encoder
        .encode(
            photo.as_raw(),
            photo.width(),
            photo.height(),
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
}

#[test]
fn decodes_images_and_loads_fonts() {
    let mut d = Document::new(64, 64);
    d.add_asset("g", Asset::image("grad.png"));
    d.add_asset("body", Asset::font("kineto:inter"));
    let mut store = AssetStore::new();
    store.add_bytes(
        "g",
        std::fs::read(common::repo("testdata/assets/grad.png")).unwrap(),
    );
    store.add_bytes(
        "body",
        resolve_reserved_src("kineto:inter").unwrap().to_vec(),
    );
    store.prepare(&d).unwrap();
    let pixmap = store.image("g");
    assert_eq!(pixmap.width(), 8);
    // grad.png pixel (x,y) = (x*32, y*32, 128, 255) — fully opaque, so
    // premultiply is a no-op (tiny-skia's opaque fast path) and the
    // decoded/premultiplied bytes equal the source values exactly.
    let data = pixmap.data();
    let px = |x: u32, y: u32| -> [u8; 4] {
        let i = ((y * pixmap.width() + x) * 4) as usize;
        [data[i], data[i + 1], data[i + 2], data[i + 3]]
    };
    assert_eq!(px(0, 0), [0, 0, 128, 255]); // x*32=0,   y*32=0
    assert_eq!(px(7, 7), [224, 224, 128, 255]); // x*32=224, y*32=224
    assert_eq!(store.family("body"), "Inter");
}

#[test]
fn decodes_jpeg() {
    let mut d = Document::new(64, 64);
    d.add_asset("p", Asset::image("photo.jpg"));
    let mut store = AssetStore::new();
    store.add_bytes(
        "p",
        std::fs::read(common::repo("testdata/assets/photo.jpg")).unwrap(),
    );
    store.prepare(&d).unwrap();
    assert_eq!(store.image("p").width(), 16);
    assert_eq!(store.image("p").height(), 16);
}

#[test]
fn loads_jetbrains_mono() {
    let mut d = Document::new(64, 64);
    d.add_asset("mono", Asset::font("kineto:jetbrains-mono"));
    let mut store = AssetStore::new();
    store.add_bytes(
        "mono",
        resolve_reserved_src("kineto:jetbrains-mono")
            .unwrap()
            .to_vec(),
    );
    store.prepare(&d).unwrap();
    assert_eq!(store.family("mono"), "JetBrains Mono");
}

#[test]
fn missing_asset_bytes_fail() {
    let mut d = Document::new(64, 64);
    d.add_asset("g", Asset::image("grad.png"));
    let mut store = AssetStore::new();
    // No add_bytes call for "g".
    let err = store.prepare(&d).unwrap_err();
    assert_eq!(err, kineto_core::DocError::UnknownAssetId("g".to_string()));
}

#[test]
fn unknown_reserved_src_is_none() {
    assert!(resolve_reserved_src("kineto:nope").is_none());
}

// ---- bounded image residency ----

/// A `w x h` PNG, encoded in memory. Decoded it costs `w*h*4` bytes, which is
/// the number the residency budget is about.
fn png_bytes(w: u32, h: u32, seed: u8) -> Vec<u8> {
    let mut img = image::RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            img.put_pixel(x, y, image::Rgba([(x as u8) ^ seed, y as u8, seed, 255]));
        }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// 20 images of 256x256 (262,144 bytes decoded each, 5.2 MB in total) against
/// a 1 MB budget.
fn many_image_store(budget: usize) -> (Document, AssetStore) {
    let mut doc = Document::new(64, 64);
    let mut store = AssetStore::new();
    store.set_image_budget(budget);
    for i in 0..20u8 {
        let id = format!("img{i}");
        doc.add_asset(&id, Asset::image("generated.png"));
        store.add_bytes(&id, png_bytes(256, 256, i));
    }
    (doc, store)
}

#[test]
fn decoded_images_stay_within_the_residency_budget() {
    // Before this, `prepare` decoded every image and held it forever: 300
    // frames of a 1280x800 tape measured 1185 MB resident, linear in frame
    // count. Nothing in the engine needs more than the frames on screen.
    const BUDGET: usize = 1 << 20;
    let (doc, mut store) = many_image_store(BUDGET);
    store.prepare(&doc).unwrap();
    assert!(
        store.resident_bytes() <= BUDGET,
        "prepare left {} bytes resident, over the {BUDGET} budget",
        store.resident_bytes()
    );

    // Touching every image in turn must not accumulate either.
    for i in 0..20u8 {
        let _ = store.image(&format!("img{i}"));
        assert!(
            store.resident_bytes() <= BUDGET,
            "resident grew to {} while fetching img{i}",
            store.resident_bytes()
        );
    }
}

#[test]
fn a_cached_image_is_actually_retained() {
    // Control for the test above: a store that decoded and immediately threw
    // everything away would satisfy the budget trivially and be useless.
    let (doc, mut store) = many_image_store(1 << 20);
    store.prepare(&doc).unwrap();
    let _ = store.image("img0");
    assert!(
        store.resident_bytes() >= 256 * 256 * 4,
        "the image just fetched is not resident: {}",
        store.resident_bytes()
    );
}

#[test]
fn an_evicted_image_decodes_back_to_the_same_pixels() {
    // Eviction must be invisible in the output. Decode is a pure function of
    // the staged bytes, so a miss has to reproduce the hit exactly — this is
    // what lets the goldens and the parity gate stay untouched.
    let (doc, mut store) = many_image_store(1 << 20);
    store.prepare(&doc).unwrap();

    let before = store.image("img0").data().to_vec();
    for i in 1..20u8 {
        let _ = store.image(&format!("img{i}")); // pushes img0 out
    }
    let after = store.image("img0").data().to_vec();
    assert_eq!(before, after, "re-decoded image differs after eviction");
}

#[test]
fn an_image_larger_than_the_budget_is_still_usable() {
    // A single frame bigger than the whole budget must not be evicted out
    // from under its own caller.
    let mut doc = Document::new(64, 64);
    let mut store = AssetStore::new();
    store.set_image_budget(1024);
    doc.add_asset("big", Asset::image("generated.png"));
    store.add_bytes("big", png_bytes(256, 256, 7));
    store.prepare(&doc).unwrap();
    assert_eq!(store.image("big").width(), 256);
}

#[test]
fn a_corrupt_image_is_still_rejected_at_prepare() {
    // The contract `validateOnly` advertises: every referenced image is read
    // and decoded up front, so a corrupt one is reported before rendering.
    // Lazy decode would quietly defer this to first draw.
    let mut doc = Document::new(64, 64);
    let mut store = AssetStore::new();
    doc.add_asset("bad", Asset::image("broken.png"));
    store.add_bytes("bad", b"not an image at all".to_vec());
    assert!(store.prepare(&doc).is_err());
}
