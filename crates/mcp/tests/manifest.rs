//! The workspace manifest has to stay internally consistent to be publishable.
//!
//! Path dependencies carry a `version` so crates.io can resolve them, while a
//! local build uses the `path`. `just release` rewrites a single
//! `^version = ` line, so those two can drift apart.
//!
//! Cargo catches the loud half of that itself: bump the workspace to 0.2.0
//! and leave a dependency at `^0.1.0` and resolution fails immediately. What
//! it does not catch is a **patch** bump -- `^0.1.0` happily matches 0.1.1,
//! so the workspace builds and tests green while the published `kineto`
//! 0.1.1 declares a dependency that crates.io may satisfy with `kineto-core`
//! 0.1.0, missing whatever the new version added. That resolves for us and
//! fails to compile for a consumer.
//!
//! Patch releases being the common case, that gap is the one worth a test.

/// Reads the workspace manifest, or `None` when it is not reachable.
///
/// Tests ship inside the published crate, where nothing sits two directories
/// up. Skipping is correct there -- the invariant is about this repository,
/// not about a consumer's checkout. The `[workspace]` check guards against
/// picking up some unrelated manifest that happens to be in that position.
fn workspace_manifest() -> Option<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let text = std::fs::read_to_string(path).ok()?;
    text.contains("[workspace]").then_some(text)
}

/// Values of `version = "..."` inside the named section, in file order.
fn versions_in_section(manifest: &str, section: &str) -> Vec<String> {
    manifest
        .split(section)
        .nth(1)
        .map(|rest| {
            rest.split("\n[")
                .next()
                .unwrap_or("")
                .lines()
                .filter(|l| !l.trim_start().starts_with('#'))
                .filter_map(|l| l.split_once("version = \""))
                .filter_map(|(_, v)| v.split_once('"'))
                .map(|(v, _)| v.to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn path_dependency_versions_match_the_workspace_version() {
    let Some(manifest) = workspace_manifest() else {
        eprintln!("skipping: not running from the workspace");
        return;
    };

    let workspace = versions_in_section(&manifest, "[workspace.package]");
    assert_eq!(
        workspace.len(),
        1,
        "expected exactly one version in [workspace.package], got {workspace:?}"
    );
    let expected = &workspace[0];

    let deps = versions_in_section(&manifest, "[workspace.dependencies]");
    assert!(
        !deps.is_empty(),
        "no versioned path dependencies found -- crates.io requires them, so \
         either this test stopped parsing or the manifest lost them"
    );

    for got in &deps {
        assert_eq!(
            got, expected,
            "a [workspace.dependencies] entry is at {got}, workspace.package \
             is at {expected} -- `just release` bumped one and not the other, \
             and cargo publish will reject this"
        );
    }
}
