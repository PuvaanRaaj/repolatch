# AgentGuard architecture

**Status:** Implemented MVP baseline (2026-07-20)

## Current system shape

AgentGuard is a Cargo workspace with five library crates and one CLI crate. `apps/desktop` contains a separate Tauri Cargo workspace so Tauri can manage its platform-specific lockfile independently:

```text
agentguard-core
  ^        ^             ^
  |        |             |
  |   agentguard-policy  agentguard-git
  |        |             |
  +--------+-------------+--- agentguard-receipt
  |
  +-------------------------- agentguard-runtime
                                  ^
                            agentguard-cli
                                  ^
                         apps/desktop/src-tauri
```

`agentguard-cli` orchestrates policy parsing, workspace construction, source snapshots, baseline initialization, execution, diff summaries, and partial/final receipt writes. The desktop implements a narrow adapter over the same libraries and has passed a local Tauri debug build.

## Crate ownership

- `agentguard-core`: validated repository-relative paths, session IDs, command argv, capability descriptions, and shared errors.
- `agentguard-policy`: strict TOML policy parsing/compilation and metadata-only repository scanning.
- `agentguard-git`: read-only source Git snapshots, synthesized-baseline initialization, and generated-workspace diff summaries.
- `agentguard-runtime`: workspace construction from a caller-supplied visibility callback, minimal environments, local execution, and Docker argv/execution.
- `agentguard-receipt`: receipt model, atomic JSON persistence, command sanitization, and terminal/JSON/Markdown renderers.
- `agentguard-cli`: command parsing and end-to-end orchestration; it enforces exact command allowlist matching.
- `apps/desktop`: local Tauri command adapters and React views; it has its own Cargo workspace rather than being a root member.

Dependencies point toward `agentguard-core`. Runtime does not directly depend on policy, Git, or receipt; the CLI and desktop adapter connect them.

## Implemented boundaries

`RelativeRepoPath` rejects unsafe source path forms, and `WorkspaceBuilder` requires an absolute destination outside the source root. It recursively copies only caller-allowed regular files, omits all source symlinks and `.git`, and applies private Unix permissions to created workspace directories/files. Its source-tree stability assumption remains a same-user race limitation.

`DockerBackend` constructs a fixed `docker run` argv with one generated-workspace bind mount and conditionally adds `--network none` for a deny policy; it does not use a shell. It clears its launch environment except for a fixed PATH and executes a provided argv. `LocalBackend` also clears the environment and runs in the generated workspace, but is advisory for filesystem isolation and cannot enforce network denial because it runs as the current OS user.

`agentguard-git` uses Git subprocesses with hooks, attributes, system config, prompts, external diffs, and text conversions disabled. It can initialize a new baseline repository in a generated workspace and summarize later changes. Callers must invoke that baseline function explicitly after building the workspace.

The CLI and desktop write a partial receipt before backend execution and finalize it after an observed outcome or execution error. Runtime remains receipt-agnostic.

## Capability reporting

Backends publish independent capability states: filesystem isolation, workspace write boundary, network denial, hostname allowlist, environment filtering, and child-command observation. Consumers must display each state as reported; they must not infer whole-backend safety. The current matrix is in [backend-enforcement-matrix.md](backend-enforcement-matrix.md).

## Verification gap

Rust tests validate the intended policy, workspace, Git, receipt, and Docker argv behavior. Docker-enabled end-to-end testing is still required to verify the installed daemon, workload network isolation, and host/image compatibility; code inspection alone is not an operational Docker proof.
