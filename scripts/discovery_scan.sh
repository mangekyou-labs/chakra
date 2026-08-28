#!/usr/bin/env bash
# T2.5: Read-only factory discovery scan. Queries public Arc RPC for
# PairCreated/PoolCreated events on configured seed factories.
# Pages getLogs in <=10k windows from CHAKRA_SCAN_FROM_BLOCK/0x0 to latest.
# NEVER writes to the aggregator allowlist.
#
# Usage: CHAKRA_SEED_FACTORIES="0xaddr:xyk,0xaddr2:stable" ./scripts/discovery_scan.sh

set -euo pipefail

RPC="${CHAKRA_RPC_HTTP:-https://rpc.testnet.arc.io}"
FACTORIES="${CHAKRA_SEED_FACTORIES:-}"
DISCOVERY="${CHAKRA_DISCOVERY_FACTORIES:-}"
CHUNK_SIZE="${CHAKRA_SCAN_CHUNK_SIZE:-8000}"
FROM_BLOCK="${CHAKRA_SCAN_FROM_BLOCK:-0x0}"
TO_BLOCK="${CHAKRA_SCAN_TO_BLOCK:-latest}"

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

# Bash topic dispatcher comment:
# case "$dtype" in
#   xyk|v2)
#   clmm|v3)
#   stable)
# esac

# Quick connectivity probe with curl -sf (ensures errors propagate immediately)
test_payload='{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}'
_probe=$(curl -sf "$RPC" -H 'Content-Type: application/json' -d "$test_payload" 2>/dev/null) || {
  echo "  ERROR: RPC connectivity failed for $RPC" >&2
  exit 1
}

export CHAKRA_RPC_HTTP="$RPC"
export CHAKRA_SEED_FACTORIES="$FACTORIES"
export CHAKRA_DISCOVERY_FACTORIES="$DISCOVERY"
export CHAKRA_SCAN_CHUNK_SIZE="$CHUNK_SIZE"
export CHAKRA_SCAN_FROM_BLOCK="$FROM_BLOCK"
export CHAKRA_SCAN_TO_BLOCK="$TO_BLOCK"
export V2_PAIR_CREATED="$V2_PAIR_CREATED"
export V3_POOL_CREATED="$V3_POOL_CREATED"
export STABLE_POOL_CREATED="$STABLE_POOL_CREATED"

# Python chunked scanner runner
python3 - <<'EOF'
import os, sys, json, time, urllib.request

rpc_url = os.environ.get("CHAKRA_RPC_HTTP", "https://rpc.testnet.arc.io")
factories_raw = os.environ.get("CHAKRA_SEED_FACTORIES", "")
discovery_raw = os.environ.get("CHAKRA_DISCOVERY_FACTORIES", "")
raw_chunk = os.environ.get("CHAKRA_SCAN_CHUNK_SIZE", "8000")
chunk_size = max(1, min(int(raw_chunk), 10000))
from_block_raw = os.environ.get("CHAKRA_SCAN_FROM_BLOCK", "0x0").strip()
to_block_raw = os.environ.get("CHAKRA_SCAN_TO_BLOCK", "latest").strip()

rpc_failovers = [
    rpc_url,
    "https://rpc.quicknode.testnet.arc.io",
    "https://rpc.drpc.testnet.arc.io",
    "https://rpc.blockdaemon.testnet.arc.io",
]

def rpc_call(method, params, timeout=15):
    payload = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    last_err = None
    for endpoint in rpc_failovers:
        for attempt in range(3):
            try:
                req = urllib.request.Request(
                    endpoint,
                    data=payload,
                    headers={"Content-Type": "application/json", "User-Agent": "curl/8.7.1"}
                )
                with urllib.request.urlopen(req, timeout=timeout) as resp:
                    data = json.load(resp)
                    if "error" in data:
                        err = data["error"]
                        last_err = err
                        if err.get("code") in [-32614, 4444, 35]:
                            raise ValueError(f"Range limit: {err.get('message')}")
                        return None, err
                    return data.get("result"), None
            except urllib.error.HTTPError as e:
                last_err = {"code": e.code, "message": f"HTTP {e.code}"}
                if e.code == 429:
                    time.sleep(0.5 * (attempt + 1))
                else:
                    break
            except Exception as e:
                last_err = {"message": str(e)}
                time.sleep(0.3)
    return None, last_err or {"message": "All RPC failovers exhausted"}

# 1. Resolve latest once
if not to_block_raw or to_block_raw == "latest":
    latest_hex, err = rpc_call("eth_blockNumber", [])
    if err or not latest_hex:
        print(f"ERROR: Failed to fetch latest block number: {err}", file=sys.stderr)
        sys.exit(1)
    to_block = int(latest_hex, 16)
else:
    to_block = int(to_block_raw, 16) if to_block_raw.startswith("0x") else int(to_block_raw)

# 2. Resolve from_block (default 0x0)
if from_block_raw and from_block_raw != "0x0" and from_block_raw != "0":
    from_block = int(from_block_raw, 16) if from_block_raw.startswith("0x") else int(from_block_raw)
