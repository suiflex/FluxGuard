# MCP Contract

## Design goals

The MCP surface should:

- be small,
- be cheap in model context,
- return structured data,
- avoid provider-specific tool names,
- allow future providers without changing the basic agent workflow.

## Tool: `resource_status`

Purpose:

Get the latest normalized resource status.

Input:

```json
{
  "detail": "compact",
  "sources": []
}
```

Fields:

- `detail`: `compact` or `summary`
- `sources`: optional source ids

Compact output:

```json
{
  "pressure": "critical",
  "recommended_mode": "completion_first",
  "bottleneck": {
    "source": "client.codex",
    "window": "weekly",
    "remaining_percent": 8,
    "resets_at": "2026-09-14T04:00:00Z"
  },
  "confidence": "high",
  "freshness": "fresh"
}
```

Do not return every window in compact mode.

## Tool: `resource_advice`

Purpose:

Tell the agent whether an intended operation is reasonable under current constraints.

Input:

```json
{
  "operation": "spawn_parallel_subagents",
  "estimated_cost": "high",
  "importance": "optional"
}
```

`estimated_cost`:

- low
- medium
- high
- unknown

`importance`:

- required
- useful
- optional

Output:

```json
{
  "proceed": "no",
  "mode": "completion_first",
  "pressure": "critical",
  "reasons": [
    "weekly_quota_low",
    "operation_cost_high",
    "operation_optional"
  ],
  "recommendations": [
    "continue_single_agent",
    "perform_targeted_inspection"
  ]
}
```

This is advisory in v0.1. It does not execute or block the operation itself.

## Tool: `resource_refresh`

Purpose:

Force one or more sources to refresh.

Input:

```json
{
  "sources": ["client.codex"]
}
```

Output:

```json
{
  "refreshed": ["client.codex"],
  "failed": [],
  "observed_at": "2026-09-12T06:00:00Z"
}
```

Apply a timeout.

Concurrent refresh calls for the same source should be coalesced where practical.

## MCP resources

### `fluxguard://status/full`

Detailed normalized state.

This may contain every window, but still must not include secrets.

### `fluxguard://sources`

Source descriptors and availability.

Example:

```json
[
  {
    "id": "client.codex",
    "kind": "client",
    "quality": "official_structured",
    "state": "ready"
  },
  {
    "id": "client.cursor",
    "kind": "client",
    "quality": "official_telemetry",
    "state": "partial"
  }
]
```

### `fluxguard://diagnostics`

Operational diagnostics:

- adapter version,
- binary detected,
- source state,
- last refresh,
- last error category,
- retry state.

Never include auth material.

## Tool descriptions

Tool descriptions are themselves model context.

Keep them concise.

Avoid embedding full policy documentation in MCP tool descriptions.

Use `CLAUDE.md` and documentation for detailed behavior.

## Errors

Return typed error codes where possible:

```text
source_unavailable
source_unauthenticated
source_unsupported
source_stale
source_timeout
source_rate_limited
source_protocol_error
invalid_operation
invalid_request
```

Do not turn a source failure into an MCP server crash.

## Protocol compatibility

Target MCP `2026-07-28` through official Rust `rmcp`.

Allow protocol negotiation for clients supporting older revisions.

Use stdio for the first release.

Remote Streamable HTTP is later scope.
