# AgentGuard

AgentGuard runs terminal coding agents in a filtered copy of a repository. A policy controls which files are visible, which commands may run, and whether network access is allowed. The original checkout is never mounted into Docker or used as the agent's working directory.

AgentGuard does not provide a model or collect prompts. It launches an agent already installed on the machine or included in a Docker image.

## Current status

The repository contains five Rust libraries, the `agentguard` CLI, and a Tauri desktop application. It is an early MVP: the CLI and desktop app run locally, but there are no signed release artifacts yet.

## Architecture

| Crate | Responsibility |
| --- | --- |
| `agentguard-core` | Validated repository-relative paths, session and capability types, shared errors. |
| `agentguard-policy` | Strict version-1 TOML parsing, deny-wins glob matching, metadata-only repository scanning. |
| `agentguard-git` | Read-only source snapshots and a synthesized visible-workspace Git baseline/diff summary. |
| `agentguard-runtime` | Private workspace copying, minimal environments, and local/Docker execution backends. |
| `agentguard-receipt` | Versioned, metadata-only receipt types, atomic persistence, and renderers. |
| `agentguard-cli` | CLI orchestration for policies, workspaces, execution, diffs, and receipts. |
| `apps/desktop` | Offline Tauri/React desktop interface over the same Rust core. |

The source repository is input only. The workspace builder copies allowed regular files into a separate directory, omits source symlinks and `.git`, and never mounts the source repository into Docker. Commands are represented as argv rather than shell strings.

## Requirements and installation

- Rust 1.88 or newer (the workspace uses edition 2024).
- Git for the Git metadata/baseline library.
- Docker only for a Docker-backed run (an explicit image is required).
- Node.js 22 and pnpm for the desktop application.

Run the CLI from a checkout:

```bash
cargo run -p agentguard-cli -- --help
```

## Quick start

Initialize a policy in a test repository. `init` refuses to overwrite an existing policy; use [examples/agentguard.toml](examples/agentguard.toml) when you need a reviewed starting point.

```bash
cargo run -p agentguard-cli -- init --repo /path/to/project
cargo run -p agentguard-cli -- policy validate --repo /path/to/project
cargo run -p agentguard-cli -- inspect --repo /path/to/project --json
cargo run -p agentguard-cli -- workspace create --repo /path/to/project --output /tmp/agentguard-workspace
cargo run -p agentguard-cli -- run --repo /path/to/project --backend local -- cargo test
```

`run` requires an exact `commands.allow` match. Arbitrary commands follow `--`; the exact argv joined by one ASCII space must be allowed. Docker runs additionally require `--backend docker --image IMAGE`; an unavailable Docker request never falls back to local. See [docs/policy-reference.md](docs/policy-reference.md) for the exact schema and current enforcement boundaries.

## Supported agents

AgentGuard does not bundle, authenticate, or discover coding agents. It provides argv-only `--agent codex`, `--agent claude`, and `--agent opencode` presets; the executable must exist in the local environment or explicit Docker image and its exact command must appear in `commands.allow`. AgentGuard never mounts provider credentials automatically.

## Enforcement limits

Docker is the only backend that advertises enforced filesystem isolation, generated-workspace mounting, and `--network none`; unavailable Docker fails rather than falling back from a Docker request. The local backend runs as the current OS user and labels filesystem boundaries advisory. Both backends construct a minimal explicit environment, but AgentGuard does not observe every child process, file read, network destination, or terminal output.

The app itself never runs in Docker. Docker is an explicit backend choice for the launched command. The MVP bind-mounts a generated, policy-filtered copy—not the original repository—so large sessions still pay Docker Desktop filesystem overhead on macOS. Use the native advisory backend when speed is more important than enforcement. A named-volume backend that seeds once and exports once is a planned performance improvement, not a current claim.

Docker hardening is covered by argv-level unit tests. The local macOS/Docker Desktop run also proved the generated-workspace mount, denied-file omission, source preservation, writable output, network denial, diff, and receipt flow. This is not proof for other host/runtime combinations; run the same E2E gate in each supported environment.

The Docker backend applies `--network none` for `network.mode = "deny"` and uses Docker's default network for `"allow"`. Hostname lists remain unavailable and are never presented as enforced. The local backend cannot enforce network denial. The CLI and desktop launcher enforce an exact command allowlist match before creating an execution request; library consumers must do so themselves.

See [docs/backend-enforcement-matrix.md](docs/backend-enforcement-matrix.md) and [docs/threat-model.md](docs/threat-model.md) before relying on AgentGuard for a security boundary.

## Continuous releases

Every successful push or merge to **main** creates a unique GitHub prerelease with CLI archives, desktop packages for Linux, macOS, and Windows, and SHA-256 checksums. Each release describes every attached file and lists the commits since the previous main build.

These automated packages are unsigned and are not notarized. See [docs/release-packages.md](docs/release-packages.md) for the package list and platform details.

## Development

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

The desktop checks are required; see [docs/packaging.md](docs/packaging.md) for release limitations:

```bash
pnpm --dir apps/desktop lint
pnpm --dir apps/desktop typecheck
pnpm --dir apps/desktop test
pnpm --dir apps/desktop exec tauri build --debug
```

Launch the desktop application during development:

```bash
pnpm --dir apps/desktop install --frozen-lockfile
pnpm --dir apps/desktop exec tauri dev
```

## Security

Please do not file security-sensitive reports in a public issue. Follow [SECURITY.md](SECURITY.md) for the reporting process and scope.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
