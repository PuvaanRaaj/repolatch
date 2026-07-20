use std::{
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use agentguard_core::{CommandSpec, RelativeRepoPath, RepoRoot, SessionId};
use agentguard_git::{initialize_visible_baseline, snapshot_source, summarize_workspace_diff};
use agentguard_policy::{
    Access, AccessDecision, CompiledPolicy, DEFAULT_POLICY_TEMPLATE, NetworkMode, ScanOptions,
    compile_policy, scan_repository,
};
use agentguard_receipt::{
    BackendReceipt, PathSummary, Receipt, ReceiptStart, ReceiptStatus, ReceiptWriter,
    SourceReceipt, policy_sha256, render_json, render_markdown, render_terminal,
};
use agentguard_runtime::{
    DockerBackend, ExecutionBackend, ExecutionRequest, LocalBackend, MinimalEnvironment,
    NetworkAccess, Workspace, WorkspaceBuilder,
};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const POLICY_FILE: &str = "agentguard.toml";

#[derive(Parser)]
#[command(
    name = "agentguard",
    version,
    about = "Policy-governed generated workspaces"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init(RepoArgs),
    Policy {
        #[command(subcommand)]
        command: PolicyCommands,
    },
    Inspect(InspectArgs),
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommands,
    },
    Run(RunArgs),
    Diff(DiffArgs),
    Receipt(ReceiptArgs),
}

#[derive(Args, Clone)]
struct RepoArgs {
    #[arg(long, default_value = ".")]
    repo: PathBuf,
}

#[derive(Subcommand)]
enum PolicyCommands {
    Validate(PolicyArgs),
}

#[derive(Args, Clone)]
struct PolicyArgs {
    #[command(flatten)]
    repo: RepoArgs,
    #[arg(long)]
    policy: Option<PathBuf>,
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct InspectArgs {
    #[command(flatten)]
    policy: PolicyArgs,
    #[arg(long, default_value_t = 100_000)]
    max_entries: usize,
}

#[derive(Subcommand)]
enum WorkspaceCommands {
    Create(CreateWorkspaceArgs),
}

#[derive(Args)]
struct CreateWorkspaceArgs {
    #[command(flatten)]
    policy: PolicyArgs,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Backend {
    Docker,
    Local,
}

#[derive(Clone, Copy, ValueEnum)]
enum AgentPreset {
    Codex,
    Claude,
    Opencode,
}

#[derive(Args)]
struct RunArgs {
    #[command(flatten)]
    policy: PolicyArgs,
    #[arg(long, value_enum)]
    backend: Backend,
    #[arg(long)]
    image: Option<String>,
    #[arg(long, value_enum, conflicts_with = "argv")]
    agent: Option<AgentPreset>,
    #[arg(long)]
    workspace: Option<PathBuf>,
    #[arg(long)]
    receipt: Option<PathBuf>,
    #[arg(long)]
    state_dir: Option<PathBuf>,
    #[arg(last = true, required_unless_present = "agent")]
    argv: Vec<String>,
}

#[derive(Args)]
struct DiffArgs {
    #[arg(long)]
    workspace: PathBuf,
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum ReceiptFormat {
    Terminal,
    Json,
    Markdown,
}

#[derive(Args)]
struct ReceiptArgs {
    #[arg(long)]
    path: PathBuf,
    #[arg(long, value_enum, default_value_t = ReceiptFormat::Terminal)]
    format: ReceiptFormat,
}

#[derive(Serialize)]
struct ValidationOutput<'a> {
    valid: bool,
    policy_sha256: String,
    docker: &'a agentguard_core::BackendCapabilities,
    local: &'a agentguard_core::BackendCapabilities,
    command_matching: &'static str,
}

#[derive(Serialize)]
struct InspectOutput {
    entries: Vec<InspectEntry>,
    warnings: Vec<String>,
}
#[derive(Serialize)]
struct InspectEntry {
    path: String,
    kind: String,
    size_bytes: Option<u64>,
    read_access: String,
    write_access: String,
    sensitive: Vec<String>,
    git_tracked: Option<bool>,
    git_ignored: Option<bool>,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("agentguard: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Commands::Init(args) => init(&args.repo),
        Commands::Policy {
            command: PolicyCommands::Validate(args),
        } => validate(args),
        Commands::Inspect(args) => inspect(args),
        Commands::Workspace {
            command: WorkspaceCommands::Create(args),
        } => create_workspace(args),
        Commands::Run(args) => run_command(args),
        Commands::Diff(args) => diff(args),
        Commands::Receipt(args) => receipt(args),
    }
}

fn init(repo: &Path) -> anyhow::Result<()> {
    let root = RepoRoot::discover(repo)?;
    let path = root.as_path().join(POLICY_FILE);
    if path.exists() {
        anyhow::bail!("refusing to overwrite existing {}", path.display());
    }
    fs::write(&path, DEFAULT_POLICY_TEMPLATE)?;
    println!("created {}", path.display());
    Ok(())
}

fn load(args: &PolicyArgs) -> anyhow::Result<(RepoRoot, String, CompiledPolicy)> {
    let root = RepoRoot::discover(&args.repo.repo)?;
    let policy_path = args
        .policy
        .clone()
        .unwrap_or_else(|| root.as_path().join(POLICY_FILE));
    let source = fs::read_to_string(&policy_path).map_err(|error| {
        anyhow::anyhow!("cannot read policy {}: {error}", policy_path.display())
    })?;
    let compiled = compile_policy(&source)?;
    Ok((root, source, compiled))
}

fn validate(args: PolicyArgs) -> anyhow::Result<()> {
    let (_, source, policy) = load(&args)?;
    let docker = DockerBackend::new("docker", "agentguard-validation")?.capabilities();
    let local = LocalBackend.capabilities();
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&ValidationOutput {
                valid: true,
                policy_sha256: policy_sha256(&source),
                docker: &docker,
                local: &local,
                command_matching: "commands.allow entries must exactly equal the argv joined by one ASCII space; no shell parsing, prefixes, or globbing"
            })?
        );
    } else {
        println!(
            "policy valid\ncommand matching: exact argv joined by one ASCII space (no shell parsing, prefixes, or globbing)"
        );
        print_capabilities("docker", &docker);
        print_capabilities("local-advisory", &local);
        let _ = policy;
    }
    Ok(())
}

