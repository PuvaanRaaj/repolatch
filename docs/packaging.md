# Packaging status

## Current state

The workspace declares Apache-2.0 licensing, version `0.1.0`, Rust edition 2024, and a minimum Rust version of 1.88 for five library crates and the `repolatch` CLI. A private Tauri desktop application lives in `apps/desktop`; its local debug build produces a macOS app bundle and DMG.

Every successful push to `main` runs the release workflow and publishes an unsigned GitHub prerelease containing the CLI and desktop packages listed in [release-packages.md](release-packages.md). No crate or npm package is published to a package registry. The public repository is <https://github.com/PuvaanRaaj/repolatch>.

## Before publishing

Publication should wait until maintainers decide which library crates and CLI APIs are stable public interfaces and supply required package metadata, API documentation, versioning policy, changelog/release process, ownership, provenance/signing posture, and verification workflow. A CLI or desktop distribution needs a supported-platform matrix, install instructions, artifact signing, update policy, and Docker end-to-end verification beyond the tested macOS/Docker Desktop host.

## CI posture

The repository CI runs Rust format, Clippy, workspace tests, and an explicit Docker isolation E2E test. It also runs desktop lint/typecheck/test and Tauri debug builds on Linux, macOS, and Windows. The desktop has committed Cargo and pnpm lockfiles plus a project-local Tauri CLI dependency. CI uses frozen dependency installation; branch protection should require these jobs once CI is enabled.
