//! Strict RepoLatch policy compilation and metadata-only repository inspection.
//!
//! This crate never opens regular files. It deliberately omits symlinks from the
//! scan result because following one can cross the repository boundary.

mod policy;
mod scanner;

pub use policy::{
    Access, AccessDecision, CompiledPolicy, DEFAULT_POLICY_TEMPLATE, FilesystemPolicy, NetworkMode,
    Policy, PolicyError, RuleMatch, compile_policy,
};
pub use scanner::{
    FileKind, RepositoryScan, ScanEntry, ScanOptions, ScanWarning, SensitiveClassification,
    SensitiveMatch, scan_repository,
};
