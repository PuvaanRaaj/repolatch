//! Safe workspace construction and execution backends for AgentGuard.
//!
//! This crate deliberately does not inspect policy files or write receipts.  Callers provide a
//! visibility decision and lifecycle sink, keeping policy and receipt implementations separate.

use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    time::{SystemTime, UNIX_EPOCH},
};

use agentguard_core::{
    AgentGuardError, BackendCapabilities, CapabilityState, CommandSpec, EnforcementLevel,
    RelativeRepoPath, RepoRoot, Result, SessionId,
};
use serde::{Deserialize, Serialize};

const CONTAINER_WORKSPACE: &str = "/workspace";
const SAFE_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// Policy adapter used while constructing a visible-only workspace.
///
/// Implement this over `agentguard-policy`'s compiled policy. `false` means the file is omitted.
pub trait VisibilityPolicy {
    fn allows(&self, path: &RelativeRepoPath) -> bool;
}

impl<F> VisibilityPolicy for F
where
    F: Fn(&RelativeRepoPath) -> bool,
{
    fn allows(&self, path: &RelativeRepoPath) -> bool {
        self(path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionManifest {
    pub session_id: SessionId,
    pub created_unix_seconds: u64,
    pub source_root: String,
    pub workspace_root: String,
    pub visible_files: Vec<String>,
    pub omitted_paths: Vec<OmittedPath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmittedPath {
    pub path: String,
    pub reason: OmissionReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OmissionReason {
    DeniedByPolicy,
    SourceSymlink,
    GitMetadata,
    UnsupportedSourceEntry,
}

#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
    source_root: PathBuf,
    manifest: SessionManifest,
}

impl Workspace {
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
    #[must_use]
    pub fn manifest(&self) -> &SessionManifest {
        &self.manifest
    }

    fn checked_workdir(&self, path: Option<&RelativeRepoPath>) -> Result<PathBuf> {
        let joined = path.map_or_else(|| self.root.clone(), |path| self.root.join(path.as_path()));
        if !joined.starts_with(&self.root) || !joined.is_dir() {
            return Err(AgentGuardError::Workspace(
                "working directory is not in generated workspace".to_owned(),
            ));
        }
        Ok(joined)
    }
}

/// Creates a new private workspace, without following any source symlinks.
pub struct WorkspaceBuilder<'a, P> {
    source: &'a RepoRoot,
    policy: &'a P,
}

impl<'a, P: VisibilityPolicy> WorkspaceBuilder<'a, P> {
    #[must_use]
    pub fn new(source: &'a RepoRoot, policy: &'a P) -> Self {
        Self { source, policy }
    }

    /// Build into a path which must not already exist. The caller should put it under a private
    /// AgentGuard state directory; this method makes the workspace itself owner-only on Unix.
    pub fn build_at(
        &self,
        session_id: SessionId,
        destination: impl AsRef<Path>,
    ) -> Result<Workspace> {
        let destination = destination.as_ref();
        if destination.exists() {
            return Err(AgentGuardError::Workspace(
                "workspace destination already exists".to_owned(),
            ));
        }
        if !destination.is_absolute() {
            return Err(AgentGuardError::Workspace(
                "workspace destination must be absolute".to_owned(),
            ));
        }
        let destination_parent = destination.parent().ok_or_else(|| {
            AgentGuardError::Workspace("workspace destination has no parent".to_owned())
        })?;
        let canonical_parent = destination_parent.canonicalize()?;
        if canonical_parent.starts_with(self.source.as_path()) {
            return Err(AgentGuardError::Workspace(
                "generated workspace must not be inside source repository".to_owned(),
            ));
        }
        fs::create_dir(destination)?;
        set_private_permissions(destination)?;
        let canonical_destination = destination.canonicalize()?;

        let mut visible_files = Vec::new();
        let mut omitted_paths = Vec::new();
        let copied = copy_tree(
            self.source.as_path(),
            destination,
            Path::new(""),
            self.policy,
            &mut visible_files,
            &mut omitted_paths,
        );
        if let Err(error) = copied {
            let _ = fs::remove_dir_all(destination);
            return Err(error);
        }
        visible_files.sort();
        omitted_paths.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(Workspace {
            root: canonical_destination.clone(),
            source_root: self.source.as_path().to_path_buf(),
            manifest: SessionManifest {
                session_id,
                created_unix_seconds: now_seconds(),
                source_root: self.source.as_path().display().to_string(),
                workspace_root: canonical_destination.display().to_string(),
                visible_files,
                omitted_paths,
            },
        })
    }
}

fn copy_tree<P: VisibilityPolicy>(
    source_root: &Path,
    destination_root: &Path,
    relative: &Path,
    policy: &P,
    visible_files: &mut Vec<String>,
    omitted_paths: &mut Vec<OmittedPath>,
) -> Result<()> {
    let source = source_root.join(relative);
    for entry in fs::read_dir(&source)? {
        let entry = entry?;
        let name = entry.file_name();
        let child_relative = relative.join(&name);
        let relative_path = RelativeRepoPath::new(&child_relative)?;
        let child_source = entry.path();
        let metadata = fs::symlink_metadata(&child_source)?;
        if metadata.file_type().is_symlink() {
            omitted_paths.push(OmittedPath {
                path: relative_path.as_str().to_owned(),
                reason: OmissionReason::SourceSymlink,
            });
            continue;
        }
        if relative_path.as_str() == ".git" || relative_path.as_str().starts_with(".git/") {
            omitted_paths.push(OmittedPath {
                path: relative_path.as_str().to_owned(),
                reason: OmissionReason::GitMetadata,
            });
            continue;
        }
        if metadata.is_dir() {
            // Directories are not policy-visible objects. Traverse them to find policy-visible files.
            copy_tree(
                source_root,
                destination_root,
                &child_relative,
                policy,
                visible_files,
                omitted_paths,
            )?;
        } else if metadata.is_file() {
            if !policy.allows(&relative_path) {
                omitted_paths.push(OmittedPath {
                    path: relative_path.as_str().to_owned(),
                    reason: OmissionReason::DeniedByPolicy,
                });
                continue;
            }
            let destination = destination_root.join(&child_relative);
            let parent = destination.parent().ok_or_else(|| {
                AgentGuardError::Workspace("invalid destination parent".to_owned())
            })?;
            fs::create_dir_all(parent)?;
            set_private_permissions(parent)?;
            copy_regular_file(&child_source, &destination)?;
            set_private_file_permissions(&destination)?;
            visible_files.push(relative_path.as_str().to_owned());
        } else {
            omitted_paths.push(OmittedPath {
                path: relative_path.as_str().to_owned(),
                reason: OmissionReason::UnsupportedSourceEntry,
            });
        }
    }
    Ok(())
}

#[cfg(unix)]
fn copy_regular_file(source: &Path, destination: &Path) -> io::Result<()> {
    use std::{fs::File, os::unix::fs::OpenOptionsExt};

    // Recheck at open time so a source file swapped to a symlink is never followed.
    let mut input = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(source)?;
    let mut output = File::create(destination)?;
    io::copy(&mut input, &mut output)?;
    Ok(())
}

#[cfg(not(unix))]
fn copy_regular_file(source: &Path, destination: &Path) -> io::Result<()> {
    fs::copy(source, destination).map(|_| ())
}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Deliberately small environment. Ambient host variables are never inherited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimalEnvironment(BTreeMap<String, String>);

impl MinimalEnvironment {
    #[must_use]
    pub fn empty() -> Self {
        Self(BTreeMap::from([("PATH".to_owned(), SAFE_PATH.to_owned())]))
    }

    /// Retain only locale and terminal settings from an ambient environment.
    #[must_use]
    pub fn from_host<I>(environment: I) -> Self
    where
        I: IntoIterator<Item = (OsString, OsString)>,
    {
        let mut result = Self::empty();
        let allowed = ["LANG", "LC_ALL", "TERM", "TZ"];
        for (name, value) in environment {
            if let (Some(name), Some(value)) = (name.to_str(), value.to_str())
                && allowed.contains(&name)
                && !value.contains('\0')
            {
                result.0.insert(name.to_owned(), value.to_owned());
            }
        }
        result
    }

    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) -> Result<()> {
        let name = name.into();
        let value = value.into();
        if !is_allowed_env_name(&name) || value.contains('\0') {
            return Err(AgentGuardError::Execution(
                "environment entry is not approved".to_owned(),
            ));
        }
        self.0.insert(name, value);
        Ok(())
    }
    #[must_use]
    pub fn values(&self) -> &BTreeMap<String, String> {
        &self.0
    }
}

fn is_allowed_env_name(name: &str) -> bool {
    matches!(name, "PATH" | "LANG" | "LC_ALL" | "TERM" | "TZ")
}

#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub command: CommandSpec,
    /// `None` selects the generated workspace root.
    pub working_directory: Option<RelativeRepoPath>,
    pub environment: MinimalEnvironment,
    pub network: NetworkAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkAccess {
    Deny,
    Allow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionOutcome {
    pub exit_code: Option<i32>,
    pub interrupted: bool,
}

impl ExecutionOutcome {
    fn from_status(status: ExitStatus) -> Self {
        Self {
            exit_code: status.code(),
            interrupted: false,
        }
    }
}

pub trait ExecutionBackend {
    fn identity(&self) -> &'static str;
    fn capabilities(&self) -> BackendCapabilities;
    fn ensure_available(&self) -> Result<()>;
    fn execute(
        &self,
        workspace: &Workspace,
        request: &ExecutionRequest,
    ) -> Result<ExecutionOutcome>;
}

#[derive(Debug, Clone, Default)]
pub struct LocalBackend;

impl ExecutionBackend for LocalBackend {
    fn identity(&self) -> &'static str {
        "local-advisory"
    }
    fn capabilities(&self) -> BackendCapabilities {
        local_capabilities()
    }
    fn ensure_available(&self) -> Result<()> {
        Ok(())
    }
    fn execute(
        &self,
        workspace: &Workspace,
        request: &ExecutionRequest,
    ) -> Result<ExecutionOutcome> {
        let workdir = workspace.checked_workdir(request.working_directory.as_ref())?;
        let mut command = Command::new(&request.command.argv()[0]);
        command
            .args(&request.command.argv()[1..])
            .current_dir(workdir)
            .env_clear()
            .envs(request.environment.values());
        command
            .status()
            .map(ExecutionOutcome::from_status)
            .map_err(AgentGuardError::from)
    }
}

#[derive(Debug, Clone)]
pub struct DockerBackend {
    executable: PathBuf,
    image: String,
}

impl DockerBackend {
    pub fn new(executable: impl Into<PathBuf>, image: impl Into<String>) -> Result<Self> {
        let image = image.into();
        if image.is_empty() || image.contains('\0') {
            return Err(AgentGuardError::Execution(
                "Docker image is invalid".to_owned(),
            ));
        }
        Ok(Self {
            executable: executable.into(),
            image,
        })
    }

    /// Structured argv for inspection and tests; it is never passed through a shell.
    pub fn argv(&self, workspace: &Workspace, request: &ExecutionRequest) -> Result<Vec<OsString>> {
        let workdir = workspace.checked_workdir(request.working_directory.as_ref())?;
        let relative_workdir = workdir
            .strip_prefix(workspace.root())
            .map_err(|_| AgentGuardError::Workspace("working directory escape".to_owned()))?;
        let mount_source = workspace.root().canonicalize()?;
        if mount_source == workspace.source_root || mount_source.starts_with(&workspace.source_root)
        {
            return Err(AgentGuardError::Workspace(
                "refusing to mount source repository".to_owned(),
            ));
        }
        if mount_source.as_os_str().to_string_lossy().contains(',') {
            return Err(AgentGuardError::Workspace(
                "workspace path cannot contain a comma for Docker --mount syntax".to_owned(),
            ));
        }
        let mut args = vec![
            OsString::from("run"),
            OsString::from("--rm"),
            OsString::from("--interactive"),
        ];
        if request.network == NetworkAccess::Deny {
            args.extend([OsString::from("--network"), OsString::from("none")]);
        }
        args.extend([
            OsString::from("--read-only"),
            OsString::from("--cap-drop"),
            OsString::from("ALL"),
            OsString::from("--security-opt"),
            OsString::from("no-new-privileges:true"),
            OsString::from("--pids-limit"),
            OsString::from("256"),
            OsString::from("--memory"),
            OsString::from("2g"),
            OsString::from("--cpus"),
            OsString::from("2"),
            OsString::from("--user"),
            workspace_user(workspace.root()),
            OsString::from("--tmpfs"),
            OsString::from("/tmp:rw,noexec,nosuid,size=64m"),
        ]);
        args.push(OsString::from("--mount"));
        args.push(OsString::from(format!(
            "type=bind,src={},dst={CONTAINER_WORKSPACE}",
            mount_source.display()
        )));
        args.push(OsString::from("--workdir"));
        args.push(if relative_workdir.as_os_str().is_empty() {
            OsString::from(CONTAINER_WORKSPACE)
        } else {
            OsString::from(format!(
                "{CONTAINER_WORKSPACE}/{}",
                relative_workdir.display()
            ))
        });
        for (name, value) in request.environment.values() {
            args.push(OsString::from("--env"));
            args.push(OsString::from(format!("{name}={value}")));
        }
        args.push(OsString::from(&self.image));
        args.extend(request.command.argv().iter().map(OsString::from));
        Ok(args)
    }
}

#[cfg(unix)]
fn workspace_user(path: &Path) -> OsString {
    use std::os::unix::fs::MetadataExt;

    fs::metadata(path).map_or_else(
        |_| OsString::from("65532:65532"),
        |metadata| OsString::from(format!("{}:{}", metadata.uid(), metadata.gid())),
    )
}

#[cfg(not(unix))]
fn workspace_user(_path: &Path) -> OsString {
    OsString::from("65532:65532")
}

impl ExecutionBackend for DockerBackend {
    fn identity(&self) -> &'static str {
        "docker"
    }
    fn capabilities(&self) -> BackendCapabilities {
        docker_capabilities()
    }
    fn ensure_available(&self) -> Result<()> {
        let status = Command::new(&self.executable)
            .args(["version", "--format", "{{.Server.Version}}"])
            .env_clear()
            .env("PATH", SAFE_PATH)
            .output();
        match status { Ok(output) if output.status.success() => Ok(()), Ok(_) => Err(AgentGuardError::BackendUnavailable("Docker daemon is unavailable".to_owned())), Err(error) if error.kind() == io::ErrorKind::NotFound => Err(AgentGuardError::BackendUnavailable("Docker executable is unavailable; enforced execution will not fall back to local".to_owned())), Err(error) => Err(AgentGuardError::BackendUnavailable(format!("Docker availability check failed: {error}"))) }
    }
    fn execute(
        &self,
        workspace: &Workspace,
        request: &ExecutionRequest,
    ) -> Result<ExecutionOutcome> {
        self.ensure_available()?;
        let args = self.argv(workspace, request)?;
        Command::new(&self.executable)
            .args(args)
            .env_clear()
            .env("PATH", SAFE_PATH)
            .status()
            .map(ExecutionOutcome::from_status)
            .map_err(AgentGuardError::from)
    }
}

fn state(level: EnforcementLevel, message: &str) -> CapabilityState {
    CapabilityState::new(level, message)
}
fn docker_capabilities() -> BackendCapabilities {
    BackendCapabilities {
        filesystem_isolation: state(
            EnforcementLevel::Enforced,
            "Docker generated-workspace boundary",
        ),
        workspace_write_boundary: state(
            EnforcementLevel::Enforced,
            "only generated workspace is mounted",
        ),
        policy_write_restrictions: state(
            EnforcementLevel::Unavailable,
            "policy write globs are reported but all visible workspace paths are writable",
        ),
        network_deny: state(EnforcementLevel::Enforced, "Docker network none"),
        hostname_allowlist: state(
            EnforcementLevel::Unavailable,
            "standard Docker does not enforce hostname allowlists",
        ),
        environment_filtering: state(EnforcementLevel::Enforced, "minimal explicit environment"),
        child_command_observation: state(
            EnforcementLevel::NotReliablyObserved,
            "only the top-level Docker process is observed",
        ),
    }
}
fn local_capabilities() -> BackendCapabilities {
    BackendCapabilities {
        filesystem_isolation: state(EnforcementLevel::Advisory, "same-user local process"),
        workspace_write_boundary: state(
            EnforcementLevel::Advisory,
            "same-user process can access host files",
        ),
        policy_write_restrictions: state(
            EnforcementLevel::Advisory,
            "policy write globs are not an operating-system boundary",
        ),
        network_deny: state(
            EnforcementLevel::Unavailable,
            "local backend does not constrain network",
        ),
        hostname_allowlist: state(
            EnforcementLevel::Unavailable,
            "local backend does not constrain hostnames",
        ),
        environment_filtering: state(EnforcementLevel::Enforced, "minimal explicit environment"),
        child_command_observation: state(
            EnforcementLevel::NotReliablyObserved,
            "only top-level command status is observed",
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    Prepared,
    Started,
    Completed(ExecutionOutcome),
    Failed(String),
    Interrupted,
}
pub trait SessionLifecycle {
    fn on_event(&mut self, event: &SessionEvent) -> Result<()>;
}

/// Emit a deterministic terminal event suitable for receipt adapters.
pub fn run_session<B: ExecutionBackend, L: SessionLifecycle>(
    backend: &B,
    workspace: &Workspace,
    request: &ExecutionRequest,
    lifecycle: &mut L,
) -> Result<ExecutionOutcome> {
    lifecycle.on_event(&SessionEvent::Prepared)?;
    if let Err(error) = backend.ensure_available() {
        lifecycle.on_event(&SessionEvent::Failed(error.to_string()))?;
        return Err(error);
    }
    lifecycle.on_event(&SessionEvent::Started)?;
    match backend.execute(workspace, request) {
        Ok(outcome) => {
            lifecycle.on_event(&SessionEvent::Completed(outcome.clone()))?;
            Ok(outcome)
        }
        Err(error) => {
            lifecycle.on_event(&SessionEvent::Failed(error.to_string()))?;
            Err(error)
        }
    }
}

/// Use from an interruption handler when an observed execution is terminated externally.
pub fn record_interruption<L: SessionLifecycle>(lifecycle: &mut L) -> Result<()> {
    lifecycle.on_event(&SessionEvent::Interrupted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fixture() -> (TempDir, TempDir, RepoRoot) {
        let source_dir = TempDir::new().unwrap();
        let output_dir = TempDir::new().unwrap();
        let root = RepoRoot::discover(source_dir.path()).unwrap();
        (source_dir, output_dir, root)
    }
    fn build(source: &RepoRoot, output: &Path) -> Workspace {
        WorkspaceBuilder::new(source, &|p: &RelativeRepoPath| p.as_str() != ".env")
            .build_at(SessionId::new(), output)
            .unwrap()
    }

    #[test]
    fn workspace_omits_denied_files_symlinks_and_git_and_preserves_source() {
        let (_source_dir, output_dir, source) = fixture();
        fs::write(source.as_path().join("keep 你好.txt"), "safe").unwrap();
        fs::write(source.as_path().join(".env"), "secret").unwrap();
        fs::create_dir(source.as_path().join(".git")).unwrap();
        fs::write(source.as_path().join(".git/config"), "secret").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("keep 你好.txt", source.as_path().join("linked")).unwrap();
        let workspace = build(&source, &output_dir.path().join("generated"));
        assert_eq!(
            fs::read_to_string(source.as_path().join(".env")).unwrap(),
            "secret"
        );
        assert!(workspace.root().join("keep 你好.txt").is_file());
        assert!(!workspace.root().join(".env").exists());
        assert!(!workspace.root().join(".git").exists());
        #[cfg(unix)]
        assert!(!workspace.root().join("linked").exists());
    }

    #[test]
    fn source_and_destination_traversal_are_rejected() {
        assert!(RelativeRepoPath::new("../escape").is_err());
        let (_source_dir, output_dir, source) = fixture();
        fs::write(source.as_path().join("ok"), "ok").unwrap();
        assert!(
            WorkspaceBuilder::new(&source, &|_: &RelativeRepoPath| true)
                .build_at(SessionId::new(), source.as_path().join("child"))
                .is_err()
        );
        assert!(!output_dir.path().join("child").exists());
    }

    #[test]
    fn docker_argv_has_one_safe_mount_network_none_and_no_source() {
        let (_source_dir, output_dir, source) = fixture();
        fs::create_dir(source.as_path().join("work")).unwrap();
        fs::write(source.as_path().join("work/space 你好"), "ok").unwrap();
        let workspace = build(&source, &output_dir.path().join("generated"));
        let backend = DockerBackend::new("docker missing", "example/image:latest").unwrap();
        let request = ExecutionRequest {
            command: CommandSpec::new(vec![
                "agent".to_owned(),
                "--file".to_owned(),
                "space 你好".to_owned(),
            ])
            .unwrap(),
            working_directory: Some(RelativeRepoPath::new("work").unwrap()),
            environment: MinimalEnvironment::empty(),
            network: NetworkAccess::Deny,
        };
        let args = backend.argv(&workspace, &request).unwrap();
        let rendered: Vec<_> = args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(
            rendered
                .windows(2)
                .any(|pair| pair == ["--network", "none"])
        );
        assert!(rendered.contains(&"--read-only".to_owned()));
        assert!(
            rendered
                .iter()
                .any(|arg| arg.contains("dst=/workspace") && !arg.contains("readonly"))
        );
        assert!(
            !rendered
                .iter()
                .any(|arg| arg.contains(&source.as_path().display().to_string()))
        );
        assert_eq!(
            rendered
                .iter()
                .filter(|arg| arg.as_str() == "--mount")
                .count(),
            1
        );

        let allow_request = ExecutionRequest {
            network: NetworkAccess::Allow,
            ..request
        };
        let allow_args = backend.argv(&workspace, &allow_request).unwrap();
        let allow_rendered = allow_args
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(
            !allow_rendered
                .windows(2)
                .any(|pair| pair == ["--network", "none"])
        );
    }

    #[test]
    fn environment_filters_secrets_and_docker_never_falls_back() {
        let environment = MinimalEnvironment::from_host([
            (OsString::from("LANG"), OsString::from("C.UTF-8")),
            (
                OsString::from("AWS_SECRET_ACCESS_KEY"),
                OsString::from("no"),
            ),
            (OsString::from("HOME"), OsString::from("/home/user")),
        ]);
        assert!(environment.values().contains_key("LANG"));
        assert!(!environment.values().contains_key("AWS_SECRET_ACCESS_KEY"));
        assert!(!environment.values().contains_key("HOME"));
        let backend = DockerBackend::new("definitely-not-agentguard-docker", "image").unwrap();
        assert!(matches!(
            backend.ensure_available(),
            Err(AgentGuardError::BackendUnavailable(_))
        ));
    }

    #[test]
    fn unavailable_execution_has_deterministic_partial_and_interrupted_events() {
        #[derive(Default)]
        struct Recorder(Vec<SessionEvent>);
        impl SessionLifecycle for Recorder {
            fn on_event(&mut self, event: &SessionEvent) -> Result<()> {
                self.0.push(event.clone());
                Ok(())
            }
        }
        let (_source_dir, output_dir, source) = fixture();
        fs::create_dir(source.as_path().join("work")).unwrap();
        fs::write(source.as_path().join("work/file"), "ok").unwrap();
        let workspace = build(&source, &output_dir.path().join("generated"));
        let request = ExecutionRequest {
            command: CommandSpec::new(vec!["agent".to_owned()]).unwrap(),
            working_directory: Some(RelativeRepoPath::new("work").unwrap()),
            environment: MinimalEnvironment::empty(),
            network: NetworkAccess::Deny,
        };
        let backend = DockerBackend::new("agentguard-docker-not-installed", "image").unwrap();
        let mut recorder = Recorder::default();
        assert!(run_session(&backend, &workspace, &request, &mut recorder).is_err());
        assert!(matches!(
            recorder.0.as_slice(),
            [SessionEvent::Prepared, SessionEvent::Failed(_)]
        ));
        record_interruption(&mut recorder).unwrap();
        assert!(matches!(recorder.0.last(), Some(SessionEvent::Interrupted)));
    }
}
