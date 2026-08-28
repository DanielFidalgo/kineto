mod harness;

use harness::Server;
use serde_json::json;

/// A 320x180 one-second solid-color document — no assets, renders fast.
fn tiny_doc() -> String {
    json!({
        "v": 1,
        "timebase": 705600000,
        "size": { "w": 320, "h": 180 },
        "scenes": [{
            "id": "s",
            "duration": 705600000,
            "elements": [{
                "type": "rect",
                "rect": [0, 0, 320, 180],
                "fill": "#3366FF"
            }]
        }]
    })
    .to_string()
}

fn call(server: &mut Server, name: &str, args: serde_json::Value) -> serde_json::Value {
    server.request("tools/call", json!({ "name": name, "arguments": args }))
}

#[test]
fn validate_only_returns_metadata_and_no_frames() {
    let mut server = Server::start();
    server.initialize();

    let resp = call(
        &mut server,
        "render_document",
        json!({ "document": tiny_doc(), "validateOnly": true }),
    );

    let result = &resp["result"];
    assert_ne!(result["isError"], json!(true), "unexpected error: {result}");
    let structured = &result["structuredContent"];
    assert_eq!(structured["width"], 320);
    assert_eq!(structured["height"], 180);
    assert_eq!(structured["frameCount"], 30);
    assert_eq!(structured["durationTicks"], 705600000);

    let images = result["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == "image")
        .count();
    assert_eq!(images, 0, "validateOnly must not render previews");
}

#[test]
fn invalid_document_is_a_tool_error_with_a_readable_message() {
    let mut server = Server::start();
    server.initialize();

    // NOTE: the document must be structurally complete. `Document::from_json`
    // runs the unknown-field walk, then the typed decode, and only then
    // `validate_semantics` — which is where the version check lives
    // (crates/core/src/validate.rs:224). A bare `{"v":99}` fails the typed
    // decode on the missing required fields and never reaches it, producing a
    // `DocError::Json` about `timebase` instead.
    let wrong_version = json!({
        "v": 99,
        "timebase": 705600000,
        "size": { "w": 320, "h": 180 },
        "scenes": [{
            "id": "s",
            "duration": 705600000,
            "elements": []
        }]
    })
    .to_string();

    let resp = call(
        &mut server,
        "render_document",
        json!({ "document": wrong_version, "validateOnly": true }),
    );

    let result = &resp["result"];
    assert_eq!(
        result["isError"],
        json!(true),
        "expected a tool error: {result}"
    );
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("unsupported document version"),
        "message was: {text}"
    );
}

#[test]
fn both_document_and_path_is_a_tool_error() {
    let mut server = Server::start();
    server.initialize();

    let resp = call(
        &mut server,
        "render_document",
        json!({
            "document": tiny_doc(),
            "documentPath": "/tmp/whatever.json",
            "validateOnly": true
        }),
    );

    assert_eq!(resp["result"]["isError"], json!(true));
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("not both"), "message was: {text}");
}

#[test]
fn bad_fps_is_a_tool_error_not_a_panic() {
    let mut server = Server::start();
    server.initialize();

    let resp = call(
        &mut server,
        "render_document",
        // 11 has a prime factor TIMEBASE (2^9 * 3^2 * 5^5 * 7^2) lacks.
        // Note 7 IS legal — it divides the timebase twice over.
        json!({ "document": tiny_doc(), "fps": 11, "validateOnly": true }),
    );

    assert_eq!(resp["result"]["isError"], json!(true));

    // The server must still be alive — a panic would have killed the process.
    let alive = call(
        &mut server,
        "render_document",
        json!({ "document": tiny_doc(), "validateOnly": true }),
    );
    assert_ne!(alive["result"]["isError"], json!(true));
}

#[test]
fn absurd_fps_is_rejected_rather_than_counting_frames() {
    let mut server = Server::start();
    server.initialize();

    // The timebase divides itself, so this used to be accepted: it reported
    // 705600000 frames after ~2s of counting, and without `validateOnly`
    // would have tried to write that many PNGs.
    let resp = call(
        &mut server,
        "render_document",
        json!({ "document": tiny_doc(), "fps": 705600000, "validateOnly": true }),
    );

    assert_eq!(resp["result"]["isError"], json!(true), "{}", resp["result"]);
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("1000"), "message was: {text}");
}

