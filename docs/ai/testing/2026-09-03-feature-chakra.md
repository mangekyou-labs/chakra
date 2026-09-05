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

## Production frontend acceptance (2026-09-04)

- Vercel deployment `dpl_A1v6Nt3McDZjFXh7gf5MsqsJgDFn` reached Ready after the
  `/stats` static-export rewrite was merged.
- Both production aliases returned HTTP 200 for `/stats`. Playwright browser
  smoke loaded `/stats?range=30d`, displayed the live 2 confirmed swaps and
  $2.00 notional, and rendered six healthy route-health directions plus Presto
  and UnitFlow venue rows.
- Final Render stats check reported chain/indexed/confirmed heads, lag 0,
  freshness 21 seconds, six healthy routes, and the expected 1 attributed / 2
  confirmed swaps.

## Fresh local verification (2026-09-04)

- `cargo test -p router-engine`: 51 passed.
- `cargo test -p api-server --test chakra_venues_test`: 7 passed.
- `cargo test --workspace`: all workspace tests passed (275 tests; doc-tests
  had no test cases).
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`: remains blocked by pre-existing formatting drift
  in unrelated areas (`crates/api-server/src/stats.rs`, `crates/dex-adapters`,
  and `crates/market-data-worker`); no unrelated formatting was applied.

## Phase 7 check verification (2026-09-05)

Worktree `/Users/kyler/repos/avax-dex-agg/.worktrees/feature-chakra` at
`318de72` (`feature-chakra` tracking `chakra/feature-chakra`), clean.
`npx ai-devkit@latest lint --feature chakra` passed. Task tracing is
unavailable (`npx ai-devkit@latest task list --name chakra --json` →
`unknown command 'task'`).

Local gates, this session:

- `cargo test -p router-engine --offline`: 51 passed, 2 suites, exit 0.
- `cargo test -p api-server --test chakra_venues_test --offline`: 7 passed,
  1 suite, exit 0.
- `cargo test --workspace --offline`: 276 passed, 16 suites, 7.59s, exit 0
  (one more unit test than the 2026-09-04 count of 275).
- `cargo clippy --workspace --all-targets --offline -- -D warnings`: no
  issues, exit 0.
- `cargo fmt --all -- --check`: exit 1. Wrapping/import diffs in this
  feature's files: `crates/api-server/src/stats.rs`,
  `crates/dex-adapters/src/evm_logs.rs`, `crates/dex-adapters/src/evm_rpc.rs`,
  `crates/market-data-worker/src/analytics.rs`,
  `crates/market-data-worker/src/evm_watcher.rs`. rustfmt also warns that
  nightly-only rustfmt.toml options cannot be set on stable. No mass-format
  was applied.
- `packages/frontend`: `npm test -- --run` 90 passed / 13 files, exit 0;
  `npm run typecheck` (`tsc --noEmit`) exit 0.

Live read-only (no broadcast, no `QA_WALLET_SECRET`):

- `GET https://chakra-api-0a5i.onrender.com/api/v1/health` HTTP 200
  `status: ok`.
- `GET /api/v1/ready` HTTP 200 `ready: true`,
  `snapshot_id: snapshot-1788538877594`, `pool_keys: []` (cluster mode does
  not list keys; six-probe still required via `CHAKRA_STRICT_READINESS`).
- `GET /api/v1/stats?range=all` HTTP 200:
  `chain_head` 60442675, `confirmed_head` / `indexed_head` 60442663,
  `lag_blocks` 0, `freshness_secs` 1, `attributed_swaps` 1,
  `unattributed_swaps` 1, overview notional `"2000000"` micros,
  `confirmed_swaps` 2, `unique_traders` 1, `split_swaps` 0; daily buckets
  2026-08-30 and 2026-09-04; venues `presto-hub` and `unitflow-v25`; six
  route-health directions (USDC↔EURC direct, USDC↔cirBTC multihop,
  EURC↔cirBTC direct).

T11.10 / T11.11 headed MetaMask remain blocked. This check did not rerun
forge, Docker, or a headed `/stats` browser walk; those stay on the
2026-09-04 records.

## Phase 8 write tests (2026-09-05)

