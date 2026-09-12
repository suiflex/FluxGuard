# ADR 0005: MCP Pull API Plus Optional Host Hooks

Status: Accepted

## Context

An MCP server cannot force an LLM to call a tool.

Resource awareness is most useful before expensive operations.

Different clients expose different lifecycle hook systems.

## Decision

Use MCP as the universal baseline.

Add optional client-specific host integrations later.

Three levels:

```text
passive       agent calls MCP manually
instructed    AGENTS/rules require calls at key points
host-assisted hooks consult policy automatically
```

## Consequences

The core remains portable.

Clients with rich hooks can become more proactive without changing provider adapters.
