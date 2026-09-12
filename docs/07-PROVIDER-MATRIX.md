# Provider and Client Matrix

Verified against public documentation on 2026-09-12.

This matrix is intentionally conservative. "Exact quota adapter" means a stable documented machine-readable surface suitable for this project, not merely a dashboard that a human can view.

## Clients / agent harnesses

| Client | MCP support | Public machine-readable usage surface | Initial support tier | FluxGuard Status | Notes |
|---|---|---|---|---|---|
| OpenAI Codex | Yes | Yes, Codex App Server `account/rateLimits/read` | A | Implemented | Best first adapter. Structured rate-limit windows and related account state are exposed by App Server. |
| GitHub Copilot | Ecosystem-dependent | Yes via Copilot CLI headless JSON-RPC `account.getQuota` | A/B | Implemented | Spawns `copilot --headless --stdio` and reads the documented SDK quota snapshots (entitlement, used, remaining percentage, reset date). |
| OpenCode | Yes | Yes for local session stats via CLI/server JSON | B | Implemented | Session stats and usage counters via CLI JSON output. |
| Cursor | Yes | No public exact subscription quota API found | B/C | Detection only | Adapter detects the install and reports `source_unsupported`; no quota data is read until a stable surface exists. |
| Google Antigravity | Yes | Interactive `/usage` / `/quota`; no stable JSON quota surface confirmed | B/C | Detection only | Adapter detects CLI/IDE/env surfaces and reports `source_unsupported`; TUI is not scraped. |
| Claude Code | Yes | Interactive usage exists; no supported generic quota payload confirmed | B/C | Detection only | Adapter detects the CLI and reports `source_unsupported`; unofficial OAuth endpoints are not used. |
| OMP | Via local MCP | No independent quota authority | C | Advisory Preflight | Harness consumes FluxGuard advice via MCP stdio or preflight hook. |
| Hermes | Via local MCP when configured | No independent quota authority | C | Advisory Preflight | Harness consumer; keep source identity separate from model provider. |
| OpenClaw | Via local MCP when configured | No independent quota authority | C | Advisory Preflight | Harness consumer; use advisory preflight package or MCP directly. |
| 9router | Local MCP through loopback | No quota authority | C | Advisory Preflight | Routing surface; FluxGuard does not inspect or proxy its credentials. |

Tier interpretation:

```text
A   exact or near-exact official structured source is available
B   useful official partial/local telemetry exists
C   MCP consumer integration is viable, quota source is limited
D   manual/experimental only
```

## Providers

| Provider | Exact remaining allowance | Rate limit metadata | Local estimation potential | FluxGuard Status | Initial approach |
|---|---|---|---|---|---|
| OpenAI API | Rate-limit headers exist per request; no standalone remaining-quota call assumed | Yes (RPM, TPM) | High if observed | Detection only | Adapter checks `OPENAI_API_KEY` and reports `source_unsupported` until header observation is wired |
| Anthropic API | Rate-limit headers (RPM, ITPM, OTPM) per request | Yes | High if observed | Detection only | Adapter checks `ANTHROPIC_API_KEY` and reports `source_unsupported` until header observation is wired |
| xAI API / Grok | Console documents per-model RPS/TPM limits; exact remaining quota API not confirmed | Limits and 429 behavior | High if observed | Detection only | Adapter checks `XAI_API_KEY` and reports `source_unsupported` |
| Z.AI / GLM Coding Plan | Usage statistics and official usage-query tooling exist | Plan errors include reset info | High | Detection only | Adapter checks `ZAI_API_KEY`/`GLM_API_KEY` and reports `source_unsupported` until the official usage surface is wired |
| OpenAI/Codex subscription | Yes through Codex App Server for account quota | Yes | High | Implemented | Codex client adapter |
| Anthropic Claude subscription | Human-visible usage, stable generic external quota API not assumed | Partial | Medium | Telemetry | Claude Code client adapter |
| Cursor-managed model pools | Dashboard shows real-time pool usage | Not confirmed as public programmatic quota API | Medium | Detection only | Cursor client adapter |
| OpenCode Console | Local OpenCode stats available | Partial | High for local sessions | Implemented | OpenCode adapter |

## Current source details

### Codex

Public Codex App Server schemas expose:

- rate limit snapshots,
- `usedPercent`,
- reset timestamp,
- window duration,
- multi-bucket `rateLimitsByLimitId`,
- account-level availability-related fields,
- credit/spend-related structures depending on plan/backend.

Caveat:

Current-month enterprise consumed credits may not be exposed by App Server even when the desktop UI can show them. Treat absent fields as unavailable.

### GitHub Copilot

The Copilot CLI headless server (`copilot --headless --stdio`) exposes the SDK's `account.getQuota` JSON-RPC method. FluxGuard calls it directly and maps each entry of `quotaSnapshots` (for example `premium_interactions`, `chat`, `completions`) to one window. Fields used:

- `entitlementRequests` (`-1` or `isUnlimitedEntitlement` marks the window not applicable),
- `usedRequests`,
- `remainingPercentage`,
- `resetDate`,
- `usageAllowedWithExhaustedQuota` / `overageAllowedWithExhaustedQuota` (an exhausted quota is only `hard_blocked` when neither permits further use).

Authentication comes from the CLI's own login or `COPILOT_GITHUB_TOKEN` / `GH_TOKEN` / `GITHUB_TOKEN`; FluxGuard never reads those values itself and discards the CLI's stderr.

### Cursor

Cursor's documented spending view shows real-time usage of its model pools.

Cursor's hooks provide agent lifecycle events such as tool calls and subagents.

No public stable exact allowance API is assumed in this blueprint.

### Antigravity

Antigravity documentation exposes:

- five-hour/weekly quota concepts,
- interactive `/usage` or `/quota`,
- MCP support.

Treat the interactive quota panel as human UI until a machine-readable supported contract is documented.

### OpenCode

OpenCode supports:

```text
opencode stats
opencode2 stats --json
```

depending on installed generation/version.

Use version detection.

Session stats are useful local telemetry, not automatically an account quota.

### Z.AI / GLM

Z.AI documents:

- five-hour and weekly coding-plan limits,
- usage statistics,
- an official usage-query plugin for Claude Code,
- plan-specific supported-tool restrictions,
- errors for exhausted limits with reset time.

Important:

The Coding Plan is restricted to supported tools/scenarios.

Do not build an integration that turns Coding Plan credentials into a general-purpose proxy.

### xAI

xAI documents per-model request/token rate limits and 429 behavior.

If a future adapter sees every API request, it can calculate a local rolling estimate.

If requests can occur elsewhere, the adapter must state that the estimate is incomplete.

## Policy implication

The project must support mixed confidence:

```text
Codex quota: exact
Cursor tool activity: exact local telemetry
xAI rolling usage: estimated
company budget: manual
```

The pressure engine should preserve these distinctions rather than pretending every source is equally authoritative.
