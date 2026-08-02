#!/bin/bash
# Deploy Chakra arbitrage bot (vault mode) to production server.
#
# Prerequisites on server:
#   - Arc-rpc + redis + Chakra-worker (snapshot publisher)
#   - /etc/mnemonic_code.txt with caller mnemonic (indices 1–9 funded for fees)
#   - configs/production-arbitrage.toml generated and reviewed
#
# Usage:
#   ./deploy_arb.sh              # build, install unit, restart service
#   ./deploy_arb.sh install      # install binary + unit only (no restart)
#   START=0 ./deploy_arb.sh        # build + install, skip systemctl restart
#
# Private config is not committed. Start from packaging/Chakra-arbitrage.toml
# when configuring a new machine.
set -euo pipefail

MODE="${1:-deploy}"
case "$MODE" in
  deploy | install) ;;
  *)
    echo "Usage: $0 [deploy|install]" >&2
    exit 1
    ;;
esac

SERVER="root@88.198.16.144"
REMOTE_SRC="/opt/Arc-dex-aggregator-src"
REMOTE_APP_DIR="/opt/Arc-dex-aggregator"
REMOTE_ARB_BIN="${REMOTE_APP_DIR}/target/release/Chakra-arbitrage-bot"
START="${START:-1}"
PRODUCTION_CONFIG="$(dirname "$0")/configs/production-arbitrage.toml"

if [[ ! -f "$PRODUCTION_CONFIG" ]]; then
  echo "ERROR: Missing ${PRODUCTION_CONFIG}; create it from packaging/Chakra-arbitrage.toml" >&2
  exit 1
fi

echo "=== Chakra arbitrage bot deploy (mode=${MODE}, start=${START}) ==="

echo "=== Syncing source code ==="
rsync -az --delete \
  --exclude target \
  --exclude .git \
  --exclude node_modules \
  --exclude thirdparty \
  --exclude out \
  --exclude packages/frontend \
  --exclude configs \
  -e "ssh -o StrictHostKeyChecking=no" \
  "$(dirname "$0")/" \
  "${SERVER}:${REMOTE_SRC}/"

echo "=== Building Chakra-arbitrage-bot + quote-sim-probe on server ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "source ~/.cargo/env && cd ${REMOTE_SRC} && cargo build --release -p arbitrage --bin Chakra-arbitrage-bot --bin quote-sim-probe --bin diag_simulate 2>&1 | tail -20"

echo "=== Uploading private Arbitrage TOML ==="
ssh -o StrictHostKeyChecking=no "$SERVER" "mkdir -p ${REMOTE_APP_DIR}/deploy"
scp -o StrictHostKeyChecking=no "$PRODUCTION_CONFIG" "${SERVER}:${REMOTE_APP_DIR}/deploy/arbitrage.toml"
ssh -o StrictHostKeyChecking=no "$SERVER" "chmod 600 ${REMOTE_APP_DIR}/deploy/arbitrage.toml"

echo "=== Installing binary + systemd unit + quote-sim-probe timer ==="
ssh -o StrictHostKeyChecking=no "$SERVER" \
  "MODE='${MODE}' START='${START}' REMOTE_SRC='${REMOTE_SRC}' REMOTE_APP_DIR='${REMOTE_APP_DIR}' REMOTE_ARB_BIN='${REMOTE_ARB_BIN}' bash -s" <<'REMOTE'
set -euo pipefail
mkdir -p "${REMOTE_APP_DIR}/target/release" "${REMOTE_APP_DIR}/deploy" "${REMOTE_APP_DIR}/scripts" "${REMOTE_APP_DIR}/logs"
deploy_arb() {
  "${REMOTE_SRC}/target/release/Chakra-arbitrage-bot" \
    --config "${REMOTE_APP_DIR}/deploy/arbitrage.toml" --check-config
  systemctl stop Chakra-arb >/dev/null 2>&1 || true
  cp "${REMOTE_SRC}/target/release/Chakra-arbitrage-bot" "${REMOTE_ARB_BIN}"
  cp "${REMOTE_SRC}/target/release/quote-sim-probe" "${REMOTE_APP_DIR}/target/release/quote-sim-probe"
  chmod +x "${REMOTE_APP_DIR}/target/release/quote-sim-probe"
  cp "${REMOTE_SRC}/scripts/run-quote-sim-probe-scheduled.sh" "${REMOTE_APP_DIR}/scripts/run-quote-sim-probe-scheduled.sh"
  chmod +x "${REMOTE_APP_DIR}/scripts/run-quote-sim-probe-scheduled.sh"
  cp "${REMOTE_SRC}/deploy/Chakra-arb.service" /etc/systemd/system/Chakra-arb.service
  cp "${REMOTE_SRC}/deploy/Chakra-quote-sim-probe.service" /etc/systemd/system/Chakra-quote-sim-probe.service
  cp "${REMOTE_SRC}/deploy/Chakra-quote-sim-probe.timer" /etc/systemd/system/Chakra-quote-sim-probe.timer
  systemctl daemon-reload
  systemctl enable Chakra-arb
  systemctl enable --now Chakra-quote-sim-probe.timer

  if [[ "${MODE}" == "install" || "${START}" == "0" ]]; then
    echo "Skipping systemctl start (MODE=${MODE} START=${START})"
    exit 0
  fi

  # Stop legacy Arc-arb if still running (best-effort)
  systemctl disable --now Arc-arb >/dev/null 2>&1 || true

  systemctl restart Chakra-arb
}

deploy_arb
REMOTE

if [[ "$MODE" == "deploy" && "$START" == "1" ]]; then
  echo "=== Arb bot status ==="
  ssh -o StrictHostKeyChecking=no "$SERVER" \
    "systemctl status Chakra-arb --no-pager | head -14; echo; journalctl -u Chakra-arb -n 20 --no-pager"
  echo "=== Quote-sim-probe timer ==="
  ssh -o StrictHostKeyChecking=no "$SERVER" \
    "systemctl status Chakra-quote-sim-probe.timer --no-pager | head -14; systemctl list-timers Chakra-quote-sim-probe.timer --no-pager"
fi

echo "=== Done ==="
echo "Vault:  CCQQ3LRFCSGOYSSD6S4MGH6RWWYVDHYPJO6KYDJYC2IDZK4OGCK6P6KN"
echo "Submission mode: edit configs/production-arbitrage.toml, redeploy, then check journalctl -u Chakra-arb"
echo "Probe timer: systemctl status Chakra-quote-sim-probe.timer  (every 30m; logs: journalctl -u Chakra-quote-sim-probe)"
