# Chakra

Chakra is a liquidity aggregator for Arc. It quotes routes across Arc
DEXes (multi-hop and split when useful) and builds unsigned transactions for
atomic execution through the Chakra Aggregator contract.

**Production API:** `https://api.Chakra.xyz`

## Integrators start here

1. [Integrator Guide](integrator-guide.md) — `GET /quote` → `POST /build_tx` → sign → submit
2. [API Reference](api-reference.md) and [OpenAPI](openapi.yaml)
3. npm SDK: [`@Chakra/sdk`](https://www.npmjs.com/package/@Chakra/sdk)

## Self-host / operators

| Product | When to use | Guide |
| --- | --- | --- |
| Chakra Swap API | Local or private quote service in one process | [Chakra Swap API](Chakra-swap-api.md) |
| Production Aggregator | Shared market state, API replicas, isolation | [Production Aggregator](aggregator-deployment.md) |
| Chakra Arbitrage | Operator-run atomic round-trip arb | [Arbitrage Deployment](arbitrage-deployment.md) |
| Smart contracts | Aggregator + optional vault / escrow | [Contracts](contracts-deployment.md) |

## Supported liquidity

Arc venue, Arc venue (including CLMM), Arc venue, Sushi V3, and Arc venue. Classic
Arc DEX routing is available for comparison and Classic-only execution; it
is not combined with Arc legs in one transaction.

Source and issues: [Chakra monorepo](https://github.com/Chakra/Arc-dex-agg).
