# Self-hosted Chakra quickstart

Run the API and market-data worker locally with Redis and Arc Testnet RPC.

```bash
cp .env.example .env
docker compose up -d redis
cargo run -p market-data-worker
cargo run -p api-server
```

Set `CHAKRA_REDIS_URL`, `CHAKRA_RPC_HTTP`, `CHAKRA_RPC_WS`,
`CHAKRA_CORS_ORIGINS`, and the active token, factory, and aggregator settings
in the environment. Keep Redis URLs and RPC credentials private.

```bash
curl -fsS http://127.0.0.1:8080/api/v1/health | jq .
curl -fsS http://127.0.0.1:8080/api/v1/ready | jq .
curl -fsS http://127.0.0.1:8080/api/v1/tokens | jq .
```

The worker is the sole Redis writer. API instances are stateless readers that
hydrate snapshots and calculate quotes locally. See the
[integrator guide](integrator-guide.md) for the request flow.
