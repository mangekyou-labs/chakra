#!/usr/bin/env bash
# T2.5: Read-only factory discovery scan. Queries public Arc RPC for
# PairCreated/PoolCreated events on configured seed factories.
# NEVER writes to the aggregator allowlist.
#
# Usage: CHAKRA_SEED_FACTORIES="0xaddr:xyk,0xaddr2:stable" ./scripts/discovery_scan.sh

set -euo pipefail

RPC="${CHAKRA_RPC_HTTP:-https://rpc.testnet.arc.io}"
FACTORIES="${CHAKRA_SEED_FACTORIES:-}"
DISCOVERY="${CHAKRA_DISCOVERY_FACTORIES:-}"

echo "=== T2.5 Factory Discovery Scan ==="
echo "RPC: $RPC"
echo ""

if [ -z "$FACTORIES" ] && [ -z "$DISCOVERY" ]; then
  echo "No seed or discovery factories configured."
  echo "Set CHAKRA_SEED_FACTORIES or CHAKRA_DISCOVERY_FACTORIES as 'address:type' tuples."
  echo "Result: no created pools to discover."
  exit 0
fi

# Correct topic0s (keccak256 of event signatures)
V2_PAIR_CREATED="0x0d3648bd0f6ba80134a33ba9275ac585d9d315f0ad8355cddefde31afa28d0e9"
V3_POOL_CREATED="0x783cca1c0412dd0d695e784568c96da2e9c22ff989357a2e8b1d9b2b4e6b7118"
STABLE_POOL_CREATED="0x9c5d829b9b23efc461f9aeef91979ec04bb903feb3bee4f26d22114abfc7335b"

# Decode an EVM address from a 32-byte zero-padded hex word
decode_address() {
  # Strip 0x prefix and leading zeros, take last 40 hex chars (20 bytes)
  local word="${1#0x}"
  echo "0x${word:24:40}"
}

# Decode a uint256 from a 32-byte hex word
decode_uint256() {
  local word="${1#0x}"
  echo "0x${word}"
}

total_found=0
total_errors=0

for entry in $(echo "${FACTORIES:+$FACTORIES,}${DISCOVERY:+$DISCOVERY}" | tr ',' '\n' | grep -v '^$'); do
  addr=$(echo "$entry" | cut -d: -f1)
  dtype=$(echo "$entry" | cut -d: -f2)
  echo "Scanning factory $addr ($dtype)..."

  # Type-specific topic: xyk -> V2, clmm -> V3, stable -> stable
  case "$dtype" in
    xyk|v2)
      topics="$V2_PAIR_CREATED"
      ;;
    clmm|v3)
      topics="$V3_POOL_CREATED"
      ;;
    stable)
      topics="$STABLE_POOL_CREATED"
      ;;
    *)
      # Unknown type: scan all three topics
      topics="$V2_PAIR_CREATED $V3_POOL_CREATED $STABLE_POOL_CREATED"
      ;;
  esac

  for topic in $topics; do
    rpc_payload="{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"eth_getLogs\",\"params\":[{\"fromBlock\":\"0x0\",\"toBlock\":\"latest\",\"address\":\"$addr\",\"topics\":[\"$topic\"]}]}"
    result=$(curl -sf "$RPC" -H 'Content-Type: application/json' -d "$rpc_payload" 2>/dev/null) || {
      echo "  ERROR: RPC request failed for factory $addr topic $topic" >&2
      total_errors=$((total_errors + 1))
      continue
    }
    if [ -z "$result" ]; then
      echo "  ERROR: RPC request failed for factory $addr topic $topic" >&2
      total_errors=$((total_errors + 1))
      continue
    fi

    # Check for RPC-level error
    rpc_error=$(echo "$result" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    err = d.get('error')
    if err:
        print(f'RPC error {err.get(\"code\",\"?\")}: {err.get(\"message\",\"unknown\")}')
    else:
        print('')
except Exception as e:
    print(f'JSON parse error: {e}')
" 2>/dev/null)
    if [ -n "$rpc_error" ]; then
      echo "  ERROR: $rpc_error" >&2
      total_errors=$((total_errors + 1))
      continue
    fi

    # Decode logs
    echo "$result" | python3 -c "
import sys, json

def decode_address(word):
    if not word or len(word) < 66:
        return None
    return '0x' + word[26:66]

logs = []
try:
    d = json.load(sys.stdin)
    logs = d.get('result', [])
except:
    print('  ERROR: failed to parse JSON response', file=sys.stderr)
    sys.exit(1)

topic = '$topic'
count = len(logs)
if count == 0:
    sys.exit(0)

print(f'  {count} creation event(s) found:')
for i, log in enumerate(logs):
    topics = log.get('topics', [])
    data = log.get('data', '0x')
    if len(topics) < 3:
        print(f'    [{i+1}] malformed (only {len(topics)} topics)')
        continue

    token0 = decode_address(topics[1])
    token1 = decode_address(topics[2])

    if topic == '$V2_PAIR_CREATED' or topic == '$STABLE_POOL_CREATED':
        # data contains pool address (32 bytes, last 20)
        data_words = [data[j:j+64] for j in range(2, len(data), 64)]
        if data_words:
            pool = decode_address('0x' + data_words[0])
        else:
            pool = '?'
        print(f'    [{i+1}] pool={pool}  token0={token0}  token1={token1}')
    elif topic == '$V3_POOL_CREATED':
        data_words = [data[j:j+64] for j in range(2, len(data), 64)]
        if len(data_words) >= 3:
            fee_raw = int(data_words[0], 16)
            fee = fee_raw // 10000 if fee_raw >= 10000 else fee_raw
            tick_spacing = int.from_bytes(bytes.fromhex(data_words[1][56:64]), 'big', signed=True)
            pool = decode_address('0x' + data_words[2])
            print(f'    [{i+1}] pool={pool}  token0={token0}  token1={token1}  fee={fee}  tickSpacing={tick_spacing}')
        else:
            print(f'    [{i+1}] token0={token0}  token1={token1}  (data too short)')
" 2>/dev/null
    if [ $? -ne 0 ]; then
      echo "  ERROR: failed to decode logs for topic $topic" >&2
      total_errors=$((total_errors + 1))
      continue
    fi
    total_found=$((total_found + count))
  done
done

echo ""
echo "Scan complete. Found $total_found pool(s), $total_errors error(s)."
echo "Results are read-only — no allowlist changes."

if [ "$total_errors" -gt 0 ]; then
  exit 1
fi
