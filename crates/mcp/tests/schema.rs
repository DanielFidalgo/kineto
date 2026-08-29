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
        &kineto::resources::read("kineto://schema/document").expect("schema resource exists"),
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

/// Every reference document must survive the rules its imitators are judged
/// by. An exemplar that trips our own lint teaches the wrong habit, and this
/// is the only thing stopping one from drifting into that state.
#[test]
fn the_reference_examples_pass_the_lint_they_are_meant_to_teach() {
    for ex in kineto::examples::examples() {
        // Validated through the real loading path, not the builder that made
        // it — a document that only exists in memory proves nothing.
        let json = ex.doc.canonical_json();
        let (doc, _) = kineto::source::load_document(Some(&json), None)
            .unwrap_or_else(|e| panic!("example '{}' does not validate: {e}", ex.name));

        let mut assets = kineto::source::resolve_assets(&doc, std::path::Path::new("."))
            .unwrap_or_else(|e| panic!("example '{}' assets: {e}", ex.name));
        assets.prepare(&doc).unwrap();

        let doc_issues = kineto::check::analyze_document(&doc);
        assert!(
            doc_issues.is_empty(),
            "example '{}' trips a document rule: {doc_issues:?}",
            ex.name
        );

        for (i, scene) in doc.scenes.iter().enumerate() {
            let starts = kineto_core::timeline::scene_starts(&doc);
            let mid = starts[i] + scene.duration / 2;
            let issues = kineto::check::analyze(&doc, &mut assets, mid);
            assert!(
                issues.is_empty(),
                "example '{}' scene '{}' trips: {issues:?}",
                ex.name,
                scene.id
            );
        }
    }
}

#[test]
fn examples_are_advertised_and_readable() {
    let uris: Vec<String> = kineto::resources::list()
        .iter()
        .map(|r| r.uri.clone())
        .collect();
    for ex in kineto::examples::examples() {
        let uri = format!("kineto://example/{}", ex.name);
        assert!(uris.contains(&uri), "{uri} not listed in {uris:?}");
        let body = kineto::resources::read(&uri).unwrap_or_else(|| panic!("{uri} not readable"));
        kineto_core::Document::from_json(&body)
            .unwrap_or_else(|e| panic!("{uri} is not a valid document: {e}"));
    }
}
