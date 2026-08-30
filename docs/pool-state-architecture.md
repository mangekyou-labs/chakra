# Pool-state architecture

Chakra separates the routing graph from live per-pool state. One market-data
worker observes Arc EVM logs and RPC, writes Redis snapshots, and publishes
topology. API instances read the snapshot, hydrate candidate pools, and quote
locally.

## Data layers

| Layer | Content | Redis key |
| --- | --- | --- |
| Graph | token pairs, pool addresses, fees, CLMM references | `chakra:snapshot:*` |
| XYK | token reserves and fee | `chakra:pool:xyk:{source}:{pool}` |
| Stable | balances, invariant parameters, fee | `chakra:pool:stable:{source}:{pool}` |
| CLMM | slot, liquidity, ticks, coverage | `chakra:pool:clmm:{source}:{pool}` |
| Factories | allowlisted factory and venue type | `chakra:factories` |

Pool keys use a long eviction TTL. TTL is cache eviction rather than a quote
freshness scheduler; logs and discovery overwrite touched records.

## Update path

```text
Arc logs/RPC -> watcher -> fetch queue -> pool state -> Redis
                       \-> discovery -> topology snapshot -> Redis
API request -> candidate paths -> Redis hydration -> local quote
```

The worker is the sole Redis writer. The API does not retain user keys and does
not submit transactions. CLMM records are published only when tick coverage is
complete; incomplete coverage is excluded from quotes.

## Configuration

Key settings are `CHAKRA_REDIS_URL`, `CHAKRA_RPC_HTTP`, `CHAKRA_RPC_WS`,
`CHAKRA_DISCOVERY_INTERVAL_SECS`, `CHAKRA_POOL_STATE_TTL_SECS`, and
`CHAKRA_QUOTE_RPC_HYDRATE_ENABLED`. See `.env.example` and `render.yaml` for
the deployment defaults.
