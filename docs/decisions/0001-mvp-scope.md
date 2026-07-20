# ADR 0001: AgentGuard MVP scope and enforcement boundary

**Status:** Accepted
**Date:** 2026-07-20

## Context

AgentGuard must supervise arbitrary coding-agent commands while making accurate security claims on macOS. A same-user subprocess cannot be a secure filesystem sandbox. Copying the source repository's `.git` directory would also preserve objects containing denied files. Standard Docker networking supports reliable on/off isolation but not hostname allowlisting.

## Decision

1. Docker is the only enforced MVP backend. Local execution exists for convenience and is always advisory.
2. Every session uses a generated visible-files-only workspace. The source repository is never mounted into Docker.
3. The workspace gets a fresh Git repository and synthesized baseline commit. Source HEAD and dirty-state metadata are recorded separately.
4. Deny rules win over all allow rules; unmatched files are invisible.
5. All source symlinks are warned about and omitted.
6. Docker network mode defaults to `none`. Hostname allowlists are parsed and reported as unavailable, not simulated.
7. Provider authentication is not discovered or mounted automatically.
8. Receipts record reliable metadata only. Child-command auditing, file-read auditing, and terminal redaction are explicitly outside the claim.
9. The desktop uses Tauri 2, React, and TypeScript, sharing the Rust security core and consuming backend capability data rather than hard-coding status.

## Consequences

The MVP provides a credible container boundary and useful review workflow without claiming full host isolation. Generated-workspace diffs omit denied files and do not reproduce every original Git behavior. Users must pre-provision a suitable container image and separately decide how an agent receives authentication. Symlink-heavy repositories need explicit future design work.

## Rejected alternatives

- **Run directly in the source tree:** modifies user state and provides no credible boundary.
- **Copy `.git`:** denied data may remain readable through Git objects.
- **Silently fall back to local execution:** converts an enforcement failure into an unsafe success.
- **Claim Docker hostname filtering:** standard Docker options do not enforce the policy's host list.
- **Follow in-repository symlinks:** canonical checks alone do not remove race and escape risks.
- **Build repository Dockerfiles automatically:** executes untrusted repository content during setup.
