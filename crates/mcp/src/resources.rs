//! Read-only resources: the document schema and the golden corpus.
//!
//! A model authoring a document gets worked examples rather than having to
//! infer structure from a bare schema — the corpus already covers every
//! element type, every easing, crossfade, wrap, and group nesting.

use rmcp::model::Resource;

pub const SCHEMA_URI: &str = "kineto://schema/document";
const CORPUS_PREFIX: &str = "kineto://corpus/";

pub fn list() -> Vec<Resource> {
    let mut out = vec![Resource::new(SCHEMA_URI, "document-schema")
        .with_title("Kineto document JSON Schema")
        .with_description(
            "JSON Schema for the canonical scene document accepted by \
                 render_document.",
        )
        .with_mime_type("application/json")];

    for entry in kineto_core::corpus::corpus() {
        out.push(
            Resource::new(format!("{CORPUS_PREFIX}{}", entry.name), entry.name)
                .with_title(format!("Example document: {}", entry.name))
                .with_description(
                    "A worked example from the golden corpus. Valid, renderable, \
                     and byte-stable.",
                )
                .with_mime_type("application/json"),
        );
    }
    out
}

pub fn read(uri: &str) -> Option<String> {
    if uri == SCHEMA_URI {
        return Some(DOCUMENT_SCHEMA.to_string());
    }
    let name = uri.strip_prefix(CORPUS_PREFIX)?;
    kineto_core::corpus::corpus()
        .into_iter()
        .find(|e| e.name == name)
        .map(|e| e.doc.canonical_json())
}

