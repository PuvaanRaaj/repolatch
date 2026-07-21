# RepoLatch launch positioning

**Public repository:** <https://github.com/PuvaanRaaj/repolatch>

## The one-line goal

Give terminal coding agents a controlled workspace—and give developers a small place to inspect, edit, and review their work without returning to a full IDE.

## The category

RepoLatch is a provider-independent execution boundary with a minimal code workbench.

It is not another AI editor. It does not ship a model, chat interface, autocomplete, debugger, language server, or extension marketplace. The terminal agent does the implementation work. RepoLatch controls what that agent can see and run, and makes the result reviewable.

## The honest comparison

### Zed

Zed is a fast, full editor that hosts built-in and external agents. Its Agent Client Protocol integration lets third-party agents use Zed's thread and editing interface.

RepoLatch should not claim to out-edit Zed. Its advantage is separation: the developer can keep using the terminal agent they already chose while RepoLatch owns repository policy, generated workspaces, backend capabilities, diffs, and receipts.

### Cursor

Cursor combines an AI-native editor, terminal agent, permission controls, and remote background agents. It is a complete model-assisted development environment.

RepoLatch is smaller and provider-independent. Its enforced Docker path uses a generated, policy-filtered workspace rather than the source checkout, and its product does not depend on Cursor's models, account, or editor.

### VS Code

VS Code combines a general-purpose editor and extension ecosystem with local, background, cloud, and third-party agents. Workspace Trust reduces automatic execution when opening untrusted code.

RepoLatch has a narrower job: supervise a terminal agent against an explicit repository policy and record the session result. It trades the IDE and extension surface for a focused control and review surface.

### Terminal agent alone

This is the closest alternative. It is fast and direct, but the agent commonly operates in the real checkout with the user's ambient host access.

RepoLatch adds a separate workspace, deny-wins visibility, explicit commands, a minimal environment, backend capability labels, guarded editing, diffs, and metadata-only receipts.

## The “I uninstalled my IDE” story

The useful claim is not that IDEs are obsolete. It is that the primary development interface has changed for some developers.

This is already visible in prominent agent-first workflows. Spotify co-CEO Gustav Söderström said some of Spotify's best developers had stopped writing code by hand and were instead generating and monitoring it. Spotify Engineering has also described teams moving from IDEs to terminal-led and background-agent workflows. Boris Cherny, the inventor of Claude Code, helped establish the terminal itself as a serious agent workbench.

RepoLatch starts where that story becomes risky. Its creator works in payments, where a repository can sit next to merchant configuration, signing material, credentials, internal endpoints, and environment-specific values. The agent may be trusted to change code and tests without being trusted to inspect or rewrite `.env` values. Those values remain human-managed; RepoLatch's job is to keep denied content outside the agent workspace and make the boundary visible.

In an agent-first workflow:

1. the terminal agent reads, edits, runs tests, and iterates;
2. RepoLatch defines the agent's repository and execution boundary;
3. the developer uses the workbench for inspection, small manual corrections, policy changes, and review;
4. a full IDE remains optional for language-heavy debugging or sustained manual coding.

The shortest expression of this idea is:

> Keep the agent. Lose the IDE. Keep the boundaries.

## Claims to make

- Provider-independent: it launches an installed or containerized agent; it does not provide a model.
- Source-preserving: the original checkout is not the agent's working directory and is never mounted into Docker.
- Explicit: file visibility, write paths, commands, and network mode come from policy.
- Honest about enforcement: Docker is the enforced backend; native same-user execution is advisory.
- Reviewable: the desktop includes a file tree, syntax-aware viewing and editing, diffs, capability status, and metadata-only receipts.

## Claims to avoid

- “A secure sandbox on every backend.” Native execution is advisory.
- “A Zed/Cursor/VS Code replacement.” It deliberately omits full-IDE features.
- “Zero access to secrets.” Denied and classified paths are filtered, but security still depends on policy, host, runtime, image, and credentials explicitly provided to a session.
- “No Docker I/O overhead.” The current Docker backend bind-mounts a generated copy and can still be slower on Docker Desktop for macOS.
- “Complete agent observability.” Child commands, file reads, network attempts, and terminal output are not comprehensively observed.

## X launch thread

**Post 1**

The “I don't use an IDE anymore” workflow is no longer fringe.

