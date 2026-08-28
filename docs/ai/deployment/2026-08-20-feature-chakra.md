---
phase: deployment
title: Deployment Strategy
description: Define deployment process, infrastructure, and release procedures
feature: chakra
date: 2026-08-20
---

# Deployment Strategy

> **Status 2026-08-28 (T8.1 + T8.2 complete):** Public Arc testnet stack is hosted and
> smoking green. API + worker + Redis on Render (Docker), UI on Vercel (static export).
> Contracts and seeded pools are on chain ID `5042002` (Arc testnet). Never target Arc
> mainnet. See requirements and design docs.

## Public URLs (2026-08-28)

| Surface | URL | Notes |
|---|---|---|
| API (Render web) | `https://chakra-api-0a5i.onrender.com` | Docker image from `./Dockerfile`, repo `mangekyou-labs/chakra`, branch `main` |
| UI (Vercel) | `https://chakra-arc-dex.vercel.app` | Static export (`output: export`), project `chakra-arc-dex`, root `packages/frontend` |
| Redis (Render KV) | `chakra-redis` (internal `redis://red-da86lmfavr4c73ekh2t0:6379`) | Free plan, Oregon, persistence off |
| Chain | Arc testnet `5042002` | Public RPC `https://rpc.testnet.arc.io` |

## Infrastructure

- **Render** (workspace "Jag Wrld's Workspace", team `tea-cspsq3ggph6c73f4ln6g`):
  - `chakra-api` — web service, Docker (rust:1.88-bookworm builder, debian bookworm runtime),
    `healthCheckPath: /api/v1/health`, `CHAKRA_LISTEN_ADDR=0.0.0.0:10000`, free plan, Oregon.
  - `chakra-redis` — Key Value (Valkey 8), internal URL unauthenticated (same region).
- **Vercel** (user `gadillacer`, team `gadillacer's projects`):
  - Project `chakra-arc-dex`, `framework: null`, `outputDirectory: out` (see `vercel.json`),
    `NEXT_PUBLIC_CHAKRA_API_URL=https://chakra-api-0a5i.onrender.com` (baked at build).
  - SSO deployment protection was **disabled** on 2026-08-28 (public testnet UI must be reachable).

## Environment Configuration

No secrets in `render.yaml` / `.env.example` (public Arc addresses only). The Render API key
lives in the local worktree `.env` (`RENDER_API_KEY`, git-ignored).

### Render env (chakra-api)

| Key | Value |
|---|---|
| `CHAKRA_REDIS_URL` | `redis://red-da86lmfavr4c73ekh2t0:6379` (internal) |
| `CHAKRA_RPC_HTTP` | `https://rpc.testnet.arc.io` |
| `CHAKRA_RPC_WS` | `wss://rpc.testnet.arc.io` |
| `CHAKRA_RPC_HTTP_FAILOVERS` | Blockdaemon / dRPC / QuickNode HTTP |
| `CHAKRA_RPC_WS_FAILOVERS` | dRPC / QuickNode WS |
| `CHAKRA_CHAIN_ID` | `5042002` |
| `CHAKRA_CORS_ORIGINS` | `https://chakra-arc-dex.vercel.app,http://localhost:3000` |
| `CHAKRA_LISTEN_ADDR` | `0.0.0.0:10000` |
| `CHAKRA_AGGREGATOR` | `0xA59ad3E82d251c3489582e1aA5Bee494d0d2a569` |
| `CHAKRA_MBTC_ADDRESS` | `0xbf5a25D7070FaACAe309D66D05372a6b212ECbdF` |
| `CHAKRA_SEED_FACTORIES` | `0x0c812E5D55D767533c8E4783D33b28EA825b4D8e:xyk,0x77Ce21FDAAea40Fd94aCf65fF3220A0A7Db7D690:stable,0xf6dEa9e6dfE392aaBE366240db4839709572fa69:clmm` |
| `CHAKRA_DISCOVERY_FACTORIES` | same three factories |
| `CHAKRA_EVM_WS_ENABLED` | `true` |

