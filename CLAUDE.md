# CLAUDE.md

This repository implements `FluxGuard`, a resource awareness layer for AI coding agents.

These instructions are authoritative for coding agents working in this repository.

## Objective

Build a reliable, provider-agnostic MCP server that:

1. reads resource and quota signals from supported sources,
2. normalizes them into a stable domain model,
3. determines current resource pressure,
4. returns concise execution advice to the calling agent.

The Codex vertical slice is complete, and Codex, OpenCode, and GitHub Copilot
read real quota today. Every other adapter in the matrix is detection only: it
probes and reports, but its refresh returns `source_unsupported` because no
verified machine-readable quota surface exists for it yet.

Do not turn a detection-only adapter into one that appears to work. Wire a real
fetch, or leave it reporting unsupported.

## Architecture constraints

Always preserve these boundaries:

```text
MCP transport
    |
Application services
    |
Domain model + policy
    |
Source abstraction
    |
Client adapters / Provider adapters
```

The core domain must not import provider-specific types.

Provider-specific JSON belongs inside the adapter that owns it.

Never let Codex-specific names leak into generic types such as `BudgetSnapshot`, `BudgetWindow`, or `ExecutionAdvice`.

## Client vs provider

Treat these as different concepts.

Client/harness examples:

- Codex
- Claude Code
- Cursor
- Antigravity
- GitHub Copilot
- OpenCode

Provider examples:

- OpenAI
- Anthropic
- xAI
- Z.AI
- Google

Do not create one generic `Provider` enum that mixes both groups.

## Source policy

Prefer data sources in this order:

1. documented official structured API,
2. documented official SDK,
3. documented official CLI machine-readable output,
4. documented response headers,
5. documented local telemetry,
6. local estimation from data the user already owns,
7. manual configuration.

Do not depend by default on:

- reverse-engineered private endpoints,
- browser cookie extraction,
- scraping dashboards,
- parsing interactive TUIs,
- undocumented credential stores,
- replaying product OAuth tokens against unofficial endpoints.

Experimental adapters may exist only behind an explicit feature flag and must clearly report `source_quality = experimental`.

## Security rules

Never log:

- API keys,
- OAuth access tokens,
- refresh tokens,
- cookies,
- authorization headers,
- complete provider responses that may contain secrets.

Redact secrets before structured logs.

Do not upload usage data anywhere in v0.1.

Default operation is local-only.

## MCP rules

Keep the tool count small.

The public tool contract for v0.1 is:

- `resource_status`
- `resource_advice`
- `resource_refresh`

Do not add provider-specific tools such as `get_codex_usage` unless an ADR changes this decision.

Return structured content suitable for machine reasoning.

Avoid verbose prose in tool results.

## Pressure semantics

Never infer that an unavailable quota has recovered simply because its reset timestamp has passed.

Unknown data stays unknown until a source confirms it.

Never convert a missing limit into `100% remaining`.

Never average unrelated limits.

The overall bottleneck is chosen from applicable hard constraints using policy rules, not arithmetic averaging.

## Freshness

Every observation must have:

- `observed_at`
- `fresh_until` or a TTL derivation
- `source_quality`

Stale data must be labeled stale.

`resource_advice` must degrade confidence when advice is based on stale or estimated data.

## Errors

Adapter failures must be isolated.

One failed source must not crash the MCP server.

Return partial snapshots when useful.

Distinguish:

- unavailable,
- unauthenticated,
- unsupported,
- stale,
- transient failure,
- rate limited,
- malformed upstream response.

## Rust expectations

Use stable Rust.

Prefer:

- explicit domain types,
- `Result` with typed errors,
- small modules,
- `tokio::sync::watch` for latest-state publication,
- cancellation-aware background tasks,
- graceful child process shutdown,
- deterministic unit tests.

Avoid:

- global mutable state,
- blocking subprocess I/O inside async tasks,
- `unwrap()` in production paths,
- provider JSON stored as untyped `Value` beyond adapter boundaries,
- premature microservices,
- a database in v0.1.

## Test expectations

Every new adapter needs:

- fixture-based parsing tests,
- unavailable/auth failure tests,
- stale data behavior tests,
- malformed payload tests.

Every policy change needs table-driven tests.

Every MCP tool needs schema and response contract tests.

## Implementation order

Keep public behavior aligned with the contracts in `docs/`.
`docs/12-CODEX-IMPLEMENTATION-PLAN.md` records how the first slice was built and
remains the reference for how a source is brought up.

A new source is finished only when it reads a documented surface. Until then it
belongs in the matrix as detection only, and `docs/07-PROVIDER-MATRIX.md` must
say so.

## Before finishing a change

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

If one cannot run, report exactly why.

Update documentation when public behavior changes. The per-crate and `npm/`
READMEs are byte-identical copies of the root one, so they move together.
`AGENTS.md` is a symlink to this file.

## Interactive commands

`install`, and `config` without a subcommand, prompt on a terminal. Each must:

- refuse rather than guess when stdin or stdout is not a terminal;
- keep a non-interactive path (`--client`, `--dry-run`, environment overrides)
  so automation never needs a TTY;
- degrade to plain text when `NO_COLOR` is set or output is redirected.

The brand mark in `crates/fluxguard/src/theme.rs` is sampled from
`assets/brand/logo-mark.svg`; regenerate it with `sh tests/logo.sh` and verify
with `sh tests/logo.sh --check` rather than hand-editing the grid.

## Definition of done

A task is done only when:

- implementation exists,
- tests cover important paths,
- errors are typed,
- no secret is logged,
- docs match behavior,
- formatting/lint/tests pass.
