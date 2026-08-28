//! Reference documents a caller can imitate.
//!
//! Distinct from `kineto_core::corpus`, which exists to exercise the renderer
//! — easings, group nesting, wrap — and is a poor model for composition. An
//! agent imitating `kitchen-sink` produces coloured rectangles; an agent with
//! nothing to imitate produces centred prose on slides. These are neither.
//!
//! Each one is small, self-contained (bundled fonts, no image assets) and
//! demonstrates one habit worth copying:
//!
//! - `flow`   — show a relationship as a path between two things
//! - `metric` — put the number on screen, large, with its context beneath
//! - `steps`  — one idea per scene, marked so progress is visible
//!
//! They are built through the Rust builders rather than embedded as JSON so
//! they cannot drift from the format, and a test renders each one through the
//! same lint the tools apply: an example we tell a model to copy has to pass
//! the rules we would judge its output by.

use kineto_core::doc::{ms, Cap, Ease, Join, Key, Prop, Track};
use kineto_core::{Asset, Document, Element, Scene};

const W: u32 = 1280;
const H: u32 = 720;
const BG: &str = "#0D1419";
const FG: &str = "#F2F5F7";
const DIM: &str = "#8FA3B0";
const ACCENT: &str = "#FF9900";
const TEAL: &str = "#4ECDC4";
const PANEL: &str = "#16212a";
const EDGE: &str = "#3d5566";

pub struct Example {
    pub name: &'static str,
    pub description: &'static str,
    pub doc: Document,
}

pub fn examples() -> Vec<Example> {
    vec![
        Example {
            name: "flow",
            description: "A relationship drawn as a path between two things, \
                          rather than described in a sentence.",
            doc: flow(),
        },
        Example {
            name: "metric",
            description: "One number, large, with its context beneath it. A \
                          measured quantity on screen beats a claim about it.",
            doc: metric(),
        },
        Example {
            name: "steps",
            description: "One idea per scene, with a rule marking progress so \
                          a viewer knows where they are.",
            doc: steps(),
        },
    ]
}

fn base() -> Document {
    let mut d = Document::new(W, H).with_fps(30).with_bg(BG);
    d.add_asset("inter", Asset::font("kineto:inter"));
    d.add_asset("mono", Asset::font("kineto:jetbrains-mono"));
    d
}

/// Opacity in by `hold`, held, out before the scene ends.
fn fade(hold_ms: i64, dur: i64) -> Track {
    Track::new(
        Prop::Opacity,
        vec![
            Key::num(0, 0.0),
            Key::num(ms(hold_ms), 1.0).with_ease(Ease::OutCubic),
            Key::num(dur - ms(340), 1.0),
            Key::num(dur, 0.0).with_ease(Ease::OutCubic),
        ],
    )
}

fn kicker(text: &str, dur: i64) -> Element {
    Element::text(text, "mono", 16.0, ACCENT, [90.0, 96.0]).with_animation(fade(240, dur))
}

fn title(text: &str, dur: i64) -> Element {
    Element::text(text, "inter", 44.0, FG, [90.0, 128.0])
        .with_max_w(1100.0)
        .with_animation(fade(400, dur))
}

fn boxed(x: f64, y: f64, w: f64, h: f64, label: &str, dur: i64, hold: i64) -> Vec<Element> {
    vec![
        Element::path(vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]])
            .with_closed(true)
            .with_path_fill(PANEL)
            .with_stroke(EDGE, 2.0)
            .with_join(Join::Round)
            .with_animation(fade(hold, dur)),
        Element::text(label, "mono", 16.0, FG, [x + 16.0, y + h / 2.0 - 10.0])
            .with_animation(fade(hold + 80, dur)),
    ]
}

/// Shaft plus an oriented head. The trigonometry lives here, in the authoring
/// layer, which is why the format carries no arrow type.
fn arrow(x0: f64, y0: f64, x1: f64, y1: f64, color: &str, dur: i64, hold: i64) -> Vec<Element> {
    let a = (y1 - y0).atan2(x1 - x0);
    let head = 14.0;
    let (bx, by) = (x1 - head * a.cos(), y1 - head * a.sin());
    let s = head * 0.44;
    vec![
        Element::path(vec![[x0, y0], [bx, by]])
            .with_stroke(color, 2.5)
            .with_cap(Cap::Round)
            .with_animation(fade(hold, dur)),
        Element::path(vec![
            [x1, y1],
            [bx - s * a.sin(), by + s * a.cos()],
            [bx + s * a.sin(), by - s * a.cos()],
        ])
        .with_closed(true)
        .with_path_fill(color)
        .with_animation(fade(hold, dur)),
    ]
}

