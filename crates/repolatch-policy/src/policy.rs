use std::fmt;

use globset::{Glob, GlobBuilder, GlobMatcher};
use repolatch_core::{RelativeRepoPath, RepoLatchError, Result};
use serde::Deserialize;

/// A documented starting point for a project-level `repolatch.toml`.
pub const DEFAULT_POLICY_TEMPLATE: &str = r#"# RepoLatch policy. Deny rules always take precedence.
version = 1

[filesystem]
read = ["src/**", "tests/**", "docs/**", "crates/*/src/**", "crates/*/tests/**", "apps/*/src/**", "README.md", "Cargo.toml"]
write = ["src/**", "tests/**", "crates/*/src/**", "crates/*/tests/**", "apps/*/src/**"]
deny = [".env", ".env.*", "credentials/**", "production/**", "**/*.pem", "**/*.key", ".ssh/**", "**/.ssh/**", ".aws/credentials", "**/.aws/credentials", ".npmrc", "**/.npmrc", ".pypirc", "**/.pypirc", ".netrc", "**/.netrc"]

[network]
mode = "deny"

[network.allow]
hosts = ["registry.npmjs.org", "crates.io"]

[commands]
allow = ["cargo test", "cargo fmt", "cargo clippy"]
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    pub version: u8,
    pub filesystem: FilesystemPolicy,
    pub network: NetworkPolicy,
    pub commands: CommandPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesystemPolicy {
    pub read: Vec<String>,
    pub write: Vec<String>,
    pub deny: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkPolicy {
    pub mode: NetworkMode,
    pub allowed_hosts: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkMode {
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPolicy {
    pub allow: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPolicy {
    version: u8,
    filesystem: RawFilesystemPolicy,
    network: RawNetworkPolicy,
    commands: RawCommandPolicy,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFilesystemPolicy {
    read: Vec<String>,
    write: Vec<String>,
    deny: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNetworkPolicy {
    mode: RawNetworkMode,
    allow: RawNetworkAllow,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RawNetworkMode {
    Allow,
    Deny,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNetworkAllow {
    hosts: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCommandPolicy {
    allow: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyError(String);

impl fmt::Display for PolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for PolicyError {}

impl Policy {
    /// Parse the only supported policy schema. Invalid input must never create a permissive policy.
    pub fn parse(source: &str) -> std::result::Result<Self, PolicyError> {
        let raw: RawPolicy = toml::from_str(source)
            .map_err(|error| PolicyError(format!("invalid repolatch.toml: {error}")))?;
        if raw.version != 1 {
            return Err(PolicyError(format!(
                "unsupported policy version {}; expected 1",
                raw.version
            )));
        }
        validate_hosts(&raw.network.allow.hosts)?;
        validate_commands(&raw.commands.allow)?;
        for pattern in raw
            .filesystem
            .read
            .iter()
            .chain(&raw.filesystem.write)
            .chain(&raw.filesystem.deny)
        {
            validate_pattern(pattern)?;
        }
        Ok(Self {
            version: raw.version,
            filesystem: FilesystemPolicy {
                read: raw.filesystem.read,
                write: raw.filesystem.write,
                deny: raw.filesystem.deny,
            },
            network: NetworkPolicy {
                mode: match raw.network.mode {
                    RawNetworkMode::Allow => NetworkMode::Allow,
                    RawNetworkMode::Deny => NetworkMode::Deny,
                },
                allowed_hosts: raw.network.allow.hosts,
            },
            commands: CommandPolicy {
                allow: raw.commands.allow,
            },
        })
    }

    pub fn compile(&self) -> std::result::Result<CompiledPolicy, PolicyError> {
        Ok(CompiledPolicy {
            policy: self.clone(),
            read: compile_rules(&self.filesystem.read)?,
            write: compile_rules(&self.filesystem.write)?,
            deny: compile_rules(&self.filesystem.deny)?,
        })
    }
}

impl TryFrom<&str> for Policy {
    type Error = PolicyError;

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        Self::parse(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessDecision {
    Allowed,
    Denied,
    Unmatched,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleMatch {
    pub pattern: String,
    pub decision: AccessDecision,
}

#[derive(Debug, Clone)]
struct CompiledRule {
    pattern: String,
    matcher: GlobMatcher,
}

/// Validated policy rules with deterministic deny-wins evaluation.
#[derive(Debug, Clone)]
pub struct CompiledPolicy {
    policy: Policy,
    read: Vec<CompiledRule>,
    write: Vec<CompiledRule>,
    deny: Vec<CompiledRule>,
}

impl CompiledPolicy {
    #[must_use]
    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    #[must_use]
    pub fn evaluate(&self, path: &RelativeRepoPath, access: Access) -> AccessDecision {
        self.explain(path, access).decision
    }

    #[must_use]
    pub fn explain(&self, path: &RelativeRepoPath, access: Access) -> RuleMatch {
        if let Some(rule) = first_match(&self.deny, path) {
            return RuleMatch {
                pattern: rule.pattern.clone(),
                decision: AccessDecision::Denied,
            };
        }
        let matched = match access {
            Access::Read => {
                first_match(&self.read, path).or_else(|| first_match(&self.write, path))
            }
            Access::Write => first_match(&self.write, path),
        };
        if let Some(rule) = matched {
            RuleMatch {
                pattern: rule.pattern.clone(),
                decision: AccessDecision::Allowed,
            }
        } else {
            RuleMatch {
                pattern: String::new(),
                decision: AccessDecision::Unmatched,
            }
        }
    }

    #[must_use]
    pub fn is_command_allowed(&self, command: &str) -> bool {
        self.policy
            .commands
            .allow
            .iter()
            .any(|allowed| allowed == command)
    }
}

fn first_match<'a>(rules: &'a [CompiledRule], path: &RelativeRepoPath) -> Option<&'a CompiledRule> {
    rules
        .iter()
        .find(|rule| rule.matcher.is_match(path.as_str()))
}

fn compile_rules(patterns: &[String]) -> std::result::Result<Vec<CompiledRule>, PolicyError> {
    patterns
        .iter()
        .map(|pattern| {
            let glob = build_glob(pattern)?;
            Ok(CompiledRule {
                pattern: pattern.clone(),
                matcher: glob.compile_matcher(),
            })
        })
        .collect()
}

fn validate_pattern(pattern: &str) -> std::result::Result<(), PolicyError> {
    if pattern.is_empty()
        || pattern.starts_with('/')
        || pattern.contains('\\')
        || pattern.split('/').any(|part| part == "..")
    {
        return Err(PolicyError(format!(
            "glob {pattern:?} must be a non-empty slash-relative repository path"
        )));
    }
    // Compile during validation, so malformed patterns never survive parsing.
    build_glob(pattern)?;
    Ok(())
}

fn build_glob(pattern: &str) -> std::result::Result<Glob, PolicyError> {
    let mut builder = GlobBuilder::new(pattern);
    builder.literal_separator(true).backslash_escape(false);
    builder
        .build()
        .map_err(|error| PolicyError(format!("invalid glob {pattern:?}: {error}")))
}

fn validate_hosts(hosts: &[String]) -> std::result::Result<(), PolicyError> {
    if hosts
        .iter()
        .any(|host| host.is_empty() || host.contains(char::is_whitespace))
    {
        return Err(PolicyError(
            "network allow.hosts must contain non-empty host names".to_owned(),
        ));
    }
    Ok(())
}

fn validate_commands(commands: &[String]) -> std::result::Result<(), PolicyError> {
    if commands
        .iter()
        .any(|command| command.is_empty() || command.contains('\0'))
    {
        return Err(PolicyError(
            "commands.allow cannot contain empty or NUL commands".to_owned(),
        ));
    }
    Ok(())
}

impl From<PolicyError> for RepoLatchError {
    fn from(error: PolicyError) -> Self {
        Self::Policy(error.to_string())
    }
}

/// Parse and compile a policy for callers using the shared domain error type.
pub fn compile_policy(source: &str) -> Result<CompiledPolicy> {
    Ok(Policy::parse(source)?.compile()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_wins_even_when_a_read_rule_matches() {
        let policy = compile_policy(DEFAULT_POLICY_TEMPLATE).unwrap();
        let path = RelativeRepoPath::new("src/production.key").unwrap();
        assert_eq!(policy.evaluate(&path, Access::Read), AccessDecision::Denied);
        assert_eq!(policy.explain(&path, Access::Read).pattern, "**/*.key");
    }

    #[test]
    fn write_permission_also_grants_read_visibility() {
        let policy = compile_policy(
            "version = 1\n[filesystem]\nread = []\nwrite = [\"src/**\"]\ndeny = []\n[network]\nmode = \"deny\"\n[network.allow]\nhosts = []\n[commands]\nallow = []\n",
        )
        .unwrap();
        let path = RelativeRepoPath::new("src/lib.rs").unwrap();
        assert_eq!(
            policy.evaluate(&path, Access::Read),
            AccessDecision::Allowed
        );
        assert_eq!(
            policy.evaluate(&path, Access::Write),
            AccessDecision::Allowed
        );
    }

    #[test]
    fn glob_is_slash_relative_and_recursive() {
        let policy = compile_policy(
            "version = 1\n[filesystem]\nread = [\"src/**\"]\nwrite = []\ndeny = []\n[network]\nmode = \"deny\"\n[network.allow]\nhosts = []\n[commands]\nallow = []\n",
        )
        .unwrap();
        assert_eq!(
            policy.evaluate(&RelativeRepoPath::new("src/a/b.rs").unwrap(), Access::Read),
            AccessDecision::Allowed
        );
        assert_eq!(
            policy.evaluate(
                &RelativeRepoPath::new("other/src/a.rs").unwrap(),
                Access::Read
            ),
            AccessDecision::Unmatched
        );
    }

    #[test]
    fn a_single_star_never_crosses_a_path_separator() {
        let policy = compile_policy(
            "version = 1\n[filesystem]\nread = [\"src/*\"]\nwrite = []\ndeny = []\n[network]\nmode = \"deny\"\n[network.allow]\nhosts = []\n[commands]\nallow = []\n",
        )
        .unwrap();
        assert_eq!(
            policy.evaluate(&RelativeRepoPath::new("src/top.rs").unwrap(), Access::Read),
            AccessDecision::Allowed
        );
        assert_eq!(
            policy.evaluate(
                &RelativeRepoPath::new("src/nested/file.rs").unwrap(),
                Access::Read
            ),
            AccessDecision::Unmatched
        );
    }

    #[test]
    fn malformed_or_unknown_fields_fail_closed() {
        assert!(Policy::parse("version = 2").is_err());
        assert!(Policy::parse("version = 1\nunexpected = true").is_err());
        assert!(
            Policy::parse(
                DEFAULT_POLICY_TEMPLATE
                    .replace("mode = \"deny\"", "mode = \"maybe\"")
                    .as_str()
            )
            .is_err()
        );
        assert!(
            Policy::parse(
                DEFAULT_POLICY_TEMPLATE
                    .replace("src/**", "../src/**")
                    .as_str()
            )
            .is_err()
        );
    }
}
