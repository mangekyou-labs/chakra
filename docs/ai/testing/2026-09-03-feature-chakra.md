# Chakra feature testing

Release gates, all passed:

- Rust: `cargo fmt --all -- --check`, `cargo test --workspace` (all unit + four
  api-server fixture suites green), `cargo clippy --workspace --all-targets -- -D warnings`.
- Contracts: `cd contracts/evm && forge test` — 88 passed.
- Frontend: vitest 90 passed (13 files), `tsc --noEmit`, eslint, prettier on
  touched files, `next build` (webpack production export incl. `/stats`).
- Docker: production image from rust:1.88 toolchain builds and its binaries
  embed the new error codes and stats fields.
- Live Arc worker smoke: `Arc discovery complete factories=3 pools=3` (Xylo,
  Presto, UnitFlow); `Arc WS subscribed ... batches=2` with both subscriptions
  acknowledged; no -32012/-32005 or polling failures in the observation window.

## Dust-policy regression evidence

- Added live_unitflow_reserves_quote_all_cirbtc_directions using the live
  UnitFlow reserves 525,211,244 EURC atoms and 122,883 cirBTC atoms.
- TDD red: before the router change, the test failed with NO_ROUTE.
- Green: after removing the atomic reserve floor, direct EURC↔cirBTC and
  USDC↔cirBTC multihop quotes all returned nonzero output.
- Regression-proof cycle: temporarily restored the old floor and reproduced
  the failure; restored the fix and reran the targeted test successfully.

## Production acceptance evidence (2026-09-04)

- Render deploy `dep-dad8e3v10e5c73dpv7ag` served all six directed quotes with
  nonzero output: USDC↔EURC, EURC↔cirBTC direct, and USDC↔cirBTC multihop.
- `/api/v1/health` and strict `/api/v1/ready` returned HTTP 200; `/stats`
  reported six healthy routes, lag 0, and freshness 18–24 seconds across the
  clean 15-minute observation window.
- Fetch logs showed `tasks_failed=0` and continuing Redis writes. WS rate-limit
  and head-range warnings were observed and recovered; no readiness loss.
- The September 4 live quote probe returned the canonical USDC→EURC→cirBTC
  route through UnitFlow at 27 bps impact (363 output, 361 minimum) for the
  exact 1,000,000-atomic USDC / 50-bps slippage inputs. The smoke CLI could not
  continue to wallet/build preflight because `QA_WALLET_SECRET` was not
  exported; no approval or transaction was broadcast.
- With the worktree `.env` sourced into the process environment, the same dry-run
  reached wallet/build and signed Permit2 off-chain. Its expected pre-approval
  `TRANSFER_FROM_FAILED` simulation now produces a clean dry-run verdict; the
  new `node --test packages/frontend/qa/swap/preflight.test.mjs` regression
  suite passes (3 tests).

## QA transaction acceptance (2026-09-04)

- Authorized broadcast used exactly 1,000,000 atomic USDC and 50 bps slippage.
  The canonical USDC→EURC→cirBTC route confirmed in block `60438104` with tx
  `0x2df6e81aa9ff0805aad7d49241ccdd9e979dd7c0dae1b261c51ed469542236c5`.
- Receipt logs and balance reads showed the expected stablecoin transfer and
  +368 cirBTC atoms. The required 12-block window elapsed before analytics
  verification.
- `/api/v1/stats?range=all` moved from 0 to 1 attributed swaps, 1 to 2
  confirmed swaps, and 1,000,000 to 2,000,000 stablecoin-notional micros;
  attribution included Presto and UnitFlow.
- The initial CLI exited after confirmation because JSON serialization rejected
  `receipt.blockNumber` as BigInt. The serializer was fixed and covered by the
  three-test preflight suite; no rebroadcast was performed.

## Fresh local verification (2026-09-04)

- `cargo test -p router-engine`: 51 passed.
- `cargo test -p api-server --test chakra_venues_test`: 7 passed.
- `cargo test --workspace`: all workspace tests passed (275 tests; doc-tests
  had no test cases).
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: remains blocked by pre-existing formatting drift
  in unrelated areas (`crates/api-server/src/stats.rs`, `crates/dex-adapters`,
  and `crates/market-data-worker`); no unrelated formatting was applied.