fn print_capabilities(name: &str, caps: &agentguard_core::BackendCapabilities) {
    println!("{name} enforcement:");
    for (control, state) in [
        ("filesystem isolation", &caps.filesystem_isolation),
        ("workspace write boundary", &caps.workspace_write_boundary),
        ("policy write restrictions", &caps.policy_write_restrictions),
        ("network deny", &caps.network_deny),
        ("hostname allowlist", &caps.hostname_allowlist),
        ("environment filtering", &caps.environment_filtering),
        ("child command observation", &caps.child_command_observation),
    ] {
        println!("  {control}: {} — {}", state.level, state.explanation);
    }
}

fn inspect(args: InspectArgs) -> anyhow::Result<()> {
    let (root, _, policy) = load(&args.policy)?;
    let scan = scan_repository(
        &root,
        &policy,
        ScanOptions {
            max_entries: args.max_entries,
            annotate_git: true,
        },
    );
    let output = InspectOutput {
        entries: scan
            .entries
            .into_iter()
            .map(|entry| InspectEntry {
                path: entry.path.as_str().to_owned(),
                kind: format!("{:?}", entry.kind).to_ascii_lowercase(),
                size_bytes: entry.size_bytes,
                read_access: format!("{:?}", entry.read_access).to_ascii_lowercase(),
                write_access: format!("{:?}", entry.write_access).to_ascii_lowercase(),
                sensitive: entry
                    .sensitive
                    .into_iter()
                    .map(|value| format!("{:?}", value.classification).to_ascii_lowercase())
                    .collect(),
                git_tracked: entry.git_tracked,
                git_ignored: entry.git_ignored,
            })
            .collect(),
        warnings: scan
            .warnings
            .into_iter()
            .map(|warning| format!("{warning:?}"))
            .collect(),
    };
    if args.policy.json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        for entry in output.entries {
            println!(
                "{}\t{}\tread={}\twrite={}{}",
                entry.path,
                entry.kind,
                entry.read_access,
                entry.write_access,
                if entry.sensitive.is_empty() {
                    String::new()
                } else {
                    format!("\tsensitive={}", entry.sensitive.join(","))
                }
            );
        }
    }
    Ok(())
}

