# Chakra

Chakra is a liquidity aggregator for Arc. It quotes routes across Arc
DEXes (multi-hop and split when useful) and builds unsigned transactions for
atomic execution through the Chakra Aggregator contract.

**Production API:** `https://api.Chakra.xyz`

## What do you want to do?

### Integrate a wallet, dApp, or bot

1. [Integrator Guide](integrator-guide.md) — `GET /quote` → `POST /build_tx` → sign → submit
2. [API Reference](api-reference.md) and [OpenAPI](openapi.yaml)
3. npm SDK: [`@Chakra/sdk`](https://www.npmjs.com/package/@Chakra/sdk)

### Self-host a quote stack

| Need | Guide |
| --- | --- |
| Single-process quote API | [Chakra Swap API](Chakra-swap-api.md) |
| Shared market state + API replicas | [Production Aggregator](aggregator-deployment.md) |
| Aggregator / vault contracts | [Smart contracts](contracts-deployment.md) |

### Run atomic round-trip arbitrage

Start with [Arbitrage Deployment](arbitrage-deployment.md) and
[Round-trip Arbitrage](round-trip-arb.md).

## Supported liquidity

Arc venue, Arc venue (including CLMM), Arc venue, Sushi V3, and Arc venue. Classic
Arc DEX routing is available for comparison and Classic-only execution; it
is not combined with Arc legs in one transaction.

Source and issues: [Chakra monorepo](https://github.com/Chakra/Arc-dex-agg).
