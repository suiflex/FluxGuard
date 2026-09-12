# ADR 0004: Normalize Constraints Without Flattening Them

Status: Accepted

## Context

Providers expose different dimensions:

- percentage only,
- requests,
- tokens,
- credits,
- currency,
- context,
- concurrency.

A single "remaining tokens" type would be incorrect.

## Decision

Represent each constraint as an independent `BudgetWindow` with:

- dimension,
- optional raw values,
- optional percentages,
- reset semantics,
- applicability,
- freshness,
- provenance.

Overall pressure selects bottlenecks using policy.

It never averages unrelated constraints.

## Consequences

The model is more verbose internally but remains correct across providers.

MCP compact views hide this complexity from the agent when detail is unnecessary.
