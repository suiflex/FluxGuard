# FluxGuard

`FluxGuard` is a provider-agnostic resource awareness layer for AI coding agents.

Its job is simple:

1. discover available resource/usage signals,
2. normalize them into one stable model,
3. calculate resource pressure,
4. advise the current agent how aggressively it should work.

The first implementation target is Codex. The architecture must also support Claude Code, Cursor, Google Antigravity, GitHub Copilot, OpenCode, xAI/Grok, Z.AI/GLM, and future clients/providers without changing the core model.

## Why this exists

Coding agents can make expensive decisions without knowing that an account, context window, request quota, credit balance, or time budget is close to exhaustion.

Examples:

- spawning several subagents with only 8% weekly quota remaining,
- starting a broad repository refactor just before a hard quota reset boundary,
- running a full test matrix when targeted tests would be enough,
- consuming a nearly-full context window with broad exploration,
- continuing optional cleanup after the current objective is already solved.

`FluxGuard` gives the agent a compact resource signal before those decisions.

## Core idea

Do not expose only raw usage percentages.

Expose a normalized decision signal:

```json
{
  "pressure": "critical",
  "bottleneck": "codex.weekly",
  "effective_remaining_percent": 8,
  "confidence": "high",
  "recommended_mode": "completion_first"
}
```

The agent can then choose a cheaper execution strategy.

## Architecture in one picture

```text
                         +----------------------+
                         |     Coding Agent     |
                         | Codex / Cursor / ... |
                         +----------+-----------+
                                    |
                                    | MCP
                                    v
+------------------------------------------------------------------+
|                         fluxguard                              |
|                                                                  |
|  MCP API -> Snapshot Service -> Pressure Engine -> Policy Engine  |
|                    ^                         ^                    |
|                    |                         |                    |
|             Source Registry          Operation Profile            |
|                    |                                              |
|       +------------+--------------+                               |
|       |                           |                               |
|  Client Sources               Provider Sources                    |
|  Codex                        OpenAI API                            |
|  Claude Code                  Anthropic API                        |
|  Cursor                       xAI                                  |
|  Antigravity                  Z.AI / GLM                           |
|  Copilot                      other providers                      |
|  OpenCode                                                          |
+------------------------------------------------------------------+
```

## Important terminology

A client and a provider are not the same thing.

Examples of clients or agent harnesses:

- Codex
- Claude Code
- Cursor
- Google Antigravity
- GitHub Copilot
- OpenCode

Examples of providers:

- OpenAI
- Anthropic
- xAI
- Z.AI
- Google
- other OpenAI-compatible or Anthropic-compatible APIs

One client may use several providers. One provider may be used from several clients. The architecture keeps these concepts separate.

## Initial stack

- Rust
- Tokio
- official `rmcp` Rust SDK
- Serde / serde_json
- tracing
- thiserror
- time
- clap
- figment or config for configuration

Use MCP `2026-07-28` where the client supports it and rely on protocol negotiation for older clients.

## Initial MCP surface

Keep the tool surface intentionally small.

### `resource_status`

Returns a compact normalized snapshot.

### `resource_advice`

Evaluates a planned operation against the current resource pressure.

### `resource_refresh`

Forces refresh of one or more sources when stale data is unacceptable.

Detailed data should be exposed as MCP resources, not as a large number of tools.

Suggested resources:

- `fluxguard://status/full`
- `fluxguard://sources`
- `fluxguard://diagnostics`

## Version 0.1

Version 0.1 is intentionally narrow:

- local stdio MCP server,
- Codex client adapter,
- normalized budget model,
- pressure engine,
- policy engine,
- configuration,
- tests,
- structured diagnostics.

Do not implement every provider in the first milestone.

## Install

The published user-facing package is the `fluxguard` CLI. Workspace libraries
remain separate crates so adapters and the MCP server keep their boundaries,
but users install one binary:

```bash
cargo install fluxguard
```

For a local checkout:

```bash
cargo install --path crates/fluxguard
```

The npm distribution is a launcher package for environments that standardize
on npm:

```bash
npm install --global @suiflex/fluxguard
```

It resolves `fluxguard` from `PATH` or `FLUXGUARD_BIN`; the Rust crate remains
the runtime that provides the actual MCP server.

Publishing uses dependency order. Run a dry check for every package first,
then publish `fluxguard-core`, `fluxguard-runtime`, `fluxguard-adapters`,
`fluxguard-mcp`, and finally `fluxguard`. The versioned `path` dependencies in
each manifest keep local workspace builds and crates.io packages compatible.

### Connect FluxGuard to a client

The interactive installer writes a merge-safe MCP entry and creates a `.bak`
backup before changing an existing JSON configuration:

```bash
fluxguard install
```

Use a non-interactive target when scripting:

```bash
fluxguard install --client claude-code
fluxguard install --client cursor
fluxguard install --client opencode
fluxguard install --client antigravity
fluxguard install --client openclaw
fluxguard install --client codex
```

Preview without writing:

```bash
fluxguard install --client claude-code --print --dry-run
```

OMP, Hermes, and 9router receive a portable stdio snippet because their
configuration ownership is harness-specific. They should launch `fluxguard
serve`; FluxGuard never reads or forwards their credentials.

## Read next

Codex should read these files in order:

1. `CLAUDE.md`
2. `docs/01-ARCHITECTURE.md`
3. `docs/02-DOMAIN-MODEL.md`
4. `docs/03-MCP-CONTRACT.md`
5. `docs/04-POLICY-ENGINE.md`
6. `docs/05-SOURCE-ADAPTERS.md`
7. `docs/12-CODEX-IMPLEMENTATION-PLAN.md`
8. `docs/07-PROVIDER-MATRIX.md` and `docs/14-RESEARCH-NOTES.md`

Provider research is in `docs/07-PROVIDER-MATRIX.md` and
`docs/14-RESEARCH-NOTES.md`.
