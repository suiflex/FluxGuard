# Contributing to FluxGuard

Thanks for contributing to FluxGuard, a local resource-awareness layer for AI
coding agents. This guide covers the repository workflow, verification rules,
and boundaries that keep the project safe to extend.

## Quick links

- [Project overview](README.md)
- [Coding rules](CLAUDE.md)
- [Security policy](SECURITY.md)
- [Architecture and contracts](docs/)
- [License](LICENSE)
- [Contributor License Agreement](CLA.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)

`CLAUDE.md` is the authoritative engineering guide. If this file and the
architecture or contract docs disagree, update the documentation before
changing implementation behavior.

## Contributor License Agreement

Before your first pull request can be merged, you must sign the
[Contributor License Agreement](CLA.md). When you open your first pull request,
the CLA bot comments with instructions. Sign it by replying with exactly:

```text
I have read the CLA Document and I hereby sign the CLA
```

Your signature is recorded on the `cla-signatures` branch and covers present
and future contributions to FluxGuard. The pull request receives a `cla: signed`
or `cla: not signed` label.

## How to contribute

- **Small fix or documentation correction** — open a focused pull request.
- **New client/provider adapter, public MCP change, or policy change** — agree
  on the design and security boundary before implementation.
- **Question or setup problem** — document the exact command and output in the
  discussion or issue.
- **Security vulnerability** — do not open a public issue; follow
  [SECURITY.md](SECURITY.md).

One pull request should address one logical change and one verifiable outcome.

## Repository layout

| Path | Responsibility |
| --- | --- |
| `crates/fluxguard-core/` | Provider-neutral domain model and policy |
| `crates/fluxguard-runtime/` | Source lifecycle, registry, refresh, and diagnostics |
| `crates/fluxguard-adapters/` | Client and provider integrations |
| `crates/fluxguard-mcp/` | MCP tools, resources, and transports |
| `crates/fluxguard/` | Installable CLI, configuration, and process assembly |
| `docs/` | Architecture, contracts, research, ADRs, and roadmap |
| `plugins/` | Advisory client hook packages |
| `npm/` | Global launcher package that resolves the release binary |
| `scripts/` | Platform install scripts used by the curl and PowerShell flows |
| `tests/logo.sh` | Regenerates the terminal brand mark from `assets/brand/` |

Client adapters and provider adapters remain separate. Provider-specific JSON
must stay inside the adapter that owns it; generic domain types must remain
provider-neutral.

## Getting started

Install stable Rust with the components used by CI:

```bash
rustup component add rustfmt clippy
```

Build the workspace:

```bash
cargo build --workspace
```

Run the local status and diagnostics commands with the Codex adapter disabled
when no live account surface is needed:

```bash
FLUXGUARD_CLIENTS_CODEX_ENABLED=false fluxguard status --json
FLUXGUARD_CLIENTS_CODEX_ENABLED=false fluxguard sources
fluxguard doctor
```

`install` and a bare `config` are interactive. Keep both usable without a
terminal — `--client`, `--dry-run`, and the `FLUXGUARD_*` environment overrides
exist so CI never needs a TTY — and keep colour optional: everything must stay
readable under `NO_COLOR=1` or when redirected.

## Build, lint, and test

The Makefile is the local verification interface:

```bash
make check
```

It runs, in order:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

For release verification:

```bash
cargo build --locked --workspace --release
```

Live client checks are opt-in. Never require real credentials in CI and never
paste account identifiers, tokens, cookies, or complete provider responses into
issues, logs, fixtures, or pull requests.

## Publishing to crates.io

Only the `fluxguard` package is installed directly by users:

```bash
cargo install fluxguard
```

The other workspace packages are published libraries consumed by that binary.
Before publishing, run:

```bash
cargo publish --locked --dry-run -p fluxguard-core
cargo publish --locked --dry-run -p fluxguard-runtime
cargo publish --locked --dry-run -p fluxguard-adapters
cargo publish --locked --dry-run -p fluxguard-mcp
cargo publish --locked --dry-run -p fluxguard
```

Publish in that order. Do not hand-edit `CHANGELOG.md` once Release Please
owns it.

The npm package is published separately as `@suiflex/fluxguard`:

```bash
make npm-test
make npm-pack
npm publish --access public
```

Use trusted publishing/OIDC in CI when configured. Do not put npm or crates.io
tokens in repository files, shell history, issue reports, or pull requests.

Release version bumps are started manually from GitHub Actions by running
`release-please`. It creates or updates a release pull request; merging that
pull request creates the `vX.Y.Z` tag consumed by `release-build.yml`.

## Adding a source

Before implementing a source:

1. classify it as a client source or provider source;
2. identify the documented machine-readable surface;
3. record authentication ownership and source quality;
4. define freshness, scope, and failure behavior;
5. add sanitized fixtures and normalization tests;
6. add unavailable, malformed, and stale-path coverage;
7. add diagnostics without authentication material;
8. update the provider matrix and relevant contract documentation.

A source that cannot yet read a documented surface must return
`SourceError::UnsupportedVersion` from `refresh`, and an empty normalized
snapshot must be `Availability::Unknown`. Publishing an empty snapshot as
`Allowed` claims a source that is not there.

Allowed source qualities are `official_structured`, `official_cli`,
`official_headers`, `official_telemetry`, `estimated`, `manual`, and
`experimental`. Experimental sources require explicit opt-in and clear
labeling.

Do not use browser cookie scraping, private endpoints, undocumented OAuth
usage APIs, credential extraction, interactive TUI parsing, or a transparent
proxy as a shortcut.

## Public contract invariants

- Client/harness identity is separate from provider identity.
- Independent budget windows are never averaged.
- Missing values remain unknown; missing limits never become `100%` remaining.
- A hard block dominates numeric pressure.
- A passed reset timestamp does not prove recovery without a new observation.
- Stale and estimated data lower confidence.
- One failed source must not crash the MCP server.
- MCP output contains normalized metadata, never raw authentication material.

## Commit conventions

Use Conventional Commits:

```text
feat(scope): add a source adapter
fix(scope): preserve stale source state
chore(scope): update build workflow
```

- Subject line at most 72 characters, imperative mood, no trailing period.
- Wrap the body at 72 characters and explain why the change exists.
- Keep one logical change per commit.
- Do not add AI-assistance markers or co-author trailers.
- Do not hand-edit release-managed changelog sections once release automation is
  enabled.

## Branches and pull requests

Branch from `develop` with a professional type-prefixed name:

```text
feat/fluxguard-codex-adapter
fix/fluxguard-stale-pressure
chore/fluxguard-ci-hardening
```

A pull request should include:

- the user problem and bounded scope;
- architecture and compatibility impact;
- security/privacy impact;
- exact commands run and their results;
- documentation changes or an explicit reason none were needed.

Keep the test plan honest. Mark only checks that actually ran.

## License

FluxGuard is licensed under the [Apache License 2.0](LICENSE). Contributions
must preserve the project license and its security boundaries.
