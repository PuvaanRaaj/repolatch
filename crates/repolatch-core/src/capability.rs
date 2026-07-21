use serde::{Deserialize, Serialize};

/// User-visible strength of an individual control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementLevel {
    Enforced,
    Advisory,
    Unavailable,
    Warning,
    NotReliablyObserved,
}

impl std::fmt::Display for EnforcementLevel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Enforced => "Enforced",
            Self::Advisory => "Advisory",
            Self::Unavailable => "Unavailable",
            Self::Warning => "Warning",
            Self::NotReliablyObserved => "Not reliably observed",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityState {
    pub level: EnforcementLevel,
    pub explanation: String,
}

impl CapabilityState {
    #[must_use]
    pub fn new(level: EnforcementLevel, explanation: impl Into<String>) -> Self {
        Self {
            level,
            explanation: explanation.into(),
        }
    }
}

/// Capabilities are independent so a UI cannot infer whole-backend safety.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendCapabilities {
    pub filesystem_isolation: CapabilityState,
    pub workspace_write_boundary: CapabilityState,
    pub policy_write_restrictions: CapabilityState,
    pub network_deny: CapabilityState,
    pub hostname_allowlist: CapabilityState,
    pub environment_filtering: CapabilityState,
    pub child_command_observation: CapabilityState,
}

#[cfg(test)]
mod tests {
    use super::EnforcementLevel;

    #[test]
    fn enforcement_labels_are_human_readable() {
        assert_eq!(EnforcementLevel::Enforced.to_string(), "Enforced");
        assert_eq!(EnforcementLevel::Advisory.to_string(), "Advisory");
        assert_eq!(EnforcementLevel::Unavailable.to_string(), "Unavailable");
        assert_eq!(EnforcementLevel::Warning.to_string(), "Warning");
        assert_eq!(
            EnforcementLevel::NotReliablyObserved.to_string(),
            "Not reliably observed"
        );
    }
}
