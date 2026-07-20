use std::{
    ffi::OsStr,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{AgentGuardError, Result};

/// Canonical path of a repository directory selected by the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRoot(PathBuf);

impl RepoRoot {
    pub fn discover(path: impl AsRef<Path>) -> Result<Self> {
        let canonical = path.as_ref().canonicalize()?;
        if !canonical.is_dir() {
            return Err(AgentGuardError::InvalidPath(
                "repository root must be a directory".to_owned(),
            ));
        }
        Ok(Self(canonical))
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Join a validated path and verify lexical containment.
    #[must_use]
    pub fn join(&self, relative: &RelativeRepoPath) -> PathBuf {
        self.0.join(relative.as_path())
    }
}

/// A normalized, non-empty, repository-relative path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RelativeRepoPath(String);

impl RelativeRepoPath {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if path.as_os_str().is_empty() {
            return Err(AgentGuardError::InvalidPath("path is empty".to_owned()));
        }

        let mut parts = Vec::new();
        for component in path.components() {
            match component {
                Component::Normal(part) => parts.push(os_to_utf8(part)?),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(AgentGuardError::InvalidPath(
                        "path must be repository-relative and cannot contain '..'".to_owned(),
                    ));
                }
            }
        }
        if parts.is_empty() {
            return Err(AgentGuardError::InvalidPath("path is empty".to_owned()));
        }
        if parts.iter().any(|part| part.contains('\0')) {
            return Err(AgentGuardError::InvalidPath(
                "path cannot contain a NUL byte".to_owned(),
            ));
        }
        Ok(Self(parts.join("/")))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }
}

impl TryFrom<String> for RelativeRepoPath {
    type Error = AgentGuardError;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<RelativeRepoPath> for String {
    fn from(value: RelativeRepoPath) -> Self {
        value.0
    }
}

fn os_to_utf8(part: &OsStr) -> Result<String> {
    part.to_str().map(ToOwned::to_owned).ok_or_else(|| {
        AgentGuardError::InvalidPath("path is not valid UTF-8 and cannot be receipted".to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn rejects_escapes_and_absolute_paths() {
        assert!(RelativeRepoPath::new("../secret").is_err());
        assert!(RelativeRepoPath::new("/etc/passwd").is_err());
        assert!(RelativeRepoPath::new("").is_err());
    }

    #[test]
    fn preserves_spaces_and_unicode() {
        let value = RelativeRepoPath::new("docs/你好 world.md").unwrap();
        assert_eq!(value.as_str(), "docs/你好 world.md");
    }

    proptest! {
        #[test]
        fn accepted_paths_never_have_parent_components(input in ".{1,128}") {
            if let Ok(path) = RelativeRepoPath::new(&input) {
                prop_assert!(!path.as_path().is_absolute());
                prop_assert!(!path.as_path().components().any(|part| matches!(part, Component::ParentDir)));
            }
        }
    }
}
