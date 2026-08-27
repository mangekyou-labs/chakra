#!/usr/bin/env bash
set -euo pipefail

# Manual seed for Arc testnet - works around Foundry's inability to simulate
# Circle CCTP USDC transfers. Uses `cast send` and `forge create` directly.

WALLET_FILE="${ARC_WALLET_FILE:-$HOME/.arc-canteen/wallet.yaml}"
RPC_URL="${ARC_RPC_URL:-https://rpc.testnet.arc.io}"

PRIVATE_KEY="$(sed -n 's/^private_key:[[:space:]]*//p' "$WALLET_FILE" | head -n 1 | tr -d "'\"")"
export PRIVATE_KEY
OPERATOR="0x12E266744f6d25D372000e066eCc0DF5a752276d"
echo "Operator: $OPERATOR"

# Deployed addresses from Deploy.s.sol
MBTC="0xbf5a25D7070FaACAe309D66D05372a6b212ECbdF"
XYK_FACTORY="0x0c812E5D55D767533c8E4783D33b28EA825b4D8e"
STABLE_FACTORY="0x77Ce21FDAAea40Fd94aCf65fF3220A0A7Db7D690"
CLMM_FACTORY="0xf6dEa9e6dfE392aaBE366240db4839709572fa69"
AGGREGATOR="0xA59ad3E82d251c3489582e1aA5Bee494d0d2a569"
USDC="0x3600000000000000000000000000000000000000"
EURC="0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a"

# Gas reserve (in 6-decimal USDC units)
GAS_RESERVE=400000

# ── Check balances ──
echo "=== Pre-seed balances ==="
USDC_BAL=$(cast call "$USDC" "balanceOf(address)" "$OPERATOR" --rpc-url "$RPC_URL")
EURC_BAL=$(cast call "$EURC" "balanceOf(address)" "$OPERATOR" --rpc-url "$RPC_URL")
MBTC_BAL=$(cast call "$MBTC" "balanceOf(address)" "$OPERATOR" --rpc-url "$RPC_URL")

echo "  USDC: $USDC_BAL"
echo "  EURC: $EURC_BAL"
echo "  mBTC: $MBTC_BAL"

# Convert hex to decimal for arithmetic
usdc_dec=$((USDC_BAL))
eurc_dec=$((EURC_BAL))
mbtc_dec=$((MBTC_BAL))

usable_usdc=$((usdc_dec - GAS_RESERVE))
echo "  Usable USDC: $usable_usdc"
echo ""

# ── Step 1: Deploy LiquiditySeeder ──
echo "=== [1/8] Deploying LiquiditySeeder ==="
SEEDER_OUT=$(cd contracts/evm && forge create src/LiquiditySeeder.sol:LiquiditySeeder \
  --rpc-url "$RPC_URL" \
  --private-key "$PRIVATE_KEY" \
  --json 2>&1)
SEEDER_ADDR=$(echo "$SEEDER_OUT" | python3 -c "import json,sys; print(json.load(sys.stdin)['deployedTo'])")
echo "  LiquiditySeeder: $SEEDER_ADDR"
echo ""

# ── Step 2: Mint mBTC ──
echo "=== [2/8] Minting mBTC ==="
MBTC_TARGET=100000000  # 10 mBTC (8 decimals)
if [ "$mbtc_dec" -ge "$MBTC_TARGET" ]; then
  echo "  mBTC already funded: $mbtc_dec >= $MBTC_TARGET"
else
  MBTC_DEFICIT=$((MBTC_TARGET - mbtc_dec))
  cast send "$MBTC" "mint(address,uint256)" "$OPERATOR" "$MBTC_DEFICIT" \
    --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1
  echo "  Minted $MBTC_DEFICIT mBTC"
fi
echo ""

