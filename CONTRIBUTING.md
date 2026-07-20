# Contributing to AgentGuard

Thank you for helping improve the project. AgentGuard treats repository contents and launched commands as untrusted, so changes to policy, workspace construction, execution, receipts, or security claims need small, reviewable diffs and matching tests.

## Setup

Use Rust 1.88 or newer. Do not run repository hooks, filters, package scripts, Dockerfiles, or Compose files merely to inspect or stage a change. Use fake secrets only in fixtures.

## Before opening a change

Run the Rust checks:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Also run the desktop checks:

```bash
pnpm --dir apps/desktop lint
pnpm --dir apps/desktop typecheck
pnpm --dir apps/desktop test
pnpm --dir apps/desktop exec tauri build --debug
```

## Design rules

- Docker is the only enforced MVP backend; local execution stays advisory.
- Deny rules win, unmatched files are invisible, and source symlinks are omitted.
- Construct commands as argv. Never interpolate user input into a shell command.
- Keep the environment explicit and minimal. Do not auto-mount home directories, credentials, devices, or the Docker socket.
- Receipts and normal logs are metadata-only: never add secret values, file contents, complete environments, or terminal transcripts.
- Make every enforcement claim specific to a tested backend capability; missing capability data must fail closed.

## Documentation and packaging

Keep README, policy examples, and enforcement tables aligned with the implemented CLI and desktop adapters. Do not describe named agent integrations, crates.io publication, installers, or Docker E2E behavior as shipped until the relevant artifact and verification exist. Packaging status is tracked in [docs/packaging.md](docs/packaging.md).
