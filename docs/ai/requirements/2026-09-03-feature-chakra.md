# Chakra feature requirements

Restore canonical Arc routing for the frozen USDC / EURC / cirBTC catalog, repair
the watcher's oversized topic filter, and publish reconstructable integer-safe
venue statistics:

- Partition the 13 watched event signatures into topic batches of at most ten
  (10 + 3) and poll every batch over the same address/block window; merge,
  deduplicate, and ingest once, advancing the poll cursor only after every batch
  succeeds.
- Register each topic batch as its own `eth_subscribe` on one WebSocket
  connection with a unique request id; declare the socket connected only after
  every acknowledgement arrives (reconnect on timeout, error, or incomplete
  acks). WSS requires a TLS-enabled tungstenite build.
- Treat JSON-RPC -32005 rate limits as retryable with bounded backoff/failover;
  never retry malformed-filter errors such as -32012.
- Use only discovered live liquidity (Xylo or Presto USDC/EURC, UnitFlow
  0x268D...9200 EURC/cirBTC). Never create liquidity or invent a direct cirBTC
  pool.
- Strict readiness only when all six directed USDC/EURC/cirBTC probes have a
  direct or multihop route; `GET /api/v1/stats?range=14d|30d|90d|all` with
  honest heads/lag/freshness semantics; integer decimal-string monetary
  analytics in replay-safe, additive Redis records.
- A QA production-smoke command (viem) that reads `QA_WALLET_SECRET` from the
  environment only, supports mnemonic/private-key accounts, defaults to
  dry-run, and requires an explicit broadcast flag.
- A `/stats` dashboard rendering `stablecoin_notional_micros` as USD with BigInt
  semantics, bounded integer chart scaling, URL-held range selection with
  stale-response protection, skeletons, empty/error states, and cirBTC naming.
