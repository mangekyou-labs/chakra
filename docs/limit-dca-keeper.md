# Limit/DCA Keeper

`Chakra-limit-keeper` is a permissionless operator for the `order-escrow`
contract. It watches limit and DCA events, asks Chakra for quotes, and submits
fills when the on-chain order conditions are satisfied.

It is currently supported on **testnet only**. Do not use the testnet escrow,
aggregator, or token IDs on mainnet.

## Configuration

The preferred startup mode uses TOML. The signing secret is kept in a separate
file and is never committed with the configuration:

```bash
cp Chakra-limit-keeper.toml keeper.toml
chmod 600 keeper.toml
printf '%s\n' 'S...' > keeper.secret
chmod 600 keeper.secret
```

Set `network = "testnet"`, the deployed contract IDs, `quote_api_url`, and:

```toml
secret_file = "./keeper.secret"
dry_run = true
```

Run a dry-run first:

```bash
./Chakra-limit-keeper --config ./keeper.toml
# Validate only, without connecting to the network:
./Chakra-limit-keeper --config ./keeper.toml --check-config
```

For live testnet fills, set `dry_run = false`. The keeper requires an Arc-funded
signer for transaction fees. `reclaim = true` is not yet a live reclaim path;
expired-order reclaim remains intentionally disabled in this MVP.

The old `KEEPER_*` environment-variable interface remains available for the
existing testnet deployment scripts.

## systemd example

```bash
sudo install -m 0755 Chakra-limit-keeper /usr/local/bin/Chakra-limit-keeper
sudo install -d -m 0750 /etc/Chakra
sudo install -m 0640 keeper.toml /etc/Chakra/limit-keeper.toml
sudo install -m 0600 keeper.secret /etc/Chakra/limit-keeper.secret
sudo install -m 0644 Chakra-limit-keeper.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now Chakra-limit-keeper
```

The service is only a process-manager example. The binary can also run under a
container, supervisor, or another service manager.

## Safety boundary

- Keep the keeper signer separate from user wallets and escrow funds.
- Start with `dry_run = true` after every contract or API change.
- Use a private or capacity-controlled Arc RPC for live operation.
- Monitor submitted fills and verify the escrow balance and order status.
- Run a contract audit before any mainnet deployment.
