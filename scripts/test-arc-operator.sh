#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

wallet_file="$tmp_dir/wallet.yaml"
forge_stub="$tmp_dir/forge"
capture_file="$tmp_dir/capture"

printf '%s\n' "address: '0x0000000000000000000000000000000000000001'" \
  "private_key: '0x1234'" >"$wallet_file"
printf '%s\n' '#!/usr/bin/env bash' \
  'printf "PRIVATE_KEY=%s\n" "${PRIVATE_KEY:-}" >"$ARC_CAPTURE_FILE"' \
  'printf "ARG=%s\n" "$@" >>"$ARC_CAPTURE_FILE"' >"$forge_stub"
chmod +x "$forge_stub"

ARC_WALLET_FILE="$wallet_file" ARC_FORGE_BIN="$forge_stub" \
  ARC_CAPTURE_FILE="$capture_file" \
  "$repo_root/scripts/arc-operator.sh" --dry-run script script/Deploy.s.sol

grep -q '^PRIVATE_KEY=0x1234$' "$capture_file"
grep -q '^ARG=script$' "$capture_file"
grep -q '^ARG=script/Deploy.s.sol$' "$capture_file"
grep -q '^ARG=--rpc-url$' "$capture_file"
if grep -q '^ARG=--private-key$' "$capture_file"; then
  echo 'private key leaked into forge arguments' >&2
  exit 1
fi

echo 'arc-operator wrapper test passed'