#[test]
fn document_default_fps_is_used_when_fps_is_omitted() {
    let mut server = Server::start();
    server.initialize();

    let doc = json!({
        "v": 1,
        "timebase": 705600000,
        "defaultFps": 60,
        "size": { "w": 320, "h": 180 },
        "scenes": [{ "id": "s", "duration": 705600000, "elements": [] }]
    })
    .to_string();

    let resp = call(
        &mut server,
        "render_document",
        json!({ "document": doc, "validateOnly": true }),
    );

    let result = &resp["result"];
    assert_ne!(result["isError"], json!(true), "unexpected error: {result}");
    assert_eq!(result["structuredContent"]["fps"], 60);
    assert_eq!(result["structuredContent"]["frameCount"], 60);
}

#[test]
fn an_explicit_fps_overrides_the_documents_default() {
    let mut server = Server::start();
    server.initialize();

    let doc = json!({
        "v": 1,
        "timebase": 705600000,
        "defaultFps": 60,
        "size": { "w": 320, "h": 180 },
        "scenes": [{ "id": "s", "duration": 705600000, "elements": [] }]
    })
    .to_string();

    let resp = call(
        &mut server,
        "render_document",
        json!({ "document": doc, "fps": 25, "validateOnly": true }),
    );

    let result = &resp["result"];
    assert_ne!(result["isError"], json!(true), "unexpected error: {result}");
    assert_eq!(result["structuredContent"]["fps"], 25);
    assert_eq!(result["structuredContent"]["frameCount"], 25);
}

#[test]
fn an_unusable_document_default_fps_says_where_the_number_came_from() {
    let mut server = Server::start();
    server.initialize();

    // `crates/core` does not validate `defaultFps`, so this parses fine and
    // only fails once we adopt it. 11 has a prime factor the timebase lacks.
    let doc = json!({
        "v": 1,
        "timebase": 705600000,
        "defaultFps": 11,
        "size": { "w": 320, "h": 180 },
        "scenes": [{ "id": "s", "duration": 705600000, "elements": [] }]
    })
    .to_string();

    let resp = call(
        &mut server,
        "render_document",
        json!({ "document": doc, "validateOnly": true }),
    );

    assert_eq!(resp["result"]["isError"], json!(true), "{}", resp["result"]);
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("defaultFps"),
        "the caller sent no `fps`; the message must name the document's own \
         field: {text}"
    );
}

#[test]
fn fps_falls_back_to_30_without_a_document_default() {
    let mut server = Server::start();
    server.initialize();

    let resp = call(
        &mut server,
        "render_document",
        json!({ "document": tiny_doc(), "validateOnly": true }),
    );

    assert_eq!(resp["result"]["structuredContent"]["fps"], 30);
}

#[test]
fn validate_only_omits_the_output_path_entirely() {
    let mut server = Server::start();
    server.initialize();

    let resp = call(
        &mut server,
        "render_document",
        json!({ "document": tiny_doc(), "validateOnly": true }),
    );

    let structured = &resp["result"]["structuredContent"];
    assert!(
        structured.get("out").is_none(),
        "spec §6: validate_only returns no `out`, got {structured}"
    );
}

#[test]
fn an_oversized_canvas_is_a_tool_error_not_an_allocation() {
    let mut server = Server::start();
    server.initialize();

    let doc = json!({
        "v": 1,
        "timebase": 705600000,
        "size": { "w": 40000, "h": 40000 },
        "scenes": [{ "id": "s", "duration": 705600000, "elements": [] }]
    })
    .to_string();

    let resp = call(
        &mut server,
        "render_document",
        json!({ "document": doc, "validateOnly": true }),
    );

    assert_eq!(resp["result"]["isError"], json!(true), "{}", resp["result"]);
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("40000"), "must name the actual size: {text}");
    assert!(text.contains("16384"), "must name the limit: {text}");
}

#[test]
fn storyboard_rejects_an_oversized_canvas() {
    let mut server = Server::start();
    server.initialize();

    let resp = call(
        &mut server,
        "render_storyboard",
        json!({
            "frames": [{ "image": "/nonexistent/a.png", "durationMs": 100 }],
            "width": 40000,
            "height": 40000,
            "validateOnly": true
        }),
    );

    assert_eq!(resp["result"]["isError"], json!(true));
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("16384"), "message was: {text}");
}

