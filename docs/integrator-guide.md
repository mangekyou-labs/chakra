# Chakra integrator guide

Chakra provides non-custodial best-execution routing across Arc Testnet
venues. The API never receives private keys and never submits transactions.

- Chain ID: `5042002` (`0x4CEF52`)
- API: `https://chakra-api-0a5i.onrender.com`
- Local API: `http://127.0.0.1:8080`

## Token catalog

| Token | Address | Decimals |
| --- | --- | ---: |
| USDC | `0x3600000000000000000000000000000000000000` | 6 |
| EURC | `0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a` | 6 |
| cirBTC | `0xf0C4a4CE82A5746AbAAd9425360Ab04fbBA432BF` | 8 |

## Quote, build, sign, submit

```bash
API=https://chakra-api-0a5i.onrender.com
USDC=0x3600000000000000000000000000000000000000
EURC=0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a

curl -fsS "$API/api/v1/health" | jq .
curl -fsS "$API/api/v1/ready" | jq .
curl -fsSG "$API/api/v1/quote" \
  --data-urlencode "token_in=$USDC" \
  --data-urlencode "token_out=$EURC" \
  --data-urlencode "amount_in=100000000" \
  --data-urlencode "slippage_bps=50" | jq .
```

Pass the returned `sub_routes` to `POST /api/v1/build_tx` with `user`, token
addresses, `amount_in`, and `min_amount_out`. The response contains `to`,
`data`, `chain_id`, `value`, `deadline`, `typed_data`, and
`required_approvals`. Call ERC-20 `approve` when requested, sign the Permit2
EIP-712 payload when present, then send the returned calldata with `value: 0`.

Each route includes explicit `dex_types`, `hop_fees`, and `hop_factories`.
Consumers must use those arrays by index; the display `source` is not a type
encoding. Active venue types are `xyk`, `stable`, `clmm`, and `xylo`.

## TypeScript SDK

```typescript
import { ChakraClient } from '@chakra-ag/sdk';

const client = new ChakraClient({ apiUrl: process.env.API_URL! });
const quote = await client.quote({
  tokenIn: '0x3600000000000000000000000000000000000000',
  tokenOut: '0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a',
  amountIn: '100000000',
  slippageBps: 50,
});
const tx = await client.buildTx({
  user: '0xYourAddress',
  tokenIn: quote.tokenIn,
  tokenOut: quote.tokenOut,
  amountIn: quote.amountIn,
  minAmountOut: quote.minimumOutput,
  subRoutes: quote.subRoutes,
});
```

## Responses and limits

All responses use `{ success, data, error }`. Common errors are `NO_ROUTE`,
`NOT_READY`, `ROUTE_INVALID`, `ZERO_AMOUNT`, `SAME_TOKEN`, `UNKNOWN_TOKEN`,
`INVALID_PARAMS`, `RATE_LIMITED`, and `RPC_ERROR`. The default limit is 10
requests per second per IP; health and readiness are exempt.

## Local harness

```bash
cargo build -p api-server --example local_harness --features test-fixture
./target/debug/examples/local_harness
cd packages/sdk && npm run build
API_URL=http://127.0.0.1:8080 npx tsx examples/quote-build.ts
```
