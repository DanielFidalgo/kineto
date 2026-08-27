use std::error::Error;
use std::path::PathBuf;
use zoetrope_asciicast::{cast_to_document, parse_cast, Theme};
use zoetrope_core::assets::AssetStore;
use zoetrope_core::export::{export_frames, ffmpeg_available, mux_with_ffmpeg};
use zoetrope_core::render::Engine;
use zoetrope_core::TIMEBASE;

fn run(input: PathBuf, output: PathBuf, fps: i64) -> Result<(), Box<dyn Error>> {
    // Read the input file
    let cast_data = std::fs::read_to_string(&input)?;

    // Parse the cast file
    let cast = parse_cast(&cast_data)?;

    // Convert to document and get assets
    let (doc, assets) = cast_to_document(&cast, &Theme::default());

    // Create asset store and populate it
    let mut asset_store = AssetStore::new();
    for (id, bytes) in assets {
        asset_store.add_bytes(&id, bytes.to_vec());
    }

    // Create engine
    let mut engine = Engine::new(doc, asset_store)?;

    // Create output directory
    std::fs::create_dir_all(&output)?;

    // Export frames
    let frame_count = export_frames(&mut engine, fps, &output)?;
    println!("wrote {} frames to {}", frame_count, output.display());

    // Try to mux with ffmpeg if available. `mux_with_ffmpeg` returns
    // `Ok(false)` both when ffmpeg is absent and when it ran but exited
    // nonzero — only `Ok(true)` means an MP4 actually landed on disk, so
    // only that case gets the success message (never print a claim the
    // bool doesn't back up).
    if ffmpeg_available() {
        let mp4_path = output.join("out.mp4");
        if mux_with_ffmpeg(&output, fps, &mp4_path)? {
            println!("wrote {}/out.mp4", output.display());
        }
    }

    Ok(())
}

fn main() {
    let mut args = pico_args::Arguments::from_env();

    // Parse required output directory
    let output: PathBuf = match args.opt_value_from_str(["-o", "--output"]) {
        Ok(Some(o)) => o,
        Ok(None) => {
            eprintln!("error: -o is required");
            eprintln!("usage: zoetrope-cast <input.cast> -o <dir> [--fps N]");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("error parsing -o: {}", e);
            std::process::exit(1);
        }
    };

    // Parse optional fps (default 30)
    let fps: i64 = match args.opt_value_from_str("--fps") {
        Ok(Some(f)) => f,
        Ok(None) => 30,
        Err(e) => {
            eprintln!("error parsing --fps: {}", e);
            std::process::exit(1);
        }
    };

    // Validate fps up front, before any work (reading input, creating the
    // output dir, rendering): same guard as `Engine::tick_for_frame`, but
    // surfaced as a clean exit-1 error instead of a panic, since this is
    // user-supplied CLI input, not a programming error.
    if fps <= 0 || TIMEBASE % fps != 0 {
        eprintln!("error: unsupported fps {fps}: must divide {TIMEBASE}");
        std::process::exit(1);
    }

    // Parse required positional input file
    let input: PathBuf = match args.free_from_str() {
        Ok(i) => i,
        Err(_) => {
            eprintln!("error: input file is required");
            eprintln!("usage: zoetrope-cast <input.cast> -o <dir> [--fps N]");
            std::process::exit(1);
        }
    };

    // Check for unknown arguments
    let remaining = args.finish();
    if !remaining.is_empty() {
        eprintln!("error: unknown arguments: {:?}", remaining);
        eprintln!("usage: zoetrope-cast <input.cast> -o <dir> [--fps N]");
        std::process::exit(1);
    }

    if let Err(e) = run(input, output, fps) {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}
