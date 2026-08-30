//! `kineto` — render a scene document from the command line.
//!
//! The project's claim is that you write a document and it compiles to a
//! video. Until this existed there was no command that did that: `kineto-cast`
//! converts asciicast recordings, and everything else went through the MCP
//! server, which needs a client. Anyone wanting to render a plain document had
//! to write one — which is a poor answer to "how do I use this".
//!
//! Lives in `crates/mcp` because that is where loading a document from a path,
//! resolving its assets against a directory, and encoding by extension already
//! are. A separate crate would have duplicated all three.
//!
//! ```text
//! kineto scene.json -o out.mp4              # or out.webp
//! kineto --scenes spec.json -o out.mp4      # themed scenes, composed for you
//! kineto scene.json -o poster.png --at 1500 # a single frame
//! kineto scene.json -o small.mp4 --width 960
//! kineto scene.json --check                 # report problems, render nothing
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use kineto::check;
use kineto::render;
use kineto::source;

const USAGE: &str = "\
kineto — compile a scene document to a video

USAGE:
    kineto <document.json> -o <output>   render (.mp4, .webp or .png)
    kineto --scenes <spec.json> -o <o>   compose themed scenes, then render
    kineto <document.json> --check       report problems, render nothing

OPTIONS:
    -o, --out <PATH>     output file; the extension picks the format
                         .mp4 h264 · .webp animated · .png one frame
        --at <MS>        which moment to write, for a .png (default 0)
        --width <PX>     scale output to PX wide, keeping aspect
        --fps <N>        override the document's own defaultFps
    -s, --scenes <PATH>  build the document from a scene spec instead:
                         { \"theme\": \"midnight\", \"scenes\": [ ... ] }
                         kinds: title, points, code, quote
        --doc-out <PATH> also write the composed document, to edit by hand
        --assets <DIR>   resolve image/font srcs against DIR
        --check          check and exit; nonzero if anything is wrong
    -h, --help           this text
";

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut args = pico_args::Arguments::from_env();

    if args.contains(["-h", "--help"]) {
        print!("{USAGE}");
        return Ok(ExitCode::SUCCESS);
    }

    let check_only = args.contains("--check");
    let out: Option<PathBuf> = args.opt_value_from_str(["-o", "--out"])?;
    let fps: Option<i64> = args.opt_value_from_str("--fps")?;
    let at_ms: Option<i64> = args.opt_value_from_str("--at")?;
    let width: Option<u32> = args.opt_value_from_str("--width")?;
    let assets_dir: Option<PathBuf> = args.opt_value_from_str("--assets")?;
    let scenes: Option<PathBuf> = args.opt_value_from_str(["-s", "--scenes"])?;
    let doc_out: Option<PathBuf> = args.opt_value_from_str("--doc-out")?;

    let rest = args.finish();
    if rest.len() > 1 {
        return Err(format!("unexpected argument: {:?}", rest[1]).into());
    }

    // Either compose a document from a spec, or load one that already exists.
    let (doc, default_base) = if let Some(spec_path) = &scenes {
        if let Some(extra) = rest.first() {
            return Err(
                format!("--scenes builds the document, so it cannot also take {extra:?}").into(),
            );
        }
        let spec = std::fs::read_to_string(spec_path)
            .map_err(|e| format!("reading {}: {e}", spec_path.display()))?;
        let json = kineto::scene::build_from_spec(&spec)?;
        if let Some(path) = &doc_out {
            std::fs::write(path, &json).map_err(|e| format!("writing {}: {e}", path.display()))?;
            println!("wrote {}", path.display());
        }
        // Assets resolve against the spec's directory, which is where a
        // caller would keep anything it refers to.
        let base = spec_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        (source::load_document(Some(&json), None)?.0, base)
    } else {
        let Some(input) = rest.first() else {
            eprint!("{USAGE}");
            return Err("no input document".into());
        };
        source::load_document(None, PathBuf::from(input).to_str())?
    };
    let fps = source::resolve_fps(fps, &doc)?;
    source::check_canvas_size(doc.size.w, doc.size.h)?;
    let base = assets_dir.unwrap_or(default_base);
    let mut assets = source::resolve_assets(&doc, &base)?;
    assets.prepare(&doc)?;

    // Checked before rendering either way: a defect costs nothing to find
    // here and a full render to find afterwards.
    let mut issues = check::analyze_document(&doc);
    let starts = kineto_core::timeline::scene_starts(&doc);
    for (i, scene) in doc.scenes.iter().enumerate() {
        issues.extend(check::analyze(
            &doc,
            &mut assets,
            starts[i] + scene.duration / 2,
        ));
    }
    let bad = issues
        .iter()
        .filter(|i| i.category == "correctness")
        .count();
    for i in &issues {
        let at = match (&i.scene, i.element) {
            (Some(s), Some(e)) => format!("scene '{s}' element {e}"),
            (Some(s), None) => format!("scene '{s}'"),
            _ => "document".to_string(),
        };
        eprintln!("{}: {at}: {} [{}]", i.category, i.detail, i.kind);
    }

    if check_only {
        if issues.is_empty() {
            println!("no issues");
        }
        // Design findings are advice; correctness findings are not.
        return Ok(if bad == 0 {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        });
    }

    let Some(out) = out else {
        eprint!("{USAGE}");
        return Err("-o is required unless --check is given".into());
    };

    let mut engine = kineto_core::Engine::new(doc, assets)?;

    // A .png is one frame, not a one-frame video, so it does not go through
    // the muxer — and needs no ffmpeg at all.
    if kineto_core::export::Format::from_path(&out) == Some(kineto_core::export::Format::Png) {
        let tick = at_ms.unwrap_or(0) * (kineto_core::doc::TIMEBASE / 1000);
        let (w, h) = kineto_core::export::write_still(&mut engine, tick, &out, width)?;
        println!(
            "wrote {} ({w}x{h} at {} ms)",
            out.display(),
            at_ms.unwrap_or(0)
        );
        return Ok(ExitCode::SUCCESS);
    }

    let outcome =
        render::render_to_file_scaled(&mut engine, fps, out.to_str().unwrap_or_default(), width)?;
    println!(
        "wrote {} ({}x{}, {} frames at {} fps, {:.3}s{})",
        out.display(),
        outcome.width,
        outcome.height,
        outcome.frame_count,
        outcome.fps,
        outcome.duration_seconds,
        match outcome.bytes {
            Some(b) => format!(", {:.1} MB", b as f64 / 1_048_576.0),
            None => String::new(),
        }
    );
    Ok(ExitCode::SUCCESS)
}
