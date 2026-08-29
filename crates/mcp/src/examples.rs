//! Reference documents a caller can imitate.
//!
//! Distinct from `kineto_core::corpus`, which exists to exercise the renderer
//! — easings, group nesting, wrap — and is a poor model for composition. An
//! agent imitating `kitchen-sink` produces coloured rectangles; an agent with
//! nothing to imitate produces centred prose on slides. These are neither.
//!
//! Each one is small, self-contained (bundled fonts, no image assets) and
//! demonstrates one *shot type*. That framing matters more than any single
//! example: a video reads as a slide deck when every scene has the same
//! shape, so the set exists to show that scenes can differ structurally, not
//! only in wording.
//!
//! - `statement` — full bleed, no chrome, one sentence. Breaks the rhythm.
//! - `split`     — text against a panel; the commonest two-column shot
//! - `flow`      — a relationship as a path between things
//! - `metric`    — one number, large, with the quantity actually shown
//! - `cards`     — a set of peers, entering in sequence
//! - `reveal`    — content arriving from behind a fixed window
//! - `steps`     — one idea per scene, with progress visible
//!
//! They are built through the Rust builders rather than embedded as JSON so
//! they cannot drift from the format, and a test renders each one through the
//! same lint the tools apply: an example we tell a model to copy has to pass
//! the rules we would judge its output by.

use kineto_core::doc::{ms, Cap, Clip, Ease, Gradient, Join, Key, Prop, Shadow, Stop, Track};
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
            name: "statement",
            description: "Full bleed, no header, one sentence. A shot with no \
                          chrome at all is what stops a sequence reading as a \
                          deck — use it to break rhythm, not to open.",
            doc: statement(),
        },
        Example {
            name: "split",
            description: "Text against a panel: the everyday two-column shot. \
                          Rounded corners, a gradient and a shadow are what \
                          separate a panel from a rectangle.",
            doc: split(),
        },
        Example {
            name: "cards",
            description: "A set of peers entering in sequence. Stagger the \
                          entrances and let them overshoot — simultaneous \
                          arrival reads as a diagram, sequenced reads as \
                          motion.",
            doc: cards(),
        },
        Example {
            name: "reveal",
            description: "Content arriving from behind a fixed window. A clip \
                          does not travel with its element, so the content \
                          slides and the frame stays — that is the only \
                          reveal in this format that is not a fade.",
            doc: reveal(),
        },
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

/// No header, no rule, no kicker: the absence of chrome is the point.
fn statement() -> Document {
    let mut d = base();
    let dur = ms(5000);
    let sc = Scene::new("statement", dur)
        // A full-canvas gradient, so the shot does not share the flat
        // background every other scene uses.
        .with_element(Element::rect(
            [0.0, 0.0, W as f64, H as f64],
            Gradient::linear(
                [0.0, 0.0],
                [1.0, 1.0],
                vec![Stop::new(0.0, "#12202b"), Stop::new(1.0, BG)],
            ),
        ))
        .with_element(
            Element::text(
                "Determinism is not a feature.\nIt is what makes the rest testable.",
                "inter",
                58.0,
                FG,
                [110.0, 250.0],
            )
            .with_max_w(1060.0)
            .with_animation(fade(500, dur)),
        )
        .with_element(
            Element::path(vec![[110.0, 470.0], [230.0, 470.0]])
                .with_stroke(ACCENT, 5.0)
                .with_cap(Cap::Round)
                .with_animation(fade(900, dur)),
        );
    d.push_scene(sc);
    d
}

/// Text on the left, a panel on the right. Rounded, gradient-filled and
/// shadowed — the three things that separate a panel from a rectangle.
fn split() -> Document {
    let mut d = base();
    let dur = ms(5500);
    let sc = Scene::new("split", dur)
        .with_element(kicker("THE SHAPE", dur))
        .with_element(
            Element::text(
                "Two columns, not two\nparagraphs.",
                "inter",
                44.0,
                FG,
                [90.0, 250.0],
            )
            .with_max_w(480.0)
            .with_animation(fade(400, dur)),
        )
        .with_element(
            Element::text(
                "Put the words on one side and the thing itself on the other.",
                "mono",
                19.0,
                DIM,
                [92.0, 400.0],
            )
            .with_max_w(470.0)
            .with_animation(fade(700, dur)),
        )
        .with_element(
            Element::rect(
                [660.0, 190.0, 520.0, 340.0],
                Gradient::linear(
                    [0.0, 0.0],
                    [1.0, 1.0],
                    vec![Stop::new(0.0, "#1d2b36"), Stop::new(1.0, "#14202a")],
                ),
            )
            .with_radius(18.0)
            .with_shadow(Shadow::new("#00000066", 24, 0.0, 12.0))
            .with_animation(fade(600, dur)),
        )
        .with_element(
            Element::path(vec![[700.0, 300.0], [900.0, 300.0]])
                .with_stroke(TEAL, 4.0)
                .with_cap(Cap::Round)
                .with_animation(fade(900, dur)),
        )
        .with_element(
            Element::path(vec![[700.0, 350.0], [1030.0, 350.0]])
                .with_stroke("#2c3d4a", 4.0)
                .with_cap(Cap::Round)
                .with_animation(fade(1000, dur)),
        )
        .with_element(
            Element::path(vec![[700.0, 400.0], [840.0, 400.0]])
                .with_stroke("#2c3d4a", 4.0)
                .with_cap(Cap::Round)
                .with_animation(fade(1100, dur)),
        );
    d.push_scene(sc);
    d
}

