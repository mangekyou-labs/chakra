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
| `GET` | `/api/v1/tokens` | Frozen catalog (USDC, EURC, cirBTC) |
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

## Quote hop metadata (T4.7)

Each `sub_route` carries explicit per-hop identity — do **not** reconstruct the
DEX type by splitting the `source` string:

```json
{
  "source": "chakra-stable",
  "path": ["0x3600…0000", "0x89B5…D72a"],
  "pool_addresses": ["0xStablePool…"],
  "dex_types": ["stable"],
  "hop_fees": [4],
  "hop_factories": [""],
  "amount_in": "100000000",
  "amount_out": "99955053",
  "fraction_bps": 10000
}
```

- `dex_types[]` — per-hop DEX type (`xyk` | `stable` | `clmm` | `xylo`),
  length == `pool_addresses`. Extensible: new venues are added without
  changing the schema.
- `hop_fees[]` — per-hop venue fee in bps (stable/xylo 4, xyk/clmm 30, …).
- `hop_factories[]` — per-hop allowlisted factory (`""` = legacy pool).

`/build_tx` validates each submitted step's token pair, DEX type, factory, and
fee against the snapshot; mismatches return `ROUTE_INVALID`.

### XyloNet hop (T-XYLO)

The XyloNet stableswap pool (Arc testnet USDC/EURC) is a scoped v1 hop:

- `dex_type: "xylo"` (on-chain enum value `3`, appended after clmm).
- Different swap ABI from the Chakra stableswap: `swap(tokenIn, tokenOut,
  amountIn, minOut, to, deadline)` pulls via `transferFrom` (the aggregator
  approves exactly `amountIn` and resets the allowance to 0 after).
- Factory membership via `getPool(address,address)`; the factory must be
  allowlisted by the owner (`addFactory(xylo)`).
- Quote math: A=200, 4 bps fee on output (`xylo_quote`), pinned to live
  same-block `calculateSwap` vectors.
- Catalog USDC/EURC only; the USDC/USYC Xylo pool is never routed.
