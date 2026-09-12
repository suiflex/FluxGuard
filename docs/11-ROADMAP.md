# Roadmap

## v0.1: Codex vertical slice

Goal:

Prove the architecture with one high-quality source.

Deliver:

- Rust workspace,
- core budget model,
- pressure/policy engine,
- Codex App Server adapter,
- MCP stdio,
- CLI status/doctor,
- tests,
- release binaries.

## v0.2: second exact/structured source

Preferred candidate:

GitHub Copilot quota through official SDK, if integration packaging remains clean.

Alternative:

OpenCode structured local stats.

## v0.3: host awareness

Add client integration packages/rules for:

- Cursor hooks,
- Claude Code hooks/rules if supported surface is appropriate,
- Antigravity skills/rules,
- OpenCode plugin/lifecycle integration.

Objective:

Reduce reliance on the model remembering to call MCP manually.

## v0.4: provider telemetry

Provider-side budgets:

- Anthropic API,
- xAI,
- Z.AI general API,
- OpenAI API where stable account/project usage APIs fit.

Do not proxy model traffic merely to observe it.

## v0.5: more resource dimensions

- context pressure,
- explicit time budget,
- dollar budget,
- user-defined budget,
- tool-call budget.

## v0.6: operation cost model

Estimate expected cost class from:

- operation type,
- recent client behavior,
- number of subagents,
- test scope,
- observed model.

Keep this heuristic transparent.

## v1.0 criteria

- at least three useful clients/sources,
- stable MCP contract,
- documented compatibility policy,
- cross-platform releases,
- no dependency on private auth endpoints,
- robust partial-failure behavior,
- security review,
- migration guide.
