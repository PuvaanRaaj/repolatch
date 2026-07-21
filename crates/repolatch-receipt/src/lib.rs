//! Stable, metadata-only execution receipts.

use repolatch_core::{BackendCapabilities, RelativeRepoPath, RepoLatchError, Result, SessionId};
use repolatch_git::DiffSummary;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const RECEIPT_SCHEMA_VERSION: u8 = 1;
pub const MAX_ARG_BYTES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub schema_version: u8,
    pub session_id: SessionId,
    pub status: ReceiptStatus,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    pub backend: BackendReceipt,
    pub command: Vec<String>,
    pub source: SourceReceipt,
    pub policy_sha256: String,
    pub visible_paths: PathSummary,
    pub denied_paths: PathSummary,
    pub diff: DiffSummary,
    pub validation_commands: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub warnings: Vec<String>,
    pub limitations: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendReceipt {
    pub id: String,
    pub capabilities: BackendCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceReceipt {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    pub dirty: bool,
    pub git_context: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathSummary {
    pub count: usize,
    pub paths: Vec<RelativeRepoPath>,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub struct ReceiptStart {
    pub session_id: SessionId,
    pub started_at: String,
    pub backend: BackendReceipt,
    pub command: Vec<String>,
    pub source: SourceReceipt,
    pub policy_sha256: String,
    pub visible_paths: PathSummary,
    pub denied_paths: PathSummary,
    pub warnings: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Partial,
    Completed,
    Failed,
    Interrupted,
}

impl Receipt {
    #[must_use]
    pub fn partial(start: ReceiptStart) -> Self {
        Self {
            schema_version: RECEIPT_SCHEMA_VERSION,
            session_id: start.session_id,
            status: ReceiptStatus::Partial,
            started_at: start.started_at,
            finished_at: None,
            backend: start.backend,
            command: sanitize_command(&start.command),
            source: start.source,
            policy_sha256: start.policy_sha256,
            visible_paths: cap_path_summary(start.visible_paths),
            denied_paths: cap_path_summary(start.denied_paths),
            diff: DiffSummary::default(),
            validation_commands: Vec::new(),
            exit_code: None,
            warnings: start.warnings,
            limitations: start.limitations,
            error_kind: None,
        }
    }
    pub fn finish(
        &mut self,
        status: ReceiptStatus,
        finished_at: impl Into<String>,
        diff: DiffSummary,
        exit_code: Option<i32>,
        error_kind: Option<&str>,
    ) -> Result<()> {
        if !matches!(self.status, ReceiptStatus::Partial) {
            return Err(RepoLatchError::Receipt(
                "only a partial receipt can be finalized".to_owned(),
            ));
        }
        if matches!(status, ReceiptStatus::Partial) {
            return Err(RepoLatchError::Receipt(
                "final receipt status cannot be partial".to_owned(),
            ));
        }
        self.status = status;
        self.finished_at = Some(finished_at.into());
        self.diff = diff;
        self.exit_code = exit_code;
        self.error_kind = error_kind.map(sanitize_error_kind);
        Ok(())
    }
}

#[must_use]
pub fn policy_sha256(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
}

fn cap_path_summary(mut summary: PathSummary) -> PathSummary {
    const MAX_PATHS: usize = 200;
    summary
        .paths
        .sort_by(|left, right| left.as_str().cmp(right.as_str()));
    summary
        .paths
        .dedup_by(|left, right| left.as_str() == right.as_str());
    summary.truncated |= summary.paths.len() > MAX_PATHS || summary.paths.len() < summary.count;
    summary.paths.truncate(MAX_PATHS);
    summary
}

pub fn sanitize_command(argv: &[String]) -> Vec<String> {
    argv.iter()
        .enumerate()
        .map(|(index, arg)| {
            if index == 0 {
                truncate_argument(arg)
            } else {
                "[ARGUMENT REDACTED]".to_owned()
            }
        })
        .collect()
}

fn truncate_argument(arg: &str) -> String {
    if arg.chars().count() > MAX_ARG_BYTES {
        format!(
            "{}…[truncated]",
            arg.chars().take(MAX_ARG_BYTES).collect::<String>()
        )
    } else {
        arg.to_owned()
    }
}

fn sanitize_error_kind(value: &str) -> String {
    match value {
        "execution" | "interrupted" | "backend_unavailable" | "capability_unavailable" => {
            value.to_owned()
        }
        _ => "execution_error".to_owned(),
    }
}

pub struct ReceiptWriter {
    path: PathBuf,
}
impl ReceiptWriter {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn write(&self, receipt: &Receipt) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(receipt).map_err(|error| {
            RepoLatchError::Receipt(format!("could not serialize receipt: {error}"))
        })?;
        let parent = self
            .path
            .parent()
            .ok_or_else(|| RepoLatchError::Receipt("receipt path has no parent".to_owned()))?;
        fs::create_dir_all(parent)?;
        let temporary = self.path.with_extension("tmp");
        fs::write(&temporary, bytes)?;
        fs::rename(temporary, &self.path)?;
        Ok(())
    }
}

pub fn render_json(receipt: &Receipt) -> Result<String> {
    serde_json::to_string_pretty(receipt)
        .map_err(|error| RepoLatchError::Receipt(format!("could not render receipt: {error}")))
}
#[must_use]
pub fn render_terminal(receipt: &Receipt) -> String {
    format!(
        "RepoLatch receipt v{}\nsession: {}\nstatus: {:?}\nbackend: {}\nsource revision: {}\npolicy sha256: {}\nvisible paths: {}\ndenied paths: {}\nchanged files: {}\nlines: +{} -{}\nexit code: {}",
        receipt.schema_version,
        receipt.session_id,
        receipt.status,
        receipt.backend.id,
        receipt.source.revision.as_deref().unwrap_or("unborn"),
        receipt.policy_sha256,
        receipt.visible_paths.count,
        receipt.denied_paths.count,
        receipt.diff.changed_paths.len(),
        receipt.diff.lines_added,
        receipt.diff.lines_removed,
        receipt
            .exit_code
            .map_or_else(|| "unknown".to_owned(), |code| code.to_string())
    )
}
#[must_use]
pub fn render_markdown(receipt: &Receipt) -> String {
    format!(
        "# RepoLatch Receipt\n\n- Schema: {}\n- Session: `{}`\n- Status: `{:?}`\n- Backend: `{}`\n- Source revision: `{}`\n- Policy SHA-256: `{}`\n- Visible paths: {}\n- Denied paths: {}\n- Changed files: {}\n- Lines: +{} / -{}\n- Exit code: `{}`\n",
        receipt.schema_version,
        receipt.session_id,
        receipt.status,
        receipt.backend.id,
        receipt.source.revision.as_deref().unwrap_or("unborn"),
        receipt.policy_sha256,
        receipt.visible_paths.count,
        receipt.denied_paths.count,
        receipt.diff.changed_paths.len(),
        receipt.diff.lines_added,
        receipt.diff.lines_removed,
        receipt
            .exit_code
            .map_or_else(|| "unknown".to_owned(), |code| code.to_string())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use repolatch_core::{CapabilityState, EnforcementLevel};
    use tempfile::TempDir;

    fn capabilities() -> BackendCapabilities {
        let state = || CapabilityState::new(EnforcementLevel::Advisory, "test");
        BackendCapabilities {
            filesystem_isolation: state(),
            workspace_write_boundary: state(),
            policy_write_restrictions: state(),
            network_deny: state(),
            hostname_allowlist: state(),
            environment_filtering: state(),
            child_command_observation: state(),
        }
    }

    fn start(command: Vec<String>) -> ReceiptStart {
        ReceiptStart {
            session_id: SessionId::new(),
            started_at: "2026-07-20T00:00:00Z".to_owned(),
            backend: BackendReceipt {
                id: "local-advisory".to_owned(),
                capabilities: capabilities(),
            },
            command,
            source: SourceReceipt {
                revision: Some("abc".to_owned()),
                dirty: false,
                git_context: "synthesized_visible_baseline".to_owned(),
            },
            policy_sha256: policy_sha256("fake policy"),
            visible_paths: PathSummary {
                count: 1,
                paths: vec![RelativeRepoPath::new("visible.txt").unwrap()],
                truncated: false,
            },
            denied_paths: PathSummary::default(),
            warnings: vec![],
            limitations: vec!["test limitation".to_owned()],
        }
    }

    #[test]
    fn json_is_stable_and_redacts_fake_secrets() {
        let receipt = Receipt::partial(start(vec![
            "tool".to_owned(),
            "token=FAKE_SECRET_DO_NOT_LEAK".to_owned(),
        ]));
        let json = render_json(&receipt).unwrap();
        assert_eq!(receipt.schema_version, 1);
        assert!(!json.contains("FAKE_SECRET_DO_NOT_LEAK"));
        assert!(json.contains("[ARGUMENT REDACTED]"));
    }

    #[test]
    fn positional_arguments_are_never_persisted_verbatim() {
        let receipt = Receipt::partial(start(vec![
            "tool".to_owned(),
            "FAKE_POSITIONAL_SECRET".to_owned(),
            "--verbose".to_owned(),
        ]));
        let json = render_json(&receipt).unwrap();
        assert!(!json.contains("FAKE_POSITIONAL_SECRET"));
        assert!(json.contains("[ARGUMENT REDACTED]"));
        assert!(!json.contains("--verbose"));
    }

    #[test]
    fn option_shaped_secrets_are_never_persisted() {
        for secret in [
            "--header=Authorization: Bearer FAKE_HEADER_SECRET",
            "--oauth2-bearer=FAKE_BEARER_SECRET",
            "--cookie=session=FAKE_COOKIE_SECRET",
            "--unknown=FAKE_UNKNOWN_SECRET",
            "https://user:FAKE_URI_SECRET@example.invalid",
        ] {
            let receipt = Receipt::partial(start(vec!["curl".to_owned(), secret.to_owned()]));
            let json = render_json(&receipt).unwrap();
            assert!(!json.contains(secret));
            assert!(!json.contains("FAKE_"));
        }
    }
    #[test]
    fn partial_receipt_survives_then_finalizes_after_error() {
        let temp = TempDir::new().unwrap();
        let writer = ReceiptWriter::new(temp.path().join("receipt.json"));
        let mut receipt = Receipt::partial(start(vec!["tool".to_owned()]));
        writer.write(&receipt).unwrap();
        assert!(render_json(&receipt).unwrap().contains("partial"));
        receipt
            .finish(
                ReceiptStatus::Interrupted,
                "end",
                DiffSummary::default(),
                None,
                Some("signal: FAKE_SECRET_DO_NOT_LEAK"),
            )
            .unwrap();
        writer.write(&receipt).unwrap();
        let saved = fs::read_to_string(writer.path()).unwrap();
        assert!(saved.contains("interrupted"));
        assert!(!saved.contains("FAKE_SECRET_DO_NOT_LEAK"));
    }
}
