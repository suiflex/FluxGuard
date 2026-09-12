# Domain Model

## Goals

The domain must represent different forms of resource limits without assuming that every source uses tokens or requests.

## Source identity

```rust
pub struct SourceId(String);

pub enum SourceKind {
    Client,
    Provider,
    Local,
    UserConfigured,
}
```

A source descriptor should include:

```text
id
kind
display_name
adapter_version
source_quality
capabilities
```

## Source quality

```rust
pub enum SourceQuality {
    OfficialStructured,
    OfficialCli,
    OfficialHeaders,
    OfficialTelemetry,
    Estimated,
    Manual,
    Experimental,
}
```

Quality affects confidence. It must not silently change numeric values.

## Metric dimensions

```rust
pub enum MetricDimension {
    Requests,
    Tokens,
    InputTokens,
    OutputTokens,
    Credits,
    Currency,
    Compute,
    ContextTokens,
    Concurrency,
    Time,
    Unknown(String),
}
```

Do not force every upstream measurement into "tokens."

## Window identity

A source may expose several independent windows.

Examples:

```text
codex.primary
codex.secondary
codex.limit.codex
anthropic.five_hour
anthropic.weekly
cursor.cursor_models_monthly
cursor.other_models_monthly
zai.five_hour
zai.weekly
xai.grok_4_6.tpm
```

Model:

```rust
pub struct BudgetWindow {
    pub id: WindowId,
    pub label: Option<String>,
    pub dimension: MetricDimension,

    pub used: Option<DecimalValue>,
    pub limit: Option<DecimalValue>,
    pub remaining: Option<DecimalValue>,

    pub used_percent: Option<f64>,
    pub remaining_percent: Option<f64>,

    pub window_duration: Option<Duration>,
    pub resets_at: Option<OffsetDateTime>,

    pub hard_blocked: bool,
    pub applicability: Applicability,

    pub observed_at: OffsetDateTime,
    pub freshness: Freshness,
    pub provenance: Provenance,
}
```

Percent values should be normalized to `0..=100`.

Do not invent raw `used`, `remaining`, or `limit` if upstream only provides percentages.

## Applicability

A limit may not apply to the current model or operation.

```rust
pub enum Applicability {
    Applicable,
    NotApplicable,
    Unknown,
}
```

Only applicable limits may become the primary bottleneck.

Unknown applicability may lower confidence.

## Availability

Account availability must be modeled independently from percentages.

```rust
pub enum Availability {
    Allowed,
    Blocked { reason: BlockReason },
    Unknown,
}
```

This matters for sources such as Codex where backend permission may be authoritative.

A reset timestamp passing is not enough to change `Blocked` to `Allowed`.

## Freshness

```rust
pub enum Freshness {
    Fresh,
    Aging,
    Stale,
}
```

Store:

```text
observed_at
fresh_until
```

Suggested defaults:

- push-updated source: 2 minutes,
- cheap official metadata call: 1 minute,
- local telemetry: 30 seconds,
- estimation: configurable.

These are configuration defaults, not hardcoded universal truths.

## Budget snapshot

```rust
pub struct BudgetSnapshot {
    pub source: SourceDescriptor,
    pub account_scope: Option<String>,
    pub availability: Availability,
    pub windows: Vec<BudgetWindow>,
    pub observed_at: OffsetDateTime,
    pub warnings: Vec<SnapshotWarning>,
}
```

Do not expose raw account identifiers through MCP unless necessary. Hash or omit them in normal views.

## Pressure levels

```rust
pub enum PressureLevel {
    Normal,
    Guarded,
    Conserve,
    Critical,
    Emergency,
    Blocked,
    Unknown,
}
```

Initial default thresholds by remaining percentage:

```text
> 50%       Normal
25..50%     Guarded
10..25%     Conserve
3..10%      Critical
0..3%       Emergency
blocked     Blocked
unknown     Unknown when no trustworthy constraint exists
```

Thresholds are configurable.

## Pressure assessment

```rust
pub struct PressureAssessment {
    pub level: PressureLevel,
    pub bottleneck: Option<ConstraintRef>,
    pub effective_remaining_percent: Option<f64>,
    pub resets_at: Option<OffsetDateTime>,
    pub confidence: Confidence,
    pub reasons: Vec<ReasonCode>,
}
```

Do not average limits.

If weekly is 8% and five-hour is 70%, the bottleneck is weekly.

## Confidence

```rust
pub enum Confidence {
    High,
    Medium,
    Low,
    Unknown,
}
```

Examples:

- fresh official structured API: high,
- fresh official CLI: high/medium,
- local estimate: medium/low,
- stale estimate: low,
- no source: unknown.

## Operations

```rust
pub enum OperationKind {
    InspectTargeted,
    SearchBroad,
    EditSmall,
    RefactorLarge,
    TestTargeted,
    TestFull,
    SpawnSubagent,
    SpawnParallelSubagents,
    ResearchExternal,
    GenerateArtifacts,
    Checkpoint,
    Finalize,
    Other(String),
}
```

## Execution advice

```rust
pub struct ExecutionAdvice {
    pub proceed: ProceedDecision,
    pub mode: ExecutionMode,
    pub pressure: PressureAssessment,
    pub recommendations: Vec<Recommendation>,
    pub reasons: Vec<ReasonCode>,
}
```

Modes:

```rust
pub enum ExecutionMode {
    Normal,
    Efficiency,
    CompletionFirst,
    CheckpointOnly,
    Blocked,
}
```

Example:

```json
{
  "proceed": "yes_with_constraints",
  "mode": "completion_first",
  "pressure": "critical",
  "recommendations": [
    "avoid_parallel_subagents",
    "avoid_optional_refactor",
    "use_targeted_tests",
    "checkpoint_after_milestone"
  ]
}
```

## Validation invariants

- percentage must be finite and clamped/rejected outside `0..=100`,
- `remaining_percent` and `used_percent` should be consistent if both exist,
- missing is not zero,
- a hard-block state dominates numeric percentage,
- timestamps are UTC internally,
- source quality is always present,
- every window keeps provenance.
