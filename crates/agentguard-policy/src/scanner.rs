use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use agentguard_core::{RelativeRepoPath, RepoRoot};
use globset::{GlobBuilder, GlobMatcher};
use walkdir::WalkDir;

use crate::{Access, AccessDecision, CompiledPolicy};

const DEFAULT_SENSITIVE_PATTERNS: &[(&str, SensitiveClassification)] = &[
    (".env", SensitiveClassification::Environment),
    (".env.*", SensitiveClassification::Environment),
    ("**/.env", SensitiveClassification::Environment),
    ("**/.env.*", SensitiveClassification::Environment),
    ("**/*.pem", SensitiveClassification::Pem),
    ("**/*.key", SensitiveClassification::PrivateKey),
    ("**/id_rsa", SensitiveClassification::SshMaterial),
    ("**/id_ed25519", SensitiveClassification::SshMaterial),
    (".ssh/**", SensitiveClassification::SshMaterial),
    ("**/.ssh/**", SensitiveClassification::SshMaterial),
    (
        ".aws/credentials",
        SensitiveClassification::CloudCredentials,
    ),
    (
        "**/.aws/credentials",
        SensitiveClassification::CloudCredentials,
    ),
    (
        ".config/gcloud/application_default_credentials.json",
        SensitiveClassification::CloudCredentials,
    ),
    (
        "**/.config/gcloud/application_default_credentials.json",
        SensitiveClassification::CloudCredentials,
    ),
    (
        ".azure/accessTokens.json",
        SensitiveClassification::CloudCredentials,
    ),
    (
        "**/.azure/accessTokens.json",
        SensitiveClassification::CloudCredentials,
    ),
    (".npmrc", SensitiveClassification::PackageAuth),
    ("**/.npmrc", SensitiveClassification::PackageAuth),
    (".pypirc", SensitiveClassification::PackageAuth),
    ("**/.pypirc", SensitiveClassification::PackageAuth),
    (".netrc", SensitiveClassification::PackageAuth),
    ("**/.netrc", SensitiveClassification::PackageAuth),
    ("credentials/**", SensitiveClassification::CloudCredentials),
    (
        "**/credentials/**",
        SensitiveClassification::CloudCredentials,
    ),
    (
        "production/**",
        SensitiveClassification::ProductionConfiguration,
    ),
    (
        "**/production/**",
        SensitiveClassification::ProductionConfiguration,
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    File,
    Directory,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensitiveClassification {
    Environment,
    Pem,
    PrivateKey,
    CloudCredentials,
    SshMaterial,
    PackageAuth,
    ProductionConfiguration,
    PolicyDenied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensitiveMatch {
    pub classification: SensitiveClassification,
    pub pattern: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanEntry {
    pub path: RelativeRepoPath,
    pub kind: FileKind,
    pub size_bytes: Option<u64>,
    pub read_access: AccessDecision,
    /// The matching read rule, when one exists. It never contains file contents.
    pub read_rule: Option<String>,
    pub write_access: AccessDecision,
    /// The matching write rule, when one exists. It never contains file contents.
    pub write_rule: Option<String>,
    pub sensitive: Vec<SensitiveMatch>,
    pub git_tracked: Option<bool>,
    pub git_ignored: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanWarning {
    SymlinkOmitted { path: PathBuf },
    InvalidPathOmitted { path: PathBuf },
    EntryLimitReached { limit: usize },
    GitMetadataUnavailable { detail: String },
    WalkError { detail: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryScan {
    pub entries: Vec<ScanEntry>,
    pub warnings: Vec<ScanWarning>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanOptions {
    /// Maximum metadata entries to return; scanning stops once the bound is reached.
    pub max_entries: usize,
    /// Annotate records via the local Git executable when it is available.
    pub annotate_git: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_entries: 100_000,
            annotate_git: true,
        }
    }
}

#[derive(Debug, Clone)]
struct SensitiveRule {
    pattern: &'static str,
    classification: SensitiveClassification,
    matcher: GlobMatcher,
}

/// Metadata-only scanner. It never reads regular-file contents, including binary and large files.
pub fn scan_repository(
    root: &RepoRoot,
    policy: &CompiledPolicy,
    options: ScanOptions,
) -> RepositoryScan {
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    let rules = default_sensitive_rules();
    let walker = WalkDir::new(root.as_path())
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.file_name() != ".git");

    for next in walker {
        let entry = match next {
            Ok(entry) => entry,
            Err(error) => {
                warnings.push(ScanWarning::WalkError {
                    detail: error.to_string(),
                });
                continue;
            }
        };
        if entry.path() == root.as_path() {
            continue;
        }
        let relative_os = match entry.path().strip_prefix(root.as_path()) {
            Ok(path) => path,
            Err(_) => {
                warnings.push(ScanWarning::InvalidPathOmitted {
                    path: entry.path().to_owned(),
                });
                continue;
            }
        };
        if entry.file_type().is_symlink() {
            warnings.push(ScanWarning::SymlinkOmitted {
                path: relative_os.to_owned(),
            });
            continue;
        }
        let path = match RelativeRepoPath::new(relative_os) {
            Ok(path) => path,
            Err(_) => {
                warnings.push(ScanWarning::InvalidPathOmitted {
                    path: relative_os.to_owned(),
                });
                continue;
            }
        };
        if entries.len() == options.max_entries {
            warnings.push(ScanWarning::EntryLimitReached {
                limit: options.max_entries,
            });
            break;
        }
        let kind = if entry.file_type().is_file() {
            FileKind::File
        } else if entry.file_type().is_dir() {
            FileKind::Directory
        } else {
            FileKind::Other
        };
        let size_bytes = if kind == FileKind::File {
            entry.metadata().ok().map(|metadata| metadata.len())
        } else {
            None
        };
        let mut sensitive = sensitive_matches(&rules, &path);
        let read_match = policy.explain(&path, Access::Read);
        let write_match = policy.explain(&path, Access::Write);
        let read_access = read_match.decision;
        let write_access = write_match.decision;
        if read_access == AccessDecision::Denied || write_access == AccessDecision::Denied {
            sensitive.push(SensitiveMatch {
                classification: SensitiveClassification::PolicyDenied,
                pattern: if read_access == AccessDecision::Denied {
                    read_match.pattern.clone()
                } else {
                    write_match.pattern.clone()
                },
            });
        }
        entries.push(ScanEntry {
            path,
            kind,
            size_bytes,
            read_access,
            read_rule: (!read_match.pattern.is_empty()).then_some(read_match.pattern),
            write_access,
            write_rule: (!write_match.pattern.is_empty()).then_some(write_match.pattern),
            sensitive,
            git_tracked: None,
            git_ignored: None,
        });
    }
    if options.annotate_git {
        annotate_git(root.as_path(), &mut entries, &mut warnings);
    }
    RepositoryScan { entries, warnings }
}

fn default_sensitive_rules() -> Vec<SensitiveRule> {
    DEFAULT_SENSITIVE_PATTERNS
        .iter()
        .filter_map(|(pattern, classification)| {
            let mut builder = GlobBuilder::new(pattern);
            builder.literal_separator(true).backslash_escape(false);
            builder.build().ok().map(|glob| SensitiveRule {
                pattern,
                classification: *classification,
                matcher: glob.compile_matcher(),
            })
        })
        .collect()
}

fn sensitive_matches(rules: &[SensitiveRule], path: &RelativeRepoPath) -> Vec<SensitiveMatch> {
    rules
        .iter()
        .filter(|rule| rule.matcher.is_match(path.as_str()))
        .map(|rule| SensitiveMatch {
            classification: rule.classification,
            pattern: rule.pattern.to_owned(),
        })
        .collect()
}

fn annotate_git(root: &Path, entries: &mut [ScanEntry], warnings: &mut Vec<ScanWarning>) {
    let tracked = match git_paths(root, &["ls-files", "-z", "--cached"]) {
        Ok(paths) => paths,
        Err(detail) => {
            warnings.push(ScanWarning::GitMetadataUnavailable { detail });
            return;
        }
    };
    let candidates = entries
        .iter()
        .filter(|entry| entry.kind == FileKind::File)
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();
    let ignored = match git_check_ignored(root, &candidates) {
        Ok(paths) => paths,
        Err(detail) => {
            warnings.push(ScanWarning::GitMetadataUnavailable { detail });
            HashMap::new()
        }
    };
    for entry in entries {
        entry.git_tracked = Some(tracked.contains_key(entry.path.as_str()));
        entry.git_ignored = Some(ignored.contains_key(entry.path.as_str()));
    }
}

fn git_paths(root: &Path, arguments: &[&str]) -> std::result::Result<HashMap<String, ()>, String> {
    let output = Command::new("git")
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .arg("--no-optional-locks")
        .args(["-c", "core.hooksPath=/dev/null"])
        .args(["-c", "core.fsmonitor=false"])
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not execute git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} exited {}",
            arguments.join(" "),
            output.status
        ));
    }
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .filter_map(|path| std::str::from_utf8(path).ok())
        .map(|path| (path.to_owned(), ()))
        .collect())
}

