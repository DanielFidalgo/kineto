use std::path::PathBuf;
use std::process::Command;

#[test]
fn test_kineto_cast_success() {
    let out_dir = tempfile::tempdir().expect("create temp dir");
    let out_path = out_dir.path();

    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixture.cast");

    let output = Command::new(env!("CARGO_BIN_EXE_kineto-cast"))
        .arg(&fixture_path)
        .arg("-o")
        .arg(out_path)
        .output()
        .expect("run kineto-cast");

    if !output.status.success() {
        eprintln!(
            "CLI failed: status={}, stdout={}, stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert!(output.status.success(), "CLI should exit 0");

    let frame_path = out_path.join("frame-00000.png");
    assert!(frame_path.exists(), "first frame should exist");
}

#[test]
fn test_kineto_cast_stdout_mentions_frames() {
    let out_dir = tempfile::tempdir().expect("create temp dir");
    let out_path = out_dir.path();

    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixture.cast");

    let output = Command::new(env!("CARGO_BIN_EXE_kineto-cast"))
        .arg(&fixture_path)
        .arg("-o")
        .arg(out_path)
        .output()
        .expect("run kineto-cast");

    assert!(output.status.success(), "CLI should exit 0");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("frames"),
        "stdout should mention frames, got: {}",
        stdout
    );
}

#[test]
fn test_kineto_cast_nonexistent_file() {
    let out_dir = tempfile::tempdir().expect("create temp dir");
    let out_path = out_dir.path();

    let status = Command::new(env!("CARGO_BIN_EXE_kineto-cast"))
        .arg("nonexistent.cast")
        .arg("-o")
        .arg(out_path)
        .status()
        .expect("run kineto-cast");

    assert!(!status.success(), "CLI should exit 1 for missing file");
}

#[test]
fn test_kineto_cast_garbage_input() {
    let out_dir = tempfile::tempdir().expect("create temp dir");
    let out_path = out_dir.path();
    let garbage_file = out_dir.path().join("garbage.cast");
    std::fs::write(&garbage_file, b"this is not valid cast format").expect("write garbage");

    let status = Command::new(env!("CARGO_BIN_EXE_kineto-cast"))
        .arg(&garbage_file)
        .arg("-o")
        .arg(out_path)
        .status()
        .expect("run kineto-cast");

    assert!(!status.success(), "CLI should exit 1 for invalid input");
}

#[test]
fn test_kineto_cast_fps_zero() {
    let out_dir = tempfile::tempdir().expect("create temp dir");
    let out_path = out_dir.path();

    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixture.cast");

    let output = Command::new(env!("CARGO_BIN_EXE_kineto-cast"))
        .arg(&fixture_path)
        .arg("-o")
        .arg(out_path)
        .arg("--fps")
        .arg("0")
        .output()
        .expect("run kineto-cast");

    assert!(
        !output.status.success(),
        "CLI should exit 1 for --fps 0, not panic"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("fps"),
        "stderr should mention fps, got: {}",
        stderr
    );
    // A clean validation error, not a Rust panic backtrace.
    assert!(
        !stderr.contains("panicked at"),
        "fps 0 should be a clean error, not a panic; got: {}",
        stderr
    );
}

#[test]
fn test_kineto_cast_fps_non_divisor() {
    let out_dir = tempfile::tempdir().expect("create temp dir");
    let out_path = out_dir.path();

    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixture.cast");

    let output = Command::new(env!("CARGO_BIN_EXE_kineto-cast"))
        .arg(&fixture_path)
        .arg("-o")
        .arg(out_path)
        .arg("--fps")
        .arg("23")
        .output()
        .expect("run kineto-cast");

    assert!(
        !output.status.success(),
        "CLI should exit 1 for a non-divisor fps"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("fps"),
        "stderr should mention fps, got: {}",
        stderr
    );
}

#[test]
fn test_kineto_cast_unknown_flag() {
    let out_dir = tempfile::tempdir().expect("create temp dir");
    let out_path = out_dir.path();

    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixture.cast");

    let output = Command::new(env!("CARGO_BIN_EXE_kineto-cast"))
        .arg(&fixture_path)
        .arg("-o")
        .arg(out_path)
        .arg("-x")
        .output()
        .expect("run kineto-cast");

    assert!(
        !output.status.success(),
        "CLI should exit 1 for unknown flag -x"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown arguments"),
        "stderr should mention unknown arguments, got: {}",
        stderr
    );
}
