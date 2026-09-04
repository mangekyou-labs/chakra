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
