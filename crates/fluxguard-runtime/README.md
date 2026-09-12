<img src="https://raw.githubusercontent.com/suiflex/FluxGuard/develop/assets/brand/logo-mark.svg" alt="" width="72" align="left">

# FluxGuard

`FluxGuard` is a provider-agnostic resource awareness layer for AI coding agents.

It measures quota, context window, request rate limits, and budget flow (flux), returning concise execution advice so agents make cost-effective decisions and avoid unexpected quota exhaustion.

> Pre-1.0: review the architecture and supported sources before deploying to production environments.

## Why this exists

Coding agents often make expensive decisions without knowing that an account, context window, request quota, credit balance, or time budget is close to exhaustion.

Examples:

- Spawning multiple concurrent subagents when only 8% weekly quota remains.
- Starting a broad codebase refactor right before a hard quota reset window.
- Running complete test matrices repeatedly when targeted test subsets suffice.
- Consuming nearly the entire context window with unfocused repo searches.
- Continuing optional code cleanups after the user's primary objective is solved.

`FluxGuard` provides agents with a compact, standardized resource signal before every major operation.

## Core idea

Instead of exposing raw usage counters or provider-specific metrics, `FluxGuard` produces a normalized decision signal:

```json
{
  "pressure": "critical",
  "bottleneck": "codex.weekly",
  "effective_remaining_percent": 8,
  "confidence": "high",
  "recommended_mode": "completion_first"
}
```

The calling agent can immediately adapt its plan (e.g. prioritize finishing active tasks, reduce parallel subagents, or bypass non-critical checks).

## Architecture

```text
                         +----------------------+
                         |     Coding Agent     |
                         | Codex / Cursor / ... |
                         +----------+-----------+
                                    |
                                    | MCP
                                    v
+------------------------------------------------------------------+
|                            fluxguard                             |
|                                                                  |
|  MCP API -> Snapshot Service -> Pressure Engine -> Policy Engine |
|                    ^                         ^                   |
|                    |                         |                   |
|             Source Registry          Operation Profile           |
|                    |                                             |
|       +------------+--------------+                              |
|       |                           |                              |
|  Client Sources               Provider Sources                   |
|  Codex                        OpenAI API                         |
|  Claude Code                  Anthropic API                      |
|  Cursor                       xAI                                |
|  Antigravity                  Z.AI / GLM                         |
|  Copilot                      Other providers                    |
|  OpenCode                                                        |
+------------------------------------------------------------------+
```

## Client vs Provider

`FluxGuard` strictly separates clients (harnesses) from providers (backends):

- **Clients / Harnesses**: Codex, Claude Code, Cursor, Google Antigravity, GitHub Copilot, OpenCode.
- **Providers**: OpenAI, Anthropic, xAI, Z.AI, Google, and standard API-compatible endpoints.

One client can use several providers, and one provider can be called from multiple clients. The core domain never couples client telemetry with provider schemas.

## Install

### macOS and Linux (curl)

```sh
curl -fsSL https://raw.githubusercontent.com/suiflex/FluxGuard/develop/scripts/install.sh | sh
```

The script downloads the release binary for your platform, verifies it against `SHA256SUMS`, and installs it to `$HOME/.local/bin`.
Set `FLUXGUARD_VERSION` to pin a release tag or `FLUXGUARD_INSTALL_DIR` to change the destination path.

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/suiflex/FluxGuard/develop/scripts/install.ps1 | iex
```

Installs to `%LOCALAPPDATA%\Programs\FluxGuard\bin` and verifies integrity against `SHA256SUMS`.
Supports `FLUXGUARD_VERSION` and `FLUXGUARD_INSTALL_DIR` overrides.

### Homebrew (macOS & Linux)

```sh
brew install suiflex/tap/fluxguard
```

Or tap once, then install:

```sh
brew tap suiflex/tap
brew install fluxguard
```

### Scoop (Windows)

```powershell
scoop bucket add suiflex https://github.com/suiflex/scoop-bucket
scoop install fluxguard
```

### npm (Global Launcher)

```sh
npm install --global @suiflex/fluxguard
```

Resolves `fluxguard` from `PATH` or `FLUXGUARD_BIN`.

### Cargo

```sh
cargo install fluxguard
```

Or from a local workspace checkout:

```sh
cargo install --path crates/fluxguard
```

## Connect to a Client

FluxGuard provides an interactive installer that writes merge-safe MCP configuration entries with automatic `.bak` backups:

```bash
fluxguard install
```

Or configure non-interactively for specific harnesses:

```bash
fluxguard install --client claude-code
fluxguard install --client cursor
fluxguard install --client opencode
fluxguard install --client antigravity
fluxguard install --client openclaw
fluxguard install --client codex
```

Preview changes without modifying files:

```bash
fluxguard install --client claude-code --print --dry-run
```

For harnesses with standalone stdio configuration (e.g. OMP, Hermes, 9router), run:

```bash
fluxguard serve
```

## MCP Tools & Resources

FluxGuard keeps its public MCP tool surface small, stable, and machine-readable:

- `resource_status`: Returns a normalized snapshot of current resource consumption and active limits.
- `resource_advice`: Evaluates an upcoming operation against current resource pressure and provides execution recommendations.
- `resource_refresh`: Forces a live refresh of one or more upstream telemetry sources.

Detailed diagnostic data is provided through MCP resources:

- `fluxguard://status/full`
- `fluxguard://sources`
- `fluxguard://diagnostics`

## Security & Local Privacy

- **Local-only execution**: Operates on your machine via stdio MCP transport. No telemetry or usage stats are ever sent to remote services.
- **Credential safety**: Redacts API keys, tokens, session cookies, and authorization headers before structured logging.
- **Merge-safe configs**: Preserves existing settings and comments when registering MCP servers with client config files.

## Documentation

For architectural specifications and development details, consult:

1. [Architecture Overview](file:///Users/telkom/Development/github/suiflex/FluxGuard/docs/01-ARCHITECTURE.md)
2. [Domain Model](file:///Users/telkom/Development/github/suiflex/FluxGuard/docs/02-DOMAIN-MODEL.md)
3. [MCP Protocol Contract](file:///Users/telkom/Development/github/suiflex/FluxGuard/docs/03-MCP-CONTRACT.md)
4. [Policy Engine](file:///Users/telkom/Development/github/suiflex/FluxGuard/docs/04-POLICY-ENGINE.md)
5. [Source Adapters](file:///Users/telkom/Development/github/suiflex/FluxGuard/docs/05-SOURCE-ADAPTERS.md)
6. [Codex Implementation Plan](file:///Users/telkom/Development/github/suiflex/FluxGuard/docs/12-CODEX-IMPLEMENTATION-PLAN.md)

## License

Apache-2.0