fn destination(output: Option<PathBuf>, session: SessionId) -> anyhow::Result<PathBuf> {
    match output {
        Some(path) => Ok(if path.is_absolute() {
            path
        } else {
            std::env::current_dir()?.join(path)
        }),
        None => Ok(std::env::temp_dir()
            .join("agentguard")
            .join(session.to_string())
            .join("workspace")),
    }
}

fn create_workspace(args: CreateWorkspaceArgs) -> anyhow::Result<()> {
    let (root, _, policy) = load(&args.policy)?;
    let session = SessionId::new();
    let output = destination(args.output, session)?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let workspace = WorkspaceBuilder::new(&root, &|path: &RelativeRepoPath| {
        policy.evaluate(path, Access::Read) == AccessDecision::Allowed
    })
    .build_at(session, &output)?;
    let baseline = initialize_visible_baseline(workspace.root())?;
    if args.policy.json {
        println!(
            "{}",
            serde_json::json!({"session_id": session.to_string(), "workspace": workspace.root(), "baseline": baseline, "visible_files": workspace.manifest().visible_files, "omitted_paths": workspace.manifest().omitted_paths})
        );
    } else {
        println!(
            "workspace: {}\nsession: {}\nbaseline: {}",
            workspace.root().display(),
            session,
            baseline
        );
    }
    Ok(())
}

fn agent_command(agent: AgentPreset) -> Vec<String> {
    vec![
        match agent {
            AgentPreset::Codex => "codex",
            AgentPreset::Claude => "claude",
            AgentPreset::Opencode => "opencode",
        }
        .to_owned(),
    ]
}

fn now() -> anyhow::Result<String> {
    Ok(OffsetDateTime::now_utc().format(&Rfc3339)?)
}

