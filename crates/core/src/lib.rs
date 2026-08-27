//! zoetrope core engine
pub mod anim;
pub mod assets;
pub mod color;
pub mod doc;
pub mod scalar;
pub mod text;
pub mod timeline;
pub mod validate;
#[cfg(feature = "bundled-fonts")]
pub use assets::resolve_reserved_src;
pub use assets::AssetStore;
pub use color::Color;
pub use doc::*;
pub use scalar::Scalar;
pub use text::{layout_text, PlacedGlyph, TextLayout};
pub use validate::DocError;
