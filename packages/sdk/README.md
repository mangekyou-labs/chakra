# @lumagg/sdk

TypeScript client for the [Chakra](https://lumagg.xyz) REST API — quote, build_tx, tokens, balances, health. No wallet secrets; all signing happens in the user's wallet.

## Install

```bash
npm install @lumagg/sdk
# or link locally during development:
cd packages/sdk && npm run build
```

## Quick start

```typescript
import { ChakraClient } from '@lumagg/sdk';

const client = new ChakraClient({ apiUrl: 'http://localhost:8080' });

const quote = await client.quote({
  tokenIn: '0x3600000000000000000000000000000000000000', // USDC (6 dp)
  tokenOut: '0x89b50855aa3be2f677cd6303cec089b5f319d72a', // EURC (6 dp)
  amountIn: '1000000', // 1 USDC atomic
  slippage: 0.5, // → slippage_bps 50
});

const tx = await client.buildTx({
  user: '0x…[your EOA]',
  tokenIn: quote.tokenIn,
  tokenOut: quote.tokenOut,
  amountIn: quote.amountIn,
  minAmountOut: quote.minimumOutput,
  subRoutes: quote.subRoutes, // quote → sub_routes[].steps mapping is automatic
});

// tx.data = splitSwap calldata; tx.typedData = Permit2 payload (or null);
// tx.requiredApprovals = ERC-20 approvals needed first.
```

## API

| Method | REST | Description |
|--------|------|-------------|
| `isHealthy()` | `GET /health` | Liveness (rate-limit exempt) |
| `isReady()` | `GET /ready` | Snapshot current AND ≥1 pool key |
| `listTokens()` | `GET /tokens` | Frozen catalog USDC/EURC/cirBTC |
| `quote()` | `GET /quote` | Best route; `slippage_bps` integer |
| `buildTx()` | `POST /build_tx` | splitSwap calldata + Permit2 typed data |
| `getBalances()` | `GET /balances` | ERC-20 + separate `native_usdc` (never summed) |

`quote()` accepts `slippage` as a percentage (0.5) and converts it to
`slippage_bps` (50); pass `slippageBps` for wire fidelity. It never sends
`prefer_soroban` or a percent `slippage`. Envelope failures throw
`ChakraApiError` whose `.code` is one of `NO_ROUTE`, `NOT_READY`, `PAUSED`,
`INVALID_PARAMS`, `ZERO_AMOUNT`, `SAME_TOKEN`, `UNKNOWN_TOKEN`,
`ROUTE_INVALID`, `RATE_LIMITED`, `RPC_ERROR`.

`buildTx` posts `user` (0x EOA) — not `from` or `user_public_key` — with
`token_in`, `amount_in`, `min_amount_out`, and `sub_routes[].steps[]`
(`dex_type` ∈ {xyk, stable, clmm}, `pool_address`, `token_in`, `token_out`).

## Catalog

| Token | Address | Decimals |
|-------|---------|----------|
| USDC  | `0x3600000000000000000000000000000000000000` | 6 |
| EURC  | `0x89b50855aa3be2f677cd6303cec089b5f319d72a` | 6 |
| cirBTC | canonical `0xf0C4…32BF` | 8 |

Native USDC (18 dp) is gas only — never a swap token. `getBalances` returns
`{ erc20, nativeUsdc }` and never sums the two encodings.

## Examples

```bash
npx tsx packages/sdk/examples/quote-build.ts
```

The example skips gracefully when the API is not running.

## Docs

- [OpenAPI](../../docs/openapi.yaml)

## License

Apache-2.0
