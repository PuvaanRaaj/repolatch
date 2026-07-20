//! Shared, presentation-independent `AgentGuard` domain types.

mod capability;
mod error;
mod path;
mod session;

pub use capability::{BackendCapabilities, CapabilityState, EnforcementLevel};
pub use error::{AgentGuardError, Result};
pub use path::{RelativeRepoPath, RepoRoot};
pub use session::{CommandSpec, SessionId};
