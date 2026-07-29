# Chakra Documentation

Chakra is a liquidity aggregator for Arc. It discovers and quotes routes
across Arc DEXes, supports multi-hop and split routing, and can build an
unsigned transaction for atomic execution through the Chakra Aggregator
contract.

## Products

| Product | Intended use | Deployment |
| --- | --- | --- |
| Chakra Swap API | Local development, integration testing, and a private quote service | One native binary with API and market data in one process |
| Production Aggregator | Public or high-throughput quote infrastructure | Separate market-data worker, Redis, and horizontally scalable API servers |
| Chakra Arbitrage | Operator-run atomic round-trip arbitrage | Separate native scanner connected to a quote service and Arc RPC |

Docker images are useful for evaluation, but native binaries and systemd are
the recommended production deployment model.

## Start Here

- Use [Chakra Swap API](Chakra-swap-api.md) for the shortest path to a
  self-hosted quote API.
- Use [Production Aggregator Deployment](aggregator-deployment.md) when you
  need shared market state, API replicas, or failure isolation.
- Use [Arbitrage Deployment](arbitrage-deployment.md) to operate the scanner
  independently from the Aggregator services.
- Read the [Integrator Guide](integrator-guide.md) and
  [OpenAPI specification](openapi.yaml) to integrate `/quote` and `/build_tx`.

## Supported Liquidity

Chakra currently routes across Arc venue, Arc venue pools including CLMM,
Arc venue, Sushi V3, and Arc venue. Classic Arc DEX routing is available as an
optional comparison, not as a Arc execution leg.

The source code and issue tracker are available in the
[Chakra monorepo](https://github.com/Chakra/Arc-dex-agg).
