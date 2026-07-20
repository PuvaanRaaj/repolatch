# Policy reference

AgentGuard policy version 1 is strict TOML. The parser rejects unknown fields, unsupported versions, malformed globs, empty/NUL command entries, empty or whitespace-containing host entries, absolute globs, backslashes, and glob path components equal to `..`.

## Complete example

```toml
version = 1

[filesystem]
read = ["src/**", "tests/**", "docs/**", "Cargo.toml", "README.md"]
write = ["src/**", "tests/**"]
deny = [".env", ".env.*", "credentials/**", "production/**", "**/*.pem", "**/*.key", ".ssh/**", "**/.ssh/**", ".aws/credentials", "**/.aws/credentials", ".npmrc", "**/.npmrc", ".pypirc", "**/.pypirc", ".netrc", "**/.netrc"]

[network]
mode = "deny"

[network.allow]
hosts = ["registry.npmjs.org", "crates.io"]

[commands]
allow = ["cargo test", "cargo fmt", "cargo clippy"]
```

## Filesystem rules

Patterns are non-empty, slash-separated paths relative to the repository. Glob path separators are literal: use `**` for recursive matching. Evaluation is deterministic:

1. Any matching `deny` pattern denies both read and write.
2. A matching `write` pattern grants both write and read.
3. A matching `read` pattern grants read only.
4. An unmatched path is invisible.

The policy crate answers read/write decisions for validated `RelativeRepoPath` values. The runtime workspace builder takes a `VisibilityPolicy` callback; the embedding application must pass a callback that applies the desired compiled-policy decision. It copies only allowed regular files, recursively traverses directories to find them, and omits source symlinks and `.git` regardless of policy.

In the MVP, read visibility is enforced by omission from the generated Docker workspace, but per-path `write` globs are not a filesystem boundary: all visible paths in that generated workspace are writable. Backend capability output labels Docker policy-write enforcement unavailable and native policy-write enforcement advisory. The `write` decision remains useful for inspection and future backends.

## Network and command fields

`network.mode` accepts only `"allow"` or `"deny"`. Docker adds `--network none` for deny and uses Docker's default network for allow. `network.allow.hosts` records non-empty host names but is not enforced by either MVP backend. The local backend cannot restrict networking and warns when the policy requests denial.

`commands.allow` is an exact list of non-empty command strings. `CompiledPolicy::is_command_allowed` reports membership. The runtime backends do not call that method, so callers must enforce it before creating an `ExecutionRequest` if command restrictions are required.

## What a policy cannot do yet

Policy files do not create credentials, a hostname firewall, a container image, Docker E2E proof, or a local OS sandbox. The CLI's `codex`, `claude`, and `opencode` presets expand only to executable argv; the command must still be explicitly allowed and available in the selected runtime.
