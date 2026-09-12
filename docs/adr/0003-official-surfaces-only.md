# ADR 0003: Official Surfaces by Default

Status: Accepted

## Context

Some coding products display quota in a UI but do not expose a supported machine-readable API.

Community tools may reverse engineer OAuth endpoints, cookie stores, or private APIs.

Those integrations are brittle and may create security or terms-of-service risk.

## Decision

Default adapters may use:

1. official structured API,
2. official SDK,
3. official machine-readable CLI,
4. official headers,
5. official local telemetry,
6. honest local estimation,
7. manual configuration.

Private/undocumented endpoints are excluded from default builds.

Experimental implementations require:

- explicit feature flag,
- clear labeling,
- no claim of stability,
- separate review.

## Consequences

Some clients will initially have partial support.

This is preferred to unsafe or brittle "full support."
