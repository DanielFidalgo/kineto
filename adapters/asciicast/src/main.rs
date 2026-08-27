use std::error::Error;
use std::path::PathBuf;
use zoetrope_asciicast::{cast_to_document, parse_cast, Theme};
use zoetrope_core::assets::AssetStore;
use zoetrope_core::export::{export_frames, ffmpeg_available, mux_with_ffmpeg};
use zoetrope_core::render::Engine;

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

    // Try to mux with ffmpeg if available
    if ffmpeg_available() {
        let mp4_path = output.join("out.mp4");
        mux_with_ffmpeg(&output, fps, &mp4_path)?;
        println!("wrote {}/out.mp4", output.display());
    }

    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 4 {
        eprintln!("usage: zoetrope-cast <input.cast> -o <dir> [--fps N]");
        std::process::exit(1);
    }

    let input = PathBuf::from(&args[1]);

    let mut output = None;
    let mut fps = 30i64;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                if i + 1 >= args.len() {
                    eprintln!("error: -o requires an argument");
                    std::process::exit(1);
                }
                output = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--fps" => {
                if i + 1 >= args.len() {
                    eprintln!("error: --fps requires an argument");
                    std::process::exit(1);
                }
                match args[i + 1].parse::<i64>() {
                    Ok(f) => fps = f,
                    Err(e) => {
                        eprintln!("error parsing --fps: {}", e);
                        std::process::exit(1);
                    }
                }
                i += 2;
            }
            _ => i += 1,
        }
    }

    let output = match output {
        Some(o) => o,
        None => {
            eprintln!("error: -o is required");
            std::process::exit(1);
        }
    };

    if let Err(e) = run(input, output, fps) {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}
