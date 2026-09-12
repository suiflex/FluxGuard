# Codex Implementation Plan

This document is written for Codex implementing the repository.

## Phase 1: scaffold only

Create the workspace and crates.

Do not write provider adapters yet.

Compile an empty binary and establish lint/test CI.

Expected root workspace:

```toml
[workspace]
resolver = "2"
members = [
  "crates/fluxguard-core",
  "crates/fluxguard-runtime",
  "crates/fluxguard-adapters",
  "crates/fluxguard-mcp",
  "crates/fluxguard",
]
```

Use workspace dependency versions to avoid drift.

## Phase 2: core model

Implement all model types first.

Keep `fluxguard-core` mostly synchronous/pure.

Do not add Tokio unless a type truly requires it.

Add:

```text
budget.rs
source.rs
pressure.rs
operation.rs
policy.rs
error.rs
```

Write tests before runtime integration.

## Phase 3: pressure engine

Implement deterministic functions:

```rust
fn assess_snapshot(snapshot: &CombinedSnapshot, cfg: &PressureConfig)
    -> PressureAssessment;

fn advise(
    pressure: &PressureAssessment,
    operation: &OperationProfile,
    cfg: &PolicyConfig,
) -> ExecutionAdvice;
```

No network calls here.

## Phase 4: runtime

Implement source registry and latest-state publication.

Recommended structures:

```rust
struct SourceRuntime {
    sources: HashMap<SourceId, Arc<dyn BudgetSource>>,
    states: HashMap<SourceId, watch::Receiver<SourceState>>,
}
```

Use `CancellationToken` from `tokio-util`.

Add bounded refresh timeout.

## Phase 5: fake adapter

Before Codex integration, build `StaticSource` or `FixtureSource`.

Use it to verify:

```text
source -> runtime -> pressure -> MCP
```

This proves the vertical slice without external dependencies.

## Phase 6: Codex App Server transport

Build a small JSON-RPC client specialized for the documented Codex App Server surface.

Do not create a generic giant JSON-RPC framework.

Responsibilities:

```text
spawn child
write request
assign id
match responses
dispatch notifications
handle stderr
detect exit
cancel
restart
```

Suggested internal channels:

```text
request callers
    |
mpsc<Request>
    |
writer task -> child stdin

child stdout -> reader task
    |
    +-> pending response map
    +-> notification channel
```

Bound message size.

Do not deadlock if stderr fills.

## Phase 7: Codex adapter

Map App Server rate-limit structures to generic `BudgetWindow`.

Preserve multiple limit ids.

Examples:

```text
primary window
secondary window
rateLimitsByLimitId["codex"]
```

If both old and multi-bucket fields exist, avoid double-counting the same logical limit.

Prefer the richer documented representation and retain compatibility fallback.

Map backend ordinary usage permission to availability.

## Phase 8: MCP

Use `rmcp`.

Expose three tools only.

Start with stdio.

The compact `resource_status` path should not trigger a provider refresh every call. It should read cached fresh state.

If state is stale, output stale status and optionally recommend refresh.

`resource_refresh` performs the explicit refresh.

## Phase 9: CLI

Commands:

```bash
fluxguard serve
fluxguard status --json
fluxguard advice spawn-parallel-subagents --json
fluxguard sources
fluxguard doctor
```

`serve` starts MCP stdio.

No background daemon required yet.

## Phase 10: config

Suggested file:

```text
~/.config/fluxguard/config.toml
```

Support platform-appropriate config directory through a crate such as `directories`.

Example:

```toml
[pressure]
guarded_remaining_percent = 50
conserve_remaining_percent = 25
critical_remaining_percent = 10
emergency_remaining_percent = 3

[clients.codex]
enabled = true
command = "codex"
refresh_interval_seconds = 60
```

Validate:

```text
100 >= guarded >= conserve >= critical >= emergency >= 0
```

## Phase 11: release hardening

Before adding provider number two:

- cross-platform test,
- graceful process cleanup,
- sanitized logs,
- fixture compatibility,
- README install instructions,
- example MCP configs for Codex/Cursor/OpenCode where supported.

## Do not implement yet

- browser dashboard scraper,
- generic OAuth credential reader,
- HTTP reverse proxy,
- central telemetry server,
- database,
- web UI,
- auto-switching user models,
- automatic purchase/credit spending,
- private provider endpoints.

## Completion report format

When finishing each phase, report:

```text
Implemented:
Tests:
Commands run:
Known limitations:
Next task:
```
