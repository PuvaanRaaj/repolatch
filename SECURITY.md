# Security policy

## Reporting a vulnerability

Please report suspected vulnerabilities privately to the maintainers through the repository host's private security-advisory feature, if enabled. If it is not available, contact the maintainer through an established private channel rather than opening a public issue.

Include a minimal reproduction, affected revision, operating system, Rust/Docker versions where relevant, expected versus observed behavior, and a clear note if fake fixture secrets were used. Do not send real credentials, customer data, source file contents, terminal transcripts, or private repositories.

We will acknowledge a report when a maintainer is able to review it, investigate privately, and coordinate disclosure and remediation with the reporter. This early project does not promise a response-time SLA.

## Scope and boundaries

The intended security controls include path validation, deny-wins policy evaluation, source-symlink omission, generated-workspace separation, minimal environment construction, Docker argv hardening, and metadata-only receipts.

The Docker backend is the only enforced MVP backend. Local execution is advisory. Docker unit tests do not substitute for Docker-enabled end-to-end verification. A compromised host, Docker daemon/runtime, trusted image, or hostile same-user process is outside the claimed boundary; see [docs/threat-model.md](docs/threat-model.md).
