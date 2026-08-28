# Chakra integrator guide

Quickstart for wallets, dApps, and trading bots integrating the Arc Testnet DEX aggregator REST API.

**Network:** Arc Testnet (`chainId` 5042002 / `0x4CEF52`)
**API base:** `https://chakra-api-0a5i.onrender.com` (hosted) or `http://localhost:8080` (local harness)
**OpenAPI:** [openapi.yaml](./openapi.yaml) · **API reference:** [api-reference.md](./api-reference.md)

## Token catalog

| Token | Address | Decimals | Notes |
|-------|---------|----------|-------|
| USDC | `0x3600000000000000000000000000000000000000` | 6 | ERC-20 (swap token; **not** native gas) |
| EURC | `0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a` | 6 | ERC-20 |
| mBTC | (set `CHAKRA_MBTC_ADDRESS` env) | 8 | ERC-20, owner-mint only; buy via swap |

Native USDC (18 dp) is **gas only** — never a swap token (SC-12).

## 1. Health + readiness

```bash
# Hosted API
API=https://chakra-api-0a5i.onrender.com
# Or local harness: API=http://localhost:8080
# Liveness
curl -s "$API/api/v1/health" | jq .

# Readiness (200 when snapshot is current and ≥1 pool key in Redis)
curl -s "$API/api/v1/ready" | jq .
```

## 2. Quote → build → send

The flow is **4 steps**: quote → build_tx → wallet signs → send on-chain.

### Step 1: GET /quote

```bash
USDC=0x3600000000000000000000000000000000000000
EURC=0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a

# Quote 100 USDC → EURC with 0.5% slippage (50 bps)
curl -sG "$API/api/v1/quote" \
  --data-urlencode "token_in=$USDC" \
  --data-urlencode "token_out=$EURC" \
  --data-urlencode "amount_in=100000000" \
  --data-urlencode "slippage_bps=50" | jq .
```

Response envelope: `{ success, data, error }`. Key fields in `data`:
- `expected_output` / `minimum_output` — in token-out atomic units
- `price_impact_bps` — integer basis points (12 = 0.12%)
- `protocol_fee_bps` — always 0
- `is_split` — true when multiple sub-routes
- `sub_routes[]` — each has `source` (e.g. `chakra-stable`), `path[]`, `pool_addresses[]`, `amount_in`, `amount_out`, `fraction_bps`

### Step 2: POST /build_tx

Map `sub_routes` from the quote into steps for `build_tx`. Each `source` maps to a `dex_type`: `chakra-stable` → `stable`, `chakra-xyk` → `xyk`, `chakra-clmm` → `clmm`.

```bash
USER=0xYourEOAAddress

curl -sX POST "$API/api/v1/build_tx" \
  -H 'Content-Type: application/json' \
  -d '{
    "user": "'"$USER"'",
    "token_in": "'"$USDC"'",
    "token_out": "'"$EURC"'",
    "amount_in": "100000000",
    "min_amount_out": "99455200",
    "sub_routes": [{
      "amount_in": "100000000",
      "steps": [{
        "dex_type": "stable",
        "pool_address": "0x...",
        "token_in": "'"$USDC"'",
        "token_out": "'"$EURC"'"
      }]
    }]
  }' | jq .
```

Response envelope fields:
- `to` — aggregator contract address
- `data` — calldata for `splitSwap(...)`
- `chain_id` — 5042002
- `value` — always `"0"` (SC-12)
- `deadline` — unix timestamp (now + 120 s)
- `typed_data` — EIP-712 Permit2 payload, or `null` if Permit2 allowance is already sufficient
- `required_approvals[]` — ERC-20 `approve(Permit2, amount)` needed; empty when already approved

### Step 3: Wallet signs + sends

1. **ERC-20 approvals** (if `required_approvals` is non-empty): call `approve(Permit2, amount)` on each token.
2. **Permit2 signature** (if `typed_data` is non-null): sign via `eth_signTypedData_v4` (EIP-712). Splice the signature into `data` using the `splitSwap` ABI fragment.
3. **Send transaction**: call `eth_sendTransaction` with `{ to, data, value: "0x0" }`.

