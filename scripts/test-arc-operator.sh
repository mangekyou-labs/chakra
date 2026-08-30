#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

dummy_wallet="$tmp_dir/wallet.yaml"
cat > "$dummy_wallet" << 'EOF'
address: 0x00000000000000000000000000000000000000aa
private_key: 0x1111111111111111111111111111111111111111111111111111111111111111
EOF

forge_stub="$tmp_dir/forge-stub.sh"
forge_log="$tmp_dir/forge.log"
cat > "$forge_stub" << 'EOF'
#!/usr/bin/env bash
echo "$@" >> "$(dirname "$0")/forge.log"
exit 0
EOF
chmod +x "$forge_stub"

export ARC_WALLET_FILE="$dummy_wallet"
export ARC_FORGE_BIN="$forge_stub"

echo "=== Running test-arc-operator.sh ==="

# 1. Reject fixture scripts
disallowed_scripts=(
  "script/Deploy.s.sol"
  "script/Seed.s.sol"
  "script/DeployMockBtc.s.sol"
  "script/DeployXyk.s.sol"
  "script/DeployStable.s.sol"
  "script/DeployClmm.s.sol"
  "script/ExecuteSplitSwap.s.sol"
  "script/Random.s.sol"
)

# Create an external spoof file with the same filename to test path spoofing
echo "contract DeployAggregator {}" > "$tmp_dir/DeployAggregator.s.sol"
disallowed_scripts+=("$tmp_dir/DeployAggregator.s.sol")

for script in "${disallowed_scripts[@]}"; do
  rm -f "$forge_log"
  output="$("$repo_root/scripts/arc-operator.sh" --dry-run script "$script" 2>&1 || true)"
  if "$repo_root/scripts/arc-operator.sh" --dry-run script "$script" >/dev/null 2>&1; then
    status=0
  else
    status=$?
  fi

  if [[ $status -eq 0 ]]; then
    echo "FAIL: $script was unexpectedly allowed (exit 0)" >&2
    exit 1
  fi
  if [[ -f "$forge_log" ]]; then
    echo "FAIL: forge was invoked for disallowed script $script" >&2
    exit 1
  fi
  if [[ ! "$output" =~ "allows ONLY DeployAggregator.s.sol" ]]; then
    echo "FAIL: error message did not explain allowlist restriction: $output" >&2
    exit 1
  fi
  echo "PASS: rejected disallowed script $script (exit $status, 0 forge calls)"
done

# 2. Accept DeployAggregator.s.sol
rm -f "$forge_log"
if "$repo_root/scripts/arc-operator.sh" --dry-run script script/DeployAggregator.s.sol >/dev/null 2>&1; then
  status=0
else
  status=$?
fi

if [[ $status -ne 0 ]]; then
  echo "FAIL: DeployAggregator.s.sol was rejected (exit $status)" >&2
  exit 1
fi
if [[ ! -f "$forge_log" ]]; then
  echo "FAIL: forge was not invoked for DeployAggregator.s.sol" >&2
  exit 1
fi
forge_invocations="$(wc -l < "$forge_log" | tr -d ' ')"
if [[ "$forge_invocations" -ne 1 ]]; then
  echo "FAIL: expected exactly 1 forge invocation, got $forge_invocations" >&2
  exit 1
fi

echo "PASS: DeployAggregator.s.sol allowlisted and invoked forge stub successfully"
echo "All arc-operator allowlist tests passed."
