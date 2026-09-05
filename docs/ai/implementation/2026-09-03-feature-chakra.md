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
- After merge, the static-export `/stats` rewrite was added in `vercel.json`;
  the Ready production deployment serves both requested aliases and the
  browser smoke confirms the dashboard consumes live stats.

## Phase 7 check (2026-09-05)

Compared HEAD `318de72` to `docs/ai/{requirements,design}/2026-09-03-feature-chakra.md`.
Shipped code matches the design model. No product-code miss; no return to
Phase 5 or design.

File-by-file:

- Watcher: `watched_topic0_batches()` chunks at `ARC_TOPIC_LIMIT = 10` (10 + 3).
  `poll_once` iterates every batch over one address/block window, merges, then
  `dedupe_logs` by `(address, block, tx, log_index, topics, data)`. The cursor
  is written only after the batch loop; a batch `?` error aborts before that
  write. Covered by `poll_keeps_cursor_when_any_topic_batch_fails` and the
  10 + 3 request-size fixture.
- WS: one socket, unique JSON-RPC ids per batch, shared ack deadline, Pong
  during handshake, reconnect on rejected/missing/timed-out acks.
  `tokio-tungstenite` is built with `native-tls` in
  `crates/market-data-worker/Cargo.toml`.
- RPC: `RpcError.retryable()` is true only for `-32005` or a missing code
  (transport). `-32012` is never retried. Unit tests in `evm_rpc.rs`.
- XYK: `local_evm_xyk_quote` rejects zero reserves and zero exact integer
  output. `MIN_XYK_RESERVE_ATOMIC_UNITS` is absent from code (docs-only
  history). Venues regression
  `live_unitflow_reserves_quote_all_cirbtc_directions`.
- Analytics: additive Redis `chakra:analytics:v1:{chain}:{aggregator}` with
  `set_heads` / `confirmed_target`. Stats `apply_heads` sets
  `lag_blocks = confirmed - indexed` and `freshness_secs` from the last
  successful poll. `GET /api/v1/stats` starts from `stats::empty` and overlays
  summaries when the namespace has data.
- Ready: `CHAKRA_STRICT_READINESS=true` in `render.yaml`;
  `all_routes_healthy` requires direct or multihop on all six lowercase
  catalog pairs. Cluster mode returns `pool_keys: []` while `ready: true`
  after engine edges are confirmed; the six-probe gate is the production
  contract.
- Dashboard: `formatMicrosUsd` BigInt (`1000000` → `$1.00`);
  `chartGeometry` (exported from `stats-format.ts`) scales with BigInt before `Number`; AbortController plus
  `requestId` on range change; `history.replaceState` for `?range=`.
  `packages/frontend/vercel.json` rewrites `/stats` → `/stats.html`.
- QA: `packages/frontend/qa/swap/smoke.mjs` reads `QA_WALLET_SECRET` from
  `process.env` only and requires `--broadcast` to send.

Deviations (low, not blocking):

1. `cargo fmt --all -- --check` fails on wrapping/import diffs in this
   feature's files (`crates/api-server/src/stats.rs`,
   `crates/dex-adapters/src/{evm_logs,evm_rpc}.rs`,
   `crates/market-data-worker/src/{analytics,evm_watcher}.rs`). Stable rustfmt
   ignores nightly options in `rustfmt.toml`. No logic change; this check did
   not mass-format.
2. Planning items 4 and 6 still said “not yet released” / “in progress”
   until the 2026-09-05 recon below.

Follow-ups outside this feature:

- T11.10 / T11.11 headed MetaMask remain blocked on the provider notification
  page. The 2026-09-04 viem CLI swap is a different evidence path and does
  not close those tasks.
- T11.12 split-route live evidence (`split_swaps` is still 0 on hosted stats).
- rustfmt wrapping drift.

Fresh gates for this check are in the testing doc (2026-09-05).

## Phase 9 review (2026-09-05)

Holistic pre-push review of worktree
`/Users/kyler/repos/avax-dex-agg/.worktrees/feature-chakra` at HEAD `318de72`
plus the uncommitted Phase 7/8 delta (`chartGeometry` export, smoke spawn
tests, lifecycle docs). Not a re-review of `chakra/main` squash commits
(`4863ab9`, `486a954`, `f6387c0`) unless a finding required comparison. No
product-code miss; no return to Phase 5 or Phase 8. Task tracing unavailable
(`npx ai-devkit@latest task` → `unknown command 'task'`).

### Design match

Shipped code still matches the 2026-09-03 design. Rechecked this review:

- Watcher: `watched_topic0_batches` 10+3 (`evm_logs.rs:102`); `poll_once`
  iterates every batch, `?` aborts before cursor write, then `dedupe_logs`
  (`evm_watcher.rs:770`). Empty-watch path advances the cursor to latest
  without fetching. `run()` is `tokio::select!` + `MissedTickBehavior::Skip`.
- WS: `subscribe_once` unique ids, shared ack deadline, Pong during
  handshake, reconnect on rejected/missing/timed-out acks;
  `tokio-tungstenite` `native-tls`.
- RPC: `RpcError::retryable` only `-32005` or missing code
  (`evm_rpc.rs:156`); `-32012` never retried. Canteen hosts banned
  (`is_canteen_rpc`).
- XYK: `local_evm_xyk_quote` rejects zero reserves and zero exact integer
  output. `MIN_XYK_RESERVE_ATOMIC_UNITS` is absent from code.