The aggregator executes atomically: if any hop fails, the entire tx reverts.

### Step 4: Confirm

`waitForTransactionReceipt` with **1 confirmation**. The swap is final on inclusion.

## 3. Envelope errors

| `error.code` | Meaning |
|--------------|---------|
| `NO_ROUTE` | No path found for this token pair |
| `NOT_READY` | Aggregator not deployed (`CHAKRA_AGGREGATOR` empty) |
| `PAUSED` | Aggregator is paused by owner |
| `ROUTE_INVALID` | Submitted routes don't match quote (continuity/amount/factory check failed) |
| `ZERO_AMOUNT` | `amount_in` is 0 |
| `SAME_TOKEN` | `token_in == token_out` |
| `UNKNOWN_TOKEN` | Token not in frozen catalog |
| `INVALID_PARAMS` | Malformed request |
| `RATE_LIMITED` | 10 req/s/IP exceeded |
| `RPC_ERROR` | Upstream RPC failure |

## 4. SDK

`packages/sdk` exports `ChakraClient`:

```typescript
import { ChakraClient } from '@lumagg/sdk';

const client = new ChakraClient({ apiUrl: process.env.API_URL || 'https://chakra-api-0a5i.onrender.com' });
// Health
const healthy = await client.isHealthy();

// Quote
const quote = await client.quote({
  tokenIn: '0x3600000000000000000000000000000000000000',
  tokenOut: '0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a',
  amountIn: '100000000',
  slippageBps: 50,
});

// Build tx
const tx = await client.buildTx({
  user: '0xYourAddress',
  tokenIn: '0x3600000000000000000000000000000000000000',
  tokenOut: '0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a',
  amountIn: '100000000',
  minAmountOut: '99455200',
  subRoutes: quote.subRoutes,
});
// tx.to, tx.data, tx.typedData, tx.requiredApprovals
```

Example script:

```bash
cd packages/sdk && npm run build
API_URL=https://chakra-api-0a5i.onrender.com npx tsx examples/quote-build.ts
```

## 5. Local development

Start the local API harness (no Redis or live chain required):

```bash
# Build
cargo build -p api-server --example local_harness --features test-fixture

# Run (starts on 127.0.0.1:8080 with in-memory T2.3 pools + fixture RPC)
./target/debug/examples/local_harness
```

Then run the SDK example against it:

```bash
cd packages/sdk && npm run build
API_URL=http://127.0.0.1:8080 npx tsx examples/quote-build.ts
```

The example completes `quote` + `build_tx` (SC-6 local evidence). The harness
sets a dummy aggregator address (`CHAKRA_AGGREGATOR`) and fixture RPC so
`paused()` is false and typed data is emitted for Permit2 approval.

## 6. Rate limits

| Tier | Limit | Auth |
|------|-------|------|
| Default | 10 req/s per IP | none |
| Health/ready | exempt | — |

HTTP `429` when exceeded. Loopback IPs are exempt (local development).

## 7. Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/api/v1/health` | Liveness |
| GET | `/api/v1/ready` | Readiness (snapshot + pool keys) |
| GET | `/api/v1/tokens` | Frozen catalog (USDC, EURC, mBTC) |
| GET | `/api/v1/quote` | Best route |
| POST | `/api/v1/build_tx` | Calldata + Permit2 typed data |
| GET | `/api/v1/balances` | ERC-20 balances + native USDC (never summed) |

## 8. Key differences from Stellar/LumAgg

- **EVM, not Stellar**: tx hash, not XDR; `eth_sendTransaction`, not `submit_tx`
- **Permit2**: EIP-712 typed data for gasless approvals; `typed_data: null` when already approved
- **No `prefer_soroban`**: Arc has no Soroban AMMs; all routes are EVM-native
- **No trustlines**: ERC-20 tokens don't need trustline setup
- **`value: "0"` always**: the aggregator never holds native ETH/USDC gas (SC-12)
- **1 confirmation**: swap is final on first inclusion
- **Frozen catalog**: only USDC, EURC, mBTC (no token discovery)
- **No partner API keys**: single rate limit tier (10 req/s/IP)