# ── Compute allocations ──
usdc_stable=$((usable_usdc * 40 / 100))
usdc_v2_eurc=$((usable_usdc * 20 / 100))
usdc_v2_mbtc=$((usable_usdc * 20 / 100))
usdc_v3_mbtc=$((usable_usdc - usdc_stable - usdc_v2_eurc - usdc_v2_mbtc))

eurc_stable=$usdc_stable
eurc_v2_eurc=$usdc_v2_eurc
eurc_v2_mbtc=$usdc_v2_mbtc

echo "=== Allocations ==="
echo "  USDC: stable=$usdc_stable v2Eurc=$usdc_v2_eurc v2Mbtc=$usdc_v2_mbtc v3Mbtc=$usdc_v3_mbtc"
echo "  EURC: stable=$eurc_stable v2Eurc=$eurc_v2_eurc v2Mbtc=$eurc_v2_mbtc"
echo ""

# ── Step 3: Create Stable USDC/EURC pool ──
echo "=== [3/8] Creating Stable USDC/EURC pool ==="
STABLE_POOL=$(cast call "$STABLE_FACTORY" "getPool(address,address)" "$USDC" "$EURC" --rpc-url "$RPC_URL")
if [ "$STABLE_POOL" = "0x0000000000000000000000000000000000000000" ]; then
  cast send "$STABLE_FACTORY" "createPool(address,address)" "$USDC" "$EURC" \
    --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1
  sleep 2
  STABLE_POOL=$(cast call "$STABLE_FACTORY" "getPool(address,address)" "$USDC" "$EURC" --rpc-url "$RPC_URL")
  echo "  Created stable pool: $STABLE_POOL"
else
  echo "  Reusing existing pool: $STABLE_POOL"
fi

echo "  Transferring USDC ($usdc_stable) to pool..."
cast send "$USDC" "transfer(address,uint256)" "$STABLE_POOL" "$usdc_stable" \
  --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1

echo "  Transferring EURC ($eurc_stable) to pool..."
cast send "$EURC" "transfer(address,uint256)" "$STABLE_POOL" "$eurc_stable" \
  --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1

echo "  Calling seedLiquidity..."
cast send "$STABLE_POOL" "seedLiquidity(uint256,uint256)" "$usdc_stable" "$eurc_stable" \
  --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1
echo "  Stable pool seeded: USDC=$usdc_stable EURC=$eurc_stable"
echo ""

# ── Step 4: V2 USDC/EURC ──
echo "=== [4/8] V2 USDC/EURC ==="
PAIR_UE=$(cast call "$XYK_FACTORY" "getPair(address,address)" "$USDC" "$EURC" --rpc-url "$RPC_URL")
if [ "$PAIR_UE" = "0x0000000000000000000000000000000000000000" ]; then
  cast send "$XYK_FACTORY" "createPair(address,address)" "$USDC" "$EURC" \
    --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1
  sleep 2
  PAIR_UE=$(cast call "$XYK_FACTORY" "getPair(address,address)" "$USDC" "$EURC" --rpc-url "$RPC_URL")
  echo "  Created pair: $PAIR_UE"
else
  echo "  Reusing pair: $PAIR_UE"
fi

# Approve tokens for LiquiditySeeder
cast send "$USDC" "approve(address,uint256)" "$SEEDER_ADDR" "$usdc_v2_eurc" \
  --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1
cast send "$EURC" "approve(address,uint256)" "$SEEDER_ADDR" "$eurc_v2_eurc" \
  --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1

# seedV2(address pair, address tokenA, address tokenB, uint256 amtA, uint256 amtB, address to)
cast send "$SEEDER_ADDR" \
  "seedV2(address,address,address,uint256,uint256,address)" \
  "$PAIR_UE" "$USDC" "$EURC" "$usdc_v2_eurc" "$eurc_v2_eurc" "$OPERATOR" \
  --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1
echo "  V2 USDC/EURC seeded"
echo ""

