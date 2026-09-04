# Chakra feature implementation

Implemented: topic-batch poll fan-out with merge/dedupe and cursor retention on
partial failure; ack-validated multi-subscription WS watcher (unique request
ids, reconnect on rejected/missing/timed-out acks) with `native-tls` enabled so
`wss://` Arc endpoints connect; -32005 retry with backoff/failover and -32012
never retried; source-preserving factory normalization and emitter attribution;
additive integer analytics indexer with correct heads/lag/freshness; six-probe
strict readiness; `GET /api/v1/stats`; viem QA smoke CLI; and the `/stats`
dashboard with BigInt USD formatting, URL range selection, and stale-response
protection.

Backend hotfix implemented: removed the unit-agnostic
MIN_XYK_RESERVE_ATOMIC_UNITS guard from both local XYK quote paths. Zero-
reserve rejection, exact integer XYK math, nonzero-output rejection,
curated/factory allowlisting, and normal slippage protection remain unchanged.
Added the live UnitFlow reserve regression in
crates/api-server/tests/chakra_venues_test.rs for direct EURC↔cirBTC and
USDC↔cirBTC multihop directions.

Readiness follow-up: route diagnostics now lowercase the three catalog token
addresses before graph lookup, matching the canonical lowercase snapshot
topology. Regression `route_health_matches_lowercase_snapshot_addresses`
protects all six readiness directions without changing the API contract.
