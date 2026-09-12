<img src="https://raw.githubusercontent.com/suiflex/FluxGuard/develop/assets/brand/logo-mark.svg" alt="" width="72" align="left">

# FluxGuard

FluxGuard gives coding agents resource awareness: when to explore, when to parallelize, when to conserve, and when to finish.

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

### Staying Up To Date

```sh
fluxguard update          # install the latest release when one exists
fluxguard update --check  # report only, install nothing
fluxguard update --json   # machine-readable result
```

The latest release is read from the repository's tags with `git ls-remote`, so no
API token is involved, and the answer is cached for a day in the platform cache
directory:

| OS | Cache location |
|---|---|
| macOS | `~/Library/Caches/FluxGuard.FluxGuard/update-check.json` |
| Linux | `$XDG_CACHE_HOME/fluxguard/` or `~/.cache/fluxguard/` |
| Windows | `%LOCALAPPDATA%\FluxGuard\FluxGuard\cache\` |

A cache is disposable, so it stays out of the configuration directory — nothing
here is backed up or synced between machines — and falls back to a directory
under the system temp when the platform reports no home. Installing reuses the
platform install script above, so the binary lands where it originally did. When
the check cannot reach the remote it reports `unknown` and exits non-zero rather
than claiming the current version is latest.

## Configure

```sh
fluxguard config          # interactive editor: pick sources, set thresholds
fluxguard config path     # print the file this machine reads
fluxguard config check    # validate the file and the environment overrides
fluxguard config --dry-run # show what the editor would write, write nothing
```

The editor probes every adapter before it asks anything, so each row shows
whether that source is present on this machine, whether enabling it actually
yields quota data, and its probe state. Sources marked `detection only` have no
verified machine-readable quota surface yet and report `source_unsupported`
when refreshed, so the editor never pre-selects one.

Already-enabled sources stay selected; on a first run the editor pre-selects the
sources that are both present and able to read quota. Writing keeps the previous
file beside the new one as `config.toml.bak`, and the whole configuration is
re-validated before anything is written. Hand-written comments do not survive a
rewrite.

Thresholds can also be set by hand or through the environment:

```toml
[pressure]
guarded_remaining_percent   = 50
conserve_remaining_percent  = 25
critical_remaining_percent  = 10
emergency_remaining_percent = 3
```

```sh
FLUXGUARD_PRESSURE_GUARDED_REMAINING_PERCENT=65   # environment wins over the file
```

## Connect to a Client

FluxGuard provides an interactive installer that writes merge-safe MCP configuration entries with automatic `.bak` backups:

```bash
fluxguard install
```

Run without `--client` on a terminal and it shows a menu of supported harnesses,
with the ones already configured on this machine marked and pre-selected. Without
a terminal it exits rather than guessing a target.

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
