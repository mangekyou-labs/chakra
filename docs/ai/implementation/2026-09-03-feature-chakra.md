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

## Stage 2 reconciliation (2026-09-04)

- Added the `/stats` dashboard, shared integer-safe formatting helpers, and the
  Stats navigation link; the page consumes the existing `GET /api/v1/stats`
  contract without a backend schema change.
- Removed duplicate obsolete `statsFadeIn` keyframes from the global stylesheet.
- Reconciled the public architecture, Circle grant proposal, and lifecycle
  records with the September 4 live quote probe: canonical USDC→EURC→cirBTC via
  UnitFlow, 27 bps impact, 363 output, and 361 minimum output for the guarded
  1-USDC request.
- The worktree `.env` was subsequently sourced into the process environment for
  the guarded dry-run. A `TRANSFER_FROM_FAILED` simulation is expected before
  the planned approval; `preflight.mjs` now classifies that case as
  approval-dependent while continuing to abort on unrelated simulation errors.
- The explicitly authorized broadcast completed successfully on Arc Testnet in
  block `60438104` (tx
  `0x2df6e81aa9ff0805aad7d49241ccdd9e979dd7c0dae1b261c51ed469542236c5`). The
  canonical USDC→EURC→cirBTC route used Presto and UnitFlow; the receipt emitted
  the expected 1,000,000-atomic stablecoin transfer and +368 cirBTC atoms.
- The first broadcast run hit a post-receipt evidence-writer BigInt
  serialization error. `stringifyEvidence` now converts BigInt fields to
  decimal strings, with a regression test; no transaction was rebroadcast.
- After more than 12 confirmations, live stats reported one additional
  attributed swap, one additional confirmed swap, and 2,000,000 total
  stablecoin-notional micros (baseline 1,000,000), with Presto and UnitFlow
  attribution.