Worktree `/Users/kyler/repos/avax-dex-agg/.worktrees/feature-chakra` on
`feature-chakra` tracking `chakra/feature-chakra`, HEAD `318de72` plus
uncommitted Phase 7 lifecycle docs and the Phase 8 test additions below.
`npx ai-devkit@latest lint --feature chakra` passed from the worktree cwd.
Task tracing is unavailable (`npx ai-devkit@latest task list --name chakra --json`
→ `unknown command 'task'`).

### Gap analysis

Requirement map vs already-shipped tests (unchanged; not duplicated):

| Requirement | Existing tests |
| --- | --- |
| Watcher 10+3, merge/dedupe, cursor only after all batches | `crates/dex-adapters/src/evm_logs.rs`; `evm_watcher.rs` (`poll_keeps_cursor_when_any_topic_batch_fails`) |
| RPC retry `-32005`, never retry `-32012` | `crates/dex-adapters/src/evm_rpc.rs` |
| XYK: nonzero reserves + nonzero exact output; no atomic floor | `crates/api-server/tests/chakra_venues_test.rs` |
| Six-probe ready / stats heads/lag/freshness / empty history | `crates/api-server/src/stats.rs`; venues fixture suite (7 tests) |
| Dashboard BigInt USD, URL range, abort/stale, empty/error/skeleton, cirBTC | `packages/frontend/src/app/stats/page.test.tsx` (6); `src/lib/stats-format.test.ts` |
| QA preflight BigInt + `TRANSFER_FROM_FAILED` | `packages/frontend/qa/swap/preflight.test.mjs` (3) |

Confirmed uncovered branches closed this phase:

1. QA smoke CLI process contract (`loadSecret` / `--broadcast` / `--help`).
2. `chartGeometry` BigInt scaling (empty / single-point / multi-point / max=0).

### Tests added

- `packages/frontend/qa/swap/smoke.test.mjs` — `node --test` spawn of
  `smoke.mjs`. Unsets `QA_WALLET_SECRET` in the child env. Never prints a
  secret, never passes one on the command line, never uses `--broadcast` with
  a funded wallet.
  - missing secret → exit 1
  - `--help` → exit 0 without a secret
  - `--broadcast` without env is not treated as a wallet secret → exit 1
  - default is dry-run (`--help` text + no `BROADCAST` when `--broadcast` is absent)
- `packages/frontend/src/lib/stats-format.test.ts` — four `chartGeometry`
  cases. TDD red: `chartGeometry is not a function`. Green: moved
  `chartGeometry` (and `CHART_WIDTH` / `CHART_HEIGHT` / `CHART_PAD`) from
  `src/app/stats/page.tsx` into `src/lib/stats-format.ts` (export only; same
  BigInt-then-Number math).

### Fresh local gates (this session)

- `cargo test -p router-engine --offline`: 51 passed, exit 0.
- `cargo test -p api-server --test chakra_venues_test --offline`: 7 passed, exit 0.
- `cargo test --workspace --offline`: 276 passed, exit 0
  (api-server 24+17+12+7+10, dex-adapters 82, market-data-worker 40,
  market-snapshot 33, router-engine 51).
- `cargo clippy --workspace --all-targets --offline -- -D warnings`: exit 0.
- `cargo fmt --all -- --check`: exit 1. Known wrapping/import drift in
  `crates/api-server/src/stats.rs`, `crates/dex-adapters/src/evm_logs.rs`,
  `crates/dex-adapters/src/evm_rpc.rs`, `crates/market-data-worker/src/analytics.rs`,
  `crates/market-data-worker/src/evm_watcher.rs`. Nightly-only rustfmt.toml
  options ignored on stable. No mass-format.
- `packages/frontend`: `npm test -- --run` 94 passed / 13 files, exit 0
  (was 90; +4 `chartGeometry`). `npm run typecheck` (`tsc --noEmit`) exit 0.
- `node --test packages/frontend/qa/swap/preflight.test.mjs qa/swap/smoke.test.mjs`:
  7 passed (3 preflight + 4 smoke spawn), exit 0.
- `cd contracts/evm && forge test`: 88 passed, 0 failed, exit 0
  (forge 1.5.1-stable).

### Coverage tooling

- `cargo llvm-cov --version`: `cargo-llvm-cov 0.8.7` (host install, not a
  repo crate). `cargo llvm-cov --workspace --offline --summary-only`:
  81.05% regions / 81.35% functions / 82.31% lines. Feature files of
  interest: `api-server/src/stats.rs` 94.46% lines; `dex-adapters/src/evm_logs.rs`
  89.04%; `dex-adapters/src/evm_rpc.rs` 90.81%; `market-data-worker/src/evm_watcher.rs`
  80.03%. Not added to manifests or CI.
