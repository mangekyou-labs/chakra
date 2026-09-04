# Chakra feature deployment

Two-stage release from the feature-chakra branch. Stage 1 ships the Rust
watcher/API, render.yaml, QA tooling, and backend lifecycle docs; stage 2 ships
the `/stats` dashboard and final documentation. Render worker/API use the
existing Arc targets and additive `chakra:analytics:*` namespace; the frontend
deploys to the linked chakra-arc-dex Vercel project. Rollback reference for the
prior backend: `dep-daagnntg1s2s73d4rh70`.

## 2026-09-04 hotfix: discovery starvation in the Arc worker loop

The stage-1 worker published its bootstrap snapshot once at boot and then never
again (prod snapshot went ~7h stale while the analytics poller stayed fresh).
Root cause: `run()` funneled 500 ms poll ticks through an mpsc event queue. Each
`poll_once` performs 1-3 sequential RPCs, so whenever the Arc endpoints ran
slower than one tick, Poll events accumulated in an unbounded backlog that
pushed the 600 s Discovery event out for hours — the loop stayed alive (poll
failures logged) but never reached discovery, so `/ready` pool routes went cold.

Fix: the run loop is now `tokio::select!`-driven with `MissedTickBehavior::Skip`
on the poll and discovery interval timers; WS logs arrive on their own channel.
A slow poll can no longer backlog past a due discovery tick. Regression test
`slow_poll_never_starves_discovery_cycle` proves boot + ≥2 periodic discovery
cycles run while each poll outlasts its own tick cadence. Live check with a 30 s
discovery interval: 4 discovery publishes in <100 s (old binary: 1 in 3 h).

## 2026-09-04 hotfix: executable thin XYK reserves and readiness diagnostics

PR #4 (`2f2d1fc`, merged as `9c5c41a`) removed the unit-agnostic
`MIN_XYK_RESERVE_ATOMIC_UNITS` guard from local XYK quote paths. PR #5
(`f248252`, merged as `834a65be`) normalized catalog addresses in route
diagnostics so lowercase snapshot topology is reflected by `/stats` and strict
`/ready`. Neither change alters a public response shape, schema, or migration.

Render deploy `dep-dad8e3v10e5c73dpv7ag` is live from `834a65be`. On 2026-09-04,
all six directed production quotes succeeded, `/api/v1/ready` returned HTTP
200, and five samples spanning a clean 15-minute observation window remained
ready with lag 0, freshness 18–24 seconds, and all six routes healthy. Worker
logs showed completed fetch tasks, zero failed tasks, and ongoing Redis writes;
the existing Arc RPC WS rate-limit and head-range warnings recovered without
loss of readiness.

The exact QA quote probe on September 4 returned the canonical USDC → EURC →
cirBTC route through UnitFlow with 27 bps impact (363 cirBTC atomic output and
361 minimum output), below the tool's 100-bps safety threshold. The explicitly
authorized 1,000,000-atomic swap then confirmed in block `60438104` (tx
`0x2df6e81aa9ff0805aad7d49241ccdd9e979dd7c0dae1b261c51ed469542236c5`). After
12 confirmations, `/api/v1/stats?range=all` showed the expected +1 attributed
swap, +1 confirmed swap, and +1,000,000 stablecoin-notional micros, attributed
to Presto and UnitFlow.