#[test]
fn asciicast_rejects_an_oversized_canvas() {
    let mut server = Server::start();
    server.initialize();
    let dir = tempfile::tempdir().unwrap();
    let cast = dir.path().join("wide.cast");
    // Cell metrics turn 4000 columns into a ~48000 px canvas.
    let header = json!({ "version": 2, "width": 4000, "height": 4 });
    std::fs::write(&cast, format!("{header}\n[0.0, \"o\", \"hi\"]\n")).unwrap();

    let resp = call(
        &mut server,
        "render_asciicast",
        json!({ "castPath": cast.to_str().unwrap(), "validateOnly": true }),
    );

    assert_eq!(resp["result"]["isError"], json!(true), "{}", resp["result"]);
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("16384"), "message was: {text}");
}

#[test]
fn storyboard_overflowing_duration_answers_instead_of_hanging() {
    let mut server = Server::start();
    server.initialize();

    // Reproduces the reported hang: this used to panic inside the handler,
    // so the request id was never answered at all. An explicit size means no
    // image is read, so the multiply is reached without touching disk.
    let resp = call(
        &mut server,
        "render_storyboard",
        json!({
            "frames": [{ "image": "/nonexistent/a.png", "durationMs": 10000000000000000i64 }],
            "width": 64,
            "height": 64,
            "validateOnly": true
        }),
    );

    assert_eq!(resp["result"]["isError"], json!(true), "{}", resp["result"]);
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("86400000"), "message was: {text}");

    // And the server is still serving.
    let alive = call(
        &mut server,
        "render_document",
        json!({ "document": tiny_doc(), "validateOnly": true }),
    );
    assert_ne!(alive["result"]["isError"], json!(true));
}

#[test]
fn storyboard_rejects_too_many_frames() {
    let mut server = Server::start();
    server.initialize();

    let frames: Vec<_> = (0..10_001)
        .map(|i| json!({ "image": format!("/nonexistent/{i}.png"), "durationMs": 1 }))
        .collect();

    let resp = call(
        &mut server,
        "render_storyboard",
        json!({ "frames": frames, "width": 64, "height": 64, "validateOnly": true }),
    );

    assert_eq!(resp["result"]["isError"], json!(true));
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("10000"), "message was: {text}");
}