Worker reads `CHAKRA_REDIS_URL` / `CHAKRA_RPC_*`; `SNAPSHOT_*` remain legacy overrides.
Factory tuples are `address:dex_type` (`xyk|stable|clmm`) — bare addresses fail parse.

### Vercel env (chakra-arc-dex, production)

| Key | Value |
|---|---|
| `NEXT_PUBLIC_CHAKRA_API_URL` | `https://chakra-api-0a5i.onrender.com` |

## Deployment Steps

### 1. Rust (Render)

1. Commit to nested `main`, `git push chakra main`.
2. Render auto-deploy (`autoDeploy: yes`) triggers a Docker build of the two binaries
   (`cargo build --release --bin chakra-api-server --bin chakra-market-data-worker`).
   If the webhook is not wired, trigger manually:
   `curl -X POST -H "Authorization: Bearer $RENDER_API_KEY" https://api.render.com/v1/services/<id>/deploys`
3. `docker-entrypoint.sh` starts worker then API; worker runs `evm_watcher::run_arc`
   (WS + poll + discovery into `chakra:` Redis keys).

### 2. UI (Vercel)

1. `cd packages/frontend && vercel --prod --yes`
2. Confirm `NEXT_PUBLIC_CHAKRA_API_URL` is set for production in the project env.
3. Ensure the `chakra-arc-dex.vercel.app` alias points at the latest production deployment
   (`vercel alias set <deployment-url> chakra-arc-dex.vercel.app`).

### Post-deployment validation (SC-5)

```sh
curl https://chakra-api-0a5i.onrender.com/api/v1/health   # {"success":true,"data":{"status":"ok"}}
curl https://chakra-api-0a5i.onrender.com/api/v1/ready    # ready:true + snapshot_id
curl "https://chakra-api-0a5i.onrender.com/api/v1/quote?token_in=0x3600...0000&token_out=0x89B5...2a&amount_in=1000000"
curl https://chakra-arc-dex.vercel.app                    # HTTP 200, Chakra UI
```

Smoke evidence (2026-08-28, after `128ff47`):
- `/health` → 200 `{"status":"ok"}`
- `/ready` → 200 `{"status":"ready","ready":true,"snapshot_id":"snapshot-…"}`
- `/quote` USDC→EURC 1e6 → `expected_output: 996915` via `chakra-stable`
  pool `0xe4a881f4211b5cc11d8298032136a0d72e93cb02` (4 bps, impact 30 bps)
- `/tokens` → USDC / EURC / mBTC catalog
- Redis holds `chakra:snapshot:current` + 4 `chakra:pool:*` keys (3 xyk + 1 stable)
- CORS: `access-control-allow-origin: https://chakra-arc-dex.vercel.app`

Known limits (not host gates): USDC→mBTC returns `NO_ROUTE` because the mBTC xyk pools
hold dust reserves (`MIN_XYK_RESERVE_STROOPS` filter) and the CLMM pool lacks complete
tick coverage (worker intentionally skips incomplete CLMM publishes).

## Rollback

Redeploy the previous GitHub SHA:

1. Render: `curl -X POST -H "Authorization: Bearer $RENDER_API_KEY" -d '{"commitId":"<prev-sha>"}' \
   https://api.render.com/v1/services/<id>/deploys` (or Dashboard → Deploy → previous commit).
2. Vercel: `vercel rollback` in `packages/frontend` (or promote the previous production deployment).

## Incident notes (2026-08-28)

- `/ready` was false with healthy Redis: `market-snapshot::ready::cluster_ready` used the
  nonexistent Redis command `COUNTKEYS`. Fixed to `SCAN` (`128ff47`) and redeployed.
- Vercel project initially had SSO deployment protection enabled, which made
  `chakra-arc-dex.vercel.app` redirect to login; disabled via
  `PATCH /v9/projects/<id> {"ssoProtection": null}`.
