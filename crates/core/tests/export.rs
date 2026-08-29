mod common;

use kineto_core::doc::{ms, Element, Scene, Transition, TIMEBASE};
use kineto_core::{AssetStore, Document, Engine};

/// 32x32 canvas, opaque-black bg (default). Scene "a" (300ms): full-canvas
/// opaque red rect. Scene "b" (300ms, 200ms crossfade in): full-canvas
/// opaque blue rect. Total duration: 400ms. This is the same doc from render.rs.
fn crossfade_doc() -> Document {
    let mut doc = Document::new(32, 32);
    doc.push_scene(
        Scene::new("a", ms(300)).with_element(Element::rect([0.0, 0.0, 32.0, 32.0], "#FF0000")),
    );
    doc.push_scene(
        Scene::new("b", ms(300))
            .with_transition(Transition::crossfade(ms(200)))
            .with_element(Element::rect([0.0, 0.0, 32.0, 32.0], "#0000FF")),
    );
    doc
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn export_frames_writes_correct_frame_count() {
    let doc = crossfade_doc();
    let mut engine = Engine::new(doc, AssetStore::new()).unwrap();
    let total_duration = engine.total_duration();

    let tempdir = tempfile::tempdir().unwrap();
    let count = kineto_core::export::export_frames(&mut engine, 30, tempdir.path()).unwrap();

    // Expected: ceil(total_duration / (TIMEBASE/30))
    let expected_count = (total_duration + (TIMEBASE / 30) - 1) / (TIMEBASE / 30);
    assert_eq!(
        count as i64, expected_count,
        "frame count should be ceil(total_duration / frame_duration)"
    );
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn export_frames_creates_frame_files() {
    let doc = crossfade_doc();
    let mut engine = Engine::new(doc, AssetStore::new()).unwrap();

    let tempdir = tempfile::tempdir().unwrap();
    let count = kineto_core::export::export_frames(&mut engine, 30, tempdir.path()).unwrap();

    // Check that frame-00000.png exists
    let frame_0 = tempdir.path().join("frame-00000.png");
    assert!(
        frame_0.exists(),
        "frame-00000.png should exist in export directory"
    );

    // Check that all expected frame files exist
    for i in 0..count {
        let frame_path = tempdir.path().join(format!("frame-{:05}.png", i));
        assert!(frame_path.exists(), "frame-{:05}.png should exist", i);
    }
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn exported_frame_has_correct_dimensions() {
    let doc = crossfade_doc();
    let mut engine = Engine::new(doc, AssetStore::new()).unwrap();
    let (width, height) = (engine.width(), engine.height());

    let tempdir = tempfile::tempdir().unwrap();
    kineto_core::export::export_frames(&mut engine, 30, tempdir.path()).unwrap();

    // Decode frame-00000.png and check dimensions
    let frame_0 = tempdir.path().join("frame-00000.png");
    let img = image::open(&frame_0).expect("should be able to open frame-00000.png as image");
    assert_eq!(
        img.width(),
        width,
        "exported frame width should match engine width"
    );
    assert_eq!(
        img.height(),
        height,
        "exported frame height should match engine height"
    );
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn exported_frame_0_has_red_pixel() {
    let doc = crossfade_doc();
    let mut engine = Engine::new(doc, AssetStore::new()).unwrap();

    let tempdir = tempfile::tempdir().unwrap();
    kineto_core::export::export_frames(&mut engine, 30, tempdir.path()).unwrap();

    // Decode frame-00000.png and check a pixel
    let frame_0 = tempdir.path().join("frame-00000.png");
    let img = image::open(&frame_0).expect("should be able to open frame-00000.png as image");
    let rgba = img.to_rgba8();

    // Scene "a" is pure red at tick 0, so pixel at center should be red
    let center_x = 16u32;
    let center_y = 16u32;
    let pixel = rgba.get_pixel(center_x, center_y);
    // RGB should be (255, 0, 0) and alpha 255 (unpremultiplied)
    assert_eq!(pixel[0], 255, "red channel should be 255");
    assert_eq!(pixel[1], 0, "green channel should be 0");
    assert_eq!(pixel[2], 0, "blue channel should be 0");
    assert_eq!(pixel[3], 255, "alpha channel should be 255");
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn ffmpeg_available_returns_bool() {
    // This should not panic
    let _available = kineto_core::export::ffmpeg_available();
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn mux_with_ffmpeg_skips_when_unavailable() {
    let tempdir = tempfile::tempdir().unwrap();
    let frames_dir = tempdir.path().join("frames");
    let out_path = tempdir.path().join("out.mp4");

    std::fs::create_dir(&frames_dir).unwrap();

    if !kineto_core::export::ffmpeg_available() {
        // When ffmpeg is unavailable, mux should return Ok(false)
        let result = kineto_core::export::mux_with_ffmpeg(&frames_dir, 30, &out_path).unwrap();
        assert!(!result, "should return false when ffmpeg unavailable");
    }
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn mux_with_ffmpeg_creates_mp4_when_available() {
    if !kineto_core::export::ffmpeg_available() {
        eprintln!("skipping mux_with_ffmpeg test: ffmpeg not available");
        return;
    }

    let doc = crossfade_doc();
    let mut engine = Engine::new(doc, AssetStore::new()).unwrap();

    let tempdir = tempfile::tempdir().unwrap();
    let frames_dir = tempdir.path().join("frames");
    std::fs::create_dir(&frames_dir).unwrap();

    kineto_core::export::export_frames(&mut engine, 30, &frames_dir).unwrap();

    let out_path = tempdir.path().join("out.mp4");
    let muxed = kineto_core::export::mux_with_ffmpeg(&frames_dir, 30, &out_path).unwrap();
    assert!(muxed, "should return true on successful ffmpeg");

    // Check that the MP4 file was created
    assert!(out_path.exists(), "output MP4 file should exist");

    // Read the first 12 bytes and check for "ftyp" signature
    let file_data = std::fs::read(&out_path).expect("should be able to read MP4 file");
    assert!(
        file_data.len() >= 12,
        "MP4 file should be at least 12 bytes"
    );
    assert_eq!(
        &file_data[4..8],
        b"ftyp",
        "MP4 file should have ftyp signature at offset 4"
    );
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn format_is_chosen_from_the_output_extension() {
    use kineto_core::export::Format;
    use std::path::Path;
    assert_eq!(Format::from_path(Path::new("a/b.mp4")), Some(Format::Mp4));
    assert_eq!(Format::from_path(Path::new("a/b.webp")), Some(Format::WebP));
    // Case-insensitive, because a caller naming a file will not think about it.
    assert_eq!(Format::from_path(Path::new("A.WEBP")), Some(Format::WebP));
    // Unknown is None rather than a default: silently writing an h264 stream
    // into a container the name did not imply is worse than refusing.
    assert_eq!(Format::from_path(Path::new("a/b.gif")), None);
    assert_eq!(Format::from_path(Path::new("noext")), None);
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn an_unsupported_extension_is_an_error_not_a_silent_mp4() {
    if !kineto_core::export::ffmpeg_available() {
        eprintln!("skipping: ffmpeg not available");
        return;
    }
    let tempdir = tempfile::tempdir().unwrap();
    let frames_dir = tempdir.path().join("frames");
    std::fs::create_dir(&frames_dir).unwrap();
    let out = tempdir.path().join("out.gif");
    assert!(kineto_core::export::mux_with_ffmpeg(&frames_dir, 30, &out).is_err());
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn mux_with_ffmpeg_creates_an_animated_webp_when_available() {
    if !kineto_core::export::ffmpeg_available() {
        eprintln!("skipping webp mux test: ffmpeg not available");
        return;
    }

    let doc = crossfade_doc();
    let mut engine = Engine::new(doc, AssetStore::new()).unwrap();

    let tempdir = tempfile::tempdir().unwrap();
    let frames_dir = tempdir.path().join("frames");
    std::fs::create_dir(&frames_dir).unwrap();
    let count = kineto_core::export::export_frames(&mut engine, 30, &frames_dir).unwrap();
    assert!(count > 1, "an animation needs more than one frame");

    let out_path = tempdir.path().join("out.webp");
    let muxed = kineto_core::export::mux_with_ffmpeg(&frames_dir, 30, &out_path).unwrap();
    assert!(muxed, "ffmpeg reported failure encoding webp");
    assert!(out_path.exists(), "output WebP file should exist");

    let data = std::fs::read(&out_path).expect("should be able to read WebP file");
    assert!(data.len() >= 16, "WebP file should be at least 16 bytes");
    // RIFF container with a WEBP fourcc.
    assert_eq!(&data[0..4], b"RIFF", "missing RIFF header");
    assert_eq!(&data[8..12], b"WEBP", "missing WEBP fourcc");
    // ANIM/ANMF chunks are what make it an *animated* WebP rather than a
    // still of the first frame — which is exactly what a wrong encoder or a
    // single-frame input would silently produce.
    let has_anim = data.windows(4).any(|w| w == b"ANIM");
    let has_anmf = data.windows(4).any(|w| w == b"ANMF");
    assert!(
        has_anim && has_anmf,
        "WebP is not animated: no ANIM/ANMF chunk"
    );
}
