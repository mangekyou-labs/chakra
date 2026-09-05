# Chakra feature plan

Status: implementation complete; stage 1 backend and stage 2 `/stats`
dashboard are live. Phase 7 Check Implementation and Phase 9 `dev-review`
on 2026-09-05 confirmed HEAD `318de72` (plus the uncommitted Phase 7/8
delta) against the 2026-09-03 design with no product-code miss. The five
Phase 9 P3 nits are on `feature-chakra` at `68953e3` (PR #10, not merged).
Follow-ups 1, 2, 4, and 5 from that PR are done in the uncommitted worktree
delta; T11.12 split-route live evidence remains open (`split_swaps` 0).

- [x] Watcher topic batches, merge/dedupe, WS acks, -32005/-32012
- [x] Additive analytics, six-probe `/ready`, `GET /api/v1/stats`
- [x] Viem QA smoke CLI (env-only secret, dry-run default)
- [x] `/stats` dashboard + BigInt USD + URL range
- [x] Local/live gates (`cargo fmt --all -- --check` green after stable-only rustfmt.toml)
- [x] Two-stage rollout
- [x] XYK dust policy / thin cirBTC reserves
- [x] Controlled QA swap + analytics attribution
- [x] Stage 2 Vercel `/stats` production acceptance

1. Watcher reliability: topic batching (10 + 3), merge/dedupe polling, WS
   multi-subscription with ack validation (+ native-tls for wss), and
   -32005/-32012 retry policy. — done, unit + live smoke verified.
2. Backend analytics: additive Redis indexer, strict six-probe readiness, and
   `/api/v1/stats` with honest heads/lag/freshness. — done, fixture tests green.
3. QA production smoke (viem CLI, env-only secret, dry-run default,
   `--broadcast` gate). — done, live preflight verified (QA wallet funded:
   3.97 USDC + gas).
4. Dashboard: `/stats` page, BigInt USD formatting, range in URL, loading /
   empty / error / stale-response states. — done and released on both
   production aliases (`dpl_A1v6Nt3McDZjFXh7gf5MsqsJgDFn`).
5. Gates: fmt, workspace tests, clippy -D warnings, forge, frontend
   tests/typecheck/lint/prettier, production build, Docker (rust:1.88),
   live Arc worker smoke (3 pools, 2 topic batches, no -32012). — all passed.
   `cargo fmt --all -- --check` is green after `rustfmt.toml` dropped
   nightly-only keys and the wrapping diffs were formatted under stable
   1.88.0.
6. Rollout (two-stage) — done. Stage 1 backend + hotfixes on Render; stage 2
   dashboard live on both Vercel production aliases. See rollout status.

7. XYK dust policy regression: curated, factory-allowlisted pools with both
   nonzero reserves and nonzero exact integer output are eligible, regardless
   of token decimal scale. — implemented, regression-tested, deployed, and
   live-accepted.

## Rollout status

- **Stage 1 backend merged.** `chakra/main` carries the stage-1 backend at
  `d937a69` (PR #2, merge commit) plus the discovery hotfix at `8d36f69`
  (PR #3), the thin-reserve hotfix at `9c5c41a` (PR #4), and the readiness
  diagnostics fix at `834a65b` (PR #5). Only Rust, Render config, QA tooling,
  and lifecycle docs were
  committed; dashboard files stay uncommitted until stage 2.
- **Post-merge incidents fixed (new scope):**
  - *Discovery starvation* (worker published once at boot, then never —
    `/ready` routes went cold): `run()` queued 500 ms poll ticks through an
    mpsc channel; a slow `poll_once` backlogged Discovery events for hours.
    Fixed with a `tokio::select!` loop + Skip-mode interval timers; regression
    test `slow_poll_never_starves_discovery_cycle`. Live-verified on Render:
    discovery republishes every 10 min (`pools=3`), pool-state writes every
    minute, analytics lag 0 / freshness <30 s, WS `batches=2` acked. Documented
    in `docs/ai/deployment/2026-09-03-feature-chakra.md`.
  - *Env regression* (self-inflicted during rollout): an env PUT swapped
    `CHAKRA_REDIS_URL` to the external `rediss://…` connection string; the
    redis client has no TLS feature, so both processes panicked at startup
    (exit 101). Restored the internal `redis://red-da86lmfavr4c73ekh2t0:6379`
    string (what render.yaml `fromKeyValue` injects). Operational rule: keep
    `fromKeyValue` as the source of truth; never replace env wholesale, and
    never use the external KV string without enabling redis TLS.
- **Live deploy:** `dep-dad5ctlg1s2s73f4d9u0` (commit `8d36f69`) is live;
  `/health` 200, `/stats` 200 (lag 0, freshness <30 s), USDC↔EURC quotes
  succeed via Presto/Xylo. Six-pool state present incl. UnitFlow
  `0x268D…9200` (EURC 525,211,244 / cirBTC 122,883 atoms).

## Blockers

- **RESOLVED:** the live-reserve regression now quotes all four cirBTC
  directions. The old `MIN_XYK_RESERVE_ATOMIC_UNITS = 100_000_000` floor was
  sized for 6-dp stablecoins and rejected the live UnitFlow cirBTC side
  (`122,883` atoms). The hotfix removes only that guard from local XYK paths;
  zero reserves, zero exact output, factory policy, and slippage remain
  enforced. The fix is live and accepted on Render.
- The 15-minute observation, strict `/ready`, and six-direction live quote
  gates passed. The QA-wallet swap and analytics attribution are now confirmed.

## Next steps

1. **Complete:** execute exactly one QA-wallet USDC→cirBTC swap (1,000,000
   atomic USDC, 50 bps, canonical multihop through EURC + UnitFlow). Receipt
   `0x2df6e81aa9ff0805aad7d49241ccdd9e979dd7c0dae1b261c51ed469542236c5`
   mined in block `60438104`; after 12 confirmations, stats showed one new
   attributed/confirmed swap and 1,000,000 stablecoin-notional micros, with
   Presto and UnitFlow attribution.
2. **Complete:** stage 2 merged via PRs #7 and #8; production deployment
   `dpl_A1v6Nt3McDZjFXh7gf5MsqsJgDFn` is Ready and both production aliases serve
   `/stats`. Render health, readiness, stats freshness, and all six routes were
   rechecked successfully.

## Summary

Implementation is complete and green (tasks 1-7 plus the dust-policy regression);
stage 1 is merged and deployed with the discovery, thin-reserve, and readiness
fixes. The worker on Render is healthy: discovery republishes every 10 minutes,
pool state is written continuously, analytics is live with lag 0, and the
15-minute acceptance window passed. The controlled QA swap is confirmed and
the stage 2 dashboard is live on both production aliases.

## Phase 7 recon (2026-09-05)

Items 4 and 6 above still said “not yet released” / “in progress” after the
stage-2 acceptance section was already written. This recon flips those
bullets to match shipped reality. File-by-file alignment is in the
implementation doc; fresh gates are in the testing doc.

T11.10 / T11.11 headed MetaMask settlement stay blocked on the provider
notification page (see `docs/ai/planning/2026-08-31-t11-chakra-arc-only-cleanup.md`).
The viem CLI swap is not that evidence. T11.12 split-route live evidence is
still a follow-up (`split_swaps` remains 0).

## Phase 9 recon (2026-09-05)

Phase 9 `dev-review` of HEAD `318de72` plus the uncommitted Phase 7/8 delta
passed with no product-code miss. File-by-file alignment is in the
implementation doc; fresh gates and the production `/stats` walk are in
the testing doc. The five P3 nits from that review are now fixed in the
uncommitted delta (TDD red then green; evidence in the testing doc).

P3 nits plus Phase 7–9 docs were committed as `68953e3` and opened as PR #10.
Do not merge that PR unless asked.

## Follow-ups after PR #10 (2026-09-05)

Authorized leftover list after the PR: live Render CORS, T11.10 / T11.11
headed MetaMask, T11.12 live `split_swaps`, rustfmt wrapping, frontend
`@vitest/coverage-v8`.

1. Live CORS: production aliases + `http://localhost:3000` allowed; leftover
   preview aliases no longer echo `Access-Control-Allow-Origin`.
2. T11.10 / T11.11: headed MetaMask 1 USDC → EURC settled. Receipt
   `0xee7bc19a990ce6691a68e9b387585baee13edc846cbf3a43551ab3dd7cfcda6c`.
3. T11.12: still open. Live `split_swaps` is 0; probed quotes are
   `is_split: false`. Do not manufacture liquidity.
4. rustfmt: `rustfmt.toml` is stable-only; `cargo fmt --all -- --check`
   exit 0.
5. `@vitest/coverage-v8` `^4.1.11` is a declared frontend devDependency;
   `npx vitest run --coverage` reports v8 coverage (104 tests / 14 files).

Next: commit / push this uncommitted follow-up delta onto PR #10 when asked.
Do not merge. Do not claim T11.12 closed.