#[test]
fn renders_an_mp4_with_preview_frames() {
    if !kineto_core::export::ffmpeg_available() {
        panic!(
            "ffmpeg is required to run this test; CI installs it (see \
             .github/workflows). Install it locally to run the full suite."
        );
    }

    let mut server = Server::start();
    server.initialize();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.mp4");

    let resp = call(
        &mut server,
        "render_document",
        json!({
            "document": tiny_doc(),
            "out": out.to_str().unwrap(),
            "previewFrames": 3
        }),
    );

    let result = &resp["result"];
    assert_ne!(result["isError"], json!(true), "render failed: {result}");
    assert!(out.exists(), "no MP4 at {}", out.display());
    assert!(std::fs::metadata(&out).unwrap().len() > 0);

    let images: Vec<_> = result["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == "image")
        .collect();
    assert_eq!(images.len(), 3);
    assert_eq!(images[0]["mimeType"], "image/png");
}

/// The smallest valid asciicast v2: a header line then one output event.
fn tiny_cast() -> String {
    let header = json!({ "version": 2, "width": 20, "height": 4 });
    format!("{header}\n[0.0, \"o\", \"hello\"]\n[0.5, \"o\", \" world\"]\n")
}

#[test]
fn asciicast_validates_without_ffmpeg() {
    let mut server = Server::start();
    server.initialize();
    let dir = tempfile::tempdir().unwrap();
    let cast = dir.path().join("demo.cast");
    std::fs::write(&cast, tiny_cast()).unwrap();

    let resp = call(
        &mut server,
        "render_asciicast",
        json!({ "castPath": cast.to_str().unwrap(), "validateOnly": true }),
    );

    let result = &resp["result"];
    assert_ne!(result["isError"], json!(true), "unexpected error: {result}");
    assert!(result["structuredContent"]["frameCount"].as_u64().unwrap() > 0);
}

#[test]
fn asciicast_accepts_theme_overrides() {
    let mut server = Server::start();
    server.initialize();
    let dir = tempfile::tempdir().unwrap();
    let cast = dir.path().join("demo.cast");
    std::fs::write(&cast, tiny_cast()).unwrap();

    let resp = call(
        &mut server,
        "render_asciicast",
        json!({
            "castPath": cast.to_str().unwrap(),
            "validateOnly": true,
            "theme": { "bg": "#101820", "fg": "#F2F2F2", "sizePx": 24 }
        }),
    );

    assert_ne!(resp["result"]["isError"], json!(true), "{}", resp["result"]);
}

#[test]
fn asciicast_theme_override_reaches_the_rendered_pixels() {
    // The smoke test above only asserts the call did not error, which a tool
    // that dropped `theme` entirely would also satisfy. This renders and
    // reads the padding pixel, which is the background and nothing else.
    if !kineto_core::export::ffmpeg_available() {
        panic!(
            "ffmpeg is required to run this test; CI installs it (see \
             .github/workflows). Install it locally to run the full suite."
        );
    }

    let mut server = Server::start();
    server.initialize();
    let dir = tempfile::tempdir().unwrap();
    let cast = dir.path().join("demo.cast");
    std::fs::write(&cast, tiny_cast()).unwrap();

    let bg_of = |server: &mut Server, label: &str, theme: serde_json::Value| -> [u8; 3] {
        let out = dir.path().join(format!("{label}.mp4"));
        let mut args = json!({
            "castPath": cast.to_str().unwrap(),
            "out": out.to_str().unwrap(),
            "previewFrames": 1
        });
        if !theme.is_null() {
            args["theme"] = theme;
        }
        let resp = call(server, "render_asciicast", args);
        let result = &resp["result"];
        assert_ne!(result["isError"], json!(true), "render failed: {result}");

        let b64 = result["content"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["type"] == "image")
            .expect("a preview frame")["data"]
            .as_str()
            .unwrap()
            .to_string();
        let png = base64_decode(&b64);
        let img = image::load_from_memory(&png).unwrap().to_rgba8();
        // (0, 0) is inside the terminal padding: background, never a glyph.
        let px = img.get_pixel(0, 0).0;
        [px[0], px[1], px[2]]
    };

    let default_bg = bg_of(&mut server, "default", serde_json::Value::Null);
    assert_eq!(default_bg, [0x0A, 0x0A, 0x0A], "the adapter's default bg");

    let themed_bg = bg_of(&mut server, "themed", json!({ "bg": "#101820" }));
    assert_eq!(
        themed_bg,
        [0x10, 0x18, 0x20],
        "the `theme.bg` override never reached the renderer"
    );
}

fn base64_decode(s: &str) -> Vec<u8> {
    use base64::prelude::{Engine as _, BASE64_STANDARD};
    BASE64_STANDARD.decode(s).expect("valid base64")
}

#[test]
fn missing_cast_file_names_the_path() {
    let mut server = Server::start();
    server.initialize();

    let resp = call(
        &mut server,
        "render_asciicast",
        json!({ "castPath": "/nonexistent/demo.cast", "validateOnly": true }),
    );

    assert_eq!(resp["result"]["isError"], json!(true));
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("/nonexistent/demo.cast"),
        "message was: {text}"
    );
}

#[test]
fn storyboard_validates_from_image_paths() {
    let mut server = Server::start();
    server.initialize();
    let dir = tempfile::tempdir().unwrap();

    let mut frames = Vec::new();
    for name in ["a.png", "b.png"] {
        let path = dir.path().join(name);
        image::RgbaImage::from_pixel(160, 90, image::Rgba([40, 40, 40, 255]))
            .save(&path)
            .unwrap();
        frames.push(json!({
            "image": path.to_str().unwrap(),
            "durationMs": 500,
            "caption": format!("step {name}")
        }));
    }

    let resp = call(
        &mut server,
        "render_storyboard",
        json!({ "frames": frames, "validateOnly": true }),
    );

    let result = &resp["result"];
    assert_ne!(result["isError"], json!(true), "unexpected error: {result}");
    assert_eq!(result["structuredContent"]["width"], 160);
    // 1000ms total at 30fps
    assert_eq!(result["structuredContent"]["frameCount"], 30);
}

