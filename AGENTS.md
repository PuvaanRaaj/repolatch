# AgentGuard contributor guidance

## Architectural invariants

- Docker is the only enforced MVP backend. Local execution is always advisory.
- Never mount or modify the source repository during workspace execution.
- Deny rules win. Unmatched files are invisible. Source symlinks are omitted.
- Construct commands as argv; never interpolate user input into a shell command.
- Pass a minimal explicit environment. Never auto-mount home, credentials, devices, or the Docker socket.
- Receipts and normal logs contain metadata only. Never log secret values, file contents, full environment dumps, or terminal transcripts.
- Enforcement labels must describe one tested backend capability. Missing enforcement fails closed and never silently downgrades.

## Crate ownership

- `agentguard-core`: validated paths and shared domain types.
- `agentguard-policy`: policy parsing, matching, scanning, sensitive classifications.
- `agentguard-git`: source snapshots and synthesized-workspace diffs.
- `agentguard-receipt`: receipt model, persistence, and renderers.
- `agentguard-runtime`: workspace construction and execution backends.
- `agentguard-cli`: CLI parsing and presentation.
- `apps/desktop`: Tauri adapters and offline desktop UI.

Dependencies point toward core; presentation code must not become a library dependency. Add a backend only through the runtime trait, publish independent capabilities, fail closed when required controls are unavailable, and add tests for every enforcement claim.

## Required checks

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
pnpm --dir apps/desktop lint
pnpm --dir apps/desktop typecheck
pnpm --dir apps/desktop test
pnpm --dir apps/desktop exec tauri build --debug
```

Use fake secrets only in fixtures. Do not run repository hooks, filters, package scripts, Dockerfiles, or Compose files during inspection or staging.
