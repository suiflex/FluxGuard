# Testing Strategy

## Test pyramid

### Core unit tests

Pure and fast.

Cover:

- percentages,
- freshness,
- pressure thresholds,
- bottleneck selection,
- availability dominance,
- operation advice.

### Adapter fixture tests

Store sanitized upstream examples.

Suggested layout:

```text
crates/fluxguard-adapters/tests/fixtures/
├── codex/
│   ├── rate_limits_basic.json
│   ├── rate_limits_multi_bucket.json
│   ├── rate_limits_blocked.json
│   └── rate_limits_missing_optional.json
└── ...
```

Fixtures must not contain real identifiers or tokens.

### Runtime integration tests

Use fake sources.

Test:

- update propagation,
- stale transitions,
- timeouts,
- cancellation,
- failed source isolation,
- refresh coalescing.

### MCP contract tests

Verify:

- tool names,
- input schema,
- output schema,
- compact response,
- resources,
- error mapping.

### Process tests

Use a fake JSON-RPC child process instead of a real Codex login in CI.

Simulate:

- normal responses,
- delayed response,
- malformed JSON,
- unexpected exit,
- notification before response,
- restart,
- shutdown.

## Property tests

Useful invariants:

- pressure never becomes healthier when remaining percent decreases and all other inputs are equal,
- blocked state never resolves without a new observation,
- normalization never emits NaN/Infinity,
- missing limit never becomes zero.

## Compatibility fixtures

When an upstream changes schema:

1. preserve old fixture,
2. add new fixture,
3. support both when practical,
4. update compatibility notes.

## Live tests

Live tests requiring real accounts must be opt-in.

Example:

```bash
AGENT_BUDGET_LIVE_CODEX=1 cargo test -p fluxguard-adapters codex_live -- --ignored
```

Never require live credentials in CI.

## Cross-platform

CI targets:

- Ubuntu latest,
- macOS latest,
- Windows latest.

The core should be platform-independent.

Process adapter tests must account for platform process semantics.
