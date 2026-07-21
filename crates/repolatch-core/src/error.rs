use thiserror::Error;

/// Error categories shared across `RepoLatch` crates.
#[derive(Debug, Error)]
pub enum RepoLatchError {
    #[error("invalid repository path: {0}")]
    InvalidPath(String),
    #[error("policy error: {0}")]
    Policy(String),
    #[error("repository scan failed: {0}")]
    Scan(String),
    #[error("Git operation failed: {0}")]
    Git(String),
    #[error("workspace operation failed: {0}")]
    Workspace(String),
    #[error("backend unavailable: {0}")]
    BackendUnavailable(String),
    #[error("requested capability unavailable: {0}")]
    CapabilityUnavailable(String),
    #[error("execution failed: {0}")]
    Execution(String),
    #[error("receipt operation failed: {0}")]
    Receipt(String),
    #[error("session interrupted: {0}")]
    Interrupted(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, RepoLatchError>;
