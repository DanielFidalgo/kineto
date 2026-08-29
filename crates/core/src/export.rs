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
/// A container the frame sequence can be encoded into.
///
/// Chosen from the output path's extension rather than a parameter: the
/// caller already names the file, and two ways to say the same thing is one
/// too many. An unrecognised extension is an error rather than a silent
/// fallback to h264, which would have written an h264 stream into whatever
/// container the name implied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// h264 in MP4. Universal, but not embeddable in markdown.
    Mp4,
    /// Animated WebP: 24-bit colour and real alpha, unlike GIF, which bands
    /// gradients into stripes and posterises soft shadows.
    WebP,
}

impl Format {
    pub fn from_path(p: &Path) -> Option<Format> {
        match p.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "mp4" => Some(Format::Mp4),
            "webp" => Some(Format::WebP),
            _ => None,
        }
    }

    /// The encoder ffmpeg is asked for, named so a failure can say which one
    /// was missing.
    pub fn encoder(&self) -> &'static str {
        match self {
            Format::Mp4 => "libx264",
            Format::WebP => "libwebp",
        }
    }
}

/// Searches for frame files named `frame-00000.png`, `frame-00001.png`, etc.
/// in `dir`, and encodes them into `out`, choosing the codec from `out`'s
/// extension:
///
/// ```bash
/// ffmpeg -y -framerate {fps} -i {dir}/frame-%05d.png -c:v libx264 -pix_fmt yuv420p out.mp4
/// ffmpeg -y -framerate {fps} -i {dir}/frame-%05d.png -c:v libwebp -lossless 0 -q:v 85 \
///        -compression_level 4 -loop 0 out.webp
/// ```
///
/// `-loop 0` is not optional: ffmpeg's WebP muxer defaults to looping once,
/// which for a README asset means it plays and then stops on the last frame.
///
/// Pick by length, not preference. WebP embeds inline in markdown and keeps
/// 24-bit colour and alpha, but costs roughly 280 KB per second at 720p; MP4
/// is around 28x smaller and needs a player or an upload. A few seconds of
/// WebP is a README loop; a minute of it is 17 MB.
///
/// Note the scope of determinism, unchanged by adding a format: the *frames*
/// are byte-identical run to run. No container is — each records its encoder
/// version and settings.
///
/// Returns:
/// - `Ok(false)` if ffmpeg is not available
/// - `Ok(true)` if ffmpeg succeeds (exit code 0)
/// - `Err(...)` if there is an I/O error, ffmpeg fails, or the extension is
///   not one of the supported formats
pub fn mux_with_ffmpeg(dir: &Path, fps: i64, out: &Path) -> io::Result<bool> {
    if !ffmpeg_available() {
        return Ok(false);
    }

    let format = Format::from_path(out).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "unsupported output extension for {}: expected .mp4 or .webp",
                out.display()
            ),
        )
    })?;

    let input_pattern = dir.join("frame-%05d.png");
    let fps_str = fps.to_string();
    let input_str = input_pattern.to_str().expect("path should be valid UTF-8");
    let out_str = out.to_str().expect("path should be valid UTF-8");

    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-framerate", &fps_str, "-i", input_str]);
    match format {
        Format::Mp4 => {
            cmd.args(["-c:v", "libx264", "-pix_fmt", "yuv420p"]);
        }
        Format::WebP => {
            // q:v 85 rather than the default: gradients and soft shadows are
            // exactly what a low-quality setting bands, and they are now the
            // point of the renderer.
            // Measured on a 5.5s 720p clip: quality has little leverage
            // (q=55 still produced 1.1 MB against 1.5 MB at q=85) and the
            // presets span only 1462-1578 KB. Animated WebP has no
            // inter-frame prediction — every ANMF chunk is essentially a
            // standalone VP8 image — so its size is structural, roughly
            // 280 KB per second at 720p, about 28x h264. Given that, quality
            // is kept high rather than traded for a saving that is not there.
            cmd.args([
                "-c:v",
                "libwebp",
                "-lossless",
                "0",
                "-q:v",
                "85",
                "-compression_level",
                "4",
                "-preset",
                "picture",
                "-loop",
                "0",
            ]);
        }
    }
    cmd.arg(out_str);

    let status = cmd.status()?;

    Ok(status.success())
}
