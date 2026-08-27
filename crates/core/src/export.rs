//! Native frame-sequence export and optional ffmpeg mux (non-wasm only).
//!
//! Exports a `Document` rendered by an `Engine` as a sequence of PNG frames,
//! with optional ffmpeg muxing to MP4 if ffmpeg is available on the system.

use crate::render::Engine;
use std::io;
use std::path::Path;
use std::process::Command;

/// Export all frames of the document to PNG files in the given directory.
///
/// Writes frames named `frame-00000.png`, `frame-00001.png`, etc., one for
/// each frame `n` where `tick_for_frame(n, fps) < total_duration()`.
///
/// Returns the number of frames written on success.
///
/// Each PNG is encoded in unpremultiplied RGBA8, i.e., the RGB channels
/// have been divided back through the alpha channel to recover the original
/// color space. This is the standard PNG encoding (browsers and image tools
/// expect unpremultiplied).
pub fn export_frames(engine: &mut Engine, fps: i64, dir: &Path) -> io::Result<u64> {
    // Create the directory if it doesn't exist
    std::fs::create_dir_all(dir)?;

    let total_duration = engine.total_duration();
    let mut frame_count = 0u64;

    loop {
        let tick = engine.tick_for_frame(frame_count as i64, fps);
        if tick >= total_duration {
            break;
        }

        // Render the frame at this tick
        let frame_data = engine.render(tick);

        // Copy the frame data (premultiplied) so we can unpremultiply it
        let mut rgba = frame_data.to_vec();

        // Unpremultiply in-place
        crate::render::unpremultiply(&mut rgba);

        // Encode as PNG
        let width = engine.width();
        let height = engine.height();
        let frame_path = dir.join(format!("frame-{:05}.png", frame_count));

        image::save_buffer(&frame_path, &rgba, width, height, image::ColorType::Rgba8)
            .map_err(io::Error::other)?;

        frame_count += 1;
    }

    Ok(frame_count)
}

/// Check if ffmpeg is available on the system.
///
/// Returns `true` if the `ffmpeg` command can be executed with the
/// `-version` flag, `false` otherwise.
pub fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Mux PNG frames to an MP4 file using ffmpeg if available.
///
/// Searches for frame files named `frame-00000.png`, `frame-00001.png`, etc.
/// in `dir`, and runs:
///
/// ```bash
/// ffmpeg -y -framerate {fps} -i {dir}/frame-%05d.png -c:v libx264 -pix_fmt yuv420p {out}
/// ```
///
/// Returns:
/// - `Ok(false)` if ffmpeg is not available
/// - `Ok(true)` if ffmpeg succeeds (exit code 0)
/// - `Err(...)` if there is an I/O error or ffmpeg fails
pub fn mux_with_ffmpeg(dir: &Path, fps: i64, out: &Path) -> io::Result<bool> {
    if !ffmpeg_available() {
        return Ok(false);
    }

    let input_pattern = dir.join("frame-%05d.png");
    let fps_str = fps.to_string();
    let input_str = input_pattern.to_str().expect("path should be valid UTF-8");
    let out_str = out.to_str().expect("path should be valid UTF-8");

    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-y",
        "-framerate",
        &fps_str,
        "-i",
        input_str,
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        out_str,
    ]);

    let status = cmd.status()?;

    Ok(status.success())
}