Spotify's co-CEO says some of its best developers now generate and monitor code instead of writing it. Boris Cherny created Claude Code around the terminal-first shift.

My use case is different. I work in payments.

**Post 2**

Payment repos sit near merchant config, credentials, signing material, internal endpoints and environment-specific values.

I use agents heavily. I still edit `.env` values myself. I do not trust an agent to rewrite them because it decided that was the easiest fix.

**Post 3**

Most AI editors optimize for giving the agent more context and autonomy. I needed explicit boundaries: show it only the code it needs, allow only reviewed commands, and keep sensitive or denied files outside its workspace.

Some changes should stay human-only.

**Post 4**

So I built RepoLatch: a provider-independent execution boundary and small workbench for terminal agents.

Native when I want speed. Docker when I need enforced isolation. File tree, guarded editing, policy, diffs and receipts for review.

**Post 5**

Not a Zed, Cursor or VS Code replacement.

It is for: “I don't use an IDE anymore, but I still need to control what my agent can touch.”

The agent writes. I control secrets, policy and what ships.

Open source: https://github.com/PuvaanRaaj/repolatch

## LinkedIn launch draft

The “I don't use an IDE anymore” workflow is no longer a fringe idea.

Spotify co-CEO Gustav Söderström said some of Spotify's best developers had not written a line of code by hand for months—they were generating code and monitoring it instead. Spotify Engineering has described teams moving from IDEs to terminals and background agents. Boris Cherny, the inventor of Claude Code, helped make the terminal itself a serious development environment.

I understand the shift. My coding agent has also become the primary place where implementation happens. But my use case has an extra constraint: I work in payments.

A payment repository can sit close to merchant configuration, credentials, signing material, internal endpoints, and environment-specific values. I may trust an agent to change validation logic, write tests, or refactor a service. I do not automatically trust it to read or rewrite `.env` values because it decided changing the environment was the easiest way to make a test pass.

I still edit those values myself.

Some changes should remain human-only.

Most AI editors are designed to give the agent more context and more autonomy. I needed a way to make the opposite choice: show an agent only the files it needs, allow only reviewed commands, keep denied content outside its workspace, choose whether network access is available, and inspect exactly what changed afterward.

That is why I built RepoLatch.

It is a local, provider-independent execution boundary for terminal coding agents, with a deliberately small desktop workbench. It creates a policy-filtered workspace, supports fast native execution with advisory host controls or Docker-backed isolation, and provides file inspection, guarded editing, diffs, capability status, and metadata-only receipts.

It does not provide a model and it is not trying to replace Zed, Cursor, or VS Code feature for feature. Those are full editors with agent workflows. This project is for a more specific workflow:

“I don't use an IDE anymore, but I still need to control what my agent can touch.”

The agent can write most of the code. I remain responsible for secrets, policy, review, and what reaches production.

The principle is simple: keep the agent, lose the IDE when you do not need it, and keep the boundaries.

RepoLatch is open source: <https://github.com/PuvaanRaaj/repolatch>

## Source map

- Zed external agents and ACP: <https://zed.dev/docs/ai/external-agents> and <https://zed.dev/acp>
- Zed agent workflows: <https://zed.dev/docs/ai/agents>
- VS Code agents: <https://code.visualstudio.com/docs/agents/overview>
- VS Code Workspace Trust: <https://code.visualstudio.com/docs/editing/workspaces/workspace-trust>
- Cursor CLI and permissions: <https://docs.cursor.com/en/cli/overview> and <https://docs.cursor.com/cli/reference/permissions>
- Cursor background agents: <https://docs.cursor.com/background-agent>
- Spotify's co-CEO on agent-generated code: <https://techcrunch.com/2026/02/12/spotify-says-its-best-developers-havent-written-a-line-of-code-since-december-thanks-to-ai/>
- Spotify Engineering on agentic-first development: <https://engineering.atspotify.com/2026/4/anthropic-agentic-development>
- Anthropic's profile of Boris Cherny as the inventor of Claude Code: <https://www.anthropic.com/webinars/claude-code-service-delivery>
- Anthropic API-key guidance on environment variables and third-party tools: <https://support.anthropic.com/en/articles/9767949-api-key-best-practices-keeping-your-keys-safe-and-secure>
