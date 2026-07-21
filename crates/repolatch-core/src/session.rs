use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{RepoLatchError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(Uuid);

impl SessionId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandSpec {
    argv: Vec<String>,
}

impl CommandSpec {
    pub fn new(argv: Vec<String>) -> Result<Self> {
        if argv.is_empty() || argv[0].is_empty() {
            return Err(RepoLatchError::Execution(
                "command argv must contain an executable".to_owned(),
            ));
        }
        if argv.iter().any(|value| value.contains('\0')) {
            return Err(RepoLatchError::Execution(
                "command arguments cannot contain NUL bytes".to_owned(),
            ));
        }
        Ok(Self { argv })
    }

    #[must_use]
    pub fn argv(&self) -> &[String] {
        &self.argv
    }
}
