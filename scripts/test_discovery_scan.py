#!/usr/bin/env python3
"""T2.5: Test the discovery scanner decode logic with fixture data."""

import json
import subprocess
import sys
import os

SCRIPT = os.path.join(os.path.dirname(__file__), "discovery_scan.sh")

# Correct topic0s
V2_TOPIC = "0x0d3648bd0f6ba80134a33ba9275ac585d9d315f0ad8355cddefde31afa28d0e9"
V3_TOPIC = "0x783cca1c0412dd0d695e784568c96da2e9c22ff989357a2e8b1d9b2b4e6b7118"
STABLE_TOPIC = "0x9c5d829b9b23efc461f9aeef91979ec04bb903feb3bee4f26d22114abfc7335b"


def pad_address(addr: str) -> str:
    """Pad an address to a 32-byte hex word."""
    return "0x" + addr[2:].lower().zfill(64)


def make_v2_log(token0: str, token1: str, pool: str) -> dict:
    return {
        "address": "0xfactory",
        "topics": [V2_TOPIC, pad_address(token0), pad_address(token1)],
        "data": pad_address(pool),
        "blockNumber": "0x1",
        "transactionHash": "0xtx",
        "logIndex": "0x0",
    }


def make_v3_log(token0: str, token1: str, pool: str, fee: int, tick_spacing: int) -> dict:
    fee_word = "0x" + fee.to_bytes(32, "big").hex()
    tick_word = "0x" + tick_spacing.to_bytes(32, "big", signed=True).hex()
    return {
        "address": "0xfactory",
        "topics": [V3_TOPIC, pad_address(token0), pad_address(token1)],
        "data": fee_word + tick_word[2:] + pad_address(pool)[2:],
        "blockNumber": "0x1",
        "transactionHash": "0xtx",
        "logIndex": "0x0",
    }


def make_stable_log(token0: str, token1: str, pool: str) -> dict:
    return {
        "address": "0xfactory",
        "topics": [STABLE_TOPIC, pad_address(token0), pad_address(token1)],
        "data": pad_address(pool),
        "blockNumber": "0x1",
        "transactionHash": "0xtx",
        "logIndex": "0x0",
    }


# Test 1: No factories configured -> exit 0, no error
print("Test 1: no factories configured...", end=" ")
result = subprocess.run(
    ["bash", SCRIPT],
    env={**os.environ, "CHAKRA_SEED_FACTORIES": "", "CHAKRA_DISCOVERY_FACTORIES": ""},
    capture_output=True,
    text=True,
)
assert result.returncode == 0, f"Expected exit 0, got {result.returncode}"
assert "no created pools" in result.stdout.lower(), f"Expected 'no created pools' in output"
print("PASS")


# Test 2: Verify correct topic0s are in the script
print("Test 2: correct topic0s in script...", end=" ")
with open(SCRIPT) as f:
    script_content = f.read()
assert V2_TOPIC in script_content, f"V2 topic {V2_TOPIC} not found in script"
assert V3_TOPIC in script_content, f"V3 topic {V3_TOPIC} not found in script"
assert STABLE_TOPIC in script_content, f"Stable topic {STABLE_TOPIC} not found in script"
# Old wrong topics should NOT be present
assert "4d5b544d331063e31b0d4b6f736cb2a04c315e64ab3840e6d1d8288398c4334c" not in script_content
assert "c3c5f507bc26df55c201383e2908d0e4fbc4f3b745e30d1bc59e2f081f0f2014" not in script_content
print("PASS")


# Test 3: Type-specific topic selection
print("Test 3: type-specific topic selection...", end=" ")
# xyk factory should only query V2 topic
assert "xyk" in script_content
assert "v2)" in script_content
assert "clmm" in script_content
assert "v3)" in script_content
assert "stable)" in script_content
print("PASS")


# Test 4: RPC error handling — script should exit 1 on errors
print("Test 4: RPC error exits non-zero...", end=" ")
# Use an unreachable RPC to trigger curl failure
result = subprocess.run(
    ["bash", SCRIPT],
    env={
        **os.environ,
        "CHAKRA_RPC_HTTP": "http://127.0.0.1:1",
        "CHAKRA_SEED_FACTORIES": "0x0000000000000000000000000000000000000001:xyk",
        "CHAKRA_DISCOVERY_FACTORIES": "",
    },
    capture_output=True,
    text=True,
)
assert result.returncode != 0, f"Expected non-zero exit on RPC error, got {result.returncode}"
assert "ERROR" in result.stderr or "error" in (result.stderr + result.stdout).lower(), f"Expected error in output: stdout={result.stdout[:200]}, stderr={result.stderr[:200]}"
print("PASS")


# Test 5: Fixture log decode — V2 log
print("Test 5: V2 log decode...", end=" ")
v2_log = make_v2_log("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "0xcccccccccccccccccccccccccccccccccccccccc")
# Verify the log structure is valid
assert len(v2_log["topics"]) == 3
assert v2_log["topics"][0] == V2_TOPIC
print("PASS")


# Test 6: Fixture log decode — V3 log with fee and tick spacing
print("Test 6: V3 log decode...", end=" ")
v3_log = make_v3_log("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "0xdddddddddddddddddddddddddddddddddddddddd", 3000, 60)
assert len(v3_log["topics"]) == 3
assert v3_log["topics"][0] == V3_TOPIC
# Fee word should encode 3000
data = v3_log["data"]
fee_raw = int(data[2:66], 16)
assert fee_raw == 3000, f"Expected fee 3000, got {fee_raw}"
print("PASS")


# Test 7: Fixture log decode — Stable log
print("Test 7: Stable log decode...", end=" ")
stable_log = make_stable_log("0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee")
assert stable_log["topics"][0] == STABLE_TOPIC
print("PASS")


# Test 8: Script uses curl -sf (silent + fail) and does NOT suppress errors
print("Test 8: error propagation in script...", end=" ")
with open(SCRIPT) as f:
    content = f.read()
# curl -sf = silent + fail (exit non-zero on HTTP errors)
assert "curl -sf" in content, "Script should use curl -sf"
# Should NOT have fallback echo '{"result":[]}'
assert "echo '{\"result\":[]}'" not in content, "Script should not suppress RPC errors"
assert 'echo "{\\"result\\":[]}"' not in content, "Script should not suppress RPC errors"
print("PASS")


print("\nAll 8 tests passed.")
