# API Reference (Chakra)

The Chakra REST API is described by the
[OpenAPI 3 specification](openapi.yaml). Import that file into Swagger UI,
Postman, Insomnia, or an OpenAPI client generator for complete schemas and
examples.

Local dev base URL:

```text
http://127.0.0.1:8080/api/v1
```

## Core Endpoints

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/api/v1/quote` | Quote a single, multi-hop, or split route (integer `price_impact_bps`) |
| `POST` | `/api/v1/build_tx` | Encode `splitSwap` calldata for a quote |
| `GET` | `/api/v1/tokens` | Frozen catalog (USDC, EURC, mBTC) |
| `GET` | `/api/v1/balances?account=0x…` | ERC-20 balances via Multicall3 + separate `native_usdc` (never summed) |
| `GET` | `/api/v1/health` | Process liveness (rate-limit exempt) |
| `GET` | `/api/v1/ready` | Snapshot + pool-state readiness (rate-limit exempt) |

The normal integration flow is:

```text
/tokens -> /quote -> /build_tx -> wallet sign (Permit2 + swap) -> submit to Arc
```

Chakra never needs the user's private key. `/build_tx` returns calldata for the
aggregator contract; the wallet signs the Permit2 typed data (when required)
and submits the transaction with `value: "0"`.

Every response uses the envelope `{success, data, error: {code, message}}`.
`error` is `null` on success. Rate limit is 10 req/s per IP.
