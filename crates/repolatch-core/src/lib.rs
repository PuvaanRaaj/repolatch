//! Shared, presentation-independent `RepoLatch` domain types.

mod capability;
mod error;
mod path;
mod session;

pub use capability::{BackendCapabilities, CapabilityState, EnforcementLevel};
pub use error::{RepoLatchError, Result};
pub use path::{RelativeRepoPath, RepoRoot};
pub use session::{CommandSpec, SessionId};
