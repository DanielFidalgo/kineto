//! Read-only resources: the document schema and the golden corpus.
//!
//! A model authoring a document gets worked examples rather than having to
//! infer structure from a bare schema — the corpus already covers every
//! element type, every easing, crossfade, wrap, and group nesting.

use rmcp::model::Resource;

pub const SCHEMA_URI: &str = "kineto://schema/document";
const CORPUS_PREFIX: &str = "kineto://corpus/";

/// URI prefix for the reference documents in `crate::examples`.
pub const EXAMPLE_PREFIX: &str = "kineto://example/";

pub fn list() -> Vec<Resource> {
    let mut out = vec![Resource::new(SCHEMA_URI, "document-schema")
        .with_title("Kineto document JSON Schema")
        .with_description(
            "JSON Schema for the canonical scene document accepted by \
                 render_document.",
        )
        .with_mime_type("application/json")];

    // Listed before the corpus: these are what a caller should imitate, and
    // whichever it reads first sets the habit.
    for ex in crate::examples::examples() {
        out.push(
            Resource::new(format!("{EXAMPLE_PREFIX}{}", ex.name), ex.name)
                .with_title(format!("Reference document: {}", ex.name))
                .with_description(ex.description)
                .with_mime_type("application/json"),
        );
    }

    for entry in kineto_core::corpus::corpus() {
        out.push(
            Resource::new(format!("{CORPUS_PREFIX}{}", entry.name), entry.name)
                .with_title(format!("Renderer test document: {}", entry.name))
                .with_description(
                    "Exercises the renderer — easings, group nesting, wrapping. \
                     Valid and byte-stable, but written to cover features rather \
                     than to be imitated; prefer kineto://example/ for that.",
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
    if let Some(name) = uri.strip_prefix(EXAMPLE_PREFIX) {
        return crate::examples::examples()
            .into_iter()
            .find(|e| e.name == name)
            .map(|e| e.doc.canonical_json());
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
            "fit": { "enum": ["stretch", "contain", "cover"], "description": "How the image fills its box when aspect ratios differ. Defaults to stretch. cover crops to the box." },
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
    "paint": {
      "description": "A flat colour, or a gradient. Gradient coordinates are unit space over the element's own bounding box: [0,0] is its top-left and [1,1] its bottom-right, so one gradient reads the same at any size.",
      "oneOf": [
        { "$ref": "#/$defs/color" },
        {
          "type": "object",
          "required": ["type", "from", "to", "stops"],
          "additionalProperties": false,
          "properties": {
            "type": { "const": "linear" },
            "from": { "$ref": "#/$defs/vec2" },
            "to": { "$ref": "#/$defs/vec2" },
            "stops": { "$ref": "#/$defs/stops" }
          }
        },
        {
          "type": "object",
          "required": ["type", "center", "radius", "stops"],
          "additionalProperties": false,
          "properties": {
            "type": { "const": "radial" },
            "center": { "$ref": "#/$defs/vec2" },
            "radius": { "type": "number", "exclusiveMinimum": 0, "description": "Fraction of the box's longer edge." },
            "stops": { "$ref": "#/$defs/stops" }
          }
        }
      ]
    },
    "stops": {
      "type": "array",
      "minItems": 2,
      "maxItems": 8,
      "description": "Stop positions must increase from 0 to 1.",
      "items": {
        "type": "object",
        "required": ["at", "color"],
        "additionalProperties": false,
        "properties": {
          "at": { "type": "number", "minimum": 0, "maximum": 1 },
          "color": { "$ref": "#/$defs/color" }
        }
      }
    },
    "element": {
      "oneOf": [
        {
          "type": "object",
          "required": ["type", "asset", "rect"],
          "properties": {
            "type": { "const": "image" },
            "fit": { "enum": ["stretch", "contain", "cover"], "description": "How the image fills its box when aspect ratios differ. Defaults to stretch. cover crops to the box." },
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
            "fill": { "$ref": "#/$defs/paint" },
            "radius": { "type": "number", "minimum": 0, "description": "Corner radius in pixels, clamped to half the shorter edge." }
          },
          "$ref": "#/$defs/commonProps"
        },
        {
          "type": "object",
          "required": ["type", "points"],
          "description": "Open or closed polyline; straight segments only. Requires at least two points and at least one of stroke/fill.",
          "properties": {
            "type": { "const": "path" },
            "points": {
              "type": "array",
              "minItems": 2,
              "items": { "$ref": "#/$defs/vec2" }
            },
            "closed": { "type": "boolean", "description": "Draw the segment from the last point back to the first. Defaults to false." },
            "stroke": { "$ref": "#/$defs/color" },
            "strokeWidth": { "type": "number", "exclusiveMinimum": 0, "description": "Defaults to 1 when omitted." },
            "cap": { "enum": ["butt", "round", "square"], "description": "Stroke terminator. Defaults to butt." },
            "join": { "enum": ["miter", "round", "bevel"], "description": "How segments meet. Defaults to miter." },
            "fill": { "$ref": "#/$defs/paint" }
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
    "clip": {
      "type": "object",
      "required": ["rect"],
      "additionalProperties": false,
      "description": "A static window the element is drawn through, in its parent's space. Not carried by the element's own transform, so content can animate behind a fixed window — that is how a wipe or a progress fill is expressed.",
      "properties": {
        "rect": { "$ref": "#/$defs/rect" },
        "radius": { "type": "number", "minimum": 0 }
      }
    },
    "commonProps": {
      "description": "Every element accepts these. Base geometry is static; only these four properties animate.",
      "properties": {
        "translate": { "$ref": "#/$defs/vec2" },
        "scale": { "type": "number" },
        "rotation": { "type": "number", "description": "Degrees." },
        "opacity": { "type": "number", "minimum": 0, "maximum": 1 },
        "animations": { "type": "array", "items": { "$ref": "#/$defs/track" } },
        "clip": { "$ref": "#/$defs/clip" }
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
              "ease": { "enum": ["linear", "inCubic", "outCubic", "inOutCubic", "inBack", "outBack", "inOutBack", "inExpo", "outExpo", "inOutExpo"] }
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
        // Derived rather than a literal: the schema, every reference example
        // and every corpus entry, so adding either kind cannot silently stop
        // being advertised.
        assert_eq!(
            list().len(),
            1 + crate::examples::examples().len() + kineto_core::corpus::corpus().len()
        );
    }
}
