# Research Notes

Verified 2026-09-12.

These notes capture public surfaces relevant to architecture. Re-verify before implementing an adapter because provider contracts change.

## MCP specification

The current MCP release is `2026-07-28`.

The official Rust SDK `rmcp` implements the stable revision and uses Tokio.

References:

- https://blog.modelcontextprotocol.io/posts/2026-07-28/
- https://github.com/modelcontextprotocol/rust-sdk
- https://rust.sdk.modelcontextprotocol.io/

## OpenAI Codex

Codex App Server is the preferred integration surface for deeper Codex client integrations.

The public App Server schema includes `GetAccountRateLimitsResponse` and rate-limit windows with `usedPercent`, reset timestamp, and duration. It also includes multi-bucket structures such as `rateLimitsByLimitId`.

Reference:

- https://github.com/openai/codex/blob/main/codex-rs/app-server-protocol/schema/json/v2/GetAccountRateLimitsResponse.json
- https://openai.com/index/unlocking-the-codex-harness/

Important limitation:

An open issue documents that enterprise current-month consumed credits shown in product UI may not be available through App Server. Absence must remain unknown.

Reference:

- https://github.com/openai/codex/issues/35592

## GitHub Copilot

Copilot SDK documents `account.getQuota`.

Quota snapshots expose fields including:

- entitlement requests,
- used requests,
- remaining percentage,
- reset date.

Reference:

- https://docs.github.com/en/copilot/how-tos/copilot-sdk/features/usage-and-billing

This is a strong candidate for a structured adapter.

## Cursor

Cursor supports MCP and rich hooks.

MCP:

- https://cursor.com/docs/mcp
- https://cursor.com/docs/cli/mcp

Hooks:

- https://cursor.com/docs/hooks

Usage:

Cursor documents real-time usage through the Spending dashboard and monthly pools.

Reference:

- https://cursor.com/help/models-and-usage/usage-limits

No stable public exact quota API is assumed by this design.

Cursor context usage is visible in product UI and can be treated separately from subscription quota if a documented programmatic hook/metadata field exists.

## Google Antigravity

Antigravity supports MCP.

Reference:

- https://antigravity.google/docs/mcp

Plans document five-hour and weekly quota concepts.

Reference:

- https://antigravity.google/docs/plans

Antigravity CLI exposes `/usage` or `/quota` interactive model quota view.

Reference:

- https://antigravity.google/docs/cli/commands/usage

Do not parse the interactive TUI in the default adapter.

Antigravity terms explicitly warn against using Antigravity login through third-party software. The intended project integration is as an MCP server consumed by Antigravity, not reusing its credentials in other clients.

Reference:

- https://antigravity.google/docs/faq

## Claude / Anthropic

Claude models can be context-aware regarding their context token budget, but account subscription quota is a separate concept.

Anthropic documents Claude Code usage limits and product UI, while community/issue references show subscription quota is not a stable generic statusline payload to depend on.

Do not make unofficial OAuth usage endpoints a default integration.

For the Anthropic API, standard API rate-limit/error handling can be a separate provider adapter.

Reference starting points:

- https://docs.anthropic.com/
- https://github.com/anthropics/claude-code

## OpenCode

OpenCode supports many providers and MCP.

References:

- https://opencode.ai/docs/providers
- https://opencode.ai/docs/mcp-servers/

OpenCode CLI supports usage/cost statistics. Newer CLI generation documents JSON stats.

References:

- https://opencode.ai/docs/cli/
- https://opencode.ai/v2/docs/cli/commands/

Treat stats as local/session telemetry, not provider entitlement unless confirmed.

## xAI / Grok

xAI documents per-model rate limits such as RPS and TPM and returns 429 when limits are exceeded.

Reference:

- https://docs.x.ai/developers/rate-limits

This is suitable for:

- static limit metadata,
- local rolling estimation if every request is observed,
- reactive rate-limit error state.

Do not claim exact remaining account quota without an official remaining-quota surface.

## Z.AI / GLM

Z.AI Coding Plan documents five-hour and weekly quota behavior.

Reference:

- https://docs.z.ai/devpack/overview
- https://docs.z.ai/devpack/usage-policy

Z.AI also documents an official usage-query plugin for Claude Code.

Reference:

- https://docs.z.ai/devpack/extension/usage-query-plugin

Coding Plan usage is restricted to supported tools/scenarios.

Reference:

- https://docs.z.ai/devpack/usage-policy
- https://docs.z.ai/legal-agreement/subscription-terms

Error documentation includes exhausted-limit conditions and reset time.

Reference:

- https://docs.z.ai/api-reference/api-code

Do not repurpose Coding Plan credentials as a general quota-proxy service.
