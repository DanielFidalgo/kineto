//! `.cast` (asciicast v2) parsing and terminal grid-state emulation.
//!
//! `parse_cast` turns the raw NDJSON-ish `.cast` text into a [`Cast`]:
//! a header (`cols`/`rows`) plus the ordered `"o"` (stdout) events.
//! `grid_states` then replays those events through an `avt::Vt` terminal
//! emulator and snapshots the visible grid after each batch of
//! same-timestamp events, resolving indexed/RGB terminal colors to plain
//! `(u8, u8, u8)` triples via the locked palette below. Task 22 converts
//! the resulting [`GridState`]s into kineto `Document`s; this module
//! knows nothing about that — it only produces plain data.

use serde::Deserialize;
use thiserror::Error;

/// A parsed `.cast` file: terminal dimensions plus the ordered stdout
/// ("o") events, each as `(time_seconds, data)`. Non-"o" events (e.g.
/// resize "r", markers "m") are dropped during parsing.
#[derive(Debug, Clone, PartialEq)]
pub struct Cast {
    pub cols: u16,
    pub rows: u16,
    pub events: Vec<(f64, String)>,
}

/// Errors from [`parse_cast`].
#[derive(Debug, Error)]
pub enum CastError {
    /// Line 1 (the header) was missing, not valid JSON, or missing the
    /// `width`/`height` fields.
    #[error("invalid cast header: {0}")]
    Header(String),
    /// An event line (2+) was not a valid `[time, type, data]` triple.
    #[error("invalid event on line {line}: {msg}")]
    Event { line: usize, msg: String },
}

/// Shape of line 1 of a `.cast` v2 file. Only the fields we need; unknown
/// fields (`env`, `title`, ...) are ignored by serde's default behavior.
#[derive(Debug, Deserialize)]
struct Header {
    #[allow(dead_code)]
    version: u32,
    width: u16,
    height: u16,
}

/// Parse a `.cast` v2 document: line 1 is the JSON header, subsequent
/// lines are `[time, "o"|other, data]` triples. Only `"o"` (stdout)
/// events are kept in [`Cast::events`]; other event types are skipped
/// without error.
pub fn parse_cast(input: &str) -> Result<Cast, CastError> {
    let mut lines = input.lines();

    let header_line = lines
        .next()
        .ok_or_else(|| CastError::Header("empty input, no header line".to_string()))?;
    let header: Header =
        serde_json::from_str(header_line.trim()).map_err(|e| CastError::Header(e.to_string()))?;

    let mut events = Vec::new();

    for (offset, line) in lines.enumerate() {
        // Header is physical line 1, so the first event line is line 2.
        let line_no = offset + 2;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let value: serde_json::Value =
            serde_json::from_str(trimmed).map_err(|e| CastError::Event {
                line: line_no,
                msg: e.to_string(),
            })?;

        let triple = value.as_array().ok_or_else(|| CastError::Event {
            line: line_no,
            msg: "expected a [time, type, data] array".to_string(),
        })?;
        if triple.len() != 3 {
            return Err(CastError::Event {
                line: line_no,
                msg: format!("expected 3 elements, got {}", triple.len()),
            });
        }

        let time = triple[0].as_f64().ok_or_else(|| CastError::Event {
            line: line_no,
            msg: "event time must be a number".to_string(),
        })?;
        let kind = triple[1].as_str().ok_or_else(|| CastError::Event {
            line: line_no,
            msg: "event type must be a string".to_string(),
        })?;
        let data = triple[2].as_str().ok_or_else(|| CastError::Event {
            line: line_no,
            msg: "event data must be a string".to_string(),
        })?;

        if kind == "o" {
            events.push((time, data.to_string()));
        }
    }

    Ok(Cast {
        cols: header.width,
        rows: header.height,
        events,
    })
}

/// One terminal cell: character plus resolved colors/bold. `fg`/`bg` are
/// `None` when the cell uses the terminal's default color (theme applies
/// later, downstream of this crate).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub fg: Option<(u8, u8, u8)>,
    pub bg: Option<(u8, u8, u8)>,
    pub bold: bool,
}

