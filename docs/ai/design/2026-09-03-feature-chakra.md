# Chakra feature design

- Watcher: `watched_topic0_batches()` returns two OR-lists of ≤10 signatures;
  each poll iterates batches over one address/block window, deduplicates merged
  logs by `(address, block, tx, log_index, topics, data)`, ingests once, and
  commits the cursor only after every batch succeeds. WS uses one socket, unique
  JSON-RPC ids per batch, a shared ack deadline, and Pong handling during the
  handshake; `tokio-tungstenite` is built with `native-tls` so `wss://` Arc
  endpoints actually connect.
- RPC: error classification splits retryable rate limits (-32005, with 500ms→30s
  bounded backoff and URL failover) from permanent malformed-filter errors
  (-32012) that surface immediately.
- XYK dust policy: curated, factory-allowlisted pools are eligible when both
  reserves are nonzero and exact integer XYK math produces nonzero output. Do
  not apply a unit-agnostic atomic reserve floor; this preserves executable
  8-decimal cirBTC liquidity such as the live UnitFlow pool.
- Analytics: additive integer Redis records (`chakra:analytics:*`) with
  per-swap attribution from decoded `splitSwap` calldata; `chain_head` is the
  latest observed Arc block, `confirmed_head` the confirmation-adjusted target,
  `indexed_head` the last committed cursor, `lag_blocks = confirmed - indexed`,
  and `freshness_secs` is the age of the last successful poll.
- API: readiness consults the engine for the six directed probes; stats never
  invents history (valid zero-history response when the namespace is empty).
- Dashboard: `formatMicrosUsd` divides with BigInt (`1000000` → `$1.00`), chart
  scaling stays integer/bounded before number conversion, and the selected range
  lives in the URL with an abort controller guarding stale responses.
