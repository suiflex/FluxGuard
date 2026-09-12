# Client Integrations

This file describes how agent clients can consume `FluxGuard`.

The quota source and the client consuming MCP do not have to be the same product.

## Generic MCP integration

Minimum integration:

```text
Client
  -> launches fluxguard as local MCP stdio server
  -> agent calls resource_status/resource_advice
```

This works for any client with local MCP support.

## Hook-compatible advice

Clients with lifecycle hooks can invoke the CLI before expensive work:

```bash
fluxguard hook spawn_parallel_subagents --importance optional --json
```

The command is advisory. It returns policy output and does not execute or block
the client operation.

## OMP, Hermes, OpenClaw, and 9router

These are harness or routing surfaces, not quota authorities. They can consume
FluxGuard through the same local MCP stdio server:

```text
OMP / Hermes / OpenClaw / 9router
    -> FluxGuard MCP stdio
    -> documented client or provider source
```

The advisory preflight packages are under `plugins/<client>/preflight.sh`.
They call `fluxguard hook` and never read or forward credentials.

Subscription coverage remains source-specific:

- Codex subscription quota uses Codex App Server.
- Claude subscription quota remains unknown/manual without a stable official
  machine-readable surface.
- Antigravity quota remains unknown/manual; its interactive quota panel is not
  scraped.

## Codex

Initial target.

Integration:

```text
Codex Agent
   |
   | MCP stdio
   v
FluxGuard
   |
   | JSON-RPC
   v
codex app-server
```

Repository `CLAUDE.md` should instruct Codex to call `resource_advice` before clearly expensive optional work.

Later, deeper Codex integration may use App Server events if an external harness owns the full agent loop.

## Claude Code

Claude Code supports MCP and lifecycle customization, but subscription quota access must use a documented stable surface.

Do not make a default adapter depend on unofficial OAuth usage endpoints.

A Claude Code integration can still benefit from:

- MCP policy calls,
- context-window data if officially exposed to hooks/status,
- provider API budgets when Claude Code is configured with an API provider,
- manual budgets.

## Cursor

Cursor supports local/remote MCP.

Cursor also supports agent hooks including:

- session lifecycle,
- pre/post tool use,
- subagent start/stop,
- before/after MCP execution,
- pre-compact,
- after agent response/thought.

This makes Cursor a strong candidate for future host-assisted enforcement even when exact subscription allowance is unavailable programmatically.

Possible future flow:

```text
Cursor preToolUse/subagentStart hook
   |
   v
fluxguard CLI/local socket
   |
   v
policy engine
   |
   v
allow / advise / block
```

Blocking behavior must be opt-in.

## Google Antigravity & Gemini Ecosystem

Antigravity spans multiple developer surfaces across Google's AI developer platform:

1. **Antigravity CLI (`agy`)**: Lightweight terminal interface for agent interaction, slash commands, and background tasks. Configured in `~/.gemini/antigravity-cli/settings.json`.
2. **Antigravity IDE**: Standalone AI-first IDE built on a VS Code fork with inline code lenses, tab completions, and `.agents/` workspace customizations.
3. **Antigravity 2.0 Desktop App**: Standalone Electron application for parallel agent orchestration, auxiliary panes, and scheduled tasks. Uses `~/.gemini/antigravity/mcp_config.json`.
4. **Gemini CLI (`gemini`)**: Developer CLI for direct Gemini interactions, configured in `~/.gemini/`.
5. **Google & Gemini Credentials**: Supports `GEMINI_API_KEY`, `GOOGLE_API_KEY`, `GOOGLE_GENAI_API_KEY`, Vertex AI credentials, and Google OAuth profiles in `~/.gemini/google_accounts.json`.

FluxGuard automatically detects these surfaces during `fluxguard doctor` and probes quota windows (5-hour and weekly quotas, plus model-level rate limits). Advisory advice is consumed via local MCP stdio or skill/hook workflows without scraping interactive TUIs.

## GitHub Copilot

GitHub's Copilot SDK exposes account quota information through `account.getQuota`, including remaining percentage and reset date.

Investigate whether the Rust process can consume that supported SDK surface directly or through a small companion process without compromising packaging.

This is a strong candidate for the second exact-quota adapter.

## OpenCode

OpenCode supports MCP and has programmatic CLI/server surfaces.

Current useful local surface includes session statistics and JSON output.

This makes OpenCode suitable for:

- client-side local token/cost observation,
- MCP consumption,
- provider-aware integrations through its configured provider catalog.

Do not treat OpenCode session statistics as provider account entitlement unless the upstream provider confirms that equivalence.

## Client instruction template

For clients that read `CLAUDE.md`, include behavior such as:

```text
Before starting expensive optional work, call resource_advice.
Check before parallel subagents, broad repository exploration, large refactors,
or full test matrices. When pressure is critical, prioritize completing the
current objective and use targeted verification.
```

Keep this instruction small.

The MCP tool itself should carry the current facts.
