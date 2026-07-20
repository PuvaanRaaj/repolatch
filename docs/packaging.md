# Packaging status

## Current state

The workspace declares Apache-2.0 licensing, version `0.1.0`, Rust edition 2024, and a minimum Rust version of 1.88 for five library crates and the `agentguard` CLI. A private Tauri desktop application lives in `apps/desktop`; its 2026-07-20 local debug build produced an app bundle and an 8.7 MB DMG. These are local debug artifacts, not signed or published releases. The repository has no release automation, package registry configuration, or publication workflow.

No package has been published or is being published by this repository. This document intentionally contains no registry, download, or remote repository URLs.

## Before publishing

Publication should wait until maintainers decide which library crates and CLI APIs are stable public interfaces and supply required package metadata, API documentation, versioning policy, changelog/release process, ownership, provenance/signing posture, and verification workflow. A CLI or desktop distribution needs a supported-platform matrix, install instructions, artifact signing, update policy, and Docker end-to-end verification beyond the tested macOS/Docker Desktop host.

## CI posture

The repository CI runs Rust format, Clippy, workspace tests, and an explicit Docker isolation E2E test. It also runs desktop lint/typecheck/test and Tauri debug builds on Linux, macOS, and Windows. The desktop has committed Cargo and pnpm lockfiles plus a project-local Tauri CLI dependency. CI uses frozen dependency installation; branch protection should require these jobs once CI is enabled.
