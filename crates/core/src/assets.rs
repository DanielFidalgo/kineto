//! AssetStore: rust-side image decode + explicit font loading.
//!
//! Determinism is law (spec §5): `FontSystem` is always built with an
//! **empty** font database and a fixed locale (`"en-US"`) — never
//! `FontSystem::new()`, which pulls in whatever fonts happen to be
//! installed on the host OS. Callers must explicitly stage font bytes via
//! `add_bytes` before `prepare`; there is no fallback to system fonts on
//! either target (native or wasm).
//!
//! Image bytes are decoded to RGBA8 by the `image` crate (same code path on
//! native and wasm) and then converted to **premultiplied** alpha, because
//! `tiny_skia::Pixmap::from_vec` requires premultiplied RGBA pixels. The
//! premultiply step uses `tiny_skia::ColorU8::premultiply`, which is pure
//! integer math (`p = ((c*a + 128) + ((c*a + 128) >> 8)) >> 8`, tiny-skia's
//! own rounding-division-by-255 formula) — no floats, so it is bit-identical
//! on native and wasm.

use crate::doc::{Asset, Document};
use crate::validate::DocError;
use std::collections::HashMap;
use std::sync::Arc;

/// Decoded/loaded assets, keyed by the doc-level asset id.
/// Default ceiling on decoded image bytes held at once.
///
/// A decoded frame is `w*h*4` — 4.1 MB at 1280x800 — so holding every image
/// a document references is linear in frame count: a 300-frame tape measured
/// 1185 MB resident, and a 10,000-frame storyboard would need ~40 GB. The
/// engine never needs more than the frames on screen at one tick, which is
/// one, or two mid-crossfade. 32 MB is eight full-canvas frames at 1280x800:
/// four times the working set, and flat in the length of the document.
pub const DEFAULT_IMAGE_BUDGET_BYTES: usize = 32 * 1024 * 1024;

pub struct AssetStore {
    /// Raw bytes staged by the host via `add_bytes`, keyed by asset id.
    /// These are the *compressed* form (a JPEG screenshot is ~150 KB against
    /// 4.1 MB decoded) and are retained: they are what a cache miss decodes
    /// from, and in wasm there is no filesystem to re-read.
    staged: HashMap<String, Vec<u8>>,
    /// Decoded images (premultiplied RGBA) currently resident, keyed by asset
    /// id. Bounded by `budget_bytes`; a miss re-decodes from `staged`.
    images: HashMap<String, tiny_skia::Pixmap>,
    /// Asset ids in least-recently-used order.
    recency: Vec<String>,
    /// Sum of `images`' decoded sizes.
    resident: usize,
    /// Ceiling on `resident`, in bytes.
    budget_bytes: usize,
    /// Resolved font family name for each font asset id.
    fonts: HashMap<String, String>,
    /// Shared font system (empty db + fixed locale); all font assets load
    /// their faces into this database during `prepare`.
    font_system: cosmic_text::FontSystem,
}

impl Default for AssetStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetStore {
    pub fn new() -> Self {
        AssetStore {
            staged: HashMap::new(),
            recency: Vec::new(),
            resident: 0,
            budget_bytes: DEFAULT_IMAGE_BUDGET_BYTES,
            images: HashMap::new(),
            fonts: HashMap::new(),
            font_system: cosmic_text::FontSystem::new_with_locale_and_db(
                "en-US".to_string(),
                fontdb::Database::new(),
            ),
        }
    }

    /// Stage raw bytes for a doc asset id. Must be called before `prepare`
    /// for every asset the document references.
    pub fn add_bytes(&mut self, id: &str, bytes: Vec<u8>) {
        self.staged.insert(id.to_string(), bytes);
    }

