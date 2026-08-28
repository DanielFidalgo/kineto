use crate::{color::Color, scalar::Scalar};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const TIMEBASE: i64 = 705_600_000;
pub fn seconds(s: f64) -> i64 {
    (s * TIMEBASE as f64).round() as i64
}
pub fn ms(m: i64) -> i64 {
    m * 705_600
}
pub fn frames(n: i64, fps: i64) -> i64 {
    assert!(
        TIMEBASE % fps == 0,
        "unsupported fps {fps}: must divide {TIMEBASE}"
    );
    n * (TIMEBASE / fps)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub v: u32,
    pub timebase: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_fps: Option<u32>,
    pub size: Size,
    #[serde(default = "default_bg", skip_serializing_if = "Color::is_default_bg")]
    pub bg: Color,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub assets: BTreeMap<String, Asset>,
    pub scenes: Vec<Scene>,
}
fn default_bg() -> Color {
    Color("#000000".into())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Asset {
    Image { src: String },
    Font { src: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scene {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition: Option<Transition>,
    pub duration: i64,
    pub elements: Vec<Element>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Transition {
    Crossfade { duration: i64 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Element {
    Image {
        asset: String,
        rect: [Scalar; 4],
        #[serde(default, skip_serializing_if = "Fit::is_default")]
        fit: Fit,
        #[serde(flatten)]
        common: Common,
    },
    Text {
        text: String,
        font: String,
        size_px: Scalar,
        color: Color,
        pos: [Scalar; 2],
        #[serde(skip_serializing_if = "Option::is_none")]
        max_w: Option<Scalar>,
        #[serde(default, skip_serializing_if = "Align::is_default")]
        align: Align,
        #[serde(flatten)]
        common: Common,
    },
    Rect {
        rect: [Scalar; 4],
        fill: Paint,
        /// Corner radius in pixels, clamped at draw time to half the shorter
        /// edge. Absent means square corners, and serialises to nothing.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        radius: Option<Scalar>,
        #[serde(flatten)]
        common: Common,
    },
    /// An open or closed polyline: straight segments only.
    ///
    /// No curves in v1, deliberately. Curve flattening carries a tolerance
    /// parameter and is the most parity-fragile part of a path renderer,
    /// while straight segments already cover connectors, arrows, axes and
    /// boxes-and-lines diagrams. Add béziers when a document needs one.
    ///
    /// Arrow *heads* are not a field here — a filled closed path expresses
    /// one, and orienting it is the authoring layer's job.
    Path {
        points: Vec<[Scalar; 2]>,
        #[serde(default, skip_serializing_if = "is_false")]
        closed: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stroke: Option<Color>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stroke_width: Option<Scalar>,
        #[serde(default, skip_serializing_if = "Cap::is_default")]
        cap: Cap,
        #[serde(default, skip_serializing_if = "Join::is_default")]
        join: Join,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<Paint>,
        #[serde(flatten)]
        common: Common,
    },
    Group {
        origin: [Scalar; 2],
        children: Vec<Element>,
        #[serde(flatten)]
        common: Common,
    },
}

// `Eq + Hash` (beyond the `PartialEq` the doc model needs) so `Align` can be
// part of the renderer's text-layout cache key (see `raster::LayoutKey`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}
impl Align {
    pub fn is_default(&self) -> bool {
        *self == Align::Left
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// What fills a shape: a flat colour, or a gradient.
///
/// Untagged so a bare `"#RRGGBB"` still deserialises — and still *serialises*
/// — exactly as before. Making this a union had to leave every existing
/// document byte-identical, or the goldens and the cross-SDK comparison would
/// all move at once for a feature they do not use.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Paint {
    Solid(Color),
    Gradient(Gradient),
}

impl From<Color> for Paint {
    fn from(c: Color) -> Self {
        Paint::Solid(c)
    }
}
impl From<&str> for Paint {
    fn from(s: &str) -> Self {
        Paint::Solid(Color(s.to_string()))
    }
}
impl From<Gradient> for Paint {
    fn from(g: Gradient) -> Self {
        Paint::Gradient(g)
    }
}

/// A gradient in **unit coordinates over the element's own bounding box**:
/// `[0,0]` is its top-left and `[1,1]` its bottom-right.
///
/// Relative rather than absolute so one gradient reads the same on a 200px
/// card and a 1200px panel — which is what makes a gradient reusable across a
/// template rather than tied to one layout.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Gradient {
    Linear {
        from: [Scalar; 2],
        to: [Scalar; 2],
        stops: Vec<Stop>,
    },
    Radial {
        center: [Scalar; 2],
        /// Fraction of the box's longer edge.
        radius: Scalar,
        stops: Vec<Stop>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stop {
    /// Position along the gradient, 0 to 1.
    pub at: Scalar,
    pub color: Color,
}

impl Stop {
    pub fn new(at: f64, color: impl Into<Color>) -> Self {
        Stop {
            at: Scalar(at),
            color: color.into(),
        }
    }
}

impl Gradient {
    pub fn linear(from: [f64; 2], to: [f64; 2], stops: Vec<Stop>) -> Self {
        Gradient::Linear {
            from: scalars2(from),
            to: scalars2(to),
            stops,
        }
    }
    pub fn radial(center: [f64; 2], radius: f64, stops: Vec<Stop>) -> Self {
        Gradient::Radial {
            center: scalars2(center),
            radius: Scalar(radius),
            stops,
        }
    }
    pub fn stops(&self) -> &[Stop] {
        match self {
            Gradient::Linear { stops, .. } | Gradient::Radial { stops, .. } => stops,
        }
    }
}

/// How a stroke terminates. These are rasterizer parameters, not geometry —
/// unlike an arrowhead, they cannot be expressed by adding points, so they
/// belong in the format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Cap {
    #[default]
    Butt,
    Round,
    Square,
}
impl Cap {
    pub fn is_default(&self) -> bool {
        *self == Cap::Butt
    }
}

/// How two stroke segments meet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Join {
    #[default]
    Miter,
    Round,
    Bevel,
}
impl Join {
    pub fn is_default(&self) -> bool {
        *self == Join::Miter
    }
}

/// A static window an element is drawn through.
///
/// In the element's **parent** coordinate space, and deliberately *not*
/// carried by the element's own transform: a clip that moved with its
/// content would never reveal anything. Content animates behind a fixed
/// window — which is what a wipe, a crop, or a progress fill actually is.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Clip {
    pub rect: [Scalar; 4],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<Scalar>,
}

impl Clip {
    pub fn new(rect: [f64; 4]) -> Self {
        Clip {
            rect: scalars4(rect),
            radius: None,
        }
    }
    pub fn with_radius(mut self, r: f64) -> Self {
        self.radius = Some(Scalar(r));
        self
    }
}

/// How an image fills its box when the aspect ratios differ.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Fit {
    /// Distort to fill exactly. The v1 behaviour, and the default so no
    /// existing document changes.
    #[default]
    Stretch,
    /// Scale until it fits inside, centred. Nothing is cropped.
    Contain,
    /// Scale until it covers, centred, cropped to the box.
    Cover,
}
impl Fit {
    pub fn is_default(&self) -> bool {
        *self == Fit::Stretch
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Common {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translate: Option<[Scalar; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<Scalar>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<Scalar>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<Scalar>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub animations: Vec<Track>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<Clip>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track {
    pub prop: Prop,
    pub keys: Vec<Key>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Prop {
    Translate,
    Scale,
    Rotation,
    Opacity,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Key {
    pub t: i64,
    pub v: KeyValue,
    #[serde(default, skip_serializing_if = "Ease::is_default")]
    pub ease: Ease,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KeyValue {
    Num(Scalar),
    Vec2([Scalar; 2]),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Ease {
    #[default]
    Linear,
    InCubic,
    OutCubic,
    InOutCubic,
    /// Overshoots past the target and settles back. What makes motion read as
    /// alive rather than mechanical — and the reason `resolve_common` clamps
    /// opacity, since these deliberately leave 0..1.
    InBack,
    OutBack,
    InOutBack,
    /// Very slow then very fast, or the reverse. For entrances that should
    /// feel sudden.
    InExpo,
    OutExpo,
    InOutExpo,
}
impl Ease {
    pub fn is_default(&self) -> bool {
        *self == Ease::Linear
    }
}

// ---- Builder impls (Rust authoring surface) ----

impl Document {
    pub fn new(w: u32, h: u32) -> Self {
        Document {
            v: 1,
            timebase: TIMEBASE,
            default_fps: None,
            size: Size { w, h },
            bg: default_bg(),
            assets: BTreeMap::new(),
            scenes: vec![],
        }
    }
    pub fn with_fps(mut self, fps: u32) -> Self {
        self.default_fps = Some(fps);
        self
    }
    pub fn with_bg(mut self, bg: impl Into<Color>) -> Self {
        self.bg = bg.into();
        self
    }
    pub fn add_asset(&mut self, id: &str, a: Asset) {
        self.assets.insert(id.to_string(), a);
    }
    pub fn push_scene(&mut self, s: Scene) {
        self.scenes.push(s);
    }
    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

impl Asset {
    pub fn image(src: &str) -> Self {
        Asset::Image {
            src: src.to_string(),
        }
    }
    pub fn font(src: &str) -> Self {
        Asset::Font {
            src: src.to_string(),
        }
    }
}

impl Scene {
    pub fn new(id: &str, duration: i64) -> Self {
        Scene {
            id: id.to_string(),
            transition: None,
            duration,
            elements: vec![],
        }
    }
    pub fn with_transition(mut self, t: Transition) -> Self {
        self.transition = Some(t);
        self
    }
    pub fn with_element(mut self, e: Element) -> Self {
        self.elements.push(e);
        self
    }
}

impl Transition {
    pub fn crossfade(duration: i64) -> Self {
        Transition::Crossfade { duration }
    }
}

fn scalars2(v: [f64; 2]) -> [Scalar; 2] {
    [Scalar(v[0]), Scalar(v[1])]
}
fn scalars4(v: [f64; 4]) -> [Scalar; 4] {
    [Scalar(v[0]), Scalar(v[1]), Scalar(v[2]), Scalar(v[3])]
}

impl Element {
    pub fn image(asset: &str, rect: [f64; 4]) -> Self {
        Element::Image {
            asset: asset.to_string(),
            rect: scalars4(rect),
            fit: Fit::default(),
            common: Common::default(),
        }
    }
    pub fn with_fit(mut self, f: Fit) -> Self {
        if let Element::Image { fit, .. } = &mut self {
            *fit = f;
        }
        self
    }
    pub fn with_clip(mut self, c: Clip) -> Self {
        self.common_mut().clip = Some(c);
        self
    }
    pub fn text(
        text: &str,
        font: &str,
        size_px: f64,
        color: impl Into<Color>,
        pos: [f64; 2],
    ) -> Self {
        Element::Text {
            text: text.to_string(),
            font: font.to_string(),
            size_px: Scalar(size_px),
            color: color.into(),
            pos: scalars2(pos),
            max_w: None,
            align: Align::default(),
            common: Common::default(),
        }
    }
    pub fn rect(rect: [f64; 4], fill: impl Into<Paint>) -> Self {
        Element::Rect {
            rect: scalars4(rect),
            fill: fill.into(),
            radius: None,
            common: Common::default(),
        }
    }
    pub fn with_radius(mut self, r: f64) -> Self {
        if let Element::Rect { radius, .. } = &mut self {
            *radius = Some(Scalar(r));
        }
        self
    }
    pub fn group(origin: [f64; 2], children: Vec<Element>) -> Self {
        Element::Group {
            origin: scalars2(origin),
            children,
            common: Common::default(),
        }
    }

    /// Read-only mirror of `common_mut`, for consumers that inspect an
    /// element's animated properties without owning it.
    pub fn common(&self) -> &Common {
        match self {
            Element::Image { common, .. } => common,
            Element::Text { common, .. } => common,
            Element::Rect { common, .. } => common,
            Element::Path { common, .. } => common,
            Element::Group { common, .. } => common,
        }
    }

    fn common_mut(&mut self) -> &mut Common {
        match self {
            Element::Image { common, .. } => common,
            Element::Text { common, .. } => common,
            Element::Rect { common, .. } => common,
            Element::Group { common, .. } => common,
            Element::Path { common, .. } => common,
        }
    }

    pub fn path(points: Vec<[f64; 2]>) -> Self {
        Element::Path {
            points: points.into_iter().map(scalars2).collect(),
            closed: false,
            stroke: None,
            stroke_width: None,
            cap: Cap::default(),
            join: Join::default(),
            fill: None,
            common: Common::default(),
        }
    }
    pub fn with_stroke(mut self, color: impl Into<Color>, width: f64) -> Self {
        if let Element::Path {
            stroke,
            stroke_width,
            ..
        } = &mut self
        {
            *stroke = Some(color.into());
            *stroke_width = Some(Scalar(width));
        }
        self
    }
    pub fn with_path_fill(mut self, color: impl Into<Paint>) -> Self {
        if let Element::Path { fill, .. } = &mut self {
            *fill = Some(color.into());
        }
        self
    }
    pub fn with_closed(mut self, v: bool) -> Self {
        if let Element::Path { closed, .. } = &mut self {
            *closed = v;
        }
        self
    }
    pub fn with_cap(mut self, c: Cap) -> Self {
        if let Element::Path { cap, .. } = &mut self {
            *cap = c;
        }
        self
    }
    pub fn with_join(mut self, j: Join) -> Self {
        if let Element::Path { join, .. } = &mut self {
            *join = j;
        }
        self
    }
    pub fn with_max_w(mut self, w: f64) -> Self {
        if let Element::Text { max_w, .. } = &mut self {
            *max_w = Some(Scalar(w));
        }
        self
    }
    pub fn with_align(mut self, align: Align) -> Self {
        if let Element::Text { align: a, .. } = &mut self {
            *a = align;
        }
        self
    }
    pub fn with_translate(mut self, v: [f64; 2]) -> Self {
        self.common_mut().translate = Some(scalars2(v));
        self
    }
    pub fn with_scale(mut self, v: f64) -> Self {
        self.common_mut().scale = Some(Scalar(v));
        self
    }
    pub fn with_rotation(mut self, v: f64) -> Self {
        self.common_mut().rotation = Some(Scalar(v));
        self
    }
    pub fn with_opacity(mut self, v: f64) -> Self {
        self.common_mut().opacity = Some(Scalar(v));
        self
    }
    pub fn with_animation(mut self, t: Track) -> Self {
        self.common_mut().animations.push(t);
        self
    }
}

impl Track {
    pub fn new(prop: Prop, keys: Vec<Key>) -> Self {
        Track { prop, keys }
    }
}

impl Key {
    pub fn num(t: i64, v: f64) -> Self {
        Key {
            t,
            v: KeyValue::Num(Scalar(v)),
            ease: Ease::default(),
        }
    }
    pub fn vec2(t: i64, v: [f64; 2]) -> Self {
        Key {
            t,
            v: KeyValue::Vec2(scalars2(v)),
            ease: Ease::default(),
        }
    }
    pub fn with_ease(mut self, ease: Ease) -> Self {
        self.ease = ease;
        self
    }
}
