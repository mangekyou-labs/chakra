# Architecture deepening implementation

Approved specification: ../specs/2026-10-15-architecture-deepening-design.md.
Baseline: b8d1666. Approved seams are the analytics backend/indexer and Redis
adapter, live topology/discovery/refresh interfaces, and production swap executor
with injected build, wallet and receipt interactions.

1. Publish successful-poll progress as a single value and consume it once in
   stats. Keep the independent page cursor. Verify failures, quiet polls,
   replay, legacy metadata and isolated Redis storage.
2. Move pool mutation, derived index/watch state and CLMM reconciliation under
   live topology ownership. Check replacement interleavings and source/address
   matching, preserving complete-coverage publication rules.
3. Extract the production swap workflow, require successful receipts, preserve
   submitted hashes, and connect SwapCard to its outcomes. Replace duplicate
   approval tests with executor tests. Make QA deployment assumptions explicit.
4. Run workspace and frontend checks, review standards and spec independently,
   address findings, commit on the current branch, merge and push main.
5. Remove obsolete personal grant documents and push that cleanup separately.

No deployment or funded transaction is included.

## Final ownership

`AnalyticsBackend::publish_progress/progress` owns a complete successful-poll
value. Redis stores JSON at the existing deployment namespace's `:progress`
key using one SET/GET. Legacy swaps and cursor remain usable. Legacy independent
head/freshness keys cannot prove a coherent poll, so stats waits for the first
new successful publication. Historical totals remain independently read.

`LivePoolTopology` owns private membership, normalized lookup, watch addresses,
watch revision and replacement generation. Discovery replaces through this
owner; created-pool insertion updates consequences before returning. CLMM RPC
runs outside its lock. Only a complete refresh accepted against its original
generation and source/address identity is forwarded to the Redis sink.

`executeSwap` owns workflow progress and outcomes. SwapCard supplies wallet,
build and receipt ports, presents progress, and records history only for its
confirmed outcome. A failed receipt preserves the swap submission hash for an
explorer link. The separate fixed-spender encoder and its disconnected test
were removed.

## Verification

- Original CLMM production arm fails the deterministic paused-RPC replacement
  regression; reconciled execution passes for pool removal and same-address
  replacement.
- Real isolated Redis verifies legacy rejection, normalized namespaces,
  repeated concurrent coherent progress reads, integer records and replay
  healing of a missing time index.
- Rust workspace tests, formatting and Clippy with warnings denied pass.
- Independent Standards and Spec reviews found no blocking findings. The
  Standards review noted optional existing duplication in pool insertion.
- Frontend: 116 tests pass; typecheck, lint and webpack production build pass.
- External wallet execution and funded transactions were not run.
