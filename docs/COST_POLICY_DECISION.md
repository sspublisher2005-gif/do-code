# Do Code product decision: no predicted-cost stopping

**Decision:** The Do Code Rust implementation will not estimate cost in advance or stop a coding mission when a predicted/assumed dollar threshold is reached. The product will behave like a conventional CLI coding agent: use the configured provider/model, honor provider/API errors and model output-token settings, and keep going until task/workflow completion or a non-financial safety/runtime gate is reached.

## Forbidden behaviors

- No price-list guess multiplied by estimated input/output tokens to make a predicted bill.
- No `--max-cost-usd`, `estimatedCostUsd`, `costThresholdUsd`, or Decision Arena trigger based on a money estimate.
- No automatic mission stop/rejection when a provider-reported cost value crosses a dollar amount.
- No assumption that missing price/usage means zero cost.

## Allowed behavior

- Display tokens if the provider actually returns them.
- Display a USD cost only if the provider explicitly returns a valid USD-denominated `cost_usd` field. Otherwise display `unknown`.
- Let provider quotas/limits/errors work normally. Do not invent pricing, and do not retry non-retriable billing/quota errors indefinitely.
- Continue enforcing output-token caps, explicit timeouts, cancellation, maximum response sizes, process/resource caps and security policies. Those controls protect the runtime/environment, not a predicted bill.

## Migration note

The preserved historical TypeScript `0.6.0-ts.2` archive contains legacy cost-budget/estimated-cost trigger options. They are reference artifacts only; the active Rust branch must never copy them. CLI parity tests assert that help does not expose money-stop flags, and mission configuration has no cost-budget fields. Future Decision Arena porting must not use estimated money as a ranking or trigger.
