# Policy Engine

## Purpose

Raw percentages are not enough.

The policy engine converts resource pressure and operation intent into useful execution advice.

## Pipeline

```text
Budget snapshots
    |
    v
Applicability filter
    |
    v
Freshness / confidence
    |
    v
Constraint pressure
    |
    v
Bottleneck selection
    |
    v
Operation policy
    |
    v
ExecutionAdvice
```

## Constraint pressure

Initial defaults:

| Remaining | Pressure |
|---|---|
| > 50% | normal |
| 25–50% | guarded |
| 10–25% | conserve |
| 3–10% | critical |
| <= 3% | emergency |

`Blocked` dominates every numeric level.

## Time-to-reset

Remaining percentage alone is incomplete.

Example:

- 3% remaining and reset in 90 seconds,
- 3% remaining and reset in 5 days.

Both are emergency constraints, but advice may differ.

Suggested classification:

```text
reset_imminent: <= 5 minutes
reset_soon: <= 1 hour
reset_later: > 1 hour
unknown
```

Do not assume a passed reset timestamp means quota is usable. Refresh first.

## Operation cost

Initial operation cost profiles:

| Operation | Default cost |
|---|---|
| targeted inspection | low |
| small edit | low |
| targeted test | low |
| external research | medium |
| broad repository search | medium |
| full test suite | medium/high |
| one subagent | medium |
| parallel subagents | high |
| large refactor | high |
| checkpoint | low |
| finalize | low |

These are heuristic policy hints, not billing estimates.

## Advice table

### Normal

- proceed normally,
- parallel work allowed,
- broad exploration allowed,
- full tests allowed when useful.

### Guarded

- proceed,
- avoid wasteful optional work,
- prefer targeted exploration.

### Conserve

- avoid optional broad research,
- limit subagents,
- prefer targeted tests,
- checkpoint after milestones.

### Critical

- focus on current objective,
- avoid parallel subagents,
- avoid optional refactors,
- avoid full test matrix unless required,
- perform targeted verification,
- checkpoint frequently.

### Emergency

- do not begin expensive optional work,
- finish the smallest path that satisfies the objective,
- save state,
- report remaining work clearly.

### Blocked

- do not advise expensive model work,
- surface reset/availability data if trustworthy,
- allow local non-model actions when the caller decides they are useful.

## Required work vs optional work

Pressure must not cause the system to recommend skipping correctness-critical work blindly.

If a required verification is expensive, advice can be:

```json
{
  "proceed": "yes_with_constraints",
  "recommendations": [
    "run_required_test_subset",
    "skip_optional_lint_matrix"
  ]
}
```

## Confidence degradation

Example:

fresh official structured source:

```text
pressure=critical
confidence=high
```

stale estimated source:

```text
pressure=critical
confidence=low
recommendation=refresh_before_expensive_work
```

## Multiple constraints

Never average:

```text
5-hour = 80% remaining
weekly = 8% remaining
context = 45% remaining
```

The weekly quota is the quota bottleneck.

The pressure engine may also report context as a secondary constraint.

Output may include:

```json
{
  "primary": "client.codex.weekly",
  "secondary": [
    "local.context"
  ]
}
```

## Provider fallback

If exact subscription quota is unavailable but request telemetry exists:

- expose what is known,
- mark quality accordingly,
- do not pretend it represents subscription allowance.

Example:

xAI static TPM limit plus locally observed tokens can estimate current rolling pressure if all requests are visible.

If not all requests are visible, mark the estimate incomplete.

## Configuration

Thresholds should be configurable globally and optionally per source.

Do not allow configuration to turn `Blocked` into `Normal`.
