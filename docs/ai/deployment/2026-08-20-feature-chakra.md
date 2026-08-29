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
>
> **Status 2026-08-29 (rebaseline preflight):** Read-only preflight complete — see
> "Read-only preflight (2026-08-29)" below. The hosted stack still runs the **previous
> pre-rebaseline revision** (catalog USDC/EURC/mBTC, aggregator `0xEa1b2C…2006`) and is
> retained for rollback. **Aggregator deployment DONE 2026-08-29 (authorized)** — new
> non-upgradeable aggregator `0xeb1235…29d8` with cirBTC sweep and all three venues
> registered; the hosted cutover (worker → snapshot → API/UI) remains **gated on
> explicit authorization**.

## Read-only preflight (2026-08-29, before any external mutation)

All checks were read-only (no broadcasts, no hosted mutations, no wallet spending):

- **Chain:** `eth_chainId` on `https://rpc.testnet.arc.io` → `0x4cef52` (5042002). ✓
- **Canonical tokens** (bytecode present, decimals verified via `cast call`):
  - USDC `0x3600…0000` — decimals 6 ✓
  - EURC `0x89B5…D72a` — decimals 6 ✓
  - cirBTC `0xf0C4…32BF` — decimals 8 ✓
- **Venue bytecode** (all 6 manifest contracts deployed):
  - Xylo factory `0x60ED…9e2`, router `0x7374…1023`, pool `0x3DF3…BB1` ✓
  - Presto hub `0x5794…24D6` ✓
  - UnitFlow factory `0xd67F…745a5`, pair `0x268D…9200` ✓
- **Factory/hub membership:**
  - Xylo `getPool(USDC,EURC)` → `0x3DF3…BB1` (matches manifest) ✓
  - UnitFlow `getPair(EURC,cirBTC)` → `0x268D…9200` (matches manifest) ✓
- **Reserves (nonzero):**
  - Xylo pool: 9,430,962,057,482 USDC / 454,267,933,226 EURC ✓
  - UnitFlow pair: 257,349,752 EURC / 249,850 cirBTC ✓
- **Quote probes:**
  - Xylo `getAmplificationParameter()` → 20000 raw (= A=200 after A_PRECISION=100, matches manifest) ✓
  - Xylo `calculateSwap(1e6 USDC→EURC)` → 780,142 ✓
- **Previous release (retained for rollback):**
  - Hosted API `https://chakra-api-0a5i.onrender.com`: `/health` 200, `/ready` 200 ready:true, `/tokens` returns the **pre-rebaseline mBTC catalog** (confirming the old revision is still live), `/quote` USDC→EURC works, cirBTC → `UNKNOWN_TOKEN` (old catalog).
  - Old aggregator `0xEa1b2C24bd41163590960F8e40afe6cb4CC92006`: `paused()==false`, owner `0x12E2…276d`, codesize 22259 chars.

**No failing venue requires repair** — all manifest venues pass bytecode/membership/reserves/probe checks. Deployment (aggregator) and cutover (worker/API/UI) remain **authorization-gated** per the plan; this preflight does not authorize any broadcast or hosted mutation.

## Aggregator deployment (2026-08-29, authorized)

Broadcast via `scripts/arc-operator.sh --broadcast script script/DeployAggregator.s.sol` (venues registered in the same tx batch; deploy tx `0x663c4614…7261e`, 5 registration calls `0x15101790…` … `0x6ec78bfa…`).

- **New aggregator:** `0xeb12351602c56d47c4ee955193335848952b29d8` (dry-run predicted the same address).
- On-chain verified: codesize 29745 chars, owner `0x12E266744f6d25D372000e066eCc0DF5a752276d`, `paused()==false`, `cirbtc()==0xf0C4…32BF`, `usdc()==0x3600…0000`, `eurc()==0x89B5…D72a`.
- Venue registration verified: Xylo factory dexType **3** + router `0x7374…1023` configured; Presto hub allowlisted `true`; UnitFlow factory dexType **0** with `factoryFeeBps == 30`.
- Old aggregator `0xEa1b2C…2006` left **intact** until the hosted cutover succeeds.
- `docs/arc-testnet-manifest.json` v2 and `.env.example` updated (`CHAKRA_AGGREGATOR`).

**Remaining gate:** hosted cutover (worker first → verified snapshot → API/UI) — separately authorized.

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
| `CHAKRA_AGGREGATOR` | `0xEa1b2C24bd41163590960F8e40afe6cb4CC92006` (redeploy pending for the 2026-08-29 surface) |
| `CHAKRA_CIRBTC_ADDRESS` | `0xf0C4a4CE82A5746AbAAd9425360Ab04fbBA432BF` |
| `CHAKRA_XYLO_ROUTER` | `0x73742278c31a76dBb0D2587d03ef92E6E2141023` |
| `CHAKRA_PRESTO_HUB` | `0x5794a8284A29493871Fbfa3c4f343D42001424D6` |
| `CHAKRA_UNITFLOW_FACTORY` | `0xd67F63A4F26a497b364d1C82e6747Aec8B5743a5` |
| `CHAKRA_SEED_FACTORIES` | `0x60EDeFB094B84BBC6430cc130B358A43Ba1979e2:xylo,0x5794a8284A29493871Fbfa3c4f343D42001424D6:presto,0xd67F63A4F26a497b364d1C82e6747Aec8B5743a5:xyk` |
| `CHAKRA_DISCOVERY_FACTORIES` | same three manifest factories |
| `CHAKRA_EVM_WS_ENABLED` | `true` |

Worker reads `CHAKRA_REDIS_URL` / `CHAKRA_RPC_*`; `SNAPSHOT_*` remain legacy overrides.
Factory tuples are `address:dex_type` (`xyk|stable|clmm|xylo|presto`) — bare addresses fail parse.
Source ids: `xylo` → `xylo-stable`, `presto` → `presto-hub`, seeded `xyk` → `unitflow-v25` (manifest
order) or `chakra-xyk`. The 2026-08-29 operator rollout is **one aggregator deployment plus venue
registration** — no token, factory, pool, or liquidity deployment. After local verification, stop
before broadcasting; deployment is separately authorized.

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

Smoke evidence (2026-08-28, redeploy + XyloNet live):
- `/health` → 200 `{"status":"ok"}`
- `/ready` → 200 `{"status":"ready","ready":true,"snapshot_id":"snapshot-…"}`
- `/quote` USDC→EURC 1e6 → `expected_output: 996915` via `chakra-stable`
  pool `0xe4a881f4211b5cc11d8298032136a0d72e93cb02` (4 bps, impact 30 bps, `dex_types: ["stable"]`)
- `/quote` USDC→EURC 5e6 → `expected_output: 4680042` routing `xylo`
  pool `0x3df3966f5138143dce7a9cfddc2c0310ce083bb1` (4 bps, `dex_types: ["xylo"]`, `hop_factories: ["0x60edefb094b84bbc6430cc130b358a43ba1979e2"]`)
- `/build_tx` (1e6 & 5e6) → `to: "0xea1b2c24bd41163590960f8e40afe6cb4cc92006"`, valid `splitSwap` calldata and Permit2 typed data
- `/tokens` → USDC / EURC / mBTC catalog
- Redis holds `chakra:snapshot:current` + `chakra:pool:*` keys
- CORS: `access-control-allow-origin: https://chakra-arc-dex.vercel.app`
Known limits (not host gates): USDC→mBTC returns `NO_ROUTE` because the mBTC xyk pools
hold dust reserves (`MIN_XYK_RESERVE_atomic unitsS` filter) and the CLMM pool lacks complete
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
