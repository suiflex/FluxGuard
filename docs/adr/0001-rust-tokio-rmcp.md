# ADR 0001: Rust, Tokio, and rmcp

Status: Accepted

## Context

The service is a local long-running process that:

- speaks MCP,
- supervises child processes,
- handles bidirectional JSON-RPC,
- maintains latest state,
- supports concurrent readers,
- should distribute as one binary.

## Decision

Use Rust with:

- Tokio,
- official `rmcp`,
- Serde,
- tracing,
- thiserror.

## Consequences

Positive:

- single-binary distribution,
- strong process/concurrency model,
- memory safety,
- good fit for long-lived local daemon-like workload,
- user/project maintainer already has Rust expertise.

Negative:

- Rust MCP SDK historically trails Tier 1 SDK status,
- compile times are higher than Go,
- contribution barrier is higher than Python/TypeScript.

## Revisit when

Revisit if:

- official Rust MCP SDK loses compatibility,
- required provider SDK exists only in another language and companion-process integration becomes simpler,
- maintainership changes significantly.
