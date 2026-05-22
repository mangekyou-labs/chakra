# Shared Arc-core (config only)

## Layout (default)

```
Arc-rpc (:8003)
  └── sole Arc-core (:11626 / :11628)
Arc-horizon     → disabled (Classic DEX uses https://horizon.Arc.org)
Chakra worker/api   → RPC_URL=http://127.0.0.1:8003 when getHealth is healthy
```

- **Do not** run `Arc-core.service` or `Arc-horizon.service` alongside this profile.
- Captive config: `/etc/Arc/Arc-captive-mainnet.cfg`
- RPC config: `/etc/Arc/Arc-rpc.toml`

## Apply on server

```bash
sudo ./deploy/setup_shared_Arc_core.sh
```

## Disable Horizon only

```bash
sudo systemctl stop Arc-horizon
sudo systemctl disable Arc-horizon
```

Postgres data under `horizon` DB is left on disk; remove separately if you need disk back.

## Reset stuck RPC ingest

```bash
sudo ./deploy/reset_Arc_rpc_ingest.sh
```

## Files

| File | Role |
|------|------|
| `Arc-rpc.toml` | RPC TOML (owned core, ports 11626/11628) |
| `Arc-captive-mainnet.cfg` | Captive core template |
| `reset_Arc_rpc_ingest.sh` | Wipe sqlite + rpc-captive, restart RPC |