/// Hand-written rather than derived.
///
/// `schemars::schema_for!` would require `kineto_core::Document` to derive
/// `JsonSchema`, which would put `schemars` into `crates/core` and break the
/// leaf-crate constraint. The format is frozen at `v: 1`, so a literal is
/// stable — and it can carry better descriptions than a derived schema would.
pub const DOCUMENT_SCHEMA: &str = r##"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Kineto document",
  "description": "A complete, serializable description of a video. Time is in integer ticks at 705600000 ticks/second; fps is only an export hint.",
  "type": "object",
  "required": ["v", "timebase", "size", "scenes"],
  "additionalProperties": false,
  "properties": {
    "v": { "const": 1, "description": "Document format version. Always 1." },
    "timebase": { "const": 705600000, "description": "Ticks per second. Always 705600000 (Flicks)." },
    "defaultFps": { "type": "integer", "minimum": 1, "description": "Export hint. Must divide 705600000 exactly." },
    "size": {
      "type": "object",
      "required": ["w", "h"],
      "additionalProperties": false,
      "properties": {
        "w": { "type": "integer", "minimum": 1 },
        "h": { "type": "integer", "minimum": 1 }
      }
    },
    "bg": { "$ref": "#/$defs/color", "description": "Canvas background. Defaults to #000000." },
    "assets": {
      "type": "object",
      "description": "Asset id -> asset. Ids must match [A-Za-z0-9_-]{1,64}.",
      "additionalProperties": { "$ref": "#/$defs/asset" }
    },
    "scenes": {
      "type": "array",
      "minItems": 1,
      "items": { "$ref": "#/$defs/scene" }
    }
  },
  "$defs": {
    "color": { "type": "string", "pattern": "^#[0-9A-Fa-f]{6}$" },
    "asset": {
      "oneOf": [
        {
          "type": "object",
          "required": ["type", "src"],
          "additionalProperties": false,
          "properties": {
            "type": { "const": "image" },
            "src": { "type": "string", "description": "Path to a PNG or JPEG, resolved against assetBaseDir." }
          }
        },
        {
          "type": "object",
          "required": ["type", "src"],
          "additionalProperties": false,
          "properties": {
            "type": { "const": "font" },
            "src": {
              "type": "string",
              "description": "Path to a TTF/OTF, or a reserved src for a bundled font: 'kineto:inter' or 'kineto:jetbrains-mono'. There are no system fonts."
            }
          }
        }
      ]
    },
    "scene": {
      "type": "object",
      "required": ["id", "duration", "elements"],
      "additionalProperties": false,
      "properties": {
        "id": { "type": "string", "pattern": "^[A-Za-z0-9_-]{1,64}$" },
        "duration": { "type": "integer", "minimum": 1, "description": "Scene length in ticks." },
        "transition": {
          "type": "object",
          "required": ["type", "duration"],
          "additionalProperties": false,
          "description": "Transition INTO this scene. Not allowed on the first scene, and must not exceed the shorter of the two adjacent scenes.",
          "properties": {
            "type": { "const": "crossfade" },
            "duration": { "type": "integer", "minimum": 1 }
          }
        },
        "elements": { "type": "array", "items": { "$ref": "#/$defs/element" } }
      }
    },
    "element": {
      "oneOf": [
        {
          "type": "object",
          "required": ["type", "asset", "rect"],
          "properties": {
            "type": { "const": "image" },
            "asset": { "type": "string" },
            "rect": { "$ref": "#/$defs/rect" }
          },
          "$ref": "#/$defs/commonProps"
        },
        {
          "type": "object",
          "required": ["type", "text", "font", "sizePx", "color", "pos"],
          "properties": {
            "type": { "const": "text" },
            "text": { "type": "string" },
            "font": { "type": "string", "description": "A font asset id." },
            "sizePx": { "type": "number", "exclusiveMinimum": 0 },
            "color": { "$ref": "#/$defs/color" },
            "pos": { "$ref": "#/$defs/vec2" },
            "maxW": { "type": "number", "exclusiveMinimum": 0, "description": "Wrap width in pixels." },
            "align": { "enum": ["left", "center", "right"] }
          },
          "$ref": "#/$defs/commonProps"
        },
        {
          "type": "object",
          "required": ["type", "rect", "fill"],
          "properties": {
            "type": { "const": "rect" },
            "rect": { "$ref": "#/$defs/rect" },
            "fill": { "$ref": "#/$defs/color" }
          },
          "$ref": "#/$defs/commonProps"
        },
        {
          "type": "object",
          "required": ["type", "origin", "children"],
          "properties": {
            "type": { "const": "group" },
            "origin": { "$ref": "#/$defs/vec2" },
            "children": { "type": "array", "items": { "$ref": "#/$defs/element" } }
          },
          "$ref": "#/$defs/commonProps"
        }
      ]
    },
    "rect": { "type": "array", "minItems": 4, "maxItems": 4, "items": { "type": "number" }, "description": "[x, y, w, h]" },
    "vec2": { "type": "array", "minItems": 2, "maxItems": 2, "items": { "type": "number" }, "description": "[x, y]" },
    "commonProps": {
      "description": "Every element accepts these. Base geometry is static; only these four properties animate.",
      "properties": {
        "translate": { "$ref": "#/$defs/vec2" },
        "scale": { "type": "number" },
        "rotation": { "type": "number", "description": "Degrees." },
        "opacity": { "type": "number", "minimum": 0, "maximum": 1 },
        "animations": { "type": "array", "items": { "$ref": "#/$defs/track" } }
      }
    },
    "track": {
      "type": "object",
      "required": ["prop", "keys"],
      "additionalProperties": false,
      "properties": {
        "prop": { "enum": ["translate", "scale", "rotation", "opacity"] },
        "keys": {
          "type": "array",
          "minItems": 1,
          "description": "Keyframes, strictly increasing in t.",
          "items": {
            "type": "object",
            "required": ["t", "v"],
            "additionalProperties": false,
            "properties": {
              "t": { "type": "integer", "description": "Time in ticks, relative to the scene." },
              "v": {
                "description": "A number, except for 'translate', which takes [x, y].",
                "oneOf": [{ "type": "number" }, { "$ref": "#/$defs/vec2" }]
              },
              "ease": { "enum": ["linear", "inCubic", "outCubic", "inOutCubic"] }
            }
          }
        }
      }
    }
  }
}"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_literal_is_valid_json() {
        serde_json::from_str::<serde_json::Value>(DOCUMENT_SCHEMA)
            .expect("DOCUMENT_SCHEMA must parse");
    }

    #[test]
    fn every_corpus_entry_is_listed_and_readable() {
        for entry in kineto_core::corpus::corpus() {
            let uri = format!("kineto://corpus/{}", entry.name);
            let text = read(&uri).unwrap_or_else(|| panic!("{uri} not readable"));
            kineto_core::Document::from_json(&text)
                .unwrap_or_else(|e| panic!("{uri} is not a valid document: {e}"));
        }
        assert_eq!(list().len(), kineto_core::corpus::corpus().len() + 1);
    }
}
