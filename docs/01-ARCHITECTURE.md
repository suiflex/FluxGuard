# Architecture

## System boundaries

```text
+--------------------------------------------------------------+
|                       Agent Client                           |
| Codex | Claude Code | Cursor | Antigravity | Copilot | ...  |
+-----------------------------+--------------------------------+
                              |
                              | MCP stdio / HTTP later
                              v
+--------------------------------------------------------------+
|                    fluxguard-mcp                          |
| tool handlers | resources | protocol negotiation             |
+-----------------------------+--------------------------------+
                              |
                              v
+--------------------------------------------------------------+
|                 Application Services                         |
| SnapshotService | AdviceService | RefreshService | Doctor     |
+-----------------------------+--------------------------------+
                              |
              +---------------+----------------+
              |                                |
              v                                v
+---------------------------+       +---------------------------+
|      Pressure Engine      |       |       Source Runtime      |
| pure deterministic logic  |       | registry/scheduler/watch  |
+-------------+-------------+       +-------------+-------------+
              |                                   |
              v                                   v
+---------------------------+       +---------------------------+
|       Policy Engine       |       |       BudgetSource        |
| operation -> advice       |       |        abstraction        |
+---------------------------+       +-------------+-------------+
                                                  |
                          +-----------------------+----------------------+
                          |                                              |
                          v                                              v
                 +-------------------+                         +-------------------+
                 | Client adapters   |                         | Provider adapters |
                 | Codex             |                         | OpenAI API         |
                 | Claude Code       |                         | Anthropic API      |
                 | Cursor            |                         | xAI                |
                 | Antigravity       |                         | Z.AI               |
                 | Copilot           |                         | ...                |
                 | OpenCode          |                         +-------------------+
                 +-------------------+
```

## Cargo workspace

Use a workspace with a small number of crates.

```text
fluxguard/
├── Cargo.toml
├── CLAUDE.md
├── crates/
│   ├── fluxguard-core/
│   │   └── src/
│   │       ├── budget.rs
│   │       ├── pressure.rs
│   │       ├── policy.rs
│   │       ├── operation.rs
│   │       └── error.rs
│   ├── fluxguard-runtime/
│   │   └── src/
│   │       ├── source.rs
│   │       ├── registry.rs
│   │       ├── scheduler.rs
│   │       ├── snapshot_service.rs
│   │       └── diagnostics.rs
│   ├── fluxguard-adapters/
│   │   └── src/
│   │       ├── clients/
│   │       │   ├── codex/
│   │       │   ├── claude_code/
│   │       │   ├── cursor/
│   │       │   ├── antigravity/
│   │       │   ├── copilot/
│   │       │   └── opencode/
│   │       └── providers/
│   │           ├── openai/
│   │           ├── anthropic/
│   │           ├── xai/
│   │           └── zai/
│   ├── fluxguard-mcp/
│   │   └── src/
│   │       ├── server.rs
│   │       ├── tools.rs
│   │       └── resources.rs
│   └── fluxguard/
│       └── src/
│           ├── main.rs
│           ├── config.rs
│           └── commands/
└── docs/
```

Do not create one crate per provider yet. Split adapters into separate crates only when dependency isolation or release ownership makes it necessary.

## Dependency direction

Allowed:

```text
cli -> mcp -> runtime -> core
cli -> runtime
runtime -> adapters -> core
mcp -> core
```

Avoid cyclic dependencies.

A practical workspace may let runtime depend on adapter traits while the CLI assembles concrete adapters. If dependency inversion becomes awkward, move `BudgetSource` trait into `core` or a tiny `source-api` crate.

## Runtime state

Use `tokio::sync::watch` for latest snapshots.

Why:

- consumers care about latest state,
- old intermediate snapshots normally have no value,
- a source can publish updates asynchronously,
- MCP reads can be cheap,
- watchers can react to source changes.

Concept:

```text
Codex adapter task
    |
    | snapshot update
    v
watch::Sender<SourceSnapshot>
    |
    +--> SnapshotService
    +--> Diagnostics
    +--> future hook integration
```

## Adapter lifecycle

Each adapter has four phases:

```text
discover
  -> initialize
  -> refresh/observe
  -> shutdown
```

Discovery answers whether the source is usable.

Initialization may spawn or connect to a local process.

Refresh produces a normalized snapshot.

Observe optionally listens for push updates.

Shutdown cancels tasks and cleans up child processes.

## Source trait

Illustrative shape:

```rust
#[async_trait]
pub trait BudgetSource: Send + Sync {
    fn descriptor(&self) -> SourceDescriptor;

    async fn probe(&self) -> Result<ProbeReport, SourceError>;

    async fn refresh(&self) -> Result<BudgetSnapshot, SourceError>;

    async fn run(
        &self,
        tx: watch::Sender<BudgetSnapshot>,
        cancel: CancellationToken,
    ) -> Result<(), SourceError>;
}
```

Do not require every source to support push observation. A default polling implementation can exist.

## App services

### SnapshotService

- reads latest source states,
- applies freshness,
- combines them without losing provenance,
- returns compact or full views.

### AdviceService

- gets snapshot,
- computes pressure,
- evaluates requested operation,
- returns execution advice.

### RefreshService

- explicitly refreshes selected sources,
- deduplicates simultaneous refresh requests,
- applies timeouts.

### Doctor

- explains discovery/auth/version/compatibility problems.

## Active vs passive awareness

MCP alone is pull-based.

Support three integration levels.

### Level 1: passive

Agent manually calls `resource_status`.

### Level 2: instructed

Repository instructions tell the agent to call `resource_advice` before expensive operations.

### Level 3: host-assisted

Client-specific hooks call or consult `FluxGuard` automatically before:

- parallel subagents,
- broad searches,
- large refactors,
- expensive test suites,
- compaction boundaries.

Host-assisted integration belongs outside the generic MCP contract.

## Local-first

v0.1 uses stdio MCP.

A remote HTTP transport can be added later.

Do not add a database.

State can be reconstructed from upstream sources.

## Failure model

A partial result is preferred to total failure.

Example:

```json
{
  "overall": "conserve",
  "sources": {
    "codex": "fresh",
    "cursor": "unsupported",
    "xai": "stale"
  }
}
```

The policy engine must include confidence/provenance in advice.
