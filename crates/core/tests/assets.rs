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