- `cargo tarpaulin --version`: not installed (exit 101).
- Frontend: `@vitest/coverage-v8` is not a declared `packages/frontend`
  dependency (lockfile lists it only as a Vitest optional). `npx vitest run --coverage`
  → `MISSING DEPENDENCY  Cannot find dependency '@vitest/coverage-v8'`.
  Not installed.

Phase 8 coverage evidence is the requirement map, the two new targeted
suites, the fresh 276 + 94 + 7 + 88 pass counts, and the host `llvm-cov`
summary. Frontend V8 coverage remains a tooling gap.

### Live read-only API

`https://chakra-api-0a5i.onrender.com` (no broadcast, no wallet secret):

- `GET /api/v1/health` HTTP 200 `status: ok`.
- `GET /api/v1/ready` HTTP 200 `ready: true`,
  `snapshot_id: snapshot-1788541878045`, `pool_keys: []`.
- `GET /api/v1/stats?range=all` HTTP 200:
  `chain_head` 60445309, `confirmed_head` / `indexed_head` 60445297,
  `lag_blocks` 0, `freshness_secs` 6, `attributed_swaps` 1,
  `unattributed_swaps` 1, overview notional `"2000000"` micros,
  `confirmed_swaps` 2; venues `presto-hub` and `unitflow-v25`; six
  route-health directions (USDC↔EURC direct, USDC↔cirBTC multihop,
  EURC↔cirBTC direct).

### playwright-cli `/stats` walk

`playwright-cli` 0.1.18. Production aliases only. No MetaMask, no wallet
connect. Sessions closed with `playwright-cli close-all`.

- Desktop 1280×720 `https://chakra-ag.vercel.app/stats`: live `$2.00`
  notional, 2 confirmed swaps, venues `presto-hub` / `unitflow-v25`, six
  cirBTC route-health rows. Click **All time** → URL
  `?range=all`, `GET .../api/v1/stats?range=all` HTTP 200.
- Mobile (`--mobile`) same host: hamburger nav, same live counters and six
  routes. Click **Last 14 days** → URL `?range=14d`,
  `GET .../api/v1/stats?range=14d` HTTP 200.
- Second alias `https://chakra-arc-dex.vercel.app/stats` 1280×720: same live
  dashboard. Click **Last 90 days** → URL `?range=90d`,
  `GET .../api/v1/stats?range=90d` HTTP 200.

### Leftovers

- T11.10 / T11.11 headed MetaMask settlement still blocked.
- T11.12 split-route live evidence still a follow-up (`split_swaps` 0).
- `cargo fmt --all -- --check` remains red on wrapping drift; do not mass-format.
- Frontend `@vitest/coverage-v8` not declared; do not add it in this phase.

## Phase 9 review verification (2026-09-05)

Worktree `/Users/kyler/repos/avax-dex-agg/.worktrees/feature-chakra` on
`feature-chakra` tracking `chakra/feature-chakra`, HEAD `318de72` plus the
uncommitted Phase 7/8 delta. `npx ai-devkit@latest lint --feature chakra`
passed from the worktree cwd. Task tracing is unavailable
(`npx ai-devkit@latest task` → `unknown command 'task'`). Commands that
were started from the parent LumAgg tree were discarded; every gate below
used the worktree cwd.

### Local gates (this session)

- `cargo test --workspace --offline`: 276 passed, 16 suites, 6.82s, exit 0.
- `cargo clippy --workspace --all-targets --offline -- -D warnings`: no
  issues, exit 0.
- `cargo fmt --all -- --check`: exit 1. Wrapping/import diffs in
  `crates/api-server/src/stats.rs`, `crates/dex-adapters/src/evm_logs.rs`,
  `crates/dex-adapters/src/evm_rpc.rs`,
  `crates/market-data-worker/src/analytics.rs`,
  `crates/market-data-worker/src/evm_watcher.rs`. Nightly-only rustfmt.toml
  options ignored on stable. No mass-format.
- `packages/frontend`: `npm test -- --run` 94 passed / 13 files, exit 0;
  `npm run typecheck` (`tsc --noEmit`) exit 0.