#[test]
fn storyboard_rejects_an_empty_frame_list() {
    let mut server = Server::start();
    server.initialize();

    let resp = call(
        &mut server,
        "render_storyboard",
        json!({ "frames": [], "validateOnly": true }),
    );

    assert_eq!(resp["result"]["isError"], json!(true));
}

#[test]
fn preview_document_returns_an_image_for_each_moment_asked_for() {
    let mut server = Server::start();
    server.initialize();

    let resp = call(
        &mut server,
        "preview_document",
        json!({ "document": tiny_doc(), "atMs": [0, 500, 990] }),
    );

    let result = &resp["result"];
    assert_ne!(result["isError"], json!(true), "unexpected error: {result}");

    let images = result["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["type"] == "image")
        .count();
    assert_eq!(images, 3, "one image per distinct frame");

    let samples = result["structuredContent"]["samples"]
        .as_array()
        .expect("samples array");
    assert_eq!(samples.len(), 3);
    assert_eq!(samples[0]["requestedMs"], 0);
    assert_eq!(samples[0]["frameIndex"], 0);
    // 500ms at 30fps is frame 15; 990ms is frame 29.
    assert_eq!(samples[1]["frameIndex"], 15);
    assert_eq!(samples[2]["frameIndex"], 29);
}

#[test]
fn preview_document_labels_each_image_with_the_moment_it_answers() {
    // Images arrive as an unlabelled sequence, so without this the model has
    // to guess which frame answers which question — and guessing wrong is
    // worse than not looking, because it looks like evidence.
    let mut server = Server::start();
    server.initialize();

    let resp = call(
        &mut server,
        "preview_document",
        json!({ "document": tiny_doc(), "atMs": [0, 500] }),
    );

    let content = resp["result"]["content"].as_array().unwrap();
    let labels: Vec<&str> = content
        .iter()
        .filter(|c| c["type"] == "text")
        .filter_map(|c| c["text"].as_str())
        .collect();

    assert!(
        labels
            .iter()
            .any(|l| l.contains("frame 15") && l.contains("500 ms")),
        "no label ties frame 15 to the 500 ms request: {labels:?}"
    );
}

#[test]
fn preview_document_writes_no_file_and_reports_no_output_path() {
    // The point of the tool: looking is cheap. It must not produce an MP4,
    // and so must not need ffmpeg at all.
    let mut server = Server::start();
    server.initialize();

    let resp = call(
        &mut server,
        "preview_document",
        json!({ "document": tiny_doc(), "atMs": [0] }),
    );

    let structured = &resp["result"]["structuredContent"];
    assert_ne!(resp["result"]["isError"], json!(true));
    assert!(
        structured.get("out").is_none(),
        "preview must not claim an output path: {structured}"
    );
}

#[test]
fn preview_document_reports_a_moment_past_the_end_as_the_last_frame() {
    let mut server = Server::start();
    server.initialize();

    let resp = call(
        &mut server,
        "preview_document",
        json!({ "document": tiny_doc(), "atMs": [9_000] }),
    );

    let samples = resp["result"]["structuredContent"]["samples"]
        .as_array()
        .expect("samples array");
    assert_eq!(samples[0]["requestedMs"], 9_000);
    assert_eq!(samples[0]["frameIndex"], 29);
    assert_eq!(
        samples[0]["actualMs"], 966,
        "the caller must be able to see it did not get 9 seconds"
    );
}

#[test]
fn preview_document_rejects_an_empty_moment_list() {
    let mut server = Server::start();
    server.initialize();

    let resp = call(
        &mut server,
        "preview_document",
        json!({ "document": tiny_doc(), "atMs": [] }),
    );

    assert_eq!(resp["result"]["isError"], json!(true));
}

#[test]
fn preview_document_rejects_a_negative_moment() {
    let mut server = Server::start();
    server.initialize();

    let resp = call(
        &mut server,
        "preview_document",
        json!({ "document": tiny_doc(), "atMs": [-1] }),
    );

    assert_eq!(resp["result"]["isError"], json!(true));
}
