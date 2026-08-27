#!/usr/bin/env bash
set -euo pipefail

# ─── Signal handling ───────────────────────────────────────────────────────────
# When tini forwards SIGTERM/SIGINT, kill both children and exit.
cleanup() {
    echo "[entrypoint] shutting down..."
    kill "$WORKER_PID" "$API_PID" 2>/dev/null || true
    wait "$WORKER_PID" "$API_PID" 2>/dev/null || true
    exit 1
}
trap cleanup SIGTERM SIGINT

# ─── Defaults ──────────────────────────────────────────────────────────────────
export CHAKRA_LISTEN_ADDR="${CHAKRA_LISTEN_ADDR:-0.0.0.0:${PORT:-8080}}"
export CHAKRA_RPC_HTTP="${CHAKRA_RPC_HTTP:-https://rpc.testnet.arc.io}"
export CHAKRA_RPC_WS="${CHAKRA_RPC_WS:-wss://rpc.testnet.arc.io}"

echo "[entrypoint] starting market-data-worker..."
chakra-market-data-worker &
WORKER_PID=$!

echo "[entrypoint] starting chakra-api-server on ${CHAKRA_LISTEN_ADDR}..."
chakra-api-server &
API_PID=$!

# ─── Wait for either to exit (fail-fast) ──────────────────────────────────────
wait -n "$WORKER_PID" "$API_PID"
EXIT_CODE=$?

echo "[entrypoint] process exited with code ${EXIT_CODE}"
cleanup
exit "$EXIT_CODE"
