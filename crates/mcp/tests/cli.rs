//! The `kineto` binary — the command the project's own claim implies.
//!
//! Driven as a subprocess rather than by calling `run()`, because the thing
//! being tested is the contract a user meets: arguments, exit codes and what
//! lands on disk.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    // Cargo builds bins of the crate under test next to its test binaries.
    let mut p = std::env::current_exe().expect("test exe path");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("kineto")
}

fn write(dir: &std::path::Path, name: &str, json: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, json).unwrap();
    p
}

/// A legible document: one readable line, well inside the canvas.
fn good() -> String {
    serde_json::json!({
        "v": 1, "timebase": 705600000, "defaultFps": 30,
        "size": { "w": 320, "h": 180 }, "bg": "#0D1419",
        "assets": { "body": { "type": "font", "src": "kineto:inter" } },
        "scenes": [{
            "id": "s", "duration": 1411200000,
            "elements": [
                { "type": "rect", "rect": [0, 120, 320, 6], "fill": "#FF9900" },
                { "type": "text", "text": "hello", "font": "body", "sizePx": 28,
                  "color": "#F2F5F7", "pos": [20, 40] }
            ]
        }]
    })
    .to_string()
}

/// Structurally perfect, visually invisible: #131b24 on #101820.
fn invisible() -> String {
    good()
        .replace("#F2F5F7", "#131b24")
        .replace("#0D1419", "#101820")
}

#[test]
fn help_succeeds_and_names_both_formats() {
    let out = Command::new(bin()).arg("--help").output().expect("run");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains(".mp4"), "{text}");
    assert!(text.contains(".webp"), "{text}");
}

#[test]
fn check_passes_a_clean_document_without_writing_anything() {
    let dir = tempfile::tempdir().unwrap();
    let doc = write(dir.path(), "d.json", &good());
    let out = Command::new(bin())
        .arg(&doc)
        .arg("--check")
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        1,
        "wrote a file"
    );
}

#[test]
fn check_fails_on_a_correctness_defect_and_names_it() {
    // The whole point of the flag: catch what is invisible in the JSON and
    // obvious on screen, before anything is rendered.
    let dir = tempfile::tempdir().unwrap();
    let doc = write(dir.path(), "d.json", &invisible());
    let out = Command::new(bin())
        .arg(&doc)
        .arg("--check")
        .output()
        .expect("run");
    assert!(!out.status.success(), "invisible text should fail --check");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("lowContrast"), "{err}");
}

#[test]
fn rendering_without_an_output_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let doc = write(dir.path(), "d.json", &good());
    let out = Command::new(bin()).arg(&doc).output().expect("run");
    assert!(!out.status.success());
}

#[test]
fn an_unsupported_extension_is_refused_before_rendering() {
    let dir = tempfile::tempdir().unwrap();
    let doc = write(dir.path(), "d.json", &good());
    let gif = dir.path().join("out.gif");
    let out = Command::new(bin())
        .arg(&doc)
        .arg("-o")
        .arg(&gif)
        .output()
        .expect("run");
    assert!(!out.status.success());
    assert!(!gif.exists(), "refused, but still wrote something");
}

#[test]
fn it_renders_a_document_to_an_mp4() {
    if !kineto_core::export::ffmpeg_available() {
        eprintln!("skipping: ffmpeg not available");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let doc = write(dir.path(), "d.json", &good());
    let mp4 = dir.path().join("out.mp4");
    let out = Command::new(bin())
        .arg(&doc)
        .arg("-o")
        .arg(&mp4)
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(mp4.exists(), "no output written");
    let data = std::fs::read(&mp4).unwrap();
    assert_eq!(&data[4..8], b"ftyp", "not an MP4");
    // The summary is what tells a caller whether the file is embeddable.
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("MB"), "size not reported: {text}");
}