# ── Step 5: V2 USDC/mBTC ──
echo "=== [5/8] V2 USDC/mBTC ==="
PAIR_UM=$(cast call "$XYK_FACTORY" "getPair(address,address)" "$USDC" "$MBTC" --rpc-url "$RPC_URL")
if [ "$PAIR_UM" = "0x0000000000000000000000000000000000000000" ]; then
  cast send "$XYK_FACTORY" "createPair(address,address)" "$USDC" "$MBTC" \
    --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1
  sleep 2
  PAIR_UM=$(cast call "$XYK_FACTORY" "getPair(address,address)" "$USDC" "$MBTC" --rpc-url "$RPC_URL")
  echo "  Created pair: $PAIR_UM"
else
  echo "  Reusing pair: $PAIR_UM"
fi

mbtc_v2_mbtc=$((usdc_v2_mbtc / 1000))
cast send "$USDC" "approve(address,uint256)" "$SEEDER_ADDR" "$usdc_v2_mbtc" \
  --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1
cast send "$MBTC" "approve(address,uint256)" "$SEEDER_ADDR" "$mbtc_v2_mbtc" \
  --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1
cast send "$SEEDER_ADDR" \
  "seedV2(address,address,address,uint256,uint256,address)" \
  "$PAIR_UM" "$USDC" "$MBTC" "$usdc_v2_mbtc" "$mbtc_v2_mbtc" "$OPERATOR" \
  --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1
echo "  V2 USDC/mBTC seeded"
echo ""

# ── Step 6: V2 EURC/mBTC ──
echo "=== [6/8] V2 EURC/mBTC ==="
PAIR_EM=$(cast call "$XYK_FACTORY" "getPair(address,address)" "$EURC" "$MBTC" --rpc-url "$RPC_URL")
if [ "$PAIR_EM" = "0x0000000000000000000000000000000000000000" ]; then
  cast send "$XYK_FACTORY" "createPair(address,address)" "$EURC" "$MBTC" \
    --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1
  sleep 2
  PAIR_EM=$(cast call "$XYK_FACTORY" "getPair(address,address)" "$EURC" "$MBTC" --rpc-url "$RPC_URL")
  echo "  Created pair: $PAIR_EM"
else
  echo "  Reusing pair: $PAIR_EM"
fi

mbtc_v2_eurc=$((eurc_v2_mbtc / 1000))
cast send "$EURC" "approve(address,uint256)" "$SEEDER_ADDR" "$eurc_v2_mbtc" \
  --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1
cast send "$MBTC" "approve(address,uint256)" "$SEEDER_ADDR" "$mbtc_v2_eurc" \
  --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1
cast send "$SEEDER_ADDR" \
  "seedV2(address,address,address,uint256,uint256,address)" \
  "$PAIR_EM" "$EURC" "$MBTC" "$eurc_v2_mbtc" "$mbtc_v2_eurc" "$OPERATOR" \
  --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1
echo "  V2 EURC/mBTC seeded"
echo ""

# ── Step 7: V3 USDC/mBTC ──
echo "=== [7/8] V3 USDC/mBTC (30 bps) ==="
CLMM_FEE=3000
POOL_V3=$(cast call "$CLMM_FACTORY" "getPool(address,address,uint24)" "$USDC" "$MBTC" "$CLMM_FEE" --rpc-url "$RPC_URL")
if [ "$POOL_V3" = "0x0000000000000000000000000000000000000000" ]; then
  cast send "$CLMM_FACTORY" "createPool(address,address,uint24)" "$USDC" "$MBTC" "$CLMM_FEE" \
    --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1
  sleep 2
  POOL_V3=$(cast call "$CLMM_FACTORY" "getPool(address,address,uint24)" "$USDC" "$MBTC" "$CLMM_FEE" --rpc-url "$RPC_URL")
  echo "  Created V3 pool: $POOL_V3"
else
  echo "  Reusing V3 pool: $POOL_V3"
fi

