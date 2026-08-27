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
