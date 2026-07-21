//! Offline desktop adapters. Commands return bounded, metadata-only data and never accept shell text.

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};

use repolatch_core::{BackendCapabilities, CommandSpec, RelativeRepoPath, RepoRoot, SessionId};
use repolatch_git::{
    DiffSummary, initialize_visible_baseline, snapshot_source, summarize_workspace_diff,
};
use repolatch_policy::{
    Access, AccessDecision, CompiledPolicy, DEFAULT_POLICY_TEMPLATE, NetworkMode, ScanOptions,
    ScanWarning, compile_policy, scan_repository,
};
use repolatch_receipt::{
    BackendReceipt, PathSummary, Receipt, ReceiptStart, ReceiptStatus, ReceiptWriter,
    SourceReceipt, policy_sha256, render_json, render_markdown, render_terminal,
};
use repolatch_runtime::{
    DockerBackend, ExecutionBackend, ExecutionRequest, LocalBackend, MinimalEnvironment,
    NetworkAccess, WorkspaceBuilder,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const POLICY_FILE: &str = "repolatch.toml";
const MAX_TREE_ENTRIES: usize = 2_000;
const MAX_PREVIEW_BYTES: u64 = 256 * 1024;
const MAX_DIFF_BYTES: usize = 512 * 1024;

#[derive(Default)]
struct DesktopState {
    repository: Mutex<Option<PathBuf>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryView {
    root: String,
    entries: Vec<TreeEntry>,
    warnings: Vec<String>,
    git: GitView,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TreeEntry {
    path: String,
    kind: String,
    size_bytes: Option<u64>,
    read_access: String,
    write_access: String,
    sensitive: Vec<String>,
    git_tracked: Option<bool>,
    git_ignored: Option<bool>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitView {
    head: Option<String>,
    dirty: bool,
    tracked_paths_truncated: bool,
    ignored_paths_truncated: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PolicyView {
    source: String,
    valid: bool,
    error: Option<String>,
    allowed_commands: Vec<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TextPreview {
    state: String,
    content: Option<String>,
    message: String,
    bytes: Option<u64>,
    editable: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackendView {
    id: String,
    available: bool,
    availability_message: Option<String>,
    capabilities: BackendCapabilities,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchRequest {
    backend: String,
    argv: Vec<String>,
    docker_image: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionView {
    id: String,
    backend: String,
    workspace: String,
    diff: DiffSummary,
    receipt_available: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReceiptView {
    json: String,
    markdown: String,
    terminal: String,
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
fn now_rfc3339() -> Result<String, String> {
    OffsetDateTime::now_utc().format(&Rfc3339).map_err(err)
}

fn read_bounded_regular_file(path: &Path) -> Result<Vec<u8>, String> {
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(err)?
    };
    #[cfg(not(unix))]
    let file = fs::File::open(path).map_err(err)?;

    let mut bytes = Vec::new();
    file.take(MAX_PREVIEW_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() as u64 > MAX_PREVIEW_BYTES {
        return Err("Preview withheld: file changed or exceeds 256 KiB.".to_owned());
    }
    Ok(bytes)
}

fn write_bounded_regular_file(path: &Path, content: &str) -> Result<(), String> {
    if content.len() as u64 > MAX_PREVIEW_BYTES {
        return Err("Edit rejected: file exceeds 256 KiB.".to_owned());
    }
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(err)?
    };
    #[cfg(not(unix))]
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(err)?;

    file.write_all(content.as_bytes()).map_err(err)?;
    file.sync_all().map_err(err)
}

fn validated_preview_path(root: &RepoRoot, relative: &RelativeRepoPath) -> Result<PathBuf, String> {
    let mut current = root.as_path().to_path_buf();
    for component in relative.as_path().components() {
        current.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&current).map_err(err)?;
        if metadata.file_type().is_symlink() {
            return Err("Preview withheld: symlink paths are not allowed.".to_owned());
        }
    }
    let canonical = current.canonicalize().map_err(err)?;
    if !canonical.starts_with(root.as_path()) {
        return Err("Preview withheld: path leaves the selected repository.".to_owned());
    }
    Ok(canonical)
}
fn selected_root(state: &State<'_, DesktopState>) -> Result<RepoRoot, String> {
    state
        .repository
        .lock()
        .map_err(err)?
        .clone()
        .ok_or_else(|| "Select a repository first.".to_owned())
        .and_then(|path| RepoRoot::discover(path).map_err(err))
}
fn policy_path(root: &RepoRoot) -> PathBuf {
    root.as_path().join(POLICY_FILE)
}
fn atomic_write_policy(path: &Path, source: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Policy path has no parent.".to_owned())?;
    let temporary = parent.join(format!(".repolatch-policy-{}.tmp", SessionId::new()));
    let write_result = (|| {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(err)?;
        file.write_all(source.as_bytes()).map_err(err)?;
        file.sync_all().map_err(err)?;
        fs::rename(&temporary, path).map_err(err)
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
}
fn load_policy(root: &RepoRoot) -> Result<(String, CompiledPolicy), String> {
    let path = policy_path(root);
    let source = if path.is_file() {
        fs::read_to_string(&path).map_err(err)?
    } else {
        DEFAULT_POLICY_TEMPLATE.to_owned()
    };
    let compiled = compile_policy(&source).map_err(err)?;
    Ok((source, compiled))
}
fn access_name(access: AccessDecision) -> String {
    match access {
        AccessDecision::Allowed => "allowed",
        AccessDecision::Denied => "denied",
        AccessDecision::Unmatched => "unmatched",
    }
    .to_owned()
}
fn scan_warning_message(warning: ScanWarning) -> String {
    match warning {
        ScanWarning::SymlinkOmitted { path } => {
            format!("Symlink omitted: {}", path.display())
        }
        ScanWarning::InvalidPathOmitted { path } => {
            format!("Path omitted because it could not be represented safely: {}", path.display())
        }
        ScanWarning::EntryLimitReached { limit } => {
            format!("Showing the first {limit} repository entries.")
        }
        ScanWarning::GitMetadataUnavailable { detail } => {
            format!("Git metadata is unavailable: {detail}")
        }
        ScanWarning::WalkError { detail } => format!("Repository scan warning: {detail}"),
    }
}
fn has_sensitive(path: &RelativeRepoPath, policy: &CompiledPolicy) -> bool {
    let value = path.as_str();
    value == ".env"
        || value.starts_with(".env.")
        || value.ends_with(".pem")
        || value.ends_with(".key")
        || value.contains("/.env")
        || value.contains("/.ssh/")
        || value.starts_with(".ssh/")
        || value.ends_with("id_rsa")
        || value.ends_with("id_ed25519")
        || value == ".aws/credentials"
        || value.ends_with("/.aws/credentials")
        || value == ".config/gcloud/application_default_credentials.json"
        || value.ends_with("/.config/gcloud/application_default_credentials.json")
        || value == ".azure/accessTokens.json"
        || value.ends_with("/.azure/accessTokens.json")
        || value.ends_with(".npmrc")
        || value.ends_with(".pypirc")
        || value.ends_with(".netrc")
        || value.starts_with("credentials/")
        || value.contains("/credentials/")
        || value.contains("/production/")
        || value.starts_with("production/")
        || policy.evaluate(path, Access::Read) == AccessDecision::Denied
}
fn state_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app.path().app_data_dir().map_err(err)?.join("sessions");
    fs::create_dir_all(&base).map_err(err)?;
    Ok(base)
}
fn session_dir(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    if !is_session_id(id) {
        return Err("Invalid session identifier.".to_owned());
    }
    let path = state_dir(app)?.join(id);
    if !path.is_dir() {
        return Err("Session was not found.".to_owned());
    }
    Ok(path)
}
fn is_session_id(id: &str) -> bool {
    id.len() == 36
        && id.chars().enumerate().all(|(index, character)| {
            matches!(index, 8 | 13 | 18 | 23) && character == '-'
                || !matches!(index, 8 | 13 | 18 | 23) && character.is_ascii_hexdigit()
        })
}

#[tauri::command]
fn select_repository_path(
    state: State<'_, DesktopState>,
    path: String,
) -> Result<String, String> {
    let root = RepoRoot::discover(path).map_err(err)?;
    *state.repository.lock().map_err(err)? = Some(root.as_path().to_path_buf());
    Ok(root.as_path().display().to_string())
}

#[tauri::command]
fn open_repository(state: State<'_, DesktopState>) -> Result<(), String> {
    let root = selected_root(&state)?;
    #[cfg(target_os = "macos")]
    {
        Command::new("/usr/bin/open")
            .arg(root.as_path())
            .spawn()
            .map_err(err)?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(root.as_path())
            .spawn()
            .map_err(err)?;
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer.exe")
            .arg(root.as_path())
            .spawn()
            .map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
async fn repository_tree(state: State<'_, DesktopState>) -> Result<RepositoryView, String> {
    let root = selected_root(&state)?;
    tauri::async_runtime::spawn_blocking(move || build_repository_view(root))
        .await
        .map_err(err)?
}

fn build_repository_view(root: RepoRoot) -> Result<RepositoryView, String> {
    let (_, policy) = load_policy(&root)?;
    let scan = scan_repository(
        &root,
        &policy,
        ScanOptions {
            max_entries: MAX_TREE_ENTRIES,
            annotate_git: true,
            respect_gitignore: true,
        },
    );
    let snapshot = snapshot_source(root.as_path()).map_err(err)?;
    Ok(RepositoryView {
        root: root.as_path().display().to_string(),
        entries: scan
            .entries
            .into_iter()
            .map(|entry| TreeEntry {
                path: entry.path.as_str().to_owned(),
                kind: format!("{:?}", entry.kind).to_lowercase(),
                size_bytes: entry.size_bytes,
                read_access: access_name(entry.read_access),
                write_access: access_name(entry.write_access),
                sensitive: entry
                    .sensitive
                    .into_iter()
                    .map(|v| format!("{:?}", v.classification))
                    .collect(),
                git_tracked: entry.git_tracked,
                git_ignored: entry.git_ignored,
            })
            .collect(),
        warnings: scan
            .warnings
            .into_iter()
            .map(scan_warning_message)
            .collect(),
        git: GitView {
            head: snapshot.head,
            dirty: snapshot.dirty,
            tracked_paths_truncated: snapshot.tracked_paths_truncated,
            ignored_paths_truncated: snapshot.ignored_paths_truncated,
        },
    })
}

#[tauri::command]
fn policy_load(state: State<'_, DesktopState>) -> Result<PolicyView, String> {
    let root = selected_root(&state)?;
    let path = policy_path(&root);
    let source = if path.is_file() {
        fs::read_to_string(path).map_err(err)?
    } else {
        DEFAULT_POLICY_TEMPLATE.to_owned()
    };
    match compile_policy(&source) {
        Ok(compiled) => Ok(PolicyView {
            source,
            valid: true,
            error: None,
            allowed_commands: compiled.policy().commands.allow.clone(),
        }),
        Err(error) => Ok(PolicyView {
            source,
            valid: false,
            error: Some(error.to_string()),
            allowed_commands: Vec::new(),
        }),
    }
}

#[tauri::command]
fn policy_save(
    state: State<'_, DesktopState>,
    source: String,
    confirm_edit: bool,
) -> Result<PolicyView, String> {
    if !confirm_edit {
        return Err("Policy edits require explicit confirmation.".to_owned());
    }
    let compiled = compile_policy(&source).map_err(err)?;
    let root = selected_root(&state)?;
    let path = policy_path(&root);
    atomic_write_policy(&path, &source)?;
    Ok(PolicyView {
        source,
        valid: true,
        error: None,
        allowed_commands: compiled.policy().commands.allow.clone(),
    })
}

#[tauri::command]
fn file_preview(
    state: State<'_, DesktopState>,
    relative_path: String,
) -> Result<TextPreview, String> {
    let root = selected_root(&state)?;
    let (_, policy) = load_policy(&root)?;
    let relative = RelativeRepoPath::new(relative_path).map_err(err)?;
    let path = validated_preview_path(&root, &relative)?;
    let metadata = fs::symlink_metadata(&path).map_err(err)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Ok(TextPreview {
            state: "withheld".to_owned(),
            content: None,
            message: "Only regular files can be previewed.".to_owned(),
            bytes: None,
            editable: false,
        });
    }
    if metadata.len() > MAX_PREVIEW_BYTES {
        return Ok(TextPreview {
            state: "withheld".to_owned(),
            content: None,
            message: "Preview withheld: file exceeds 256 KiB.".to_owned(),
            bytes: Some(metadata.len()),
            editable: false,
        });
    }
    if has_sensitive(&relative, &policy) {
        if relative.as_str() == ".env" || relative.as_str().starts_with(".env.") {
            let keys = String::from_utf8(read_bounded_regular_file(&path)?)
                .map_err(|_| "Non-UTF-8 environment content is withheld.".to_owned())?
                .lines()
                .filter_map(|line| {
                    line.split_once('=')
                        .map(|(key, _)| format!("{key}=[MASKED]"))
                })
                .collect::<Vec<_>>()
                .join("\n");
            return Ok(TextPreview {
                state: "masked".to_owned(),
                content: Some(keys),
                message: "Environment values are masked.".to_owned(),
                bytes: Some(metadata.len()),
                editable: false,
            });
        }
        return Ok(TextPreview {
            state: "withheld".to_owned(),
            content: None,
            message: "Sensitive or policy-denied content is withheld.".to_owned(),
            bytes: Some(metadata.len()),
            editable: false,
        });
    }
    if policy.evaluate(&relative, Access::Read) != AccessDecision::Allowed {
        return Ok(TextPreview {
            state: "withheld".to_owned(),
            content: None,
            message: "The policy does not allow reading this file.".to_owned(),
            bytes: Some(metadata.len()),
            editable: false,
        });
    }
    let bytes = read_bounded_regular_file(&path)?;
    if bytes.contains(&0) {
        return Ok(TextPreview {
            state: "withheld".to_owned(),
            content: None,
            message: "Binary content is not previewed.".to_owned(),
            bytes: Some(metadata.len()),
            editable: false,
        });
    }
    let content =
        String::from_utf8(bytes).map_err(|_| "Non-UTF-8 content is not previewed.".to_owned())?;
    Ok(TextPreview {
        state: "available".to_owned(),
        content: Some(content),
        message: "Read-only preview.".to_owned(),
        bytes: Some(metadata.len()),
        editable: policy.evaluate(&relative, Access::Write) == AccessDecision::Allowed,
    })
}

#[tauri::command]
fn file_save(
    state: State<'_, DesktopState>,
    relative_path: String,
    content: String,
    confirm_edit: bool,
) -> Result<TextPreview, String> {
    if !confirm_edit {
        return Err("File edits require an explicit edit action.".to_owned());
    }
    let root = selected_root(&state)?;
    let (_, policy) = load_policy(&root)?;
    let relative = RelativeRepoPath::new(&relative_path).map_err(err)?;
    if has_sensitive(&relative, &policy) {
        return Err("Sensitive or policy-denied files cannot be edited.".to_owned());
    }
    if policy.evaluate(&relative, Access::Write) != AccessDecision::Allowed {
        return Err("The policy does not allow writing this file.".to_owned());
    }
    let path = validated_preview_path(&root, &relative)?;
    let metadata = fs::symlink_metadata(&path).map_err(err)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Only existing regular files can be edited.".to_owned());
    }
    write_bounded_regular_file(&path, &content)?;
    file_preview(state, relative_path)
}

#[tauri::command]
fn backend_status() -> Result<Vec<BackendView>, String> {
    let local = LocalBackend;
    let docker = DockerBackend::new("docker", "repolatch/runtime:latest").map_err(err)?;
    Ok(vec![backend_view(&local), backend_view(&docker)])
}
fn backend_view<B: ExecutionBackend>(backend: &B) -> BackendView {
    let available = backend.ensure_available();
    BackendView {
        id: backend.identity().to_owned(),
        available: available.is_ok(),
        availability_message: available.err().map(|e| e.to_string()),
        capabilities: backend.capabilities(),
    }
}

#[tauri::command]
fn launch_workspace(
    app: AppHandle,
    state: State<'_, DesktopState>,
    request: LaunchRequest,
) -> Result<SessionView, String> {
    if request.argv.is_empty() || request.argv.len() > 64 {
        return Err(
            "Provide 1 to 64 argv items; shell command strings are not accepted.".to_owned(),
        );
    }
    let root = selected_root(&state)?;
    let (policy_source, policy) = load_policy(&root)?;
    let command_text = request.argv.join(" ");
    if !policy.is_command_allowed(&command_text) {
        return Err("Command is not explicitly allowed by the policy.".to_owned());
    }
    let docker_image = request.docker_image.clone();
    let network = match policy.policy().network.mode {
        NetworkMode::Deny => NetworkAccess::Deny,
        NetworkMode::Allow => NetworkAccess::Allow,
    };
    let (backend_id, capabilities, warnings) = match request.backend.as_str() {
        "docker" => {
            let image = docker_image
                .as_deref()
                .filter(|image| !image.trim().is_empty())
                .ok_or_else(|| "Docker launch requires an explicit image.".to_owned())?;
            let backend = DockerBackend::new("docker", image).map_err(err)?;
            backend.ensure_available().map_err(err)?;
            let warnings = if network == NetworkAccess::Allow {
                vec![
                    "Container networking is enabled; hostname allowlisting is unavailable."
                        .to_owned(),
                ]
            } else {
                Vec::new()
            };
            (
                backend.identity().to_owned(),
                backend.capabilities(),
                warnings,
            )
        }
        "local-advisory" => {
            let backend = LocalBackend;
            let mut warnings = vec![
                "Native execution is advisory: a same-user process can access host files."
                    .to_owned(),
            ];
            if network == NetworkAccess::Deny {
                warnings.push(
                    "Policy requests network denial, but native execution cannot enforce it."
                        .to_owned(),
                );
            }
            (
                backend.identity().to_owned(),
                backend.capabilities(),
                warnings,
            )
        }
        _ => return Err("Unsupported backend.".to_owned()),
    };
    let snapshot = snapshot_source(root.as_path()).map_err(err)?;
    let id = SessionId::new();
    let id_text = id.to_string();
    let dir = state_dir(&app)?.join(&id_text);
    fs::create_dir(&dir).map_err(err)?;
    let workspace = WorkspaceBuilder::new(&root, &|path: &RelativeRepoPath| {
        policy.evaluate(path, Access::Read) == AccessDecision::Allowed
    })
    .build_at(id, dir.join("workspace"))
    .map_err(err)?;
    initialize_visible_baseline(workspace.root()).map_err(err)?;
    let command = CommandSpec::new(request.argv).map_err(err)?;
    let execution = ExecutionRequest {
        command,
        working_directory: None,
        environment: MinimalEnvironment::from_host(std::env::vars_os()),
        network,
    };
    let visible = workspace
        .manifest()
        .visible_files
        .iter()
        .filter_map(|path| RelativeRepoPath::new(path).ok())
        .collect::<Vec<_>>();
    let denied = workspace
        .manifest()
        .omitted_paths
        .iter()
        .filter(|item| {
            matches!(
                item.reason,
                repolatch_runtime::OmissionReason::DeniedByPolicy
            )
        })
        .filter_map(|item| RelativeRepoPath::new(&item.path).ok())
        .collect::<Vec<_>>();
    let mut receipt = Receipt::partial(ReceiptStart {
        session_id: id,
        started_at: now_rfc3339()?,
        backend: BackendReceipt {
            id: backend_id.clone(),
            capabilities,
        },
        command: execution.command.argv().to_vec(),
        source: SourceReceipt {
            revision: snapshot.head,
            dirty: snapshot.dirty,
            git_context: "synthesized_visible_baseline".to_owned(),
        },
        policy_sha256: policy_sha256(&policy_source),
        visible_paths: PathSummary {
            count: visible.len(),
            paths: visible,
            truncated: false,
        },
        denied_paths: PathSummary {
            count: denied.len(),
            paths: denied,
            truncated: false,
        },
        warnings,
        limitations: vec![
            "Child command activity is not reliably observed; only the top-level process status is recorded."
                .to_owned(),
            "Hostname allowlisting is unavailable in the MVP.".to_owned(),
        ],
    });
    let writer = ReceiptWriter::new(dir.join("receipt.json"));
    writer.write(&receipt).map_err(err)?;
    let outcome = match request.backend.as_str() {
        "docker" => {
            let backend = DockerBackend::new(
                "docker",
                docker_image.expect("validated before workspace construction"),
            )
            .map_err(err)?;
            backend.execute(&workspace, &execution)
        }
        "local-advisory" => LocalBackend.execute(&workspace, &execution),
        _ => unreachable!("backend validated before workspace construction"),
    };
    let diff = summarize_workspace_diff(workspace.root()).unwrap_or_default();
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            receipt
                .finish(
                    ReceiptStatus::Failed,
                    now_rfc3339()?,
                    diff,
                    None,
                    Some("execution"),
                )
                .map_err(err)?;
            writer.write(&receipt).map_err(err)?;
            return Err(err(error));
        }
    };
    receipt
        .finish(
            if outcome.exit_code == Some(0) {
                ReceiptStatus::Completed
            } else {
                ReceiptStatus::Failed
            },
            now_rfc3339()?,
            diff.clone(),
            outcome.exit_code,
            None,
        )
        .map_err(err)?;
    writer.write(&receipt).map_err(err)?;
    Ok(SessionView {
        id: id_text,
        backend: request.backend,
        workspace: workspace.root().display().to_string(),
        diff,
        receipt_available: true,
    })
}

#[tauri::command]
fn session_diff(app: AppHandle, session_id: String) -> Result<String, String> {
    let workspace = session_dir(&app, &session_id)?.join("workspace");
    let output = Command::new("/usr/bin/git")
        .current_dir(&workspace)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args([
            "--no-optional-locks",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.attributesFile=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "HEAD",
            "--",
            ".",
        ])
        .output()
        .map_err(err)?;
    if !output.status.success() {
        return Err("Could not render workspace diff.".to_owned());
    }
    let bytes = output.stdout;
    let truncated = bytes.len() > MAX_DIFF_BYTES;
    let shown = &bytes[..bytes.len().min(MAX_DIFF_BYTES)];
    let text = String::from_utf8_lossy(shown).into_owned();
    Ok(if truncated {
        format!("{text}\n\n[Diff truncated at 512 KiB]")
    } else {
        text
    })
}

#[tauri::command]
fn receipt_load(app: AppHandle, session_id: String) -> Result<ReceiptView, String> {
    let path = session_dir(&app, &session_id)?.join("receipt.json");
    let bytes =
        fs::read(path).map_err(|_| "No receipt is available for this session.".to_owned())?;
    let receipt: Receipt = serde_json::from_slice(&bytes).map_err(err)?;
    Ok(ReceiptView {
        json: render_json(&receipt).map_err(err)?,
        markdown: render_markdown(&receipt),
        terminal: render_terminal(&receipt),
    })
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(DesktopState::default())
        .invoke_handler(tauri::generate_handler![
            select_repository_path,
            open_repository,
            repository_tree,
            policy_load,
            policy_save,
            file_preview,
            file_save,
            backend_status,
            launch_workspace,
            session_diff,
            receipt_load
        ])
        .run(tauri::generate_context!())
        .expect("error while running RepoLatch desktop");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_preview_paths_are_withheld_before_content_read() {
        let policy = compile_policy(DEFAULT_POLICY_TEMPLATE).unwrap();
        assert!(has_sensitive(
            &RelativeRepoPath::new(".env.production").unwrap(),
            &policy
        ));
        assert!(has_sensitive(
            &RelativeRepoPath::new("keys/service.pem").unwrap(),
            &policy
        ));
        assert!(!has_sensitive(
            &RelativeRepoPath::new("src/main.rs").unwrap(),
            &policy
        ));
    }

    #[test]
    fn session_identifier_requires_uuid_shape() {
        assert!(is_session_id("6b9b0558-2b25-4b6b-99d4-6955c20c5987"));
        assert!(!is_session_id("../../receipt"));
        assert!(!is_session_id("6b9b0558_2b25-4b6b-99d4-6955c20c5987"));
    }

    #[cfg(unix)]
    #[test]
    fn preview_rejects_symlinked_ancestor_directory() {
        let repository = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), "FAKE_OUTSIDE_SECRET").unwrap();
        fs::create_dir(repository.path().join("src")).unwrap();
        std::os::unix::fs::symlink(outside.path(), repository.path().join("src/link")).unwrap();
        let root = RepoRoot::discover(repository.path()).unwrap();
        let relative = RelativeRepoPath::new("src/link/secret.txt").unwrap();
        assert!(validated_preview_path(&root, &relative).is_err());
    }

    #[test]
    fn bounded_file_write_updates_regular_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("editable.txt");
        fs::write(&path, "before").unwrap();

        write_bounded_regular_file(&path, "after\n").unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), "after\n");
    }

    #[test]
    fn bounded_file_write_rejects_large_content() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("editable.txt");
        fs::write(&path, "before").unwrap();
        let too_large = "x".repeat(MAX_PREVIEW_BYTES as usize + 1);

        assert!(write_bounded_regular_file(&path, &too_large).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "before");
    }

    #[cfg(unix)]
    #[test]
    fn bounded_file_write_does_not_follow_symlinks() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target.txt");
        let link = directory.path().join("editable.txt");
        fs::write(&target, "unchanged").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert!(write_bounded_regular_file(&link, "replacement").is_err());
        assert_eq!(fs::read_to_string(target).unwrap(), "unchanged");
    }

    #[test]
    fn policy_write_preserves_unrelated_predictable_temp_file() {
        let repository = tempfile::tempdir().unwrap();
        let policy = repository.path().join("repolatch.toml");
        let sentinel = repository.path().join("repolatch.toml.tmp");
        fs::write(&sentinel, "editor sentinel").unwrap();
        atomic_write_policy(&policy, DEFAULT_POLICY_TEMPLATE).unwrap();
        assert_eq!(fs::read_to_string(sentinel).unwrap(), "editor sentinel");
        assert_eq!(fs::read_to_string(policy).unwrap(), DEFAULT_POLICY_TEMPLATE);
    }
}
