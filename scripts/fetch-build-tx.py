#!/usr/bin/env python3
"""T9.3 helper: quote + build_tx + save calldata for ExecuteSplitSwap."""
import json
import os
import sys
import urllib.request

API = os.environ.get("CHAKRA_API_URL", "https://chakra-api-0a5i.onrender.com")
USER = os.environ.get("T93_USER", "0x12E266744f6d25D372000e066eCc0DF5a752276d")
USDC = "0x3600000000000000000000000000000000000000"
EURC = "0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a"
AMOUNT = "5000000"  # 5 USDC (6 dp)


def get(path):
    with urllib.request.urlopen(API + path, timeout=30) as r:
        return json.loads(r.read())


def post(path, payload):
    req = urllib.request.Request(
        API + path,
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.loads(r.read())


quote = get(f"/api/v1/quote?token_in={USDC}&token_out={EURC}&amount_in={AMOUNT}&slippage_bps=50")["data"]
print(f"is_split={quote['is_split']} expected={quote['expected_output']} min={quote['minimum_output']} legs={len(quote['sub_routes'])}")

steps_payload = []
for sr in quote["sub_routes"]:
    steps = []
    pools = sr["pool_addresses"]
    path = sr["path"]
    for i, pool in enumerate(pools):
        steps.append(
            {
                "dex_type": sr["dex_types"][i] if i < len(sr.get("dex_types", [])) else "stable",
                "pool_address": pool,
                "token_in": path[i],
                "token_out": path[i + 1] if i + 1 < len(path) else EURC,
                "fee_bps": sr["hop_fees"][i] if i < len(sr.get("hop_fees", [])) else None,
            }
        )
    steps_payload.append({"amount_in": sr["amount_in"], "steps": steps})

build = post(
    "/api/v1/build_tx",
    {
        "user": USER,
        "token_in": USDC,
        "token_out": EURC,
        "amount_in": AMOUNT,
        "min_amount_out": quote["minimum_output"],
        "sub_routes": steps_payload,
    },
)
if not build.get("success"):
    print("build_tx error:", build.get("error"))
    sys.exit(1)
d = build["data"]
print(f"to={d['to']} value={d['value']} data_len={len(d['data'])}")
print(f"typed_data={d['typed_data']} required_approvals={d['required_approvals']}")
out = os.environ.get("T93_CALLDATA_OUT")
if out:
    with open(out, "w") as f:
        f.write(d["data"])
    print(f"calldata saved to {out} (len {len(d['data'])})")