    /// Decode/load every asset referenced by `doc` from its staged bytes.
    /// An asset id with no staged bytes is `DocError::UnknownAssetId`.
    pub fn prepare(&mut self, doc: &Document) -> Result<(), DocError> {
        for (id, asset) in &doc.assets {
            let bytes = self
                .staged
                .get(id)
                .ok_or_else(|| DocError::UnknownAssetId(id.clone()))?;
            match asset {
                Asset::Image { .. } => {
                    // Still decoded here, even though the pixmap may be
                    // dropped again immediately: `validateOnly` promises that
                    // a missing or corrupt image is reported before anything
                    // renders. Deferring decode to first draw would quietly
                    // break that. Peak stays at the budget plus one image.
                    let pixmap = decode_image(id, bytes)?;
                    self.insert_image(id.clone(), pixmap);
                    self.evict_to_budget(None);
                }
                Asset::Font { .. } => {
                    let family = load_font(id, bytes, &mut self.font_system)?;
                    self.fonts.insert(id.clone(), family);
                }
            }
        }
        Ok(())
    }

    /// Decoded image bytes currently held. Diagnostic, and what the
    /// residency test asserts on.
    pub fn resident_bytes(&self) -> usize {
        self.resident
    }

    /// Set the ceiling on decoded image bytes. Affects memory only, never
    /// output: decode is a pure function of the staged bytes, so a cache miss
    /// reproduces a hit exactly.
    pub fn set_image_budget(&mut self, bytes: usize) {
        self.budget_bytes = bytes;
        self.evict_to_budget(None);
    }

    fn insert_image(&mut self, id: String, pixmap: tiny_skia::Pixmap) {
        if let Some(old) = self.images.remove(&id) {
            self.resident -= old.data().len();
        }
        self.resident += pixmap.data().len();
        self.recency.retain(|k| k != &id);
        self.recency.push(id.clone());
        self.images.insert(id, pixmap);
    }

    /// Drop least-recently-used images until within budget, never evicting
    /// `keep` — a single image larger than the whole budget must still be
    /// usable by the caller that just asked for it.
    fn evict_to_budget(&mut self, keep: Option<&str>) {
        while self.resident > self.budget_bytes && self.recency.len() > 1 {
            let Some(pos) = self.recency.iter().position(|k| Some(k.as_str()) != keep) else {
                break;
            };
            let victim = self.recency.remove(pos);
            if let Some(p) = self.images.remove(&victim) {
                self.resident -= p.data().len();
            }
        }
    }

    /// The decoded (premultiplied RGBA) pixmap for an image asset id,
    /// decoding it if it is not currently resident.
    ///
    /// Panics if `id` was not prepared as an image asset — `prepare` has
    /// already proven every referenced image decodes, so a failure here is a
    /// bug rather than bad input.
    pub fn image(&mut self, id: &str) -> &tiny_skia::Pixmap {
        if !self.images.contains_key(id) {
            let pixmap = {
                let bytes = self
                    .staged
                    .get(id)
                    .unwrap_or_else(|| panic!("asset '{id}' is not a prepared image"));
                decode_image(id, bytes)
                    .unwrap_or_else(|e| panic!("asset '{id}' failed to re-decode: {e}"))
            };
            self.insert_image(id.to_string(), pixmap);
            self.evict_to_budget(Some(id));
        } else {
            // Refresh recency on a hit.
            self.recency.retain(|k| k != id);
            self.recency.push(id.to_string());
        }
        self.images
            .get(id)
            .unwrap_or_else(|| panic!("asset '{id}' is not a prepared image"))
    }

    /// The resolved font family name for a font asset id.
    /// Panics if `id` was not prepared as a font asset.
    pub fn family(&self, id: &str) -> &str {
        self.fonts
            .get(id)
            .unwrap_or_else(|| panic!("asset '{id}' is not a prepared font"))
    }

    /// Mutable access to the shared font system (for shaping/layout).
    pub fn font_system(&mut self) -> &mut cosmic_text::FontSystem {
        &mut self.font_system
    }
}

