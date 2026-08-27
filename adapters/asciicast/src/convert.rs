//! Grid states -> zoetrope `Document` conversion (Task 22).
//!
//! [`cast_to_document`] turns the [`GridState`] snapshots produced by
//! [`crate::grid_states`] into a cut-joined `Document` (spec §4.4: no
//! transitions between scenes) — one scene per grid state, each row's
//! cells coalesced into background `rect`s and monospace `text` runs, with
//! an optional cursor `rect` painted last.
//!
//! **v1 renders `bold` as regular weight**: only one JetBrains Mono weight
//! (Regular) is bundled, so there's no separate bold face to draw with.
//! `Cell::bold` still participates in run-splitting below (a bold run
//! never merges with an adjacent non-bold run that happens to share the
//! same colors) so the run boundaries match the source terminal exactly,
//! even though the boldness itself has no visual effect yet.

use crate::{Cast, Cell, GridState};
use zoetrope_core::{seconds, Asset, Document, Element, Scene};

/// Sizing/coloring knobs for [`cast_to_document`]. `cell_w`/`cell_h` are
/// JetBrains Mono's 0.6em advance / 1.3em line-height at `size_px`px
/// (locked constants — see task-22 brief; matches Task 7's line-height
/// rule), so cells and glyphs stay aligned without per-glyph measurement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub bg: &'static str,
    pub fg: &'static str,
    pub size_px: f64,
    pub cell_w: f64,
    pub cell_h: f64,
    pub pad: f64,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            bg: "#0A0A0A",
            fg: "#D4D4D4",
            size_px: 20.0,
            cell_w: 12.0,
            cell_h: 26.0,
            pad: 16.0,
        }
    }
}

/// Doc-level asset id used for the terminal font on every converted
/// document (referenced by every `text` element's `font` field).
const FONT_ID: &str = "term";
/// Reserved src resolved by `zoetrope_core::resolve_reserved_src` to the
/// bundled JetBrains Mono bytes (native-only, `bundled-fonts` feature).
const FONT_SRC: &str = "zoetrope:jetbrains-mono";

/// Convert a parsed `.cast` into a `Document` plus the asset bytes the
/// caller (Task 23's CLI) must stage into an `AssetStore` before
/// rendering — `[("term", <JetBrains Mono bytes>)]`.
///
/// One scene per grid state (`avt` replay via [`crate::grid_states`]),
/// joined by cuts: scene `i`'s duration is the gap to the next grid
/// state's timestamp, and the final scene holds for 1 second.
pub fn cast_to_document(cast: &Cast, theme: &Theme) -> (Document, Vec<(String, &'static [u8])>) {
    let states = crate::grid_states(cast);

    let w = round_up_even(cast.cols as f64 * theme.cell_w + 2.0 * theme.pad);
    let h = round_up_even(cast.rows as f64 * theme.cell_h + 2.0 * theme.pad);

    let mut doc = Document::new(w, h).with_fps(30).with_bg(theme.bg);
    doc.add_asset(FONT_ID, Asset::font(FONT_SRC));

    for (i, state) in states.iter().enumerate() {
        let duration = match states.get(i + 1) {
            Some(next) => seconds(next.time_s - state.time_s),
            None => seconds(1.0),
        };
        doc.push_scene(scene_for_state(i, state, duration, theme));
    }

    let bytes = zoetrope_core::resolve_reserved_src(FONT_SRC)
        .expect("bundled JetBrains Mono font missing (bundled-fonts feature must be enabled)");
    (doc, vec![(FONT_ID.to_string(), bytes)])
}

/// `ceil`, then round up to the nearest even integer (yuv420p export
/// needs even width/height).
fn round_up_even(x: f64) -> u32 {
    let v = x.ceil() as u32;
    if v.is_multiple_of(2) {
        v
    } else {
        v + 1
    }
}

fn scene_for_state(index: usize, state: &GridState, duration: i64, theme: &Theme) -> Scene {
    let id = format!("state-{index:03}");
    // Cut-joined (spec §4.4): `Scene::new` defaults `transition` to `None`,
    // which is exactly a cut, so nothing further to set here.
    let mut scene = Scene::new(&id, duration);

    for (row_idx, row) in state.rows.iter().enumerate() {
        for element in row_elements(row_idx, row, theme) {
            scene = scene.with_element(element);
        }
    }

    // Cursor last: painted over every row's text/background (paint order).
    if let Some((col, row)) = state.cursor {
        scene = scene.with_element(cursor_element(col, row, theme));
    }

    scene
}

/// Coalesce one row into runs of equal `(fg, bg, bold)`, emitting a
/// background `rect` for runs with a non-default `bg` and a `text`
/// element for runs that aren't entirely spaces (trailing spaces trimmed).
fn row_elements(row_idx: usize, row: &[Cell], theme: &Theme) -> Vec<Element> {
    let mut elements = Vec::new();
    let y = theme.pad + row_idx as f64 * theme.cell_h;

    let mut col = 0;
    while col < row.len() {
        let cell = row[col];
        let mut end = col + 1;
        while end < row.len() && cells_match(row[end], cell) {
            end += 1;
        }
        let len = end - col;
        let x = theme.pad + col as f64 * theme.cell_w;

        // `Cell::bg == None` is the terminal's default background (see
        // term.rs docs), so any `Some` value is a non-default run.
        if let Some(bg) = cell.bg {
            elements.push(Element::rect(
                [x, y, len as f64 * theme.cell_w, theme.cell_h],
                rgb_to_hex(bg).as_str(),
            ));
        }

        let text: String = row[col..end].iter().map(|c| c.ch).collect();
        let trimmed = text.trim_end_matches(' ');
        if !trimmed.is_empty() {
            let color = cell
                .fg
                .map(rgb_to_hex)
                .unwrap_or_else(|| theme.fg.to_string());
            elements.push(Element::text(
                trimmed,
                FONT_ID,
                theme.size_px,
                color.as_str(),
                [x, y],
            ));
        }

        col = end;
    }

    elements
}

fn cells_match(a: Cell, b: Cell) -> bool {
    a.fg == b.fg && a.bg == b.bg && a.bold == b.bold
}

fn cursor_element(col: u16, row: u16, theme: &Theme) -> Element {
    let x = theme.pad + col as f64 * theme.cell_w;
    let y = theme.pad + row as f64 * theme.cell_h;
    Element::rect([x, y, theme.cell_w, theme.cell_h], theme.fg).with_opacity(0.6)
}

fn rgb_to_hex((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}
