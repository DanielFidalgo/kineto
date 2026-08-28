//! The document JSON Schema is hand-written (`crates/mcp/src/resources.rs`)
//! while the document model is Rust types, so nothing makes them agree. That
//! is a standing drift risk: an element type can be added to the engine and
//! silently missed in the schema a calling model reads.
//!
//! This closes the gap from the side that matters. Every element type the
//! corpus actually exercises must be describable by the schema — so adding
//! an element to the engine without teaching the schema about it fails here.

use std::collections::BTreeSet;

use serde_json::Value;

/// Element `type` consts the schema can describe.
fn schema_element_types() -> BTreeSet<String> {
    let schema: Value = serde_json::from_str(
        &kineto_mcp::resources::read("kineto://schema/document").expect("schema resource exists"),
    )
    .expect("schema is valid JSON");

    schema["$defs"]["element"]["oneOf"]
        .as_array()
        .expect("element.oneOf is an array")
        .iter()
        .filter_map(|v| v["properties"]["type"]["const"].as_str())
        .map(str::to_string)
        .collect()
}

/// Every `"type"` appearing on an element anywhere in a document.
fn used_element_types(v: &Value, out: &mut BTreeSet<String>) {
    match v {
        Value::Array(items) => items.iter().for_each(|i| used_element_types(i, out)),
        Value::Object(map) => {
            // Assets and transitions also carry `type`; only elements live
            // under an `elements`/`children` array, which is how they are
            // reached here.
            for key in ["elements", "children"] {
                if let Some(Value::Array(els)) = map.get(key) {
                    for el in els {
                        if let Some(t) = el["type"].as_str() {
                            out.insert(t.to_string());
                        }
                        used_element_types(el, out);
                    }
                }
            }
            for val in map.values() {
                if val.is_array() || val.is_object() {
                    used_element_types(val, out);
                }
            }
        }
        _ => {}
    }
}

#[test]
fn the_schema_describes_every_element_type_the_corpus_uses() {
    let described = schema_element_types();
    assert!(
        described.contains("rect"),
        "control: the schema should always describe rect, got {described:?}"
    );

    let mut used = BTreeSet::new();
    for entry in kineto_core::corpus::corpus() {
        let json: Value = serde_json::from_str(&entry.doc.canonical_json()).expect("corpus JSON");
        used_element_types(&json, &mut used);
    }
    assert!(
        !used.is_empty(),
        "control: the corpus must exercise some elements"
    );

    let missing: Vec<&String> = used.difference(&described).collect();
    assert!(
        missing.is_empty(),
        "the hand-written schema does not describe element type(s) {missing:?} \
         that the corpus renders; schema knows {described:?}"
    );
}
