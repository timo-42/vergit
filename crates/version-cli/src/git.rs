//! Git state, read by shelling out to `git`.
//!
//! Shelling out rather than linking a git library is a deliberate size choice:
//! `gix` would add megabytes to a binary whose whole point is being small, and
//! `git` is present anywhere this tool runs.

use std::path::Path;
use std::process::Command;

/// The state of `HEAD` itself.
pub struct Head {
    /// Abbreviated hash, `g`-prefixed the way `git describe` writes it.
    pub hash: String,
    pub dirty: bool,
}

/// Reads `HEAD`, failing when there is no repository or no commit yet.
pub fn head(dir: Option<&Path>) -> Result<Head, String> {
    let hash = run(dir, &["rev-parse", "--short=8", "HEAD"])?.ok_or_else(|| {
        "not a git repository, or it has no commits yet (run `git init` and commit first)"
            .to_owned()
    })?;
    Ok(Head {
        hash: format!("g{hash}"),
        dirty: run(dir, &["status", "--porcelain"])?.is_some_and(|s| !s.is_empty()),
    })
}

/// Every tag matching `prefix` on the commit that `rev` points at.
///
/// A commit can carry any number of tags, so the caller picks between them by
/// version order.
pub fn tags_at(dir: Option<&Path>, rev: &str, prefix: &str) -> Result<Vec<String>, String> {
    let commit = format!("{rev}^{{commit}}");
    list_tags(dir, prefix, &["--points-at", &commit])
}

/// Commits between `tag` and `HEAD` — `git rev-list --count <tag>..HEAD`.
pub fn count_since(dir: Option<&Path>, tag: &str) -> Result<u64, String> {
    let range = format!("{tag}..HEAD");
    count(dir, &range)
}

/// Total commits reachable from `HEAD`.
pub fn count_all(dir: Option<&Path>) -> Result<u64, String> {
    count(dir, "HEAD")
}

fn count(dir: Option<&Path>, range: &str) -> Result<u64, String> {
    run(dir, &["rev-list", "--count", range])?
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| format!("could not count commits in {range}"))
}

/// A shallow clone silently truncates history, so both the tag search and the
/// commit count would be quietly wrong. CI checkouts default to depth 1.
pub fn ensure_not_shallow(dir: Option<&Path>) -> Result<(), String> {
    if run(dir, &["rev-parse", "--is-shallow-repository"])?.as_deref() == Some("true") {
        return Err(
            "shallow clone: no tags are visible and the commit count is truncated.\n       \
             Fetch full history first (`fetch-depth: 0` in actions/checkout)."
                .to_owned(),
        );
    }
    Ok(())
}

/// Every tag matching `prefix` that is reachable from `HEAD`.
///
/// Restricted to reachable tags so a maintenance branch keeps bumping within its
/// own series instead of jumping to whatever the mainline has released.
pub fn tags_merged(dir: Option<&Path>, prefix: &str) -> Result<Vec<String>, String> {
    list_tags(dir, prefix, &["--merged", "HEAD"])
}

fn list_tags(dir: Option<&Path>, prefix: &str, filter: &[&str]) -> Result<Vec<String>, String> {
    let pattern = format!("{prefix}*");
    let mut args = vec!["tag", "--list", &pattern];
    args.extend_from_slice(filter);

    Ok(run(dir, &args)?
        .into_iter()
        .flat_map(|out| {
            out.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect())
}

/// Runs `git`, returning trimmed stdout, or `None` if git exited non-zero.
///
/// A non-zero exit is an expected outcome here (no tags, no commits), so only
/// a failure to *launch* git is an error.
fn run(dir: Option<&Path>, args: &[&str]) -> Result<Option<String>, String> {
    let mut cmd = Command::new("git");
    if let Some(dir) = dir {
        cmd.arg("-C").arg(dir);
    }
    let out = cmd
        .args(args)
        .output()
        .map_err(|e| format!("could not run `git`: {e}"))?;

    if !out.status.success() {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&out.stdout).trim().to_owned()))
}