/// Three peers, entering one after another and overshooting slightly. The
/// stagger is what turns a diagram into motion.
fn cards() -> Document {
    let mut d = base();
    let dur = ms(6000);
    let mut sc = Scene::new("cards", dur)
        .with_element(kicker("A SET", dur))
        .with_element(title("Peers arrive in sequence.", dur));
    let labels = ["deterministic", "headless", "inspectable"];
    for (i, label) in labels.iter().enumerate() {
        let x = 100.0 + i as f64 * 366.0;
        let delay = 400 + i as i64 * 220;
        sc = sc
            .with_element(
                Element::rect([x, 280.0, 330.0, 200.0], "#16212a")
                    .with_radius(16.0)
                    .with_shadow(Shadow::new("#00000059", 18, 0.0, 10.0))
                    .with_animation(fade(delay, dur))
                    // Overshoot: it rises past its resting place and settles.
                    .with_animation(Track::new(
                        Prop::Translate,
                        vec![
                            Key::vec2(0, [0.0, 46.0]),
                            Key::vec2(ms(delay), [0.0, 46.0]),
                            Key::vec2(ms(delay + 620), [0.0, 0.0]).with_ease(Ease::OutBack),
                        ],
                    )),
            )
            .with_element(
                Element::text(label, "mono", 19.0, FG, [x + 24.0, 330.0])
                    .with_animation(fade(delay + 120, dur)),
            )
            .with_element(
                Element::path(vec![[x + 24.0, 430.0], [x + 24.0 + 60.0, 430.0]])
                    .with_stroke([ACCENT, TEAL, "#C77DFF"][i], 4.0)
                    .with_cap(Cap::Round)
                    .with_animation(fade(delay + 200, dur)),
            );
    }
    d.push_scene(sc);
    d
}

/// A fixed window with content sliding in behind it. The clip does not move
/// with the element, which is what makes this a reveal rather than a fade.
fn reveal() -> Document {
    let mut d = base();
    let dur = ms(5500);
    let (wx, wy, ww, wh) = (150.0, 250.0, 980.0, 180.0);
    let sc = Scene::new("reveal", dur)
        .with_element(kicker("ARRIVAL", dur))
        .with_element(title("Slide in behind a window.", dur))
        // The window frame, so the mechanism is visible.
        .with_element(
            Element::rect([wx - 2.0, wy - 2.0, ww + 4.0, wh + 4.0], "#16212a")
                .with_radius(14.0)
                .with_animation(fade(400, dur)),
        )
        .with_element(
            Element::rect(
                [wx, wy, ww, wh],
                Gradient::linear(
                    [0.0, 0.0],
                    [1.0, 0.0],
                    vec![Stop::new(0.0, ACCENT), Stop::new(1.0, "#C77DFF")],
                ),
            )
            .with_radius(12.0)
            .with_clip(Clip::new([wx, wy, ww, wh]).with_radius(12.0))
            .with_animation(Track::new(
                Prop::Translate,
                vec![
                    Key::vec2(0, [-ww, 0.0]),
                    Key::vec2(ms(1400), [0.0, 0.0]).with_ease(Ease::OutCubic),
                ],
            )),
        )
        .with_element(
            Element::text(
                "the clip stays put; the content moves",
                "mono",
                18.0,
                DIM,
                [wx + 2.0, wy + wh + 34.0],
            )
            .with_animation(fade(1600, dur)),
        );
    d.push_scene(sc);
    d
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
    // Two bars: the quantity is shown, not only stated. The second carries a
    // gradient — a flat fill reads as a chart, a gradient reads as designed,
    // and it costs one extra field.
    let before =
        Element::rect([190.0, 512.0, 1000.0, 18.0], "#26333d").with_animation(fade(1200, dur));
    let after = Element::rect(
        [190.0, 566.0, 45.0, 18.0],
        Gradient::linear(
            [0.0, 0.0],
            [1.0, 0.0],
            vec![Stop::new(0.0, TEAL), Stop::new(1.0, "#7CE0D8")],
        ),
    )
    .with_animation(fade(1400, dur));
    sc = sc
        .with_element(
            Element::text("before", "mono", 15.0, DIM, [90.0, 508.0])
                .with_animation(fade(1200, dur)),
        )
        .with_element(before)
        .with_element(
            Element::text("after", "mono", 15.0, DIM, [90.0, 562.0])
                .with_animation(fade(1400, dur)),
        )
        .with_element(after);
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
