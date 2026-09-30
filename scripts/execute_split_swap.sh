#!/usr/bin/env bash
# T9.3: Automated on-chain split swap execution runner.
# Fetches fresh /quote & /build_tx from Chakra API, validates value == 0,
# runs a dry-run simulation, and broadcasts via arc-operator.sh.
#
# Usage:
#   ./scripts/execute_split_swap.sh [--dry-run | --broadcast]

set -euo pipefail

MODE="${1:---dry-run}"
API_URL="${CHAKRA_API_URL:-https://chakra-api-0a5i.onrender.com}"
RPC_URL="${CHAKRA_RPC_HTTP:-https://rpc.testnet.arc.io}"
USER_ADDR="0x12E266744f6d25D372000e066eCc0DF5a752276d"
USDC="0x3600000000000000000000000000000000000000"
EURC="0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a"
AMOUNT_IN="5000000" # 5 USDC

echo "=== T9.3 On-Chain Split Swap Runner ==="
echo "Mode: $MODE"
echo "API: $API_URL"
echo "User: $USER_ADDR"
echo "Input: $AMOUNT_IN atomic ($(($AMOUNT_IN / 1000000)) USDC)"
echo ""

# 1. Fetch fresh quote
echo "1. Fetching fresh 5e6 USDC -> EURC quote..."
QUOTE_RESP=$(curl -sf "${API_URL}/api/v1/quote?token_in=${USDC}&token_out=${EURC}&amount_in=${AMOUNT_IN}&slippage_bps=50" -H 'Content-Type: application/json')

IS_SPLIT=$(echo "$QUOTE_RESP" | python3 -c "import sys, json; d=json.load(sys.stdin); print(d.get('data',{}).get('is_split', False))")
EXPECTED_OUT=$(echo "$QUOTE_RESP" | python3 -c "import sys, json; d=json.load(sys.stdin); print(d.get('data',{}).get('expected_output', '0'))")
MIN_OUT=$(echo "$QUOTE_RESP" | python3 -c "import sys, json; d=json.load(sys.stdin); print(d.get('data',{}).get('minimum_output', '0'))")
SUBROUTES_COUNT=$(echo "$QUOTE_RESP" | python3 -c "import sys, json; d=json.load(sys.stdin); print(len(d.get('data',{}).get('sub_routes', [])))")

echo "  Quote received: expected_out=$EXPECTED_OUT, min_out=$MIN_OUT, is_split=$IS_SPLIT, legs=$SUBROUTES_COUNT"
if [ "$IS_SPLIT" != "True" ] && [ "$IS_SPLIT" != "true" ]; then
  echo "  WARNING: Quote is not split (is_split=$IS_SPLIT)"
fi

# 2. Build transaction payload
echo "2. Building transaction payload via /build_tx..."
BUILD_PAYLOAD=$(echo "$QUOTE_RESP" | python3 -c "
import sys, json
q = json.load(sys.stdin)['data']
payload = {
    'user': '$USER_ADDR',
    'token_in': '$USDC',
    'token_out': '$EURC',
    'amount_in': '$AMOUNT_IN',
    'min_amount_out': q['minimum_output'],
    'sub_routes': [{
        'amount_in': sr['amount_in'],
        'steps': [{
            'dex_type': sr['dex_types'][i],
            'pool_address': sr['pool_addresses'][i],
            'token_in': '$USDC',
            'token_out': '$EURC',
            'fee_bps': sr['hop_fees'][i]
        } for i in range(len(sr['pool_addresses']))]
    } for sr in q['sub_routes']]
}
print(json.dumps(payload))
")

BUILD_RESP=$(curl -sf "${API_URL}/api/v1/build_tx" -H 'Content-Type: application/json' -d "$BUILD_PAYLOAD")

TX_TO=$(echo "$BUILD_RESP" | python3 -c "import sys, json; d=json.load(sys.stdin); print(d.get('data',{}).get('to', ''))")
TX_VALUE=$(echo "$BUILD_RESP" | python3 -c "import sys, json; d=json.load(sys.stdin); print(d.get('data',{}).get('value', ''))")
TX_DATA=$(echo "$BUILD_RESP" | python3 -c "import sys, json; d=json.load(sys.stdin); print(d.get('data',{}).get('data', ''))")

echo "  Build_tx received:"
echo "    to: $TX_TO"
echo "    value: $TX_VALUE"
echo "    data length: $((${#TX_DATA} / 2)) bytes"

if [ "$TX_VALUE" != "0" ]; then
  echo "ERROR: Aggregator requires value == '0', got '$TX_VALUE'" >&2
  exit 1
fi

if [ -z "$TX_DATA" ] || [ "$TX_DATA" == "" ]; then
  echo "ERROR: Calldata is empty" >&2
  exit 1
fi

export CHAKRA_AGGREGATOR="$TX_TO"
export CHAKRA_SWAP_CALLDATA="$TX_DATA"
export CHAKRA_CALL_VALUE="$TX_VALUE"

# 3. Execute via arc-operator.sh
echo ""
echo "3. Running Foundry script with mode: $MODE..."
chmod +x ./scripts/arc-operator.sh
./scripts/arc-operator.sh "$MODE" script script/ExecuteSplitSwap.s.sol
