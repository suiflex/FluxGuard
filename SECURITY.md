# Security

`FluxGuard` handles account usage metadata and may interact with authenticated local developer tools.

Treat local credentials as highly sensitive.

## Threat model

Primary risks:

- credential leakage through logs,
- accidental exposure of local usage metadata,
- executing an untrusted provider binary,
- malicious or malformed upstream JSON,
- local MCP clients requesting excessive diagnostic detail,
- command injection through configurable executable paths,
- following symlinks or reading credential files that are not required.

## Rules

- Never print access tokens.
- Never print refresh tokens.
- Never print API keys.
- Never print cookies.
- Never persist credentials.
- Never send usage telemetry off-device by default.
- Do not read browser storage.
- Do not extract browser cookies.
- Do not discover credentials by recursively scanning a home directory.
- Prefer invoking documented local client surfaces that already own authentication.

## Process execution

Executable paths must be passed as process arguments, not interpolated into a shell command.

Prefer `tokio::process::Command`.

Do not invoke `sh -c`, `bash -c`, `cmd /C`, or PowerShell unless a specific integration requires it and the risk is documented.

## Logs

Default logging should contain:

- source id,
- event kind,
- latency,
- success/failure class,
- freshness,
- retry count.

Logs should not contain complete upstream payloads.

## Reporting

If this repository becomes public, replace this section with a private vulnerability reporting address before the first release.
