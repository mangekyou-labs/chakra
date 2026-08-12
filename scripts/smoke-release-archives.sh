#!/usr/bin/env bash
# Smoke test Chakra release tarballs after packaging.
#
# Usage:
#   ./scripts/smoke-release-archives.sh
#   DIST_DIR=./dist ./scripts/smoke-release-archives.sh
#
# The check is intentionally offline. It always verifies archive contents. On
# Linux x86_64 it also verifies binary startup and TOML parsing without requiring
# RPC, Redis, or chain access.
set -euo pipefail

DIST_DIR="${DIST_DIR:-.}"
SWAP_ARCHIVE="${SWAP_ARCHIVE:-Chakra-swap-api-linux-x86_64.tar.gz}"
AGG_ARCHIVE="${AGG_ARCHIVE:-Chakra-aggregator-linux-x86_64.tar.gz}"
ARB_ARCHIVE="${ARB_ARCHIVE:-Chakra-arbitrage-bot-linux-x86_64.tar.gz}"
KEEPER_ARCHIVE="${KEEPER_ARCHIVE:-Chakra-limit-keeper-linux-x86_64.tar.gz}"

smoke_dir="$(mktemp -d)"
trap 'rm -rf "$smoke_dir"' EXIT

tar -xzf "$DIST_DIR/$SWAP_ARCHIVE" -C "$smoke_dir"
tar -xzf "$DIST_DIR/$AGG_ARCHIVE" -C "$smoke_dir"
tar -xzf "$DIST_DIR/$ARB_ARCHIVE" -C "$smoke_dir"
tar -xzf "$DIST_DIR/$KEEPER_ARCHIVE" -C "$smoke_dir"

swap="$smoke_dir/${SWAP_ARCHIVE%.tar.gz}"
cluster="$smoke_dir/${AGG_ARCHIVE%.tar.gz}"
arb="$smoke_dir/${ARB_ARCHIVE%.tar.gz}"
keeper="$smoke_dir/${KEEPER_ARCHIVE%.tar.gz}"

test -x "$swap/Chakra-swap-api"
test -f "$swap/README.md"
test -f "$swap/openapi.yaml"
test -f "$swap/Chakra-swap-api.toml"
test -f "$swap/Chakra-swap-api.service"

test -x "$cluster/Chakra-api-server"
test -x "$cluster/Chakra-market-data-worker"
test -x "$cluster/Chakra-analytics-indexer"
test -f "$cluster/README.md"
test -f "$cluster/Chakra-aggregator.toml"
test -f "$cluster/aggregator-configuration.md"
test -f "$cluster/analytics-indexer.md"
test -f "$cluster/openapi.yaml"
test -f "$cluster/systemd/Chakra-api@.service"
test -f "$cluster/systemd/Chakra-market-data-worker.service"
test -f "$cluster/systemd/Chakra-analytics-indexer.service"
for unit in \
  "$cluster/systemd/Chakra-api@.service" \
  "$cluster/systemd/Chakra-market-data-worker.service" \
  "$cluster/systemd/Chakra-analytics-indexer.service"; do
  grep -F -- '--config /etc/Chakra/Chakra-aggregator.toml' "$unit" >/dev/null
done

test -x "$arb/Chakra-arbitrage-bot"
test -f "$arb/README.md"
test -f "$arb/arbitrage-configuration.md"
test -f "$arb/Chakra-arbitrage.toml"
test -f "$arb/Chakra-arbitrage.service"

test -x "$keeper/Chakra-limit-keeper"
test -f "$keeper/README.md"
test -f "$keeper/Chakra-limit-keeper.toml"
test -f "$keeper/Chakra-limit-keeper.service"
grep -F -- '--config /etc/Chakra/limit-keeper.toml' "$keeper/Chakra-limit-keeper.service" >/dev/null

host_os="$(uname -s)"
host_arch="$(uname -m)"
if [[ "${EXECUTE_BINARIES:-auto}" == "0" || "${EXECUTE_BINARIES:-auto}" == "false" ]]; then
  echo "archive structure smoke test passed; binary execution skipped"
  exit 0
fi
if [[ "${EXECUTE_BINARIES:-auto}" == "auto" && ! ( "$host_os" == "Linux" && "$host_arch" == "x86_64" ) ]]; then
  echo "archive structure smoke test passed; binary execution skipped on $host_os/$host_arch"
  exit 0
fi

"$swap/Chakra-swap-api" --version
"$cluster/Chakra-api-server" --version
"$cluster/Chakra-market-data-worker" --version
"$cluster/Chakra-analytics-indexer" --version
"$arb/Chakra-arbitrage-bot" --version
"$keeper/Chakra-limit-keeper" --help
cp "$keeper/Chakra-limit-keeper.toml" "$smoke_dir/keeper.toml"
"$keeper/Chakra-limit-keeper" --config "$smoke_dir/keeper.toml" --check-config

cp "$swap/Chakra-swap-api.toml" "$smoke_dir/swap-api.toml"
"$swap/Chakra-swap-api" --config "$smoke_dir/swap-api.toml" --check-config

cp "$cluster/Chakra-aggregator.toml" "$smoke_dir/aggregator.toml"
perl -pi -e 's/aggregator_contract = "CHANGE_ME"/aggregator_contract = "CDUMMYAGGREGATORCONTRACT000000000000000000000000000000000000"/' "$smoke_dir/aggregator.toml"
"$cluster/Chakra-market-data-worker" --config "$smoke_dir/aggregator.toml" --check-config
"$cluster/Chakra-api-server" --config "$smoke_dir/aggregator.toml" --check-config
"$cluster/Chakra-analytics-indexer" --config "$smoke_dir/aggregator.toml" --check-config

cp "$arb/Chakra-arbitrage.toml" "$smoke_dir/arbitrage.toml"
perl -pi -e 's/aggregator = "CHANGE_ME"/aggregator = "CDUMMYAGGREGATORCONTRACT000000000000000000000000000000000000"/' "$smoke_dir/arbitrage.toml"
perl -pi -e 's/bridge_tokens = \["CHANGE_ME"\]/bridge_tokens = ["CDUMMYBRIDGETOKEN000000000000000000000000000000000000000"]/' "$smoke_dir/arbitrage.toml"
"$arb/Chakra-arbitrage-bot" --config "$smoke_dir/arbitrage.toml" --check-config

echo "release archive smoke test passed"
