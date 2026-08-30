//! Composing a release video from commit subjects.
//!
//! `points` is pure, so every interesting case is testable without a
//! repository. The one that matters most is the fallback: a generator that
//! only worked on projects sharing this one's commit conventions would
//! demonstrate this project rather than the tool.

use kineto::changelog::{build, points, Options};

fn subjects(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|s| s.to_string()).collect()
}

#[test]
fn conventional_commits_keep_only_user_visible_types() {
    let got = points(
        &subjects(&[
            "feat: a thing",
            "chore: tidy",
            "fix: a crash",
            "docs: words",
            "perf: faster",
        ]),
        4,
        58,
    );
    assert_eq!(got, ["a thing", "a crash", "faster"]);
}

#[test]
fn a_scope_and_a_breaking_marker_are_stripped() {
    let got = points(&subjects(&["feat(mcp)!: scoped and breaking"]), 4, 58);
    assert_eq!(got, ["scoped and breaking"]);
}

#[test]
fn a_repository_without_conventions_falls_back_to_plain_subjects() {
    let got = points(&subjects(&["Add dark mode", "Fix a crash"]), 4, 58);
    assert_eq!(got, ["Add dark mode", "Fix a crash"]);
}

#[test]
fn noise_is_dropped_in_the_fallback() {
    let got = points(
        &subjects(&[
            "Merge branch 'x'",
            "Bump version to 2.0.0",
            "Add dark mode",
            "Revert something",
            "v1.2.3",
        ]),
        4,
        58,
    );
    assert_eq!(got, ["Add dark mode"]);
}

#[test]
fn a_project_that_uses_conventions_but_shipped_only_chores_says_nothing() {
    // The flaw a test caught: falling back whenever a *range* produced nothing
    // would list this project's chores as though they were features. The
    // fallback keys off whether the repository uses conventions at all.
    let got = points(&subjects(&["chore: tidy", "docs: words"]), 4, 58);
    assert!(got.is_empty(), "chores were presented as changes: {got:?}");
}

#[test]
fn duplicates_collapse_regardless_of_case() {
    let got = points(&subjects(&["Add dark mode", "add dark MODE"]), 4, 58);
    assert_eq!(got, ["Add dark mode"]);
}

#[test]
fn overlong_subjects_are_dropped_rather_than_truncated() {
    // Truncating mid-sentence reads worse on screen than omitting the line.
    let long = "x".repeat(80);
    let got = points(&subjects(&[&long, "Short one"]), 4, 58);
    assert_eq!(got, ["Short one"]);
}

#[test]
fn the_limit_is_honoured() {
    let many: Vec<String> = (0..9).map(|i| format!("feat: n{i}")).collect();
    assert_eq!(points(&many, 3, 58).len(), 3);
}

#[test]
fn an_empty_history_still_composes_something_honest() {
    let opts = Options {
        title: "Acme 2.0".into(),
        ..Default::default()
    };
    let json = build(&[], &opts).expect("builds");
    assert!(
        json.contains("maintenance and internal changes"),
        "an empty changelog produced no honest fallback line"
    );
}

#[test]
fn a_missing_title_is_refused_with_the_flag_that_supplies_it() {
    let err = build(&["a change".into()], &Options::default()).expect_err("must fail");
    assert!(err.to_string().contains("--title"), "{err}");
}

#[test]
fn the_composed_document_is_renderable_and_clean() {
    // The whole point: what comes out is a document the engine renders and the
    // linter passes, not merely well-formed JSON.
    let opts = Options {
        title: "Acme 2.0".into(),
        subtitle: Some("what shipped".into()),
        install: vec!["npm i acme".into()],
        ..Default::default()
    };
    let json = build(&["Add dark mode".into(), "Fix a crash".into()], &opts).expect("builds");

    let (doc, _) = kineto::source::load_document(Some(&json), None).expect("loads");
    let mut assets =
        kineto::source::resolve_assets(&doc, std::path::Path::new(".")).expect("assets");
    assets.prepare(&doc).expect("prepare");

    let mut issues = kineto::check::analyze_document(&doc);
    let starts = kineto_core::timeline::scene_starts(&doc);
    for (i, scene) in doc.scenes.iter().enumerate() {
        issues.extend(kineto::check::analyze(
            &doc,
            &mut assets,
            starts[i] + scene.duration / 2,
        ));
    }
    assert!(issues.is_empty(), "not clean: {issues:#?}");
    assert_eq!(doc.scenes.len(), 3, "title, points and install");
}
