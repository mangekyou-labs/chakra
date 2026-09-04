# Chakra feature plan

Status: backend hotfix implemented and locally verified; stage 1 is deployed
and live-accepted. The controlled QA transaction and analytics attribution
completed on 2026-09-04; stage 2 is ready for commit and review.

1. Watcher reliability: topic batching (10 + 3), merge/dedupe polling, WS
   multi-subscription with ack validation (+ native-tls for wss), and
   -32005/-32012 retry policy. — done, unit + live smoke verified.
2. Backend analytics: additive Redis indexer, strict six-probe readiness, and
   `/api/v1/stats` with honest heads/lag/freshness. — done, fixture tests green.
3. QA production smoke (viem CLI, env-only secret, dry-run default,
   `--broadcast` gate). — done, live preflight verified (QA wallet funded:
   3.97 USDC + gas).
4. Dashboard: `/stats` page, BigInt USD formatting, range in URL, loading /
   empty / error / stale-response states. — code done + frontend gates green;
   **not yet released (stage 2)**.
5. Gates: fmt, workspace tests, clippy -D warnings, forge, frontend
   tests/typecheck/lint/prettier, production build, Docker (rust:1.88),
   live Arc worker smoke (3 pools, 2 topic batches, no -32012). — all passed.
6. Rollout (two-stage) — in progress, see below.

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
2. Stage 2: commit the dashboard + final docs, PR → merge, confirm the
   chakra-arc-dex Vercel deploy reaches Ready (both production aliases),
   recheck Render health/readiness after the main merge.

## Summary

Implementation is complete and green (tasks 1-5 plus the dust-policy regression);
stage 1 is merged and deployed with the discovery, thin-reserve, and readiness
fixes. The worker on Render is healthy: discovery republishes every 10 minutes,
pool state is written continuously, analytics is live with lag 0, and the
15-minute acceptance window passed. The controlled QA swap is confirmed; the
separate stage 2 dashboard release is next.
