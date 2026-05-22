#!/bin/bash
# Config-only shared Arc node: RPC owns the only Arc-core (:11626 / :11628).
# Arc-horizon is disabled (Chakra Classic uses public Horizon).
#
# Usage (on server as root): ./deploy/configure_shared_core.sh
#
# Optional: SHARED_CORE_MODE=external  (experimental; often fails on Arc-rpc v26)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MODE="${SHARED_CORE_MODE:-owned}"
CORE_HTTP_PORT=11626

echo "=== Shared core setup (mode=${MODE}) ==="
echo "=== Stopping dependents ==="
systemctl stop Chakra-worker.service 2>/dev/null || true
for port in 3100 3101 3102 3103; do
  systemctl stop "Chakra-api@${port}" 2>/dev/null || true
done
systemctl stop Arc-rpc.service Arc-horizon.service 2>/dev/null || true
sleep 2

pkill -f 'Arc-core.*rpc-captive' 2>/dev/null || true
pkill -f 'Arc-core.*catchup' 2>/dev/null || true
sleep 2

mkdir -p /etc/systemd/system/Arc-rpc.service.d
cp "${SCRIPT_DIR}/Arc-rpc.service.d-config-path.conf" \
  /etc/systemd/system/Arc-rpc.service.d/config-path.conf
mkdir -p /Arc/rpc-captive
chown -R Arc:Arc /Arc/rpc-captive

if [[ "$MODE" == "external" ]]; then
  cp "${SCRIPT_DIR}/Arc-captive-core-remote.cfg" /etc/Arc/Arc-captive-core-remote.cfg
  chown Arc:Arc /etc/Arc/Arc-captive-core-remote.cfg
  cp "${SCRIPT_DIR}/Arc-rpc.external.toml" /etc/Arc/Arc-rpc.toml
  if [[ -f "${SCRIPT_DIR}/Arc-shared-core.cfg" ]]; then
    cp "${SCRIPT_DIR}/Arc-shared-core.cfg" /etc/Arc/Arc-core.cfg
    chown Arc:Arc /etc/Arc/Arc-core.cfg
  fi
  systemctl daemon-reload
  systemctl enable Arc-core.service 2>/dev/null || true
  systemctl start Arc-core.service
else
  cp "${SCRIPT_DIR}/Arc-captive-mainnet.cfg" /etc/Arc/Arc-captive-mainnet.cfg
  chown Arc:Arc /etc/Arc/Arc-captive-mainnet.cfg
  cp "${SCRIPT_DIR}/Arc-rpc.toml" /etc/Arc/Arc-rpc.toml
  chown Arc:Arc /etc/Arc/Arc-rpc.toml
  systemctl stop Arc-core.service 2>/dev/null || true
  systemctl disable Arc-core.service 2>/dev/null || true
  pkill -x Arc-core 2>/dev/null || true
  sleep 2
fi

echo "=== Disable Arc-horizon (not used; avoids extra captive core) ==="
systemctl stop Arc-horizon.service 2>/dev/null || true
systemctl disable Arc-horizon.service 2>/dev/null || true

systemctl daemon-reload

if [[ "$MODE" == "external" ]]; then
  for i in $(seq 1 24); do
    if curl -sf "http://127.0.0.1:${CORE_HTTP_PORT}/info" >/dev/null 2>&1; then
      echo "Arc-core.service /info OK"
      break
    fi
    sleep 5
    [[ "$i" -eq 24 ]] && echo "WARN: Arc-core /info not ready" >&2
  done
fi

systemctl reset-failed Arc-rpc.service 2>/dev/null || true
systemctl start Arc-rpc.service
sleep 20

RPC_HEALTH=$(curl -s --max-time 10 -X POST http://127.0.0.1:8003/ -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' || true)
if echo "$RPC_HEALTH" | grep -q '"status":"healthy"'; then
  echo "Arc-rpc healthy — Chakra → http://127.0.0.1:8003"
  LUM_RPC='http://127.0.0.1:8003'
else
  echo "WARN: Arc-rpc not healthy — Chakra stays on public RPC"
  LUM_RPC='https://Arc-rpc.mainnet.Arc.gateway.fm'
fi
mkdir -p /etc/systemd/system/Chakra-worker.service.d /etc/systemd/system/Chakra-api@.service.d
cat >/etc/systemd/system/Chakra-worker.service.d/rpc-override.conf <<EOF
[Service]
Environment=RPC_URL=${LUM_RPC}
EOF
cat >/etc/systemd/system/Chakra-api@.service.d/rpc-override.conf <<EOF
[Service]
Environment=RPC_URL=${LUM_RPC}
EOF
systemctl daemon-reload
systemctl start Chakra-worker.service
for port in 3100 3101 3102 3103; do systemctl start "Chakra-api@${port}" 2>/dev/null || true; done

echo ""
echo "=== Status ==="
echo "Arc-core processes: $(pgrep -c -x Arc-core 2>/dev/null || echo 0)"
pgrep -a -x Arc-core 2>/dev/null || true
systemctl is-active Arc-core.service 2>/dev/null || echo "Arc-core.service: disabled (owned)"
systemctl is-active Arc-rpc.service Chakra-worker.service || true
echo "Arc-horizon: $(systemctl is-enabled Arc-horizon.service 2>/dev/null || echo disabled)"
systemctl show Arc-rpc -p NRestarts,ActiveState --value
curl -sf "http://127.0.0.1:${CORE_HTTP_PORT}/info" 2>/dev/null | head -c 160 || echo "(core /info not ready — catchup in progress)"
echo
echo "getHealth: $RPC_HEALTH"
echo ""
echo "Architecture: Arc-rpc (:8003) → sole Arc-core (:${CORE_HTTP_PORT}); horizon disabled"
