# Architecture deepening: analytics, swap execution, live pool topology

Status: approved; implementation and verification recorded in the accompanying plan.

Reviewed against commit `b8d1666`, including the Arc mainnet and UnitFlow V3
cutover. Earlier testnet design records are historical where the mainnet
deployment record supersedes them.

## Recommendation and alternatives

Implement all three opportunities as one coordinated change set, with three
independently testable modules and sequential implementation checkpoints.
Analytics and topology share worker files, so their edits should be ordered.

Alternatives considered:

- Separate projects: smaller individual reviews, but delays related correctness
  work and repeats the same worker exploration.
- One general state/workflow framework: combines unrelated domain rules and
  increases caller knowledge. Reject this approach.

The combined scope is deliberate: each change must remove caller obligations
or put production behavior behind the same interface used by its tests.

## Analytics progress

The current poller publishes chain, confirmed, and indexed heads through three
separate writes, then separately writes poll freshness. The stats reader reads
those values independently. Existing in-memory tests do not model interrupted
Redis publication or reads spanning two publications.

Deepen ownership of successful-poll progress. Its interface presents a coherent
progress value containing all three heads and the successful-poll timestamp.
The Redis adapter publishes and reads that value atomically; the memory adapter
exercises the same semantics. Choose storage that retains the existing namespace
and supports existing stored progress during rollout. Legacy partial metadata
must not be presented as a new coherent successful poll.

Keep the page cursor independently durable: a failed later page may leave the
cursor advanced while the last successful-poll progress stays unchanged.
Quiet-chain polls still refresh successful-poll freshness. Keep integer amounts,
case-normalized deployment namespaces, and the public stats response shape.
Coherent progress does not promise that historical totals and progress are one
transactional snapshot; that larger change is outside this scope.

Verification: page replay, quiet polls, publication failure, coherent reads,
legacy metadata, and namespace isolation. Exercise the real Redis adapter in
addition to memory tests; the test-only index_swap model is not evidence that
Redis commands execute correctly.

## Swap execution

The production sequence lives in SwapCard, while approval tests call a separate
encoder. Both approval and swap receipt statuses are currently ignored before
advancing the sequence or recording confirmation.

Deepen a swap execution module around build, requested approvals, optional
Permit2 signing, signature insertion, fee selection, submission, and receipt
validation. Its interface owns execution progress and outcomes. Presentation
retains quote selection, input formatting, wallet connection, and the unaudited
acknowledgement prompt. Inject wallet, build, and receipt interactions at their
existing seams; avoid extracting more shallow helpers.

Stop after an unsuccessful approval receipt. Only a successful swap receipt may
produce confirmed history and presentation. Preserve the submitted transaction
hash if confirmation subsequently fails. Preserve requested approval spender
and amount, optional Permit2 signing, zero swap value, the fee floor, and one
confirmation. Do not silently substitute the helper's fixed spender for the
transaction envelope's spender.

Verification uses the production execution interface: optional approvals and
signing, sequencing, paused/build failure, rejected prompts, unsuccessful
approval and swap receipts, and transaction-hash preservation. Remove or adapt
tests that only cover a disconnected duplicate implementation.

Existing wallet QA and CLI smoke contain testnet-specific settings despite the
mainnet cutover. Make deployment assumptions explicit in relevant QA validation;
do not convert an existing testnet broadcast command into an implicit mainnet
broadcast. External funded transactions are not part of local verification.
Sharing all CLI execution code is not required for this refactor.

## Live pool topology

Discovery and created-pool ingestion separately coordinate topology, lookup
index, and watch revision. CLMM refresh reads topology before awaiting RPC and
can later reinsert a pool that completed discovery removed. Its read and write
identity matching also differ.

Deepen the existing worker's live pool topology ownership. Concentrate mutations,
derived lookup/watch state, consistent pool identity, and refresh reconciliation
behind one interface. Keep scheduling and RPC outside this ownership; do not
add a polymorphic topology adapter without an actual second implementation.

Preserve current discovery semantics: an operation failing before replacement
leaves topology intact; completed discovery replaces it with verified pools.
A failed individual verification may therefore remove a pool. An unsuccessful
or incomplete refresh does not overwrite previously accepted pool state.

A refresh result belongs to the topology version and pool identity from which
it started. Reject it when that version has been replaced or its pool is no
longer current. Rejected results must not mutate topology or be forwarded for
publication. Mutations must update lookup/watch consequences together, without
requiring callers to remember synchronization order.

Verification covers created-pool lookup/watch visibility, discovery replacement,
refresh completion after replacement, source/address identity, failed refresh,
and complete CLMM coverage. Preserve the worker as sole Redis writer and local
quote computation. This does not add transactional coherence across all Redis
market snapshots or redefine factory eligibility.

## Execution and acceptance

Implement analytics, topology, then swap execution as separate reviewable steps
within this change set. Establish regression coverage at each module interface
before changing the behavior it protects. Update the domain glossary when the
module terms are agreed, and document the final ownership rules.

Run focused module tests first, then Rust workspace tests and formatting, plus
frontend tests, type checking, lint, and production build as applicable. Exercise
Redis storage semantics with an isolated local instance. No deployment is
included in this change set.

Recheck baseline: 10 analytics tests, 29 watcher tests, and 31 frontend helper,
hook, and scheduler tests passed. These establish a baseline, not regression
coverage for the proposed fixes.

## Evidence

- crates/market-data-worker/src/analytics.rs: AnalyticsStore and AnalyticsIndexer
- crates/api-server/src/handlers.rs: get_stats
- packages/frontend/src/components/SwapCard.tsx: handlePrimary
- packages/frontend/src/lib/swap-send.ts and swap-send.test.ts
- crates/market-data-worker/src/evm_watcher.rs: discover_once and ingest_logs
- crates/market-data-worker/src/fetch_pipeline.rs: CLMM refresh execution
- docs/ai/deployment/2026-10-15-arc-mainnet.md

The topology interleaving and mixed-progress risks are source-level findings;
they have not yet been reproduced by new regression tests. The receipt-status
omission is directly visible in the production sequence.