# Initialize if slot0.sqrtPriceX96 == 0
SLOT0=$(cast call "$POOL_V3" "slot0()" --rpc-url "$RPC_URL" 2>&1)
SQRT_PRICE=$(echo "$SLOT0" | python3 -c "import sys; raw=sys.stdin.read().strip(); print(raw.split(',')[0].replace('(','').strip())" 2>/dev/null || echo "0x0")
if [ "$SQRT_PRICE" = "0" ] || [ "$SQRT_PRICE" = "0x0" ] || [ "$SQRT_PRICE" = "0x0000000000000000000000000000000000000000000000000000000000000000" ]; then
  # USDC is token0 (address ordering), use REF_SQRT_USDC_TOKEN0
  REF_SQRT=2505414483750479311864138015
  cast send "$POOL_V3" "initialize(uint160)" "$REF_SQRT" \
    --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1
  echo "  Initialized V3 pool with sqrtPriceX96=$REF_SQRT"
fi

# Determine token0 ordering
TOKEN0=$(cast call "$POOL_V3" "token0()" --rpc-url "$RPC_URL")
TOKEN0_LOWER=$(echo "$TOKEN0" | tr '[:upper:]' '[:lower:]')
USDC_LOWER=$(echo "$USDC" | tr '[:upper:]' '[:lower:]')
if [ "$TOKEN0_LOWER" = "$USDC_LOWER" ]; then
  AMOUNT0_MAX=$usdc_v3_mbtc
  AMOUNT1_MAX=$((usdc_v3_mbtc / 1000))
else
  AMOUNT0_MAX=$((usdc_v3_mbtc / 1000))
  AMOUNT1_MAX=$usdc_v3_mbtc
fi

LIQUIDITY=$((usdc_v3_mbtc / 64))
echo "  token0=$TOKEN0, liquidity=$LIQUIDITY, amount0Max=$AMOUNT0_MAX, amount1Max=$AMOUNT1_MAX"

cast send "$USDC" "approve(address,uint256)" "$SEEDER_ADDR" "$usdc_v3_mbtc" \
  --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1
cast send "$MBTC" "approve(address,uint256)" "$SEEDER_ADDR" "$AMOUNT1_MAX" \
  --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1

# seedV3(address pool, int24 lower, int24 upper, uint128 liquidity, uint256 amount0Max, uint256 amount1Max, address to)
cast send "$SEEDER_ADDR" \
  "seedV3(address,int24,int24,uint128,uint256,uint256,address)" \
  "$POOL_V3" -887220 887220 "$LIQUIDITY" "$AMOUNT0_MAX" "$AMOUNT1_MAX" "$OPERATOR" \
  --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1
echo "  V3 USDC/mBTC seeded"
echo ""

# ── Step 8: Allowlist stable pool ──
echo "=== [8/8] Allowlisting stable pool ==="
cast send "$AGGREGATOR" "addStablePool(address)" "$STABLE_POOL" \
  --rpc-url "$RPC_URL" --private-key "$PRIVATE_KEY" > /dev/null 2>&1
echo "  Allowlisted: $STABLE_POOL"
echo ""

# ── Final balances ──
echo "=== Post-seed balances ==="
echo "  USDC: $(cast call "$USDC" "balanceOf(address)" "$OPERATOR" --rpc-url "$RPC_URL")"
echo "  EURC: $(cast call "$EURC" "balanceOf(address)" "$OPERATOR" --rpc-url "$RPC_URL")"
echo "  mBTC: $(cast call "$MBTC" "balanceOf(address)" "$OPERATOR" --rpc-url "$RPC_URL")"
echo ""
echo "=== Seed Complete ==="
echo ""
echo "Set these env vars:"
echo "  CHAKRA_LIQUIDITY_SEEDER=$SEEDER_ADDR"
echo "  CHAKRA_STABLE_POOLS=$STABLE_POOL"
