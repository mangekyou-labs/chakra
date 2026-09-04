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

## Fresh local verification (2026-09-04)

- `cargo test -p router-engine`: 51 passed.
- `cargo test -p api-server --test chakra_venues_test`: 7 passed.
- `cargo test --workspace`: all workspace tests passed (275 tests; doc-tests
  had no test cases).
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: remains blocked by pre-existing formatting drift
  in untouched files (`crates/api-server/src/stats.rs`, `crates/dex-adapters`,
  and `crates/market-data-worker`); no unrelated formatting was applied.