fn flow() -> Document {
    let mut d = base();
    let dur = ms(6000);
    let mut sc = Scene::new("flow", dur)
        .with_element(kicker("HOW IT MOVES", dur))
        .with_element(title("A request, end to end.", dur));
    for e in boxed(110.0, 330.0, 250.0, 84.0, "  Client", dur, 600) {
        sc = sc.with_element(e);
    }
    for e in boxed(515.0, 330.0, 250.0, 84.0, "  API", dur, 900) {
        sc = sc.with_element(e);
    }
    for e in boxed(920.0, 330.0, 250.0, 84.0, "  Store", dur, 1200) {
        sc = sc.with_element(e);
    }
    for e in arrow(370.0, 372.0, 505.0, 372.0, ACCENT, dur, 1500) {
        sc = sc.with_element(e);
    }
    for e in arrow(775.0, 372.0, 910.0, 372.0, ACCENT, dur, 1700) {
        sc = sc.with_element(e);
    }
    sc = sc.with_element(
        Element::text(
            "each hop is a path, not a bullet",
            "mono",
            17.0,
            DIM,
            [110.0, 470.0],
        )
        .with_animation(fade(2000, dur)),
    );
    d.push_scene(sc);
    d
}

fn metric() -> Document {
    let mut d = base();
    let dur = ms(6000);
    let mut sc = Scene::new("metric", dur)
        .with_element(kicker("RESULT", dur))
        .with_element(title("Peak memory, after bounding residency.", dur))
        .with_element(
            Element::text("53 MB", "inter", 128.0, FG, [90.0, 268.0])
                .with_animation(fade(500, dur)),
        )
        .with_element(
            Element::text(
                "down from 1185 MB, and flat in document length",
                "mono",
                20.0,
                DIM,
                [94.0, 424.0],
            )
            .with_animation(fade(900, dur)),
        );
    // Two bars: the quantity is shown, not only stated.
    let bars = [("before", 1000.0, "#3d5566"), ("after", 45.0, TEAL)];
    for (i, (label, width, colour)) in bars.iter().enumerate() {
        let y = 512.0 + i as f64 * 54.0;
        sc = sc
            .with_element(
                Element::text(label, "mono", 15.0, DIM, [90.0, y - 4.0])
                    .with_animation(fade(1200 + i as i64 * 200, dur)),
            )
            .with_element(
                Element::rect([190.0, y, *width, 18.0], *colour)
                    .with_animation(fade(1300 + i as i64 * 200, dur)),
            );
    }
    d.push_scene(sc);
    d
}

fn steps() -> Document {
    let mut d = base();
    // Four rather than three, deliberately: the deckShaped rule only judges
    // documents of four scenes or more, so a three-scene example would sit
    // outside the very check this document exists to demonstrate passing.
    let beats = [
        ("Record what happened", "one semantic beat per thing done"),
        (
            "Project it to a document",
            "the journal is the record, not the video",
        ),
        (
            "Check before rendering",
            "correctness costs a tenth of a picture",
        ),
        (
            "Render once, at the end",
            "seconds and a file, only when it is right",
        ),
    ];
    for (i, (head, sub)) in beats.iter().enumerate() {
        let dur = ms(4200);
        let done = 90.0 + 1100.0 * (i + 1) as f64 / beats.len() as f64;
        let sc = Scene::new(&format!("s{i}"), dur)
            .with_element(kicker(&format!("STEP {}", i + 1), dur))
            .with_element(title(head, dur))
            .with_element(
                Element::text(sub, "mono", 20.0, DIM, [92.0, 240.0]).with_animation(fade(700, dur)),
            )
            .with_element(
                Element::path(vec![[90.0, 560.0], [1190.0, 560.0]])
                    .with_stroke("#22303b", 3.0)
                    .with_animation(fade(300, dur)),
            )
            .with_element(
                Element::path(vec![[90.0, 560.0], [done, 560.0]])
                    .with_stroke(ACCENT, 3.0)
                    .with_cap(Cap::Round)
                    .with_animation(fade(500, dur)),
            );
        d.push_scene(sc);
    }
    d
}
