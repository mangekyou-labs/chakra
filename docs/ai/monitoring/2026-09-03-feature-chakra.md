# Chakra feature monitoring

Watch for readiness pair failures (any of the six directed USDC/EURC/cirBTC
probes without a route), RPC topic-limit errors (-32012), retry storms around
-32005, and analytics lag above 100 blocks or freshness above 300 seconds for
two consecutive polls. Preserve unattributed summaries and confirm the indexer
cursor advances monotonically; quiet chains are fresh while the poller runs —
freshness is poll age, never swap age.

## 2026-09-04 backend acceptance observation

Render deploy `dep-dad8e3v10e5c73dpv7ag` remained healthy for a clean 15-minute
window. Samples at three-minute intervals reported `/health` ok, `/ready` true,
lag 0, freshness 18–24 seconds, and all six routes healthy. Fetch-pipeline
signals showed `tasks_failed=0`, `high_queue_depth=0`, and increasing Redis
writes. WS `-32005` rate-limit and block-range-beyond-head warnings occurred in
the worker log but recovered; they did not produce fetch failures, analytics
lag, or readiness loss.

The post-window QA preflight returned a canonical EURC + UnitFlow multihop but
reported 1,462 bps price impact for 1,000,000 atomic USDC. The smoke command
aborted before approvals/broadcast under its 100-bps guard; do not bypass that
guard while waiting for a safe quote or policy direction.
