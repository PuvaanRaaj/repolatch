# AgentGuard product scope

**Status:** Runnable MVP baseline (2026-07-20)

## Product statement

AgentGuard is intended to be a local, provider-independent, non-AI supervision layer for terminal coding-agent commands. It does not call an LLM, capture prompts, or provide an agent. Its current Rust libraries provide building blocks for policy validation, safe workspace construction, explicit command execution, Git summaries, and metadata-only receipts.

## Implemented today

1. Parse and compile policy version 1 with deny-wins filesystem decisions.
2. Scan repository metadata and classify selected sensitive path patterns without reading file contents.
3. Build a separate workspace from allowed regular files while omitting source symlinks and `.git`.
4. Construct/run local advisory or Docker execution backends with an explicit minimal environment.
5. Snapshot source Git metadata, initialize a synthesized workspace baseline, and summarize workspace changes.
6. Create, finalize, persist, and render metadata-only receipt values.

The CLI connects these functions through `init`, `policy validate`, `inspect`, `workspace create`, `run`, `diff`, and `receipt`. The desktop includes local repository selection, policy editing, metadata tree views, bounded/masked previews, capability status, launch, diff, and receipt views. Its local debug app and DMG build completed successfully.

## Planned, not shipped

- a packaged installer or published distribution;
- provider authentication handling and bundled coding agents;
- published desktop artifacts, complete session history across restarts, and a supported-platform matrix;
- Docker-enabled end-to-end verification beyond the tested macOS/Docker Desktop host and a supported-platform matrix;
- hostname allowlisting, command allowlist enforcement in runtime, and local OS sandboxing.

## Security-status vocabulary

- **Enforced**: a backend advertises and applies the capability at its launch boundary.
- **Advisory**: preparation or warnings exist, but a same-user process can bypass the boundary.
- **Unavailable**: the backend cannot provide the requested control.
- **Not reliably observed**: activity cannot be credibly recorded comprehensively.

The current Docker backend reports enforced generated-workspace boundaries, network denial, and minimal environment construction. It reports hostname allowlisting unavailable and child-command observation not reliably observed. Local filesystem controls are advisory, local network controls unavailable, and its minimal environment construction enforced. Docker execution fails if its availability check fails; it does not silently use the local backend.

## Non-goals for the current libraries

The libraries do not provide AI features, cloud services, telemetry, a custom container builder, automatic credential mounting, source-repository mounting, arbitrary host mounts, network host filtering, or protection against a compromised host/Docker daemon/runtime/trusted image. They do not make a successful Docker unit test equivalent to deployed Docker isolation.

## Next integration acceptance criteria

The CLI bridges compiled policy into runtime visibility, rejects disallowed commands before execution, initializes synthesized baselines, writes receipts, and renders independent capability states. The desktop performs the same integration through Tauri commands. Docker-enabled tests on declared platforms remain required before claiming operational Docker proof.