else:
    from_block = 0

def decode_address(word):
    if not word or len(word) < 66:
        return None
    return "0x" + word[26:66].lower()

def decode_v3_data(data_hex, topics):
    data_clean = data_hex[2:] if data_hex.startswith("0x") else data_hex
    words = [data_clean[i:i+64] for i in range(0, len(data_clean), 64)]
    
    if len(topics) >= 4:
        token0 = decode_address(topics[1])
        token1 = decode_address(topics[2])
        fee_raw = int(topics[3], 16)
        fee = fee_raw // 10000 if fee_raw >= 10000 else fee_raw
        if len(words) >= 2:
            tick_spacing = int.from_bytes(bytes.fromhex(words[0][56:64]), "big", signed=True)
            pool = decode_address("0x" + words[1])
            return token0, token1, fee, tick_spacing, pool
        elif len(words) >= 1:
            pool = decode_address("0x" + words[0])
            return token0, token1, fee, 0, pool
    elif len(words) >= 3:
        token0 = decode_address(topics[1])
        token1 = decode_address(topics[2])
        fee_raw = int(words[0], 16)
        fee = fee_raw // 10000 if fee_raw >= 10000 else fee_raw
        tick_spacing = int.from_bytes(bytes.fromhex(words[1][56:64]), "big", signed=True)
        pool = decode_address("0x" + words[2])
        return token0, token1, fee, tick_spacing, pool
    return None, None, 0, 0, None

V2_TOPIC = os.environ.get("V2_PAIR_CREATED", "0x0d3648bd0f6ba80134a33ba9275ac585d9d315f0ad8355cddefde31afa28d0e9")
V3_TOPIC = os.environ.get("V3_POOL_CREATED", "0x783cca1c0412dd0d695e784568c96da2e9c22ff989357a2e8b1d9b2b4e6b7118")
STABLE_POOL_CREATED = os.environ.get("STABLE_POOL_CREATED", "0x9c5d829b9b23efc461f9aeef91979ec04bb903feb3bee4f26d22114abfc7335b")

entries = [e.strip() for e in (factories_raw + "," + discovery_raw).split(",") if e.strip()]

total_found = 0
total_errors = 0

for entry in entries:
    parts = entry.split(":")
    addr = parts[0].strip()
    dtype = parts[1].strip() if len(parts) > 1 else "unknown"
    print(f"Scanning factory {addr} ({dtype})...")

    if dtype in ["xyk", "v2"]:
        topics = [V2_TOPIC]
    elif dtype in ["clmm", "v3"]:
        topics = [V3_TOPIC]
    elif dtype in ["stable"]:
        topics = [STABLE_POOL_CREATED]
    else:
        topics = [V2_TOPIC, V3_TOPIC, STABLE_POOL_CREATED]

    for topic in topics:
        cur_from = from_block
        topic_logs = []
        has_error = False

        # Iterate inclusive non-overlapping [cur_from, min(cur_from + chunk - 1, to_block)]
        while cur_from <= to_block:
            cur_to = min(cur_from + chunk_size - 1, to_block)
            logs, err = rpc_call("eth_getLogs", [{
                "fromBlock": hex(cur_from),
                "toBlock": hex(cur_to),
                "address": addr,
                "topics": [topic]
            }])
            if err:
                print(f"  ERROR: getLogs failed for {addr} blocks {cur_from}..{cur_to}: {err}", file=sys.stderr)
                has_error = True
                total_errors += 1
                break
            if logs:
                topic_logs.extend(logs)
            cur_from = cur_to + 1
            time.sleep(0.02)

        if has_error:
            continue

        if topic_logs:
            print(f"  {len(topic_logs)} creation event(s) found for topic {topic[:10]}...:")
            for i, log in enumerate(topic_logs):
                t_list = log.get("topics", [])
                d_str = log.get("data", "0x")
                if topic == V2_TOPIC or topic == STABLE_POOL_CREATED:
                    token0 = decode_address(t_list[1]) if len(t_list) > 1 else "?"
                    token1 = decode_address(t_list[2]) if len(t_list) > 2 else "?"
                    data_words = [d_str[j:j+64] for j in range(2, len(d_str), 64)]
                    pool = decode_address("0x" + data_words[0]) if data_words else "?"
                    print(f"    [{i+1}] pool={pool}  token0={token0}  token1={token1}")
                elif topic == V3_TOPIC:
                    token0, token1, fee, tick_spacing, pool = decode_v3_data(d_str, t_list)
                    print(f"    [{i+1}] pool={pool}  token0={token0}  token1={token1}  fee={fee}  tickSpacing={tick_spacing}")
            total_found += len(topic_logs)

print("")
print(f"Scan complete. Found {total_found} pool(s), {total_errors} error(s).")
print("Results are read-only — no allowlist changes.")

if total_errors > 0:
    sys.exit(1)
EOF
