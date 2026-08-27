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
fn renders_an_mp4_with_preview_frames() {
    if !zoetrope_core::export::ffmpeg_available() {
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
