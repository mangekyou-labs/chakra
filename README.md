# Chakra Arc Testnet DEX Aggregator

Multi-venue DEX aggregator for Arc testnet (chain `5042002`). Routes swaps across Uniswap V2 (xy=k), StableSwap, and Uniswap V3 (CLMM) venues with split-order optimization and Permit2-native execution.

## Architecture

```
┌──────────────┐    REST     ┌──────────────┐    Redis    ┌──────────────┐
│   Frontend   │ ──────────► │   API Server │ ◄────────── │    Worker    │
│  (Next.js)   │  /quote     │  (Rust)      │  snapshots  │  (Rust)      │
│  Vercel      │  /build_tx  │  Render      │             │  Render      │
└──────────────┘             └──────┬───────┘             └──────┬───────┘
                                    │                            │
                                    ▼                            ▼
                             ┌──────────────┐            ┌──────────────┐
                             │  Aggregator  │            │  Arc RPC     │
                             │  (Solidity)  │            │  Testnet     │
                             └──────────────┘            └──────────────┘
```

### Components

| Component | Language | Location | Role |
|-----------|----------|----------|------|
| **API Server** | Rust | `crates/api-server` | REST `/quote`, `/build_tx`, `/tokens`, `/health`, `/ready` |
| **Worker** | Rust | `crates/market-data-worker` | EVM log watcher, pool state updates, snapshot publisher |
| **Router Engine** | Rust | `crates/router-engine` | BFS pathfinder, local quote math, split optimizer |
| **DEX Adapters** | Rust | `crates/dex-adapters` | EVM RPC, pool index, on-chain quote math |
| **SDK** | TypeScript | `packages/sdk` | Client library for `/quote` and `/build_tx` |
| **Frontend** | TypeScript/Next.js | `packages/frontend` | Static export swap UI (Vercel) |
| **Aggregator** | Solidity | `contracts/evm/src/Aggregator.sol` | On-chain split swap with Permit2 |
| **Operator** | Bash | `scripts/arc-operator.sh` | Wallet-safe Foundry deploy/seed wrapper |

### Venues

| Venue | Type | Factory | Pool Math |
|-------|------|---------|-----------|
| Uniswap V2 / XYK | xy=k | Vendored UniswapV2Factory | Constant product |
| StableSwap | Stable | Custom StableSwapFactory | Curve-style invariant |
| Uniswap V3 / CLMM | Concentrated | Vendored UniswapV3Factory | Full-range liquidity |

## Development

```bash
# Rust workspace
cargo check --workspace
cargo test --workspace
cargo fmt --all --check

# Foundry contracts
cd contracts/evm && forge test

# Frontend
cd packages/frontend && npm install && npm test && npm run typecheck && npm run build

# SDK
cd packages/sdk && npm test
```

## Deployment

See [`docs/deployment-overview.md`](docs/deployment-overview.md) and the
[T11 deployment record](docs/ai/deployment/2026-08-31-t11-chakra-arc-only-cleanup.md)
for the rollout runbook.

### Contract addresses (Arc testnet)

```bash
# From docs/arc-testnet-manifest.json
# Filled after broadcast — see manifest for current values
```

### Hosting

| Service | Platform | Purpose |
|---------|----------|---------|
| `chakra-api` | Render | API server + market data worker |
| `chakra-redis` | Render | Redis for pool state snapshots |
| `chakra-arc-dex` | Vercel | Static frontend |

## Configuration

Key environment variables (see `.env.example`):

| Variable | Description |
|----------|-------------|
| `ARC_RPC_URL` | Arc testnet JSON-RPC endpoint |
| `CHAIN_ID` | Must be `5042002` |
| `CHAKRA_AGGREGATOR` | Deployed aggregator contract address (`0xeb12351602c56d47c4ee955193335848952b29d8`) |
| `CHAKRA_CIRBTC_ADDRESS` | Canonical cirBTC token address (`0xf0c4a4ce82a5746abaad9425360ab04fbba432bf`) |
| `CHAKRA_XYLO_FACTORY` | XyloNet factory address |
| `CHAKRA_XYLO_ROUTER` | XyloNet router address |
| `CHAKRA_PRESTO_HUB` | Presto hub address |
| `CHAKRA_UNITFLOW_FACTORY` | UnitFlow V2.5 factory address |
| `CHAKRA_SEED_FACTORIES` | Comma-separated factory:dex_type pairs for worker |
## Safety

- Testnet only (chain `5042002`). Never deploy to mainnet.
- Private keys loaded via `~/.arc-canteen/wallet.yaml` env — never passed on CLI.
- `scripts/arc-operator.sh` wraps all Foundry broadcasts.
- No secrets in git, logs, or command lines.

## License

Apache-2.0