- `node --test packages/frontend/qa/swap/preflight.test.mjs packages/frontend/qa/swap/smoke.test.mjs`:
  7 passed, exit 0.
- `cd contracts/evm && forge test`: 88 passed, 0 failed, exit 0.

### Live read-only API

`https://chakra-api-0a5i.onrender.com` (no broadcast, no wallet secret):

- `GET /api/v1/health` HTTP 200 `{"success":true,"data":{"status":"ok"}}`.
- `GET /api/v1/ready` HTTP 200, `ready: true`,
  `snapshot_id: snapshot-1788544546076`, `pool_keys: []`.
- `GET /api/v1/stats?range=all` HTTP 200:
  `chain_head` 60550987, `confirmed_head` / `indexed_head` 60550975,
  `lag_blocks` 0, `freshness_secs` 10, `attributed_swaps` 1,
  `unattributed_swaps` 1, overview notional `"2000000"` micros,
  `confirmed_swaps` 2, `unique_traders` 1, `split_swaps` 0; daily buckets
  2026-08-30 and 2026-09-04; venues `presto-hub` and `unitflow-v25`; six
  lowercase catalog route-health rows (USDC↔EURC Direct 2 pools,
  USDC↔cirBTC Multihop 3 pools, EURC↔cirBTC Direct 1 pool).

### playwright-cli `/stats` walk

`playwright-cli` 0.1.18, session `chakra-stats`, production alias
`https://chakra-ag.vercel.app/stats`. No MetaMask, no wallet connect.
Closed with `playwright-cli -s=chakra-stats close` (did not `close-all`;
unrelated sessions were left running).

- Default `?range=30d`: Notional `$2.00`, Confirmed swaps `2`, Unique
  traders `1`, Split share `0%`, chart peak `$1.00`, venues presto-hub /
  unitflow-v25, six cirBTC route-health rows, indexed/confirmed/chain
  ~60.6M, lag 0 blocks, refreshed ~17s, 1 unattributed.
- Click **All time** → URL `?range=all`, same live counters, chart subtitle
  “USD · All time”.
- Click **Last 14 days** → URL `?range=14d`.
- Network: `GET .../api/v1/stats?range=30d|all|14d` all HTTP 200.
- Console: 0 errors / 0 warnings.

Phase 8 already walked the second production alias
(`https://chakra-arc-dex.vercel.app/stats`) and a mobile viewport; this
review did not repeat those.

### Verdict

Pass. No P0/P1/P2 product-code miss. Leftovers unchanged: T11.10 / T11.11
headed MetaMask, T11.12 `split_swaps` still 0, rustfmt wrapping drift,
frontend `@vitest/coverage-v8` not declared. P3 nits were listed in the
implementation Phase 9 section and were not fixed in the review itself.

### P3 nits verification (same day, after review)

Worktree cwd. TDD red was confirmed, then production, then these greens:

- `npx vitest run src/lib/stats-format.test.ts`: 23 passed, including
  junk → `$0.00` for `formatMicrosUsd` / `formatUsdCompact`.
- `packages/frontend`: `npm test -- --run` 96 passed / 13 files, exit 0;
  `npx tsc --noEmit` exit 0.
- `node --test packages/frontend/qa/swap/smoke.test.mjs`: 5 passed,
  including `--amount-in` without a value →
  `--amount-in requires a value` before `QA_WALLET_SECRET`.
- `cargo test -p api-server --lib --offline
  empty_route_health_uses_lowercase_catalog_addresses`: 1 passed.
- `cargo test -p api-server --lib --offline
  render_cors_allowlist_is_production_and_localhost`: 1 passed.
- `cargo test -p market-data-worker --lib --offline index_swap`: 3
  passed (insert, replay-idempotent, heal missing `by_time`).
- `cargo test -p api-server --lib --offline`: 26 passed.
- `cargo test -p market-data-worker --lib --offline`: 43 passed.
- `cargo clippy -p market-data-worker -p api-server --offline
  --all-targets -- -D warnings`: exit 0 (`index_swap` is `#[cfg(test)]`).
- `npx ai-devkit@latest lint --feature chakra`: all checks passed.

Live `/health` `/ready` `/stats` and playwright `/stats` were not
re-walked for this P3 pass. Live Render CORS still has preview aliases
until `render.yaml` is redeployed.

Next: commit / `dev-pr` when asked. Do not claim T11.10 / T11.11 / T11.12
closed.
