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
pub struct AssetStore {
    /// Raw bytes staged by the host via `add_bytes`, keyed by asset id.
    staged: HashMap<String, Vec<u8>>,
    /// Decoded images (premultiplied RGBA), keyed by asset id.
    images: HashMap<String, tiny_skia::Pixmap>,
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
                    let pixmap = decode_image(id, bytes)?;
                    self.images.insert(id.clone(), pixmap);
                }
                Asset::Font { .. } => {
                    let family = load_font(id, bytes, &mut self.font_system)?;
                    self.fonts.insert(id.clone(), family);
                }
            }
        }
        Ok(())
    }

    /// The decoded (premultiplied RGBA) pixmap for an image asset id.
    /// Panics if `id` was not prepared as an image asset.
    pub fn image(&self, id: &str) -> &tiny_skia::Pixmap {
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
/// (`assets/fonts/`), so docs can reference them without a file path.
/// Native-only (`bundled-fonts` feature, default-on): `crates/wasm` disables
/// this feature so the fonts are never compiled into the wasm binary — the
/// JS host supplies the bytes there instead.
#[cfg(feature = "bundled-fonts")]
pub fn resolve_reserved_src(src: &str) -> Option<&'static [u8]> {
    match src {
        "zoetrope:inter" => Some(include_bytes!("../../../assets/fonts/Inter-Regular.ttf")),
        "zoetrope:jetbrains-mono" => Some(include_bytes!(
            "../../../assets/fonts/JetBrainsMono-Regular.ttf"
        )),
        _ => None,
    }
}
