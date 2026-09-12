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

## Commands

| Command | What it does |
| --- | --- |
| `fluxguard serve` | Serve MCP over stdio — the mode a client launches |
| `fluxguard serve-http --listen 127.0.0.1:8080` | Serve MCP over HTTP |
| `fluxguard daemon` | Run the source supervisor in the foreground |
| `fluxguard status [--json]` | Current combined pressure |
| `fluxguard advice <operation> [--importance …] [--json]` | Advice for one operation (alias: `fluxguard hook`) |
| `fluxguard sources` | Registered sources and their state |
| `fluxguard doctor` | Probe every configured source and say why one is unavailable |
| `fluxguard config [check\|path] [--dry-run]` | Edit, validate, or locate the configuration |
| `fluxguard install [--client …]` | Register FluxGuard with a harness |
| `fluxguard update [--check] [--json]` | Check for and install a newer release |

`<operation>` is one of `inspect_targeted`, `search_broad`, `edit_small`,
`refactor_large`, `test_targeted`, `test_full`, `spawn_subagent`,
`spawn_parallel_subagents`, `research_external`, `generate_artifacts`,
`checkpoint`, `finalize`. Underscores, not hyphens: an unrecognized name is
accepted as a custom operation whose cost is `unknown`, which weakens the advice
rather than failing. `--importance` is `required`, `useful`, or `optional`
(default `optional`).

```bash
fluxguard advice spawn_parallel_subagents --importance useful --json
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

| Document | What it covers |
| --- | --- |
| [Vision](docs/00-VISION.md) | What FluxGuard is for, and what it refuses to become |
| [Architecture](docs/01-ARCHITECTURE.md) | Layer boundaries and the crate map |
| [Domain Model](docs/02-DOMAIN-MODEL.md) | `BudgetSnapshot`, windows, freshness, provenance |
| [MCP Contract](docs/03-MCP-CONTRACT.md) | The three tools and their response shapes |
| [Policy Engine](docs/04-POLICY-ENGINE.md) | How pressure becomes advice |
| [Source Adapters](docs/05-SOURCE-ADAPTERS.md) | The adapter contract and its failure modes |
| [Client Integrations](docs/06-CLIENT-INTEGRATIONS.md) | Per-harness wiring |
| [Provider Matrix](docs/07-PROVIDER-MATRIX.md) | What each source can actually read today |
| [Security & Privacy](docs/08-SECURITY-PRIVACY.md) | What is never logged or uploaded |
| [Observability](docs/09-OBSERVABILITY.md) | Diagnostics and redaction |
| [Testing](docs/10-TESTING.md) | What every adapter and policy change must cover |
| [Roadmap](docs/11-ROADMAP.md) | Sequence of work |
| [Codex Implementation Plan](docs/12-CODEX-IMPLEMENTATION-PLAN.md) | The first vertical slice, phase by phase |
| [Non-Goals](docs/13-NON-GOALS.md) | Deliberate exclusions |
| [Research Notes](docs/14-RESEARCH-NOTES.md) | Source investigation notes |
| [ADRs](docs/adr/) | Decisions and their reasoning |

Contributing, including how to add a source: [CONTRIBUTING.md](CONTRIBUTING.md).
Agent-facing rules for this repository: [CLAUDE.md](CLAUDE.md) (`AGENTS.md` is a
symlink to it).

## License

Apache-2.0
