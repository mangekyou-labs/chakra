#!/bin/bash
# Deploy Arc DEX Aggregator to server
# Usage: ./deploy.sh

set -e

SERVER="root@88.198.16.144"
SSH_KEY="~/.ssh/id_rsa"
REMOTE_DIR="/opt/Arc-dex-aggregator"

echo "=== Building release binary ==="
cargo build -p api-server --release

echo "=== Uploading binary ==="
ssh -i $SSH_KEY $SERVER "mkdir -p $REMOTE_DIR/data"
scp -i $SSH_KEY target/release/api-server $SERVER:$REMOTE_DIR/api-server

echo "=== Uploading pool cache ==="
if [ -f data/pool_cache.json ]; then
    scp -i $SSH_KEY data/pool_cache.json $SERVER:$REMOTE_DIR/data/pool_cache.json
fi

echo "=== Creating systemd service ==="
ssh -i $SSH_KEY $SERVER "cat > /etc/systemd/system/Chakra-api.service << 'EOF'
[Unit]
Description=Chakra DEX Aggregator API
After=network.target Arc-rpc.service

[Service]
Type=simple
User=root
WorkingDirectory=/opt/Arc-dex-aggregator
Environment=RPC_URL=http://127.0.0.1:8003
Environment=HORIZON_URL=http://144.76.79.46:8000
Environment=LISTEN_ADDR=0.0.0.0:3100
Environment=REFRESH_INTERVAL_SECS=5
Environment=RUST_LOG=info
ExecStart=/opt/Arc-dex-aggregator/api-server
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF"

echo "=== Starting service ==="
ssh -i $SSH_KEY $SERVER "systemctl daemon-reload && systemctl enable Chakra-api && systemctl restart Chakra-api && sleep 2 && systemctl status Chakra-api | head -8"

echo "=== Testing ==="
sleep 3
curl -s http://88.198.16.144:3100/api/v1/health && echo " ✓"

echo "=== Done ==="
