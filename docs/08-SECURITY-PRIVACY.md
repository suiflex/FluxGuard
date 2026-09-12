# Security and Privacy Architecture

## Default trust boundary

Everything stays on the local machine in v0.1.

```text
agent client
    |
local MCP stdio
    |
FluxGuard
    |
documented local/API metadata surfaces
```

No central telemetry service.

## Credential ownership

Prefer adapters where another local client owns authentication.

Example:

```text
fluxguard -> codex app-server -> OpenAI
```

This is better than reading and replaying Codex auth tokens directly.

## Forbidden default patterns

- browser cookie scraping,
- browser automation to read a usage dashboard,
- credential extraction from keychains for undocumented endpoints,
- reading another client's token file solely to replay private API calls,
- MITM proxying TLS,
- logging raw authentication responses.

## Configuration secrets

If future provider adapters need API keys:

- prefer environment variables or OS secret manager integration,
- configuration file stores the variable name, not the secret,
- redact values in diagnostics.

## MCP exposure

MCP tools must return usage metadata only.

Do not expose:

- email address,
- full account id,
- organization secrets,
- raw auth headers.

Use a stable local source id.

## Untrusted upstream data

Treat labels/error messages as untrusted strings.

Bound sizes.

Do not render upstream strings as shell commands.

Do not deserialize into recursive/unbounded structures without reasonable limits.

## Child processes

Validate executable configuration.

Pass args directly.

Implement:

- startup timeout,
- read timeout where applicable,
- output line/packet size limit,
- restart limit,
- shutdown timeout.

## Diagnostics

Good diagnostic:

```json
{
  "source": "client.codex",
  "state": "unavailable",
  "reason": "binary_not_found"
}
```

Bad diagnostic:

```json
{
  "authorization": "Bearer eyJ..."
}
```

## Remote transport later

If remote MCP support is added:

- require authenticated transport,
- define tenant boundaries,
- prohibit forwarding local product credentials to remote server,
- revisit threat model with a new ADR.