- Analytics: additive Redis `chakra:analytics:v1:{chain}:{aggregator}`
  (`analytics.rs:218`); `put_swap` runs SET NX + always-ZADD in one
  MULTI/EXEC; cursor after every successful page; heads + `polled_at` only
  after a fully successful poll.
- Ready: `CHAKRA_STRICT_READINESS=true` in `render.yaml`; six-probe
  `all_routes_healthy`. Cluster `/ready` 200 with `pool_keys: []` is expected.
- Build tx: aggregator `paused()` → 503 (`handlers.rs:510`); Permit2
  `typedData` omitted when allowance is sufficient (`build_tx.rs:585`).
- Rate limit 10 req/s per IP; `/health` and `/ready` exempt
  (`rate_limit.rs:123`).
- Dashboard: `chartGeometry` exported from `stats-format.ts` (same
  BigInt-then-Number math); `vercel.json` rewrites `/stats` → `/stats.html`.
- QA: `QA_WALLET_SECRET` env-only; `--help` before `loadSecret`; defaults
  dry-run; smoke spawn tests unset the secret.

### Findings

No P0 / P1 / P2 product-code issues. P3 nits (not blocking, not fixed in
this review):

1. `formatMicrosUsd` / `formatUsdCompact` (`stats-format.ts:36`, `:56`) use
   `BigInt(micros || '0')` and can throw on junk, unlike `microsBigInt`
   (`stats-format.ts:125`) which `chartGeometry` already uses. Live `/stats`
   serves integer decimal strings, so the dashboard does not crash on the
   current contract.
2. `smoke.mjs` `--amount-in` as the last argument with no value
   (`smoke.mjs:76`) sets `amountIn` to `undefined`; `BigInt(undefined)` then
   throws (`smoke.mjs:401`). CLI edge, not a product path.
3. `stats::empty` (`stats.rs:152`) copies mixed-case catalog constants from
   `decimals.rs` (`EURC` / `CIRBTC`). Live `get_stats` always overwrites
   `route_health` with lowercase `route_health_for_engine`
   (`handlers.rs:59-60`).
4. Analytics `put_swap` SET NX then zadd is not one Redis transaction
   (`analytics.rs:225-233`). A crash between the two can hide a record from
   `by_time`. Replay remains idempotent on the swap key.
5. `CHAKRA_CORS_ORIGINS` in `render.yaml:30` includes both production
   aliases plus leftover preview aliases
   `frontend-ruddy-two-90.vercel.app` and
   `chakra-arc-dex-gadillacers-projects.vercel.app`, plus localhost. Empty
   env falls back to `http://localhost:3000` (`lib.rs:53`).

### P3 nits fixed (same day, after review)

TDD: failing tests first, then production. All five nits are fixed in the
uncommitted worktree delta. Live Render CORS still has the old allowlist
until the next `render.yaml` deploy.

1. `formatMicrosUsd` / `formatUsdCompact` call `microsBigInt` (junk →
   `$0.00`). Tests: `treats API junk as zero instead of throwing`.
2. `smoke.mjs` handles `--help` first, then `requireFlagValue` for
   `--amount-in` / `--slippage-bps` / `--api` / `--rpc` (undefined or a
   following `--flag` → `❌ ${flag} requires a value`, exit 1) before
   `loadSecret` / `BigInt(flags.amountIn)`.
3. `stats::empty` lowercases catalog addresses the same way as
   `route_health_for_engine`. Test:
   `empty_route_health_uses_lowercase_catalog_addresses`.
4. `put_swap` is `redis::pipe().atomic()` SET NX + always-ZADD;
   `Ok(set_nx.is_some())`. In-memory `index_swap` (test-only) always
   writes `by_time` so replay heals an orphaned swap key.
5. `render.yaml` `CHAKRA_CORS_ORIGINS` is production aliases +
   `http://localhost:3000`. Test:
   `render_cors_allowlist_is_production_and_localhost` via
   `include_str!("../../../render.yaml")`.

### Leftovers (unchanged)

- T11.10 / T11.11 headed MetaMask remain blocked. The 2026-09-04 viem CLI
  tx `0x2df6e81aa9ff0805aad7d49241ccdd9e979dd7c0dae1b261c51ed469542236c5` is
  a different evidence path and does not close those tasks.
- T11.12 split-route live evidence (`split_swaps` still 0).
- `cargo fmt --all -- --check` wrapping/import drift; do not mass-format.
- Frontend `@vitest/coverage-v8` is not a declared dependency; do not add it.

### Checklist

- Design match: yes.
- Logic gaps: none blocking.
- Security: env-only wallet secret; Canteen RPC banned; paused aggregator
  503; rate limit on non-health paths; CORS allowlist is production aliases
  + localhost in `render.yaml` (live Render still needs a redeploy).
- Integration: live `/health` `/ready` `/stats` and production `/stats`
  dashboard verified in the Phase 9 review; P3 CORS is config-only until
  redeploy.
- Tests after P3: `api-server --lib` 26 passed; `market-data-worker --lib`
  43 passed; frontend vitest 96 / 13 files; node smoke tests 5 passed;
  clippy `-D warnings` on those two crates clean. Workspace cargo 276 and
  forge 88 were the Phase 9 review gates, not re-run for this P3 pass.
- Docs: this section plus the testing Phase 9 record.

Ready to push and open a PR when asked. Do not commit unless asked.
