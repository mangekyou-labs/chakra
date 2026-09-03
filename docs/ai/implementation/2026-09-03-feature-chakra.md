# Chakra feature implementation

Implemented: topic-batch poll fan-out with merge/dedupe and cursor retention on
partial failure; ack-validated multi-subscription WS watcher (unique request
ids, reconnect on rejected/missing/timed-out acks) with `native-tls` enabled so
`wss://` Arc endpoints connect; -32005 retry with backoff/failover and -32012
never retried; source-preserving factory normalization and emitter attribution;
additive integer analytics indexer with correct heads/lag/freshness; six-probe
strict readiness; `GET /api/v1/stats`; viem QA smoke CLI; and the `/stats`
dashboard with BigInt USD formatting, URL range selection, and stale-response
protection.
