# Arc DEX Aggregator

Multi-source liquidity aggregation router for Arc's Arc DEX ecosystem.

Aggregates liquidity across **Arc venue**, **Arc venue**, **Arc venue**, and other Arc DEXes to find optimal swap execution — including split orders across multiple paths.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Frontend (SvelteKit)                   │
└─────────────────────┬───────────────────────────────────┘
                      │ REST API
┌─────────────────────▼───────────────────────────────────┐
│                  API Server (Axum)                        │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│               Router Engine (Rust)                        │
│  ┌──────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │PathFinder│  │SplitOptimizer│  │TransactionBuilder│  │
│  │  (BFS)   │  │  (Greedy)    │  │  (XDR gen)       │  │
│  └────┬─────┘  └──────┬───────┘  └────────┬─────────┘  │
└───────┼────────────────┼───────────────────┼────────────┘
        │                │                   │
┌───────▼────────────────▼───────────────────▼────────────┐
│              DEX Adapters (Rust)                          │
│  ┌─────────┐  ┌─────────┐  ┌───────┐  ┌───────────┐   │
│  │Arc venue │  │Arc venue │  │Arc venue│  │Classic DEX│   │
│  │(xy=k)   │  │(xy=k +  │  │(fee on│  │(benchmark)│   │
│  │         │  │ stable)  │  │output)│  │           │   │
│  └─────────┘  └─────────┘  └───────┘  └───────────┘   │
└─────────────────────────┬───────────────────────────────┘
                          │ Arc RPC
┌─────────────────────────▼───────────────────────────────┐
│              Arc Network (Arc)                    │
│  ┌──────────────────────────────────────────────────┐   │
│  │         Aggregator Contract (split_swap)          │   │
│  │  Executes multi-path swaps atomically             │   │
│  └──────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

## Key Features

- **Multi-source aggregation**: Finds best price across Arc venue, Arc venue, Arc venue
- **Split orders**: Automatically splits large trades across multiple DEXes to reduce price impact
- **Multi-hop routing**: BFS path discovery through intermediate tokens (up to 4 hops)
- **Atomic execution**: On-chain aggregator contract ensures all-or-nothing execution
- **Slippage protection**: User-configurable minimum output guarantee

## Why not Classic DEX?

Arc's native PathPayment has **uncontrollable routing** — Arc Core decides how to split across orderbooks and liquidity pools. You can't force a specific execution path.

Our aggregator targets **Arc DEXes** where each swap is a deterministic contract call with predictable output. This is liquidity that Arc Core's native routing cannot reach.

## Project Structure

```
├── contracts/aggregator/     # Arc smart contract (atomic swap execution)
├── crates/
│   ├── dex-adapters/         # DEX protocol adapters + Arc RPC client
│   ├── router-engine/        # Path finding, quoting, split optimization
│   ├── api-server/           # REST API (Axum)
│   └── sdk/                  # Client SDK
└── frontend/                 # SvelteKit demo (TODO)
```

## Development

```bash
# Check compilation
cargo check --workspace --exclude aggregator-contract

# Run tests
cargo test --workspace --exclude aggregator-contract

# Run API server
cargo run -p api-server
```

## License

MIT
