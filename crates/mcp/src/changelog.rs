//! Release videos composed from a repository's own commit history.
//!
//! The commits already are the release notes. Reading them beats maintaining a
//! second list that drifts from the first.
//!
//! Reading git belongs here for the same reason muxing does: the alternative
//! was a script beside the tool, and a project whose own pipeline reaches past
//! its tools has admitted the tools are incomplete. `git` is shelled out to
//! exactly as `ffmpeg` is, and only this one entry point needs it.
//!
//! [`points`] is pure so it can be tested without a repository — the parsing
//! is where every interesting case lives.

use std::path::Path;
use std::process::Command;

use crate::error::ToolError;
use crate::scene::{build_document, SceneSpec};

/// Conventional-commit types whose changes a reader of release notes cares
/// about. The rest is real work that does not belong on a title card.
const USER_VISIBLE: &[&str] = &["feat", "fix", "perf"];

/// Prefixes that are never interesting, in any repository.
const NOISE: &[&str] = &["merge ", "revert ", "bump ", "release ", "wip ", "wip:"];

/// `type(scope)!: text` split into its type and its text.
///
/// Hand-parsed rather than pulling in a regex engine for one pattern.
fn conventional(subject: &str) -> Option<(&str, &str)> {
    let (head, text) = subject.split_once(": ")?;
    let head = head.strip_suffix('!').unwrap_or(head);
    // A scope is parenthesised and must close before the colon.
    let ty = match head.split_once('(') {
        Some((ty, rest)) => {
            if !rest.ends_with(')') {
                return None;
            }
            ty
        }
        None => head,
    };
    if ty.is_empty() || !ty.chars().all(|c| c.is_ascii_lowercase()) {
        return None;
    }
    Some((ty, text))
}

fn is_noise(subject: &str) -> bool {
    let lower = subject.to_ascii_lowercase();
    if NOISE.iter().any(|p| lower.starts_with(p)) {
        return true;
    }
    // A bare version, like "v1.2.3" or "1.2.3".
    let bare = lower.strip_prefix('v').unwrap_or(&lower);
    !bare.is_empty() && bare.chars().all(|c| c.is_ascii_digit() || c == '.') && bare.contains('.')
}

/// The user-visible changes among `subjects`, newest first, deduplicated.
///
/// Where a repository uses conventional commits their prefixes say which
/// changes were user-visible. Where it does not, every subject is a candidate
/// and only obvious noise is dropped — a generator that worked solely on
/// projects sharing this one's conventions would demonstrate this project
/// rather than the tool.
pub fn points(subjects: &[String], limit: usize, max_len: usize) -> Vec<String> {
    // Keyed off whether the *repository* uses conventions, not whether this
    // range happened to contain a user-visible commit: a project that uses
    // them and shipped only chores should say so rather than list the chores
    // as though they were features.
    let uses_conventional = subjects.iter().any(|s| conventional(s.trim()).is_some());

    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<String> = Vec::new();
    for subject in subjects {
        let subject = subject.trim();
        if subject.is_empty() || is_noise(subject) {
            continue;
        }
        let parsed = conventional(subject);
        let text = if uses_conventional {
            match parsed {
                Some((ty, text)) if USER_VISIBLE.contains(&ty) => text,
                _ => continue,
            }
        } else {
            parsed.map(|(_, t)| t).unwrap_or(subject)
        };
        // Overlong lines stop being scannable, which is all a release video is
        // for. Dropping one reads better than truncating it mid-sentence.
        let lower = text.to_ascii_lowercase();
        if text.chars().count() > max_len || seen.contains(&lower) {
            continue;
        }
        seen.push(lower);
        out.push(text.to_string());
        if out.len() == limit {
            break;
        }
    }
    out
}

/// Commit subjects in `range`, newest first.
pub fn subjects(repo: Option<&Path>, range: Option<&str>) -> Result<Vec<String>, ToolError> {
    let mut cmd = Command::new("git");
    if let Some(dir) = repo {
        cmd.arg("-C").arg(dir);
    }
    cmd.args(["log", "--no-merges", "--pretty=%s"]);
    if let Some(r) = range.filter(|r| !r.is_empty()) {
        cmd.arg(r);
    }
    let out = cmd.output().map_err(|e| {
        ToolError::DocumentSource(format!(
            "could not run git ({e}). A changelog needs git on PATH."
        ))
    })?;
    if !out.status.success() {
        return Err(ToolError::DocumentSource(format!(
            "git log failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect())
}

/// Everything since the previous tag, or all history when there is none.
pub fn default_range(repo: Option<&Path>) -> Option<String> {
    let mut cmd = Command::new("git");
    if let Some(dir) = repo {
        cmd.arg("-C").arg(dir);
    }
    cmd.args(["describe", "--tags", "--abbrev=0", "HEAD^"]);
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let prev = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!prev.is_empty()).then(|| format!("{prev}..HEAD"))
}

pub struct Options {
    pub title: String,
    pub subtitle: Option<String>,
    pub heading: String,
    pub theme: String,
    pub width: u32,
    pub height: u32,
    pub max_points: usize,
    pub max_length: usize,
    pub install: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            title: String::new(),
            subtitle: None,
            heading: "What changed".into(),
            theme: "midnight".into(),
            width: 1280,
            height: 720,
            max_points: 4,
            max_length: 58,
            install: Vec::new(),
        }
    }
}

/// Composes the document. Returns its JSON, so the caller can render it, write
/// it, or hand it back for editing.
pub fn build(items: &[String], opts: &Options) -> Result<String, ToolError> {
    if opts.title.trim().is_empty() {
        return Err(ToolError::DocumentSource(
            "a changelog needs a title, e.g. --title \"Acme 2.0\"".into(),
        ));
    }
    // An honest card beats an empty list, which the builder would reject.
    let items: Vec<String> = if items.is_empty() {
        vec!["maintenance and internal changes".into()]
    } else {
        items.to_vec()
    };

    let mut scenes = vec![
        SceneSpec {
            kind: "title".into(),
            text: Some(opts.title.clone()),
            subtitle: opts.subtitle.clone(),
            heading: None,
            items: Vec::new(),
            attribution: None,
            seconds: None,
        },
        SceneSpec {
            kind: "points".into(),
            text: None,
            subtitle: None,
            heading: Some(opts.heading.clone()),
            items,
            attribution: None,
            seconds: None,
        },
    ];
    if !opts.install.is_empty() {
        scenes.push(SceneSpec {
            kind: "code".into(),
            text: None,
            subtitle: None,
            heading: Some("Install".into()),
            items: opts.install.clone(),
            attribution: None,
            seconds: None,
        });
    }
    build_document(&opts.theme, opts.width, opts.height, &scenes)
}
