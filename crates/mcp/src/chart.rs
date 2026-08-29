//! Data to a chart document.
//!
//! Deliberately not an element type. A `chart` in the format would bake
//! plotting opinion into a renderer that has none, and every choice here —
//! how many ticks, where the baseline sits, what a bar's corner radius is —
//! is opinion. This emits ordinary paths, rects and text, so the engine never
//! learns what a chart is and the output can be edited afterwards like any
//! other document.
//!
//! Layout is measured rather than guessed: the left margin is the width of
//! the widest y-axis label, and category labels are centred on their column
//! by their own width. Hand-placed magic numbers are what make a chart look
//! nearly right at one size and wrong at every other.

use kineto_core::doc::{Cap, Ease, Gradient, Join, Key, Prop, Stop, Track, TIMEBASE};
use kineto_core::{Asset, AssetStore, Document, Element, Scene};
use serde::{Deserialize, Serialize};

use crate::error::ToolError;

const BG: &str = "#0B1116";
const FG: &str = "#F4F7F9";
const DIM: &str = "#8FA3B0";
const GRID: &str = "#1c2b36";
const AXIS: &str = "#2b3d4a";

/// Enough distinct hues for a legible chart. Past six series a chart is a
/// table, and colour stops helping.
const PALETTE: [&str; 6] = [
    "#FF9F45", "#4ECDC4", "#C77DFF", "#6BAFFF", "#F45B69", "#9BE564",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChartKind {
    Line,
    /// A line with the region beneath it filled by a fading gradient.
    Area,
    Bar,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Series {
    pub name: String,
    pub values: Vec<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartSpec {
    pub kind: ChartKind,
    /// One label per category, along the x axis.
    pub labels: Vec<String>,
    pub series: Vec<Series>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default = "default_w")]
    pub width: u32,
    #[serde(default = "default_h")]
    pub height: u32,
    /// Seconds the chart holds. Series animate in over the first second.
    #[serde(default = "default_seconds")]
    pub seconds: f64,
}

fn default_w() -> u32 {
    1280
}
fn default_h() -> u32 {
    720
}
fn default_seconds() -> f64 {
    6.0
}

/// Axis ticks at round numbers covering `min..max`.
///
/// Steps are chosen from 1, 2, 2.5 and 5 times a power of ten — the set that
/// produces labels a reader can do arithmetic with. A naive `range / count`
/// gives ticks like 37.4, which is accurate and useless.
pub fn nice_ticks(min: f64, max: f64, target: usize) -> Vec<f64> {
    let target = target.max(2);
    // A flat series has no range to divide; give it a band around its value
    // so the line lands mid-plot rather than on the axis.
    let (min, max) = if (max - min).abs() < f64::EPSILON {
        let v = min;
        let pad = if v.abs() < f64::EPSILON {
            1.0
        } else {
            v.abs() * 0.5
        };
        (v - pad, v + pad)
    } else {
        (min, max)
    };

    let raw = (max - min) / target as f64;
    let mag = 10f64.powf(raw.abs().log10().floor());
    let norm = raw / mag;
    let step = if norm <= 1.0 {
        1.0
    } else if norm <= 2.0 {
        2.0
    } else if norm <= 2.5 {
        2.5
    } else if norm <= 5.0 {
        5.0
    } else {
        10.0
    } * mag;

    let lo = (min / step).floor() * step;
    let hi = (max / step).ceil() * step;

    // Multiplying instead of accumulating is not enough: a 0.1 step is
    // already inexact in binary, so 0.1 * 3 is 0.30000000000000004 however it
    // is reached. Each tick is rounded to the step's own precision, which is
    // the only precision the label will ever show anyway.
    let decimals = (-step.log10().floor()).max(0.0) as i32;
    let scale = 10f64.powi(decimals);
    let n = ((hi - lo) / step).round() as i64;
    (0..=n)
        .map(|i| ((lo + step * i as f64) * scale).round() / scale)
        .collect()
}

/// Format a tick so the axis reads cleanly: no trailing zeros, and thousands
/// abbreviated once the numbers get long.
pub fn format_tick(v: f64, step: f64) -> String {
    if v.abs() >= 1000.0 && step >= 100.0 {
        let k = v / 1000.0;
        if (k - k.round()).abs() < 1e-9 {
            return format!("{}k", k.round() as i64);
        }
        // Only abbreviate when it stays exact. 1250 rendered as "1.2k" is a
        // label that disagrees with the gridline it sits on, and an axis that
        // lies quietly is worse than one that is merely long.
        let one = format!("{k:.1}");
        if (one.parse::<f64>().unwrap_or(f64::NAN) - k).abs() < 1e-9 {
            return format!("{one}k");
        }
    }
    // Decimal places follow the step, not the value: a 0.5 step needs one
    // place on every label, including the ones that land on integers.
    let dp = if step >= 1.0 {
        0
    } else if step >= 0.1 {
        1
    } else {
        2
    };
    format!("{v:.dp$}")
}

fn measure(assets: &mut AssetStore, family: &str, text: &str, size: f32) -> (f32, f32) {
    let fam = assets.family(family).to_string();
    let l = kineto_core::layout_text(
        assets.font_system(),
        &fam,
        text,
        size,
        None,
        kineto_core::doc::Align::Left,
    );
    (l.width, l.height)
}

/// Build a document from a chart spec.
pub fn build(spec: &ChartSpec) -> Result<Document, ToolError> {
    if spec.series.is_empty() {
        return Err(ToolError::Invalid(
            "a chart needs at least one series".into(),
        ));
    }
    if spec.labels.is_empty() {
        return Err(ToolError::Invalid(
            "a chart needs at least one category label".into(),
        ));
    }
    for s in &spec.series {
        if s.values.len() != spec.labels.len() {
            return Err(ToolError::Invalid(format!(
                "series '{}' has {} values but there are {} labels — a chart \
                 with a ragged row is a chart nobody can read",
                s.name,
                s.values.len(),
                spec.labels.len()
            )));
        }
    }
    let seconds = spec.seconds;
    // Spelled out rather than `!(x > 0.0)`: NaN has to be rejected too, and
    // the negated comparison hid that.
    if seconds.is_nan() || seconds <= 0.0 {
        return Err(ToolError::Invalid("`seconds` must be positive".into()));
    }

    let mut doc = Document::new(spec.width, spec.height)
        .with_fps(30)
        .with_bg(BG);
    doc.add_asset("inter", Asset::font("kineto:inter"));
    doc.add_asset("mono", Asset::font("kineto:jetbrains-mono"));

    // A private store just for measurement; the caller's assets are its own.
    let mut assets = AssetStore::new();
    for (id, src) in [("inter", "kineto:inter"), ("mono", "kineto:jetbrains-mono")] {
        assets.add_bytes(
            id,
            kineto_core::resolve_reserved_src(src)
                .ok_or_else(|| ToolError::Invalid(format!("missing bundled font {src}")))?
                .to_vec(),
        );
    }
    assets.prepare(&doc)?;

    let (w, h) = (spec.width as f64, spec.height as f64);
    let dur = (seconds * TIMEBASE as f64) as i64;

    // ---- scale -----------------------------------------------------------
    let all: Vec<f64> = spec
        .series
        .iter()
        .flat_map(|s| s.values.iter().copied())
        .collect();
    let dmin = all.iter().cloned().fold(f64::INFINITY, f64::min);
    let dmax = all.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    // Bars are read against zero; a bar chart whose axis starts at 40
    // exaggerates every difference on it.
    let base = if spec.kind == ChartKind::Bar {
        dmin.min(0.0)
    } else {
        dmin
    };
    let ticks = nice_ticks(base, dmax, 5);
    let (ylo, yhi) = (ticks[0], *ticks.last().unwrap());
    let step = if ticks.len() > 1 {
        ticks[1] - ticks[0]
    } else {
        1.0
    };

    // ---- margins, measured ----------------------------------------------
    let tick_size = 15.0f32;
    let label_size = 16.0f32;
    let widest = ticks
        .iter()
        .map(|t| measure(&mut assets, "mono", &format_tick(*t, step), tick_size).0)
        .fold(0.0f32, f32::max) as f64;

    let top = match (&spec.title, &spec.subtitle) {
        (Some(_), Some(_)) => 150.0,
        (Some(_), None) => 116.0,
        _ => 56.0,
    };
    let left = 56.0 + widest + 18.0;
    let right = w - 56.0;
    let legend_h = if spec.series.len() > 1 { 40.0 } else { 0.0 };
    let bottom =
        h - 56.0 - measure(&mut assets, "mono", "Ag", label_size).1 as f64 - 14.0 - legend_h;
    let plot_w = right - left;
    let plot_h = bottom - top;
    if plot_w <= 40.0 || plot_h <= 40.0 {
        return Err(ToolError::Invalid(
            "canvas is too small for the labels this chart needs".into(),
        ));
    }

    let y_of = |v: f64| bottom - (v - ylo) / (yhi - ylo) * plot_h;
    let n = spec.labels.len();
    // Line points sit on the edges; bars sit in bands between them.
    let x_line = |i: usize| {
        if n == 1 {
            left + plot_w / 2.0
        } else {
            left + plot_w * i as f64 / (n - 1) as f64
        }
    };
    let band = plot_w / n as f64;
    let x_band = |i: usize| left + band * (i as f64 + 0.5);

    let mut scene = Scene::new("chart", dur);

    // ---- titles ----------------------------------------------------------
    if let Some(t) = &spec.title {
        scene = scene.with_element(
            Element::text(t, "inter", 38.0, FG, [56.0, 48.0])
                .with_max_w(w - 112.0)
                .with_animation(fade_in(0, dur)),
        );
    }
    if let Some(t) = &spec.subtitle {
        let y = if spec.title.is_some() { 100.0 } else { 48.0 };
        scene = scene.with_element(
            Element::text(t, "mono", 18.0, DIM, [58.0, y])
                .with_max_w(w - 112.0)
                .with_animation(fade_in(120, dur)),
        );
    }

    // ---- grid and y labels ----------------------------------------------
    for (i, t) in ticks.iter().enumerate() {
        let y = y_of(*t);
        scene = scene.with_element(
            Element::path(vec![[left, y], [right, y]])
                .with_stroke(
                    if i == 0 { AXIS } else { GRID },
                    if i == 0 { 2.0 } else { 1.0 },
                )
                .with_animation(fade_in(60 + i as i64 * 30, dur)),
        );
        let label = format_tick(*t, step);
        let lw = measure(&mut assets, "mono", &label, tick_size).0 as f64;
        // Right-aligned against the axis, which is the whole reason the
        // widths are measured rather than assumed.
        scene = scene.with_element(
            Element::text(
                &label,
                "mono",
                tick_size as f64,
                DIM,
                [left - 18.0 - lw, y - 9.0],
            )
            .with_animation(fade_in(60 + i as i64 * 30, dur)),
        );
    }

    // ---- category labels --------------------------------------------------
    for (i, label) in spec.labels.iter().enumerate() {
        let cx = if spec.kind == ChartKind::Bar {
            x_band(i)
        } else {
            x_line(i)
        };
        let lw = measure(&mut assets, "mono", label, label_size).0 as f64;
        scene = scene.with_element(
            Element::text(
                label,
                "mono",
                label_size as f64,
                DIM,
                [cx - lw / 2.0, bottom + 16.0],
            )
            .with_animation(fade_in(200 + i as i64 * 40, dur)),
        );
    }

    // ---- series -----------------------------------------------------------
    let group_w = band * 0.68;
    let bar_w = group_w / spec.series.len() as f64;
    for (si, s) in spec.series.iter().enumerate() {
        let color = s
            .color
            .clone()
            .unwrap_or_else(|| PALETTE[si % PALETTE.len()].to_string());
        let delay = 320 + si as i64 * 160;

        match spec.kind {
            ChartKind::Bar => {
                for (i, v) in s.values.iter().enumerate() {
                    let x = x_band(i) - group_w / 2.0 + bar_w * si as f64;
                    let (y0, y1) = (y_of(0.0f64.max(ylo)), y_of(*v));
                    let (top_y, bh) = if y1 <= y0 {
                        (y1, y0 - y1)
                    } else {
                        (y0, y1 - y0)
                    };
                    if bh < 0.5 {
                        continue;
                    }
                    // Grows from the baseline: scale about the centre would
                    // make a bar rise out of the middle of the plot.
                    scene = scene.with_element(
                        Element::rect([x + bar_w * 0.1, top_y, bar_w * 0.8, bh], color.as_str())
                            .with_radius((bar_w * 0.16).min(8.0))
                            .with_clip(kineto_core::doc::Clip::new([left, top, plot_w, plot_h]))
                            .with_animation(Track::new(
                                Prop::Translate,
                                vec![
                                    Key::vec2(0, [0.0, bh]),
                                    Key::vec2(ms(delay + i as i64 * 45), [0.0, bh]),
                                    Key::vec2(ms(delay + i as i64 * 45 + 520), [0.0, 0.0])
                                        .with_ease(Ease::OutCubic),
                                ],
                            )),
                    );
                }
            }
            ChartKind::Line | ChartKind::Area => {
                let pts: Vec<[f64; 2]> = s
                    .values
                    .iter()
                    .enumerate()
                    .map(|(i, v)| [x_line(i), y_of(*v)])
                    .collect();

                if spec.kind == ChartKind::Area && pts.len() > 1 {
                    let mut area = pts.clone();
                    area.push([x_line(n - 1), bottom]);
                    area.push([x_line(0), bottom]);
                    scene = scene.with_element(
                        Element::path(area)
                            .with_closed(true)
                            .with_path_fill(Gradient::linear(
                                [0.0, 0.0],
                                [0.0, 1.0],
                                vec![
                                    // The series colour at 35% alpha fading
                                    // to nothing: an area fill that competes
                                    // with its own line reads as a blob.
                                    Stop::new(0.0, format!("{color}59").as_str()),
                                    Stop::new(1.0, format!("{color}00").as_str()),
                                ],
                            ))
                            .with_animation(fade_in(delay + 240, dur)),
                    );
                }

                if pts.len() > 1 {
                    scene = scene.with_element(
                        Element::path(pts.clone())
                            .with_stroke(color.as_str(), 3.5)
                            .with_cap(Cap::Round)
                            .with_join(Join::Round)
                            .with_animation(fade_in(delay, dur)),
                    );
                }
                for (i, p) in pts.iter().enumerate() {
                    scene = scene.with_element(
                        Element::rect([p[0] - 4.5, p[1] - 4.5, 9.0, 9.0], color.as_str())
                            .with_radius(4.5)
                            .with_animation(fade_in(delay + 120 + i as i64 * 45, dur)),
                    );
                }
            }
        }
    }

    // ---- legend -----------------------------------------------------------
    if spec.series.len() > 1 {
        let mut x = left;
        let y = h - 46.0;
        for (si, s) in spec.series.iter().enumerate() {
            let color = s
                .color
                .clone()
                .unwrap_or_else(|| PALETTE[si % PALETTE.len()].to_string());
            scene = scene
                .with_element(
                    Element::rect([x, y, 22.0, 6.0], color.as_str())
                        .with_radius(3.0)
                        .with_animation(fade_in(600 + si as i64 * 90, dur)),
                )
                .with_element(
                    Element::text(&s.name, "mono", 15.0, DIM, [x + 32.0, y - 8.0])
                        .with_animation(fade_in(600 + si as i64 * 90, dur)),
                );
            x += 32.0 + measure(&mut assets, "mono", &s.name, 15.0).0 as f64 + 34.0;
        }
    }

    doc.push_scene(scene);
    Ok(doc)
}

fn ms(v: i64) -> i64 {
    v * (TIMEBASE / 1000)
}

/// Hold at zero until `delay_ms`, ramp in, hold.
///
/// Both ends need care. A zero delay would emit two keys at t=0, and a delay
/// running past the chart's own duration would emit them out of order —
/// validation rejects either, and both are easy to reach from a series index.
fn fade_in(delay_ms: i64, dur: i64) -> Track {
    let start = ms(delay_ms).min(dur.saturating_sub(2).max(0));
    let end = ms(delay_ms + 420).min(dur.saturating_sub(1).max(1));
    let mut keys = Vec::new();
    if start > 0 {
        keys.push(Key::num(0, 0.0));
        keys.push(Key::num(start, 0.0));
    } else {
        keys.push(Key::num(0, 0.0));
    }
    keys.push(Key::num(end.max(start + 1), 1.0).with_ease(Ease::OutCubic));
    let last = end.max(start + 1);
    if dur > last {
        keys.push(Key::num(dur, 1.0));
    }
    Track::new(Prop::Opacity, keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: ChartKind, values: Vec<f64>) -> ChartSpec {
        ChartSpec {
            kind,
            labels: (0..values.len()).map(|i| format!("q{i}")).collect(),
            series: vec![Series {
                name: "s".into(),
                values,
                color: None,
            }],
            title: Some("Title".into()),
            subtitle: None,
            width: 1280,
            height: 720,
            seconds: 6.0,
        }
    }

    #[test]
    fn ticks_are_round_numbers_that_span_the_data() {
        let t = nice_ticks(0.0, 97.0, 5);
        assert_eq!(t.first(), Some(&0.0));
        assert!(
            *t.last().unwrap() >= 97.0,
            "ticks must cover the data: {t:?}"
        );
        // Every step identical, and a number a reader can do arithmetic with.
        let step = t[1] - t[0];
        assert!(
            [1.0, 2.0, 2.5, 5.0, 10.0, 20.0, 25.0, 50.0]
                .iter()
                .any(|c| (step - c).abs() < 1e-9),
            "step {step} is not a round number"
        );
    }

    #[test]
    fn ticks_do_not_drift() {
        // Accumulated addition gives 0.30000000000000004, which shows up on
        // the axis as a label nobody wants to explain.
        let t = nice_ticks(0.0, 0.5, 5);
        for v in &t {
            assert!(
                (v * 1e9).round() / 1e9 == *v,
                "tick {v} carries floating-point noise"
            );
        }
    }

    #[test]
    fn a_flat_series_still_gets_a_range() {
        // Every value equal means max-min is zero; without a band the scale
        // divides by zero and every point lands on the same pixel.
        let t = nice_ticks(42.0, 42.0, 5);
        assert!(t.len() >= 2, "{t:?}");
        assert!(t[0] < 42.0 && *t.last().unwrap() > 42.0, "{t:?}");
        let t0 = nice_ticks(0.0, 0.0, 5);
        assert!(t0.len() >= 2, "{t0:?}");
    }

    #[test]
    fn negative_values_are_covered() {
        let t = nice_ticks(-30.0, 80.0, 5);
        assert!(t[0] <= -30.0 && *t.last().unwrap() >= 80.0, "{t:?}");
    }

    #[test]
    fn tick_labels_follow_the_step_not_the_value() {
        // With a 0.5 step, 1.0 must read "1.0" — mixing "1" and "1.5" on one
        // axis looks like a mistake.
        assert_eq!(format_tick(1.0, 0.5), "1.0");
        assert_eq!(format_tick(1.5, 0.5), "1.5");
        assert_eq!(format_tick(40.0, 20.0), "40");
        assert_eq!(format_tick(20000.0, 5000.0), "20k");
        assert_eq!(format_tick(2500.0, 500.0), "2.5k");
    }

    #[test]
    fn thousands_are_only_abbreviated_when_it_stays_exact() {
        assert_eq!(format_tick(20000.0, 5000.0), "20k");
        assert_eq!(format_tick(2500.0, 500.0), "2.5k");
        // 1250 at one decimal is 1.2k, which disagrees with its own gridline,
        // so it stays written out.
        assert_eq!(format_tick(1250.0, 250.0), "1250");
    }

    #[test]
    fn every_kind_builds_a_valid_document() {
        for kind in [ChartKind::Line, ChartKind::Area, ChartKind::Bar] {
            let d = build(&spec(kind, vec![12.0, 40.0, 31.0, 78.0, 64.0])).unwrap();
            // Through the real loading path, not just the builder: a document
            // that only exists in memory proves nothing about what renders.
            let json = d.canonical_json();
            Document::from_json(&json)
                .unwrap_or_else(|e| panic!("{kind:?} chart does not validate: {e}"));
        }
    }

    #[test]
    fn a_ragged_series_is_rejected() {
        let mut s = spec(ChartKind::Bar, vec![1.0, 2.0, 3.0]);
        s.series[0].values.pop();
        assert!(build(&s).is_err());
    }

    #[test]
    fn an_empty_chart_is_rejected() {
        let mut s = spec(ChartKind::Line, vec![1.0, 2.0]);
        s.series.clear();
        assert!(build(&s).is_err());
    }

    #[test]
    fn the_left_margin_grows_with_the_widest_label() {
        // The reason widths are measured: a chart with five-digit values must
        // give its axis more room than one counting to ten, or the labels
        // collide with the plot.
        fn first_gridline_x(d: &Document) -> f64 {
            d.scenes[0]
                .elements
                .iter()
                .find_map(|e| match e {
                    Element::Path { points, .. } => Some(points[0][0].0),
                    _ => None,
                })
                .expect("a gridline")
        }
        let small = build(&spec(ChartKind::Line, vec![1.0, 9.0])).unwrap();
        let large = build(&spec(ChartKind::Line, vec![10000.0, 99000.0])).unwrap();
        assert!(
            first_gridline_x(&large) > first_gridline_x(&small),
            "the plot did not move right for wider labels"
        );
    }
}
