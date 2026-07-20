# AgentGuard Threat Model

**Status:** MVP security baseline
**Date:** 2026-07-20

## Assets

- the original repository and its current working-tree state;
- denied repository files and all secret values;
- the user's home, SSH/cloud/package credentials, provider tokens, and Docker socket;
- host network identity and other host processes;
- trustworthy session metadata and enforcement claims.

## Adversary

The launched agent command and repository content are untrusted. They may run arbitrary subprocesses, attempt path traversal or symlink escape, mutate visible files, execute malicious build hooks, poison terminal output, probe inherited environment variables, and exfiltrate through any deliberately permitted credential or network route.

The developer, AgentGuard binary, host operating system, Docker daemon/Desktop VM, and explicitly selected container image are trusted for the MVP. Protection from another hostile process already running as the same OS user is not claimed.

## Security invariants

1. The original repository is never mounted into an enforced container.
2. Workspace paths are repository-relative, containment-checked, and deny-wins.
3. Source symlinks are never followed or copied.
4. Docker and user commands are constructed as argv, never interpolated into a shell.
5. Host environment inheritance is disabled; only documented names and explicitly approved values may pass.
6. Home, Docker socket, credential directories, devices, and arbitrary host resources are not mounted in the MVP.
7. Inspection and staging never execute repository hooks, filters, scripts, Dockerfiles, or configuration.
8. Receipts and normal AgentGuard logs contain metadata, not secret values, file contents, full environment dumps, or terminal transcripts.
9. Docker unavailability fails an enforced request; there is no silent downgrade.
10. Every enforcement claim is scoped to a backend capability and backed by a test.

## Control matrix

| Control | Docker workspace backend | Local backend |
|---|---|---|
| Source repository not mounted | Enforced | Advisory |
| Denied paths absent from generated workspace | Enforced by builder | Advisory after launch |
| Writes limited to generated workspace | Enforced within stated Docker trust boundary | Advisory |
| Home/credential paths absent | Enforced by mount/env construction | Advisory |
| Default network disabled | Enforced with `--network none` | Unavailable |
| Hostname allowlist | Unavailable | Unavailable |
| Environment filtering | Enforced at launch boundary | Advisory after launch |
| All child commands/file reads observed | Not reliably observed | Not reliably observed |
| Secret-safe terminal output | Not reliably enforceable | Not reliably enforceable |
| Protection from Docker/host compromise | Unavailable | Unavailable |

## Attack surfaces and mitigations

### Traversal and symlinks

Absolute paths, root/prefix components, parent traversal, malformed policy globs, and non-contained joins are rejected. Traversal uses non-following metadata and omits all symlinks. Destination entries are created within a private session root. Tests cover direct, encoded/Unicode-adjacent, space-containing, file-link, and directory-link cases.

### Container escape and mounts

The Docker builder emits a fixed option set: generated workspace only, no host namespace modes, no privilege, no devices, no added capabilities, no Docker socket, no arbitrary mounts, minimal environment, no-new-privileges, and network none by default. Docker daemon administrators and runtime/kernel exploits remain trusted-boundary limitations.

### Environment and authentication

AgentGuard does not inherit the host environment wholesale and does not automatically locate or mount provider authentication. Future credential resources must be explicit and independently reviewed. Names and scopes may be receipted; values may not. Authenticated network sessions necessarily expand the exfiltration boundary.

### Malicious repository content

Inspection and staging treat the repository as data. Git hooks, filters, package scripts, Cargo config, shell startup files, Dockerfiles, and Compose files are not executed. When the user deliberately launches a build or agent inside the generated workspace, repository scripts are part of the untrusted command workload.

### Logs and receipts

Arbitrary output cannot be reliably redacted. AgentGuard therefore uses live process I/O without a durable transcript by default. Receipts store the top-level command after argument redaction, aggregate path information, and diff metadata. Sensitive content and environment values are excluded.

## Residual risks

- A hostile same-user process can race source inspection/copying; the MVP assumes a stable source during staging.
- Docker Desktop shares host paths through a Linux VM, and Docker daemon control is privileged. The design minimizes shared paths but does not eliminate daemon/runtime risk.
- Container image pulls occur outside the container's `none` network and are not evidence of workload network access.
- Disk quotas for bind-mounted workspace data are not strongly enforced in the MVP.
- A user-approved credential or future network route can be abused by the launched command.
- AgentGuard cannot enumerate every child command, file read, or network attempt without stronger instrumentation.

## Required proof tests

- deny precedence and fail-closed policy parsing;
- absolute/parent traversal rejection and contained joins;
- source file and directory symlinks omitted;
- fake `.env`/key/credential files absent from generated workspaces and receipts;
- original repository unchanged by staging and execution setup;
- Docker argv contains only the generated-workspace mount and required hardening flags;
- Docker `none` network cannot reach an external endpoint when E2E tests run;
- missing/unhealthy Docker never falls back;
- minimal environment excludes fake host/provider secrets;
- partial receipts survive deterministic interruption/failure;
- spaces, Unicode, large files, and binary files do not crash inspection or preview.
