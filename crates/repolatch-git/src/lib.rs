//! Safe, local-only Git metadata for RepoLatch workspaces.
//!
//! This crate invokes only Git's read-only porcelain/plumbing commands with
//! repository configuration, hooks, attributes and external diff drivers disabled.

use std::{fs, path::Path, process::Command};

use repolatch_core::{RelativeRepoPath, RepoLatchError, Result};
use serde::{Deserialize, Serialize};

pub const MAX_RECORDED_PATHS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSnapshot {
    pub head: Option<String>,
    pub dirty: bool,
    pub tracked_paths: Vec<RelativeRepoPath>,
    pub ignored_paths: Vec<RelativeRepoPath>,
    pub tracked_paths_truncated: bool,
    pub ignored_paths_truncated: bool,
}

/// Capture source Git state without executing repository hooks, filters, or config.
pub fn snapshot_source(repository: impl AsRef<Path>) -> Result<SourceSnapshot> {
    let root = repository.as_ref();
    if !root.is_dir() {
        return Err(RepoLatchError::Git(
            "repository root is not a directory".to_owned(),
        ));
    }
    let head = git(root, ["rev-parse", "--verify", "HEAD"])
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let status = git(
        root,
        ["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let dirty = !status.is_empty();
    let tracked = paths_from_nul(&git(root, ["ls-files", "-z"])?);
    let ignored = paths_from_nul(&git(
        root,
        [
            "ls-files",
            "-z",
            "--others",
            "--ignored",
            "--exclude-standard",
        ],
    )?);
    let (tracked_paths, tracked_paths_truncated) = cap_paths(tracked);
    let (ignored_paths, ignored_paths_truncated) = cap_paths(ignored);
    Ok(SourceSnapshot {
        head,
        dirty,
        tracked_paths,
        ignored_paths,
        tracked_paths_truncated,
        ignored_paths_truncated,
    })
}

/// Create an unrelated baseline Git repository in a visible workspace.
/// No source `.git` objects or configuration are copied.
pub fn initialize_visible_baseline(workspace: impl AsRef<Path>) -> Result<String> {
    let root = workspace.as_ref();
    if !root.is_dir() {
        return Err(RepoLatchError::Git(
            "workspace is not a directory".to_owned(),
        ));
    }
    let git_dir = root.join(".git");
    if git_dir.exists() {
        return Err(RepoLatchError::Git(
            "workspace already has a .git directory".to_owned(),
        ));
    }
    git(root, ["init", "--quiet"])?;
    git(root, ["add", "--all", "--", "."])?;
    // `-c` config makes this independent of user/repository configuration.
    git(
        root,
        [
            "-c",
            "user.name=RepoLatch",
            "-c",
            "user.email=repolatch@local",
            "commit",
            "--quiet",
            "--no-verify",
            "-m",
            "RepoLatch visible workspace baseline",
        ],
    )?;
    git(root, ["rev-parse", "--verify", "HEAD"]).map(|value| value.trim().to_owned())
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffSummary {
    pub changed_paths: Vec<RelativeRepoPath>,
    pub changed_paths_truncated: bool,
    pub lines_added: u64,
    pub lines_removed: u64,
}

/// Summarize the generated workspace relative to its synthesized baseline.
pub fn summarize_workspace_diff(workspace: impl AsRef<Path>) -> Result<DiffSummary> {
    let root = workspace.as_ref();
    let names = git(
        root,
        [
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--name-only",
            "-z",
            "HEAD",
        ],
    )?;
    let mut paths = paths_from_nul(&names);
    // Untracked files are changes too, but do not add them to the baseline index.
    paths.extend(paths_from_nul(&git(
        root,
        ["ls-files", "--others", "--exclude-standard", "-z"],
    )?));
    let (changed_paths, changed_paths_truncated) = cap_paths(paths);
    let stat = git(
        root,
        [
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--numstat",
            "HEAD",
        ],
    )?;
    let (mut lines_added, lines_removed) = parse_numstat(&stat);
    for path in git(root, ["ls-files", "--others", "--exclude-standard", "-z"])?
        .split('\0')
        .filter(|value| !value.is_empty())
    {
        let full_path = root.join(path);
        if let Ok(bytes) = fs::read(full_path)
            && !bytes.contains(&0)
        {
            lines_added += bytes.iter().filter(|byte| **byte == b'\n').count() as u64
                + u64::from(!bytes.is_empty() && !bytes.ends_with(b"\n"));
        }
    }
    Ok(DiffSummary {
        changed_paths,
        changed_paths_truncated,
        lines_added,
        lines_removed,
    })
}

fn git<const N: usize>(root: &Path, args: [&str; N]) -> Result<String> {
    let output = Command::new("git")
        .current_dir(root)
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .arg("--no-optional-locks")
        .arg("-c")
        .arg("core.hooksPath=/dev/null")
        .arg("-c")
        .arg("core.attributesFile=/dev/null")
        .arg("-c")
        .arg("core.fsmonitor=false")
        .args(args)
        .output()
        .map_err(|error| RepoLatchError::Git(format!("could not execute git: {error}")))?;
    if !output.status.success() {
        return Err(RepoLatchError::Git(format!(
            "git command failed with status {}",
            output.status
        )));
    }
    String::from_utf8(output.stdout)
        .map_err(|_| RepoLatchError::Git("git returned non-UTF-8 path data".to_owned()))
}

fn paths_from_nul(input: &str) -> Vec<RelativeRepoPath> {
    input
        .split('\0')
        .filter_map(|path| {
            (!path.is_empty())
                .then(|| RelativeRepoPath::new(path).ok())
                .flatten()
        })
        .collect()
}

fn cap_paths(paths: Vec<RelativeRepoPath>) -> (Vec<RelativeRepoPath>, bool) {
    let mut unique = paths;
    unique.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    unique.dedup_by(|left, right| left.as_str() == right.as_str());
    let truncated = unique.len() > MAX_RECORDED_PATHS;
    (
        unique.into_iter().take(MAX_RECORDED_PATHS).collect(),
        truncated,
    )
}

fn parse_numstat(input: &str) -> (u64, u64) {
    input.lines().fold((0, 0), |(added, removed), line| {
        let mut fields = line.split('\t');
        let add = fields
            .next()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        let remove = fields
            .next()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        (added + add, removed + remove)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn repo() -> TempDir {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("tracked.txt"), "one\n").unwrap();
        initialize_visible_baseline(temp.path()).unwrap();
        temp
    }
    #[test]
    fn captures_dirty_state_without_exposing_file_content() {
        let temp = repo();
        fs::write(temp.path().join("tracked.txt"), "two\n").unwrap();
        fs::write(temp.path().join(".gitignore"), "ignored.txt\n").unwrap();
        fs::write(temp.path().join("ignored.txt"), "FAKE_SECRET_DO_NOT_LEAK").unwrap();
        let snapshot = snapshot_source(temp.path()).unwrap();
        assert!(snapshot.dirty);
        assert!(
            snapshot
                .tracked_paths
                .iter()
                .any(|path| path.as_str() == "tracked.txt")
        );
        assert!(
            snapshot
                .ignored_paths
                .iter()
                .any(|path| path.as_str() == "ignored.txt")
        );
        assert!(!format!("{snapshot:?}").contains("FAKE_SECRET_DO_NOT_LEAK"));
    }
    #[test]
    fn baseline_diff_counts_added_and_removed_lines() {
        let temp = repo();
        let mut file = fs::File::create(temp.path().join("tracked.txt")).unwrap();
        writeln!(file, "two").unwrap();
        writeln!(file, "three").unwrap();
        fs::write(temp.path().join("new.txt"), "new\nline\n").unwrap();
        let diff = summarize_workspace_diff(temp.path()).unwrap();
        assert!(
            diff.changed_paths
                .iter()
                .any(|path| path.as_str() == "tracked.txt")
        );
        assert_eq!((diff.lines_added, diff.lines_removed), (4, 1));
    }

    #[cfg(unix)]
    #[test]
    fn workspace_diff_never_executes_configured_fsmonitor() {
        use std::os::unix::fs::PermissionsExt;

        let temp = repo();
        let marker = temp.path().join("fsmonitor-executed");
        let monitor = temp.path().join("monitor.sh");
        fs::write(
            &monitor,
            format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
        )
        .unwrap();
        fs::set_permissions(&monitor, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(
            Command::new("git")
                .current_dir(temp.path())
                .args(["config", "core.fsmonitor", monitor.to_str().unwrap()])
                .status()
                .unwrap()
                .success()
        );
        fs::write(temp.path().join("tracked.txt"), "changed\n").unwrap();
        summarize_workspace_diff(temp.path()).unwrap();
        assert!(!marker.exists());
    }
}
