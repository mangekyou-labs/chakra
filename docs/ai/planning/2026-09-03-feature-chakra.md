# Chakra feature plan

Status: backend hotfix implemented and locally verified; **stage-1 rollout is
pending deploy and live acceptance** (see § Rollout below). Planning doc
reconciled 2026-09-04.

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
   of token decimal scale. — done locally; live acceptance pending.

## Rollout status

- **Stage 1 backend merged.** `chakra/main` carries the stage-1 backend at
  `d937a69` (PR #2, merge commit) plus the discovery hotfix at `8d36f69`
  (PR #3). Only Rust, Render config, QA tooling, and lifecycle docs were
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

- **RESOLVED LOCALLY:** the live-reserve regression now quotes all four cirBTC
  directions. The old `MIN_XYK_RESERVE_ATOMIC_UNITS = 100_000_000` floor was
  sized for 6-dp stablecoins and rejected the live UnitFlow cirBTC side
  (`122,883` atoms). The hotfix removes only that guard from local XYK paths;
  zero reserves, zero exact output, factory policy, and slippage remain
  enforced. Deployment and live acceptance are still required.
- Observation window (15 min, no retry storm, lag ≤100, freshness <300), strict
  `/ready`, and the QA-wallet swap + analytics attribution remain queued behind
  the hotfix deploy.

## Next steps (after the dust-floor fix merges + redeploys)

1. Commit and release the focused backend hotfix, then verify all six directed
   quotes succeed and strict `/ready` returns 200 with the expected live pools.
2. Observe health, freshness, lag, and retry behavior for 15 minutes.
3. Execute exactly one QA-wallet USDC→cirBTC swap (1,000,000 atomic USDC,
   50 bps, canonical multihop through EURC + UnitFlow) and confirm the
   attributed analytics record after 12 confirmations.
4. Stage 2: commit the dashboard + final docs, PR → merge, confirm the
   chakra-arc-dex Vercel deploy reaches Ready (both production aliases),
   recheck Render health/readiness after the main merge.

## Summary

Implementation is complete and green (tasks 1-5 plus the dust-policy regression);
stage 1 merged and deployed twice with two post-merge fixes (discovery starvation,
env regression), and the worker on Render is healthy: discovery republishes every
10 minutes,
pool state is written continuously, analytics is live with lag 0. The local
hotfix removes the flat 6-dp dust floor that rejected the real (thin) UnitFlow
EURC/cirBTC pool. The live-reserve test now proves direct and multihop cirBTC
quotes; deploy, six-direction live readiness, observation, and the
approval-gated QA swap remain before stage 2 dashboard release.
