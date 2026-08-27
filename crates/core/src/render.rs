//! `Engine`: owns a validated `Document` + prepared `AssetStore` and renders
//! individual ticks to pixels. This is the one API native export, the wasm
//! shim, the corpus runner, and the bench harness all drive — see the task
//! brief for the exact shape.
//!
//! Determinism is law (spec §5): `render` is a pure function of
//! `(doc, assets, tick)` — the only mutation across calls is reuse of the
//! `frame`/`scratch` pixmap buffers (an allocation optimization, not a
//! semantic dependency on prior calls; two consecutive `render()` calls at
//! the same tick must produce identical bytes, see `tests/render.rs`).

use crate::assets::AssetStore;
use crate::doc::{Document, TIMEBASE};
use crate::raster::Renderer;
use crate::timeline::{layers_at, total_duration};
use crate::validate::DocError;
use tiny_skia::{BlendMode, FilterQuality, Pixmap, PixmapPaint, Transform};

/// Owns everything needed to render frames of one `Document`: the doc
/// itself, its prepared assets, the element renderer (glyph cache etc.),
/// and two reused full-canvas pixmap buffers (`frame`, the output buffer;
/// `scratch`, a transparent staging layer for crossfading scenes).
pub struct Engine {
    doc: Document,
    assets: AssetStore,
    renderer: Renderer,
    frame: Pixmap,
    scratch: Pixmap,
}

impl Engine {
    /// Construct an `Engine` for `doc`, decoding/loading `assets` against it.
    ///
    /// `doc` was already validated once by `Document::from_json` on the way
    /// in, but `new` doesn't trust that — it's a public API that can be
    /// called with a `Document` built any other way (e.g. the Rust SDK
    /// builder in `doc.rs`, which never touches `from_json` at all), so it
    /// re-runs semantic validation defensively via `validate::check`.
    pub fn new(doc: Document, mut assets: AssetStore) -> Result<Engine, DocError> {
        crate::validate::check(&doc)?;
        assets.prepare(&doc)?;

        let (w, h) = (doc.size.w, doc.size.h);
        let frame = Pixmap::new(w, h)
            .ok_or_else(|| DocError::Json(format!("invalid canvas size {w}x{h}")))?;
        let scratch = Pixmap::new(w, h)
            .ok_or_else(|| DocError::Json(format!("invalid canvas size {w}x{h}")))?;

        Ok(Engine {
            doc,
            assets,
            renderer: Renderer::new(),
            frame,
            scratch,
        })
    }

    pub fn width(&self) -> u32 {
        self.doc.size.w
    }

    pub fn height(&self) -> u32 {
        self.doc.size.h
    }

    pub fn total_duration(&self) -> i64 {
        total_duration(&self.doc)
    }

    /// The tick for output frame number `n` at export rate `fps`.
    pub fn tick_for_frame(&self, n: i64, fps: i64) -> i64 {
        n * (TIMEBASE / fps)
    }

    /// Render `tick` into the reused frame buffer and return its bytes:
    /// premultiplied RGBA8, row-major, `width() * height() * 4` long.
    ///
    /// Buffer reuse is an implementation detail, not an observable one:
    /// `frame` is fully overwritten every call (background fill, then every
    /// visible layer), so the same `tick` always produces the same bytes
    /// regardless of what was rendered before.
    pub fn render(&mut self, tick: i64) -> &[u8] {
        let (r, g, b, a) = self.doc.bg.rgba8();
        self.frame.fill(tiny_skia::Color::from_rgba8(r, g, b, a));

        for layer in layers_at(&self.doc, tick) {
            let scene = &self.doc.scenes[layer.scene];
            if layer.alpha >= 1.0 {
                self.renderer.draw_elements(
                    &mut self.frame.as_mut(),
                    &scene.elements,
                    &mut self.assets,
                    layer.local,
                    (0.0, 0.0),
                );
            } else {
                self.scratch.fill(tiny_skia::Color::TRANSPARENT);
                self.renderer.draw_elements(
                    &mut self.scratch.as_mut(),
                    &scene.elements,
                    &mut self.assets,
                    layer.local,
                    (0.0, 0.0),
                );
                let paint = PixmapPaint {
                    opacity: layer.alpha as f32,
                    blend_mode: BlendMode::SourceOver,
                    quality: FilterQuality::Bilinear,
                };
                self.frame.as_mut().draw_pixmap(
                    0,
                    0,
                    self.scratch.as_ref(),
                    &paint,
                    Transform::identity(),
                    None,
                );
            }
        }

        self.frame.data()
    }

    /// The current frame buffer, without rendering — a stable accessor for
    /// hosts (e.g. the wasm shim) that need a pointer to the bytes `render`
    /// last produced without triggering another render pass.
    pub fn frame_data(&self) -> &[u8] {
        self.frame.data()
    }
}