fn run_command(args: RunArgs) -> anyhow::Result<()> {
    let (root, source, policy) = load(&args.policy)?;
    let argv = args.agent.map(agent_command).unwrap_or(args.argv);
    let command_text = argv.join(" ");
    if !policy.is_command_allowed(&command_text) {
        anyhow::bail!("command denied: policy commands.allow requires an exact argv match");
    }
    let command = CommandSpec::new(argv)?;
    let session = SessionId::new();
    let workspace_path = destination(
        args.workspace.or_else(|| {
            args.state_dir
                .map(|dir| dir.join(session.to_string()).join("workspace"))
        }),
        session,
    )?;
    if let Some(parent) = workspace_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let workspace = WorkspaceBuilder::new(&root, &|path: &RelativeRepoPath| {
        policy.evaluate(path, Access::Read) == AccessDecision::Allowed
    })
    .build_at(session, &workspace_path)?;
    initialize_visible_baseline(workspace.root())?;
    let receipt_path = args.receipt.unwrap_or_else(|| {
        let name = workspace
            .root()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("workspace");
        workspace
            .root()
            .parent()
            .unwrap_or(workspace.root())
            .join(format!("{name}.agentguard-receipt.json"))
    });
    let source_snapshot = snapshot_source(root.as_path())?;
    let network = match policy.policy().network.mode {
        NetworkMode::Deny => NetworkAccess::Deny,
        NetworkMode::Allow => NetworkAccess::Allow,
    };
    let (backend_id, caps, warnings) = match args.backend {
        Backend::Docker => {
            let image = args.image.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "Docker execution requires an explicit --image and will not fall back to local"
                )
            })?;
            let backend = DockerBackend::new("docker", image)?;
            let warnings = if network == NetworkAccess::Allow {
                vec![
                    "Docker networking is enabled; hostname allowlisting is unavailable".to_owned(),
                ]
            } else {
                Vec::new()
            };
            (backend.identity(), backend.capabilities(), warnings)
        }
        Backend::Local => {
            let backend = LocalBackend;
            let mut warnings = vec![
                "LOCAL ADVISORY: same-user native execution is not filesystem or network isolated"
                    .to_owned(),
            ];
            if network == NetworkAccess::Deny {
                warnings.push(
                    "Policy requests network denial, but native execution cannot enforce it"
                        .to_owned(),
                );
            }
            (backend.identity(), backend.capabilities(), warnings)
        }
    };
    for warning in &warnings {
        eprintln!("WARNING: {warning}");
    }
    let (visible, denied) = path_summaries(&workspace);
    let mut receipt = Receipt::partial(ReceiptStart {
        session_id: session,
        started_at: now()?,
        backend: BackendReceipt {
            id: backend_id.to_owned(),
            capabilities: caps,
        },
        command: command.argv().to_vec(),
        source: SourceReceipt {
            revision: source_snapshot.head,
            dirty: source_snapshot.dirty,
            git_context: "synthesized_visible_baseline".to_owned(),
        },
        policy_sha256: policy_sha256(&source),
        visible_paths: visible,
        denied_paths: denied,
        warnings,
        limitations: vec![
            "Only the top-level command and resulting workspace diff are reliably observed"
                .to_owned(),
            "Hostname allowlisting is unavailable in the MVP".to_owned(),
        ],
    });
    if matches!(
        command_text.as_str(),
        "cargo test" | "cargo fmt" | "cargo fmt --check" | "cargo clippy"
    ) {
        receipt.validation_commands.push(command_text.clone());
    }
    let writer = ReceiptWriter::new(&receipt_path);
    writer.write(&receipt)?;
    let request = ExecutionRequest {
        command,
        working_directory: None,
        environment: MinimalEnvironment::from_host(std::env::vars_os()),
        network,
    };
    let result = match args.backend {
        Backend::Docker => {
            let backend = DockerBackend::new("docker", args.image.expect("checked above"))?;
            backend.execute(&workspace, &request)
        }
        Backend::Local => LocalBackend.execute(&workspace, &request),
    };
    let summary = summarize_workspace_diff(workspace.root()).unwrap_or_default();
    match result {
        Ok(outcome) => {
            let status = if outcome.exit_code == Some(0) {
                ReceiptStatus::Completed
            } else {
                ReceiptStatus::Failed
            };
            receipt.finish(status, now()?, summary, outcome.exit_code, None)?;
            writer.write(&receipt)?;
            if args.policy.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "workspace": workspace.root(),
                        "receipt": receipt_path,
                        "exit_code": outcome.exit_code,
                        "status": format!("{status:?}").to_ascii_lowercase(),
                    })
                );
            } else {
                println!(
                    "workspace: {}\nreceipt: {}\nexit code: {}",
                    workspace.root().display(),
                    receipt_path.display(),
                    outcome
                        .exit_code
                        .map_or_else(|| "unknown".to_owned(), |code| code.to_string())
                );
            }
            if outcome.exit_code == Some(0) {
                Ok(())
            } else {
                anyhow::bail!("command exited unsuccessfully")
            }
        }
        Err(error) => {
            receipt.finish(
                ReceiptStatus::Failed,
                now()?,
                summary,
                None,
                Some("execution"),
            )?;
            writer.write(&receipt)?;
            Err(error.into())
        }
    }
}

fn path_summaries(workspace: &Workspace) -> (PathSummary, PathSummary) {
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
                agentguard_runtime::OmissionReason::DeniedByPolicy
            )
        })
        .filter_map(|item| RelativeRepoPath::new(&item.path).ok())
        .collect::<Vec<_>>();
    (
        PathSummary {
            count: visible.len(),
            paths: visible,
            truncated: false,
        },
        PathSummary {
            count: denied.len(),
            paths: denied,
            truncated: false,
        },
    )
}

fn diff(args: DiffArgs) -> anyhow::Result<()> {
    let summary = summarize_workspace_diff(&args.workspace)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!(
            "changed files: {}\nlines: +{} -{}",
            summary.changed_paths.len(),
            summary.lines_added,
            summary.lines_removed
        );
        for path in summary.changed_paths {
            println!("{}", path.as_str());
        }
    }
    Ok(())
}

fn receipt(args: ReceiptArgs) -> anyhow::Result<()> {
    let source = fs::read_to_string(args.path)?;
    let receipt: Receipt = serde_json::from_str(&source)?;
    let output = match args.format {
        ReceiptFormat::Terminal => render_terminal(&receipt),
        ReceiptFormat::Json => render_json(&receipt)?,
        ReceiptFormat::Markdown => render_markdown(&receipt),
    };
    println!("{output}");
    Ok(())
}
