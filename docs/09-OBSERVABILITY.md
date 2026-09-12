# Observability

## Goal

Debug adapters without leaking credentials or creating noisy logs.

## Tracing

Use `tracing`.

Suggested spans:

```text
source.probe
source.refresh
source.run
source.reconnect
app_server.request
pressure.assess
policy.advise
mcp.tool
```

Suggested safe fields:

```text
source_id
adapter
operation
duration_ms
result
error_kind
freshness
window_count
pressure_level
retry_count
```

Do not log complete provider payloads at info/debug by default.

## Metrics

No remote metrics exporter in v0.1.

Internal counters can later include:

```text
refresh_success_total
refresh_failure_total
source_reconnect_total
source_stale_total
mcp_call_total
mcp_call_duration
```

## Doctor command

`fluxguard doctor` should provide actionable information.

Example:

```text
client.codex
  binary: found
  app-server: reachable
  auth: delegated to Codex
  quota surface: supported
  last refresh: 4s ago
  state: ready
```

Example failure:

```text
client.codex
  binary: not found
  searched: PATH
  action: install Codex or set clients.codex.command
```

Never print credential paths unless required for a documented integration.

## Debug fixture mode

Sanitized adapter fixtures live in `crates/fluxguard-adapters/tests/fixtures/`
and are replayed by the adapters' own parsing tests, so adapter debugging is
reproducible without real account data:

```bash
cargo test -p fluxguard-adapters
```

A `--fixture` flag on `doctor` is not implemented; the fixtures are exercised
through the test suite instead.
