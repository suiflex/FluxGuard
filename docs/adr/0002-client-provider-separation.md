# ADR 0002: Separate Clients from Providers

Status: Accepted

## Context

Names such as Codex, Cursor, Claude Code, and OpenCode describe agent clients/harnesses.

Names such as OpenAI, Anthropic, xAI, and Z.AI describe model/service providers.

A client may use multiple providers.

## Decision

Model two source families:

```text
ClientSource
ProviderSource
```

Both implement the generic `BudgetSource` contract.

## Consequences

- Cursor using xAI remains representable without pretending Cursor is xAI.
- OpenCode can report local session stats while Z.AI reports account quota.
- policy can reason about client context and provider quota simultaneously.
- new providers do not require new MCP tools.