fn decode_image(id: &str, bytes: &[u8]) -> Result<tiny_skia::Pixmap, DocError> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| DocError::Json(format!("asset '{id}': image decode failed: {e}")))?
        .to_rgba8();
    let (w, h) = img.dimensions();
    let premultiplied = premultiply_rgba(img.into_raw());
    let size = tiny_skia::IntSize::from_wh(w, h)
        .ok_or_else(|| DocError::Json(format!("asset '{id}': invalid image size {w}x{h}")))?;
    tiny_skia::Pixmap::from_vec(premultiplied, size)
        .ok_or_else(|| DocError::Json(format!("asset '{id}': pixmap construction failed")))
}

/// Convert straight-alpha RGBA8 bytes to premultiplied-alpha RGBA8 bytes,
/// one pixel (4 bytes) at a time. Pure integer math — see module docs.
fn premultiply_rgba(raw: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    for px in raw.chunks_exact(4) {
        let c = tiny_skia::ColorU8::from_rgba(px[0], px[1], px[2], px[3]).premultiply();
        out.push(c.red());
        out.push(c.green());
        out.push(c.blue());
        out.push(c.alpha());
    }
    out
}

fn load_font(
    id: &str,
    bytes: &[u8],
    font_system: &mut cosmic_text::FontSystem,
) -> Result<String, DocError> {
    let ids = font_system
        .db_mut()
        .load_font_source(fontdb::Source::Binary(Arc::new(bytes.to_vec())));
    let face_id = ids
        .first()
        .ok_or_else(|| DocError::Json(format!("asset '{id}': no font faces found in data")))?;
    font_system
        .db()
        .face(*face_id)
        .and_then(|face| face.families.first())
        .map(|(name, _)| name.clone())
        .ok_or_else(|| DocError::Json(format!("asset '{id}': font has no family name")))
}

/// Reserved src strings that resolve to fonts bundled in this repo
/// (`crates/core/assets/fonts/`, inside this crate so `cargo publish`
/// packages them), so docs can reference them without a file path.
/// Native-only (`bundled-fonts` feature, default-on): `crates/wasm` disables
/// this feature so the fonts are never compiled into the wasm binary — the
/// JS host supplies the bytes there instead.
#[cfg(feature = "bundled-fonts")]
pub fn resolve_reserved_src(src: &str) -> Option<&'static [u8]> {
    match src {
        "kineto:inter" => Some(include_bytes!("../assets/fonts/Inter-Regular.ttf")),
        "kineto:jetbrains-mono" => Some(include_bytes!(
            "../assets/fonts/JetBrainsMono-Regular.ttf"
        )),
        _ => None,
    }
}

#[cfg(test)]
mod premultiply_tests {
    use super::premultiply_rgba;

    /// Exercises the multiplicative branch (alpha < 255) of
    /// `tiny_skia::ColorU8::premultiply`, which our fixtures (both fully
    /// opaque) never hit. Expected values computed by hand from tiny-skia's
    /// own formula, `prod = c*a + 128; p = ((prod + (prod >> 8)) >> 8)`:
    ///   r: c=200, a=128 -> prod=25728 -> (25728+100)>>8 = 100
    ///   g: c=100, a=128 -> prod=12928 -> (12928+50)>>8  = 50
    ///   b: c=50,  a=128 -> prod=6528  -> (6528+25)>>8   = 25
    ///   a: unchanged = 128
    #[test]
    fn applies_integer_formula_when_alpha_less_than_opaque() {
        let raw = vec![200u8, 100, 50, 128];
        let out = premultiply_rgba(raw);
        assert_eq!(out, vec![100u8, 50, 25, 128]);
    }

    /// Alpha == 255 takes tiny-skia's opaque fast path: channels pass
    /// through unchanged (this is the only branch our fixtures exercise).
    #[test]
    fn is_identity_when_fully_opaque() {
        let raw = vec![10u8, 20, 30, 255];
        let out = premultiply_rgba(raw);
        assert_eq!(out, vec![10u8, 20, 30, 255]);
    }
}
