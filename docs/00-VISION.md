# Vision

## Product thesis

AI agents should be aware of the resources constraining their current work.

The useful abstraction is broader than subscription quota.

A future agent may be constrained by:

- account quota,
- API rate limit,
- token budget,
- context remaining,
- prepaid credit,
- dollar budget,
- time budget,
- tool-call budget,
- provider concurrency,
- client-specific limits.

`FluxGuard` converts these heterogeneous constraints into an execution signal.

## Desired behavior

Without resource awareness:

```text
Agent
  -> broad exploration
  -> spawn several subagents
  -> large refactor
  -> full tests
  -> hard quota limit
  -> incomplete result
```

With resource awareness:

```text
Agent
  -> check resource pressure
  -> detect weekly quota at 9%
  -> avoid optional exploration
  -> keep one execution path
  -> use targeted tests
  -> checkpoint
  -> complete user objective
```

## Long-term product shape

`FluxGuard` should become a small local control plane.

It should work with different agent clients through MCP and optional native lifecycle hooks.

It should not become an LLM proxy unless a separate future design explicitly chooses that direction.

## Success criteria

A successful implementation:

- works without changing model providers,
- returns useful signals quickly,
- fails safely when source information is unavailable,
- does not require private API reverse engineering,
- supports partial information,
- keeps agent-facing responses concise,
- can add a provider without changing core policy types.