/// A full-grid snapshot at a point in time: one `Vec<Cell>` per row
/// (length == `Cast::cols`), `rows.len() == Cast::rows`, and the cursor
/// position as `(col, row)` (`None` when the cursor is hidden).
#[derive(Debug, Clone, PartialEq)]
pub struct GridState {
    pub time_s: f64,
    pub rows: Vec<Vec<Cell>>,
    pub cursor: Option<(u16, u16)>,
}

/// Replay `cast.events` through an `avt::Vt` terminal emulator and return
/// one [`GridState`] per distinct visible grid. Events sharing an exact
/// timestamp are fed as one batch before snapshotting (so a chunked
/// write, e.g. two `"o"` events both at `t=0.5`, yields a single grid);
/// consecutive snapshots with identical rows/cursor are also deduped.
pub fn grid_states(cast: &Cast) -> Vec<GridState> {
    let mut vt = avt::Vt::builder()
        .size(cast.cols as usize, cast.rows as usize)
        .build();

    let mut states: Vec<GridState> = Vec::new();
    let mut i = 0;

    while i < cast.events.len() {
        let time_s = cast.events[i].0;

        let mut j = i;
        while j < cast.events.len() && cast.events[j].0 == time_s {
            vt.feed_str(&cast.events[j].1);
            j += 1;
        }
        i = j;

        let rows = snapshot_rows(&vt);
        let cursor = snapshot_cursor(&vt);

        let is_dup = states
            .last()
            .is_some_and(|prev| prev.rows == rows && prev.cursor == cursor);

        if !is_dup {
            states.push(GridState {
                time_s,
                rows,
                cursor,
            });
        }
    }

    states
}

fn snapshot_rows(vt: &avt::Vt) -> Vec<Vec<Cell>> {
    vt.view()
        .map(|line| {
            line.cells()
                .iter()
                .map(|cell| {
                    let pen = cell.pen();
                    Cell {
                        ch: cell.char(),
                        fg: resolve_color(pen.foreground()),
                        bg: resolve_color(pen.background()),
                        bold: pen.is_bold(),
                    }
                })
                .collect()
        })
        .collect()
}

fn snapshot_cursor(vt: &avt::Vt) -> Option<(u16, u16)> {
    let cursor = vt.cursor();
    Option::<(usize, usize)>::from(cursor).map(|(col, row)| (col as u16, row as u16))
}

fn resolve_color(color: Option<avt::Color>) -> Option<(u8, u8, u8)> {
    match color? {
        avt::Color::RGB(rgb) => Some((rgb.r, rgb.g, rgb.b)),
        avt::Color::Indexed(n) => Some(indexed_to_rgb(n)),
    }
}

/// Locked terminal palette (see task-21 brief §11): indices 0-15 are the
/// fixed 16-color table (8 normal + 8 "bright" duplicates per the brief),
/// 16-231 are the 6x6x6 color cube, 232-255 are the grayscale ramp.
const PALETTE_0_15: [(u8, u8, u8); 16] = [
    (0x00, 0x00, 0x00),
    (0xDD, 0x3C, 0x69),
    (0x4E, 0xBF, 0x22),
    (0xDD, 0xAF, 0x3C),
    (0x26, 0xB0, 0xD7),
    (0xB9, 0x54, 0xE1),
    (0x54, 0xE1, 0xB9),
    (0xD9, 0xD9, 0xD9),
    (0x4D, 0x4D, 0x4D),
    (0xDD, 0x3C, 0x69),
    (0x4E, 0xBF, 0x22),
    (0xDD, 0xAF, 0x3C),
    (0x26, 0xB0, 0xD7),
    (0xB9, 0x54, 0xE1),
    (0x54, 0xE1, 0xB9),
    (0xFF, 0xFF, 0xFF),
];

const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

fn indexed_to_rgb(n: u8) -> (u8, u8, u8) {
    match n {
        0..=15 => PALETTE_0_15[n as usize],
        16..=231 => {
            let cube = (n - 16) as usize;
            let r = CUBE_LEVELS[cube / 36];
            let g = CUBE_LEVELS[(cube / 6) % 6];
            let b = CUBE_LEVELS[cube % 6];
            (r, g, b)
        }
        232..=255 => {
            let gray = 8 + 10 * (n as u16 - 232);
            (gray as u8, gray as u8, gray as u8)
        }
    }
}
