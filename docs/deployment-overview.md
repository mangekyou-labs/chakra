# Chakra deployment overview

Chakra runs on Arc Testnet as a Rust API, Rust market-data worker, Redis state,
Solidity aggregator, and Next.js frontend.

```text
Arc logs/RPC -> market-data-worker -> Redis -> api-server -> Vercel frontend
                                      \-> aggregator calldata -> wallet
```

Render hosts the API, worker, and Redis. Vercel hosts the frontend. Service
definitions and environment names are in `render.yaml` and `.env.example`;
secrets remain in the hosting providers.

## Release checks

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --workspace --release
cd contracts/evm && forge test
cd ../../packages/sdk && npm test && npm run build && npm pack --dry-run
cd ../frontend && npm test && npm run typecheck && npm run lint && npm run build
```

Validate the hosted API with `/api/v1/health`, `/api/v1/ready`, `/api/v1/tokens`,
`/api/v1/quote`, and `/api/v1/build_tx`. Validate the web alias, API origin,
CORS, metadata, split-ring icon, documentation link, and responsive swap flow.

## Safety

- Deploy only to Arc Testnet (`5042002`).
- Load signing keys from provider secrets or the operator wallet file.
- Never place private keys in command arguments, logs, or the repository.
- Use `value: 0` for aggregator transactions; approvals and Permit2 signatures
  are wallet actions.
