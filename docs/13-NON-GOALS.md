# Non-Goals

The following are explicitly outside v0.x unless an ADR changes scope.

## Model gateway

`FluxGuard` is not an LLM gateway or reverse proxy.

It should not require model traffic to pass through itself.

## Credential broker

It is not a credential extraction or sharing tool.

## Billing system

It does not replace provider invoices.

Local estimates are not authoritative billing records.

## Automatic model switching

The first releases advise the agent.

They do not automatically switch from an expensive model to a cheap model.

That can be a future host integration with explicit user opt-in.

## Automatic purchasing

Never buy credits, enable overages, or change subscriptions.

## Circumventing provider limits

The project must not be designed to bypass quotas, rotate accounts, evade rate limits, or violate provider terms.

## Exact cost prediction

Operation cost classes are heuristic.

Do not promise exact future token consumption.

## Universal quota support on day one

Some products expose usage only in human UI.

Support should remain partial rather than depending on brittle scraping.
