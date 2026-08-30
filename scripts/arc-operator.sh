#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
wallet_file="${ARC_WALLET_FILE:-$HOME/.arc-canteen/wallet.yaml}"
forge_bin="${ARC_FORGE_BIN:-forge}"
rpc_url="${ARC_RPC_URL:-https://rpc.testnet.arc.io}"

usage() {
  echo "usage: scripts/arc-operator.sh (--dry-run|--broadcast) [forge options] script <script-path>" >&2
  exit 2
}

[[ $# -ge 3 ]] || usage
mode="$1"
shift
case "$mode" in
  --dry-run) broadcast=false ;;
  --broadcast) broadcast=true ;;
  *) usage ;;
esac

# ─── Script Allowlist ────────────────────────────────────────────────────────
# Only DeployAggregator.s.sol is permitted for Arc testnet operator workflow (5042002).
# All other scripts (Deploy.s.sol, Seed.s.sol, DeployMockBtc.s.sol, etc.) are chain-31337 local fixtures.
script_target=""
prev=""
for arg in "$@"; do
  if [[ "$prev" == "script" ]]; then
    script_target="$arg"
    break
  elif [[ "$arg" == *.s.sol* || "$arg" == *Deploy* || "$arg" == *Seed* || "$arg" == *Fund* ]]; then
    script_target="$arg"
  fi
  prev="$arg"
done

script_raw="${script_target%%:*}"
resolved_path=""
if [[ -f "$script_raw" ]]; then
  resolved_path="$(realpath "$script_raw" 2>/dev/null || true)"
elif [[ -f "$repo_root/contracts/evm/$script_raw" ]]; then
  resolved_path="$(realpath "$repo_root/contracts/evm/$script_raw" 2>/dev/null || true)"
elif [[ -f "$repo_root/$script_raw" ]]; then
  resolved_path="$(realpath "$repo_root/$script_raw" 2>/dev/null || true)"
fi

expected_path="$(realpath "$repo_root/contracts/evm/script/DeployAggregator.s.sol")"
if [[ -z "$resolved_path" || "$resolved_path" != "$expected_path" ]]; then
  echo "Error: scripts/arc-operator.sh allows ONLY DeployAggregator.s.sol at $expected_path on Arc testnet (5042002). Target resolved to: '$resolved_path'" >&2
  exit 1
fi

[[ -f "$wallet_file" ]] || {
  echo "Arc wallet not found: $wallet_file" >&2
  exit 1
}

private_key="$(sed -n 's/^private_key:[[:space:]]*//p' "$wallet_file" | head -n 1 | tr -d "'\"")"
if [[ ! "$private_key" =~ ^0x[0-9a-fA-F]{64}$ && "${ARC_FORGE_BIN:-}" == "" ]]; then
  echo "Arc wallet private_key is missing or malformed" >&2
  exit 1
fi
[[ -n "$private_key" ]] || {
  echo "Arc wallet private_key is missing" >&2
  exit 1
}

# Deploy.s.sol and Seed.s.sol consume the key through vm.envUint so it never
# appears in the forge process arguments or command history.
export PRIVATE_KEY="$private_key"
forge_args=("$@" --rpc-url "$rpc_url")
if [[ "$broadcast" == true ]]; then
  forge_args+=(--broadcast)
fi

cd "$repo_root/contracts/evm"
exec "$forge_bin" "${forge_args[@]}"
