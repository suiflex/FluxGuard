# Source Adapters

## Adapter philosophy

Adapters translate external resource information into the generic budget model.

They should not contain execution strategy.

Execution strategy belongs in the policy engine.

## Two adapter families

### Client adapters

Observe a coding agent/harness.

Examples:

- Codex App Server quota,
- Cursor hooks/session context,
- OpenCode session stats,
- Antigravity quota surface.

### Provider adapters

Observe a model/API provider.

Examples:

- xAI rate limits,
- Anthropic API rate limits,
- Z.AI usage,
- OpenAI API budgets.

## Source capability descriptor

Each adapter declares capabilities.

Example:

```json
{
  "supports_snapshot": true,
  "supports_push_updates": true,
  "supports_reset_time": true,
  "supports_exact_remaining_percent": true,
  "supports_model_scope": false,
  "supports_cost": false
}
```

## Discovery

`probe()` should be cheap.

Possible outputs:

```text
ready
binary_missing
not_authenticated
unsupported_version
disabled
partial
```

Do not authenticate a user as a side effect of `probe()`.

## Structured API source

Preferred source.

Typical adapter:

```text
connect
-> request usage metadata
-> deserialize typed payload
-> normalize
-> publish
```

## CLI machine-readable source

Allowed when documented.

Prefer:

```text
tool usage --json
```

Avoid parsing ANSI TUI rendering.

Apply process timeouts.

## Response-header source

Useful for provider API rate limits when documented.

This normally requires one of:

- the MCP server is itself making provider requests,
- the client forwards headers,
- a supported plugin/hook reports headers.

Do not introduce a transparent proxy in v0.1.

## Local telemetry source

Examples:

- session token logs,
- client hook events,
- OpenCode stats.

This can measure local activity but may not equal account-level quota.

The snapshot must describe scope honestly.

## Estimated source

An estimate should declare:

- what traffic it observes,
- what traffic may be missing,
- how the estimate is calculated.

Never label an estimate as exact.

## Manual source

Useful when no programmatic quota API exists.

Example:

```toml
[[sources.manual]]
id = "company.monthly-ai-budget"
dimension = "currency"
limit = 500
used = 410
resets_at = "2026-10-01T00:00:00Z"
```

Manual sources are valid constraints with `source_quality = manual`.

## Adapter contract

Illustrative trait:

```rust
#[async_trait]
pub trait BudgetSource: Send + Sync {
    fn descriptor(&self) -> SourceDescriptor;
    async fn probe(&self) -> Result<ProbeReport, SourceError>;
    async fn refresh(&self) -> Result<BudgetSnapshot, SourceError>;

    async fn run(
        &self,
        updates: watch::Sender<BudgetSnapshot>,
        cancel: CancellationToken,
    ) -> Result<(), SourceError> {
        // optional default polling implementation
    }
}
```

## Source errors

```rust
pub enum SourceError {
    Unavailable,
    Unauthenticated,
    UnsupportedVersion,
    Timeout,
    RateLimited,
    Protocol,
    InvalidPayload,
    ProcessExited,
    PermissionDenied,
    Other,
}
```

Keep sensitive upstream messages out of user-facing errors unless sanitized.

## Codex adapter

Preferred integration:

```text
FluxGuard
    |
    +--> codex app-server
            |
            +--> account/rateLimits/read
            +--> rate-limit updates
```

Important mapping:

- primary window -> independent window,
- secondary window -> independent window,
- `rateLimitsByLimitId` -> preserve each bucket,
- credit/spend data -> separate constraints when present,
- backend availability/permission -> `Availability`.

Do not flatten everything into one `remaining_percent`.

## Process supervision

For adapters that own child processes:

- spawn without a shell,
- pipe stdin/stdout,
- drain stderr safely,
- detect unexpected exit,
- restart with bounded exponential backoff,
- cancel on MCP shutdown,
- kill child after graceful-shutdown timeout.

## Polling

Default metadata polling should be conservative.

Suggested initial range:

```text
30–60 seconds
```

Push-capable sources can refresh less often and use polling as reconciliation.

Never poll faster solely to update a percentage animation.
