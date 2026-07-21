# RepoLatch product scope

**Status:** Runnable MVP baseline (2026-07-20)

## Product statement

RepoLatch is intended to be a local, provider-independent execution boundary and minimal code workbench for terminal coding agents. It is for developers whose agent is the primary implementation interface but who still need a controlled place to inspect files, make small edits, choose an execution boundary, review changes, and audit a completed session.

It does not call an LLM, capture prompts, or provide an agent. Its Rust libraries provide policy validation, safe workspace construction, explicit command execution, Git summaries, and metadata-only receipts. The desktop app makes those controls visible without becoming a general-purpose IDE.

## Product goal

Let a developer use any supported terminal coding agent without giving up explicit repository boundaries or reopening a full IDE for routine supervision.

RepoLatch succeeds when the developer can:

1. open a repository without executing repository-owned hooks or scripts;
2. see exactly which files and commands are in scope;
3. run the agent through a backend whose enforcement limits are stated accurately;
4. inspect and edit allowed source files in a focused workbench;
5. review the resulting diff and metadata-only receipt before accepting the work.

## Category and positioning

RepoLatch is not positioned as a feature-for-feature rival to Zed, Cursor, or VS Code. Those products are full code editors with language tooling, extensions, debugging, and integrated agent experiences. RepoLatch is an agent control plane with an editor-sized review surface.

The intended workflow is agent-first and IDE-light: the terminal agent performs most implementation work; RepoLatch supplies repository policy, execution isolation, file inspection, targeted manual editing, diff review, and session evidence. It remains useful across model and agent providers because it does not bundle or authenticate a model.

## Implemented today

1. Parse and compile policy version 1 with deny-wins filesystem decisions.
2. Scan repository metadata and classify selected sensitive path patterns without reading file contents.
3. Build a separate workspace from allowed regular files while omitting source symlinks and `.git`.
4. Construct/run local advisory or Docker execution backends with an explicit minimal environment.
5. Snapshot source Git metadata, initialize a synthesized workspace baseline, and summarize workspace changes.
6. Create, finalize, persist, and render metadata-only receipt values.

The CLI connects these functions through `init`, `policy validate`, `inspect`, `workspace create`, `run`, `diff`, and `receipt`. The desktop includes local repository selection, a hierarchical file explorer, syntax-aware previews, policy-gated source editing, policy editing, bounded or masked content states, backend capability status, launch, diff, and receipt views. Its local debug app and DMG build completed successfully.

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

The desktop is not intended to provide language-server features, autocomplete, debugging, an extension marketplace, embedded model chat, or the breadth of a general-purpose IDE. These are deliberate scope boundaries, not missing claims.

## Next integration acceptance criteria

The CLI bridges compiled policy into runtime visibility, rejects disallowed commands before execution, initializes synthesized baselines, writes receipts, and renders independent capability states. The desktop performs the same integration through Tauri commands. Docker-enabled tests on declared platforms remain required before claiming operational Docker proof.
