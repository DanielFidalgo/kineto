//! The GitHub Action drives the CLI, and nothing else connects the two.
//!
//! A renamed flag would leave `action.yml` passing an argument the binary no
//! longer accepts, and nothing would notice until someone cut a release and
//! watched it fail — the slowest possible feedback for a one-word change. This
//! is the cheap version of that check.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Flags `action.yml` passes to `kineto`.
///
/// Listed here rather than scraped out of the YAML so the assertion runs both
/// ways: each must exist in the CLI *and* still be used by the action. A flag
/// silently dropped from the action is drift too.
const USED_BY_ACTION: &[&str] = &["--scenes", "--at", "--width", "--fps", "--check", "-o"];

fn bin() -> PathBuf {
    let mut p = std::env::current_exe().expect("test exe path");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("kineto")
}

/// The action lives at the repository root, which is not there once this crate
/// is published.
fn action_yml() -> Option<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../action.yml");
    std::fs::read_to_string(path).ok()
}

#[test]
fn every_flag_the_action_passes_is_one_the_cli_accepts() {
    let out = Command::new(bin())
        .arg("--help")
        .output()
        .expect("run --help");
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);

    for flag in USED_BY_ACTION {
        assert!(
            help.contains(flag),
            "action.yml passes {flag}, which `kineto --help` does not list"
        );
    }
}

#[test]
fn every_flag_listed_here_is_still_used_by_the_action() {
    let Some(yml) = action_yml() else {
        eprintln!("skipping: not running from the repository");
        return;
    };
    for flag in USED_BY_ACTION {
        assert!(
            yml.contains(flag),
            "{flag} is asserted here but action.yml no longer uses it"
        );
    }
}

#[test]
fn the_action_installs_a_build_for_every_target_the_release_produces() {
    // The action maps RUNNER_OS-RUNNER_ARCH to a rust target and downloads
    // that archive. A target the release stops building, or starts building
    // without the action knowing, is a download 404 on someone else's runner.
    let (Some(yml), Some(workflow)) = (action_yml(), {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/release.yml");
        std::fs::read_to_string(p).ok()
    }) else {
        eprintln!("skipping: not running from the repository");
        return;
    };

    let targets: Vec<&str> = workflow
        .lines()
        .filter_map(|l| l.trim().strip_prefix("- target: "))
        .collect();
    assert!(!targets.is_empty(), "parsed no targets from release.yml");

    for target in &targets {
        assert!(
            yml.contains(target),
            "release.yml builds {target}, but action.yml cannot install it"
        );
    }
}
