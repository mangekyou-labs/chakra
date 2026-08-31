# Chakra evidence index

This directory contains release evidence for the Chakra Arc Testnet
aggregator. Fresh command output and hosted checks are recorded in the T11
testing and deployment records.

## Coordinates

| Component | Coordinate |
| --- | --- |
| Network | Arc Testnet, chain ID `5042002` |
| Explorer | `https://testnet.arcscan.app` |
| Aggregator | `0xeb12351602c56d47c4ee955193335848952b29d8` |
| Permit2 | `0x000000000022D473030F116dDEE9F6B43aC78BA3` |
| API | `https://chakra-api-0a5i.onrender.com` |
| Web | `https://chakra-ag.vercel.app` |

## Required evidence

- Rust workspace format, tests, lint, release build, Docker build, and EVM
  contract tests.
- SDK unit/build/package checks and clean-project registry installation.
- Frontend unit, type, lint, format, production build, contrast, link, theme,
  and split-ring regression checks.
- Hosted API health, readiness, token catalog, quote, and transaction-builder
  checks.
- Vercel alias, CORS, metadata, responsive browser review, and the wallet
  quote → approval/signature → submit → confirmation path.

## Honest follow-up

The healthy 1 USDC to EURC route is the production critical path. Split-route
proof and thin cirBTC liquidity remain follow-up evidence until multiple
healthy routes exist; no artificial liquidity is created for testing.