fn git_check_ignored(
    root: &Path,
    candidates: &[&str],
) -> std::result::Result<HashMap<String, ()>, String> {
    let mut child = Command::new("git")
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .arg("--no-optional-locks")
        .args(["-c", "core.hooksPath=/dev/null"])
        .args(["-c", "core.fsmonitor=false"])
        .args(["check-ignore", "-z", "--stdin"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not execute git check-ignore: {error}"))?;
    if let Some(stdin) = child.stdin.as_mut() {
        for path in candidates {
            stdin
                .write_all(path.as_bytes())
                .and_then(|()| stdin.write_all(&[0]))
                .map_err(|error| format!("could not provide paths to git check-ignore: {error}"))?;
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("could not wait for git check-ignore: {error}"))?;
    // Exit 1 means none matched and is not an error.
    if !output.status.success() && output.status.code() != Some(1) {
        return Err(format!("git check-ignore exited {}", output.status));
    }
    Ok(output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .filter_map(|path| std::str::from_utf8(path).ok())
        .map(|path| (path.to_owned(), ()))
        .collect())
}

#[cfg(test)]
mod tests {
    use std::{fs, process::Command};

    use super::*;
    use crate::compile_policy;
    use tempfile::tempdir;

    fn policy() -> CompiledPolicy {
        compile_policy(crate::DEFAULT_POLICY_TEMPLATE).unwrap()
    }

    #[test]
    fn classifies_env_without_reading_its_value() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join(".env"), "TOKEN=not-to-be-disclosed").unwrap();
        let root = RepoRoot::discover(directory.path()).unwrap();
        let scan = scan_repository(
            &root,
            &policy(),
            ScanOptions {
                annotate_git: false,
                ..ScanOptions::default()
            },
        );
        let entry = scan
            .entries
            .iter()
            .find(|entry| entry.path.as_str() == ".env")
            .unwrap();
        assert!(
            entry
                .sensitive
                .iter()
                .any(|item| item.classification == SensitiveClassification::Environment)
        );
        assert_eq!(entry.size_bytes, Some(25));
    }

    #[test]
    fn default_detection_covers_nested_env_and_pem_paths() {
        let directory = tempdir().unwrap();
        fs::create_dir_all(directory.path().join("service/config")).unwrap();
        fs::write(directory.path().join("service/config/.env.local"), "fake").unwrap();
        fs::write(directory.path().join("service/config/server.pem"), "fake").unwrap();
        let root = RepoRoot::discover(directory.path()).unwrap();
        let scan = scan_repository(
            &root,
            &policy(),
            ScanOptions {
                annotate_git: false,
                ..ScanOptions::default()
            },
        );
        let env = scan
            .entries
            .iter()
            .find(|entry| entry.path.as_str() == "service/config/.env.local")
            .unwrap();
        let pem = scan
            .entries
            .iter()
            .find(|entry| entry.path.as_str() == "service/config/server.pem")
            .unwrap();
        assert!(
            env.sensitive
                .iter()
                .any(|item| item.classification == SensitiveClassification::Environment)
        );
        assert!(
            pem.sensitive
                .iter()
                .any(|item| item.classification == SensitiveClassification::Pem)
        );
    }

    #[test]
    fn omits_symlinks_conservatively() {
        let directory = tempdir().unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/passwd", directory.path().join("escape")).unwrap();
        #[cfg(unix)]
        {
            let root = RepoRoot::discover(directory.path()).unwrap();
            let scan = scan_repository(
                &root,
                &policy(),
                ScanOptions {
                    annotate_git: false,
                    ..ScanOptions::default()
                },
            );
            assert!(
                scan.entries
                    .iter()
                    .all(|entry| entry.path.as_str() != "escape")
            );
            assert!(
                scan.warnings
                    .iter()
                    .any(|warning| matches!(warning, ScanWarning::SymlinkOmitted { .. }))
            );
        }
    }

    #[test]
    fn handles_unicode_spaces_and_large_binary_as_metadata_only() {
        let directory = tempdir().unwrap();
        let filename = "src/你好 file.bin";
        fs::create_dir_all(directory.path().join("src")).unwrap();
        fs::write(directory.path().join(filename), vec![0_u8; 1_048_576]).unwrap();
        let root = RepoRoot::discover(directory.path()).unwrap();
        let scan = scan_repository(
            &root,
            &policy(),
            ScanOptions {
                annotate_git: false,
                ..ScanOptions::default()
            },
        );
        let entry = scan
            .entries
            .iter()
            .find(|entry| entry.path.as_str() == filename)
            .unwrap();
        assert_eq!(entry.size_bytes, Some(1_048_576));
    }

    #[test]
    fn adds_git_tracked_and_ignored_annotations() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join(".gitignore"), "ignored.txt\n").unwrap();
        fs::write(directory.path().join("tracked.txt"), "fake data").unwrap();
        fs::write(directory.path().join("ignored.txt"), "fake data").unwrap();
        let init = Command::new("git")
            .args(["init", "-q"])
            .current_dir(directory.path())
            .status();
        if init.map_or(true, |status| !status.success()) {
            return;
        }
        assert!(
            Command::new("git")
                .args(["add", ".gitignore", "tracked.txt"])
                .current_dir(directory.path())
                .status()
                .unwrap()
                .success()
        );
        let root = RepoRoot::discover(directory.path()).unwrap();
        let scan = scan_repository(&root, &policy(), ScanOptions::default());
        assert_eq!(
            scan.entries
                .iter()
                .find(|entry| entry.path.as_str() == "tracked.txt")
                .unwrap()
                .git_tracked,
            Some(true)
        );
        assert_eq!(
            scan.entries
                .iter()
                .find(|entry| entry.path.as_str() == "ignored.txt")
                .unwrap()
                .git_ignored,
            Some(true)
        );
    }

    #[test]
    fn stops_at_configured_entry_bound() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join("one"), "x").unwrap();
        fs::write(directory.path().join("two"), "x").unwrap();
        let root = RepoRoot::discover(directory.path()).unwrap();
        let scan = scan_repository(
            &root,
            &policy(),
            ScanOptions {
                max_entries: 1,
                annotate_git: false,
            },
        );
        assert_eq!(scan.entries.len(), 1);
        assert!(
            scan.warnings
                .iter()
                .any(|warning| matches!(warning, ScanWarning::EntryLimitReached { limit: 1 }))
        );
    }
}
