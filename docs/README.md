# Chakra Documentation

Chakra is a liquidity aggregator for Arc. It quotes routes across Arc
DEXes (multi-hop and split when useful) and builds unsigned transactions for
atomic execution through the Chakra Aggregator contract.

**Production API:** `https://api.Chakra.xyz`

**Complete docs:** `https://Chakra.gitbook.io/`

## What do you want to do?

### Integrate a wallet, dApp, or bot

1. [Integrator Guide](integrator-guide.md) — `GET /quote` → `POST /build_tx` → sign → submit
2. [JavaScript / TypeScript Example](javascript-integration-example.md) — browser integration with an application-owned wallet adapter
3. [API Reference](api-reference.md) and [OpenAPI](openapi.yaml)
4. npm SDK: [`@Chakra/sdk`](https://www.npmjs.com/package/@Chakra/sdk)

### Self-host a quote stack

Start with [Deployment Overview](deployment-overview.md) if you are choosing
between the release binaries, or use the
[Self-hosted Aggregator Quickstart](self-hosted-aggregator-quickstart.md) to
run the split worker/API topology directly.

| Need | Guide |
| --- | --- |
| Single-process quote API | [Chakra Swap API](Chakra-swap-api.md) |
| Split worker/API quote stack | [Self-hosted Aggregator Quickstart](self-hosted-aggregator-quickstart.md) |
| Shared market state + API replicas | [Production Aggregator](aggregator-deployment.md) |
| Public stats, swap history, and arbitrage history | [Analytics Indexer](analytics-indexer.md) |
| Aggregator / vault contracts | [Smart contracts](contracts-deployment.md) |

### Run atomic round-trip arbitrage

Start with [Arbitrage Deployment](arbitrage-deployment.md) and
[Round-trip Arbitrage](round-trip-arb.md).

## Supported liquidity

Arc venue, Arc venue (including CLMM), Arc venue, Sushi V3, and Arc venue. Classic
Arc DEX routing is available for comparison and Classic-only execution; it
is not combined with Arc legs in one transaction.

Source and issues: [Chakra monorepo](https://github.com/Chakra/Arc-dex-agg).
