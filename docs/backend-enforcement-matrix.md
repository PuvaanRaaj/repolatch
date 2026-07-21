# Backend enforcement matrix

Capability labels describe individual controls, not overall backend safety. The matrix reflects the current runtime capability report and Docker argv construction.

| Control | Docker backend | Local backend | Notes |
| --- | --- | --- | --- |
| Generated-workspace filesystem boundary | Enforced | Advisory | Docker mounts only the generated workspace; local runs as the current OS user. |
| Writes limited to generated workspace | Enforced | Advisory | Docker receives one generated-workspace bind mount. Policy `write` matches are reported but are not a per-file Docker write boundary in the MVP. |
| Policy `write` globs | Unavailable | Advisory | All visible generated-workspace paths are writable in the MVP; the UI and inspector report matches without claiming enforcement. |
| Network denial | Enforced when requested | Unavailable | Docker uses `--network none` for deny and its default network for allow; local has no network control. |
| Hostname allowlist | Unavailable | Unavailable | Standard Docker provides no hostname filtering. |
| Minimal explicit environment | Enforced at launch | Enforced at launch | The child environment is cleared and rebuilt from approved names; same-user local code can still read host files and services. |
| Child-command observation | Not reliably observed | Not reliably observed | Only the top-level process status and resulting diff are recorded. |
| Source repository mount | Not mounted | Not applicable | The Docker backend rejects a generated workspace inside the source root. |

## Docker request shape

The runtime conditionally requests `--network none`, plus a read-only root filesystem, dropped capabilities, `no-new-privileges`, resource limits, a non-root UID/GID, a bounded `/tmp` tmpfs, and one read-write bind mount at `/workspace`. It invokes Docker and the workload as argv, never through a shell. It clears Docker's process environment except for a fixed `PATH`.

These claims have unit coverage plus a live macOS/Docker Desktop 29.6.1 test proving network denial, denied-file omission, source preservation, writable workspace output, diff generation, and receipt finalization. Other daemon, image, and host combinations still require their own proof. Image pulling occurs outside the workload's `none` network; Docker/host compromise remains outside the security boundary.

## macOS storage and I/O

RepoLatch is a native application. Only the selected command runs in Docker. The current backend bind-mounts the generated policy-filtered workspace, never the original repository. This preserves host-side diff review but can be slower on large repositories because Docker Desktop mediates filesystem I/O through its Linux VM. Native advisory execution avoids that overhead. Moving container sessions to a Docker named volume with one-time import/export is future work.
