# Production Aggregator Deployment

The scalable Chakra topology runs two native binaries as separate processes:

```text
Chakra-market-data-worker -> Redis -> Chakra-api-server x N
```

Use it for a public API, high request volume, shared market state, or an
arbitrage operator. For a small private service without Redis, use
[Chakra Swap API](Chakra-swap-api.md).

## Deployment rules

- Run exactly one `Chakra-market-data-worker` for each Redis namespace. The
  worker is the single writer and does not implement leader election.
- Run one or more stateless `Chakra-api-server` processes against that Redis.
- Keep Redis private and do not expose port 6379 to the internet.
- Use a low-latency, capacity-controlled Arc RPC. Public rate-limited RPCs
  are suitable for evaluation, not a reliable production data plane.
- Run both binaries as an unprivileged user. Put TLS and public traffic handling
  in a reverse proxy or load balancer.

## Download

Linux x86_64 binaries are published in the monorepo releases:

<https://github.com/Chakra/Arc-dex-agg/releases>

Download `Chakra-aggregator-linux-x86_64.tar.gz` and `SHA256SUMS`, then verify
and extract them:

```bash
grep 'Chakra-aggregator-linux-x86_64.tar.gz$' SHA256SUMS | sha256sum --check
tar -xzf Chakra-aggregator-linux-x86_64.tar.gz
cd Chakra-aggregator-linux-x86_64
```

The archive contains both binaries, a complete `Chakra-aggregator.toml`, the
configuration reference, and optional systemd units.

To build the same binaries from source instead:

```bash
git clone https://github.com/Chakra/Arc-dex-agg.git
cd Arc-dex-agg
git checkout <release-tag-or-commit>
cargo build --locked --release -p market-data-worker -p api-server
```

The Cargo package names remain `market-data-worker` and `api-server`; their
release binaries are `target/release/Chakra-market-data-worker` and
`target/release/Chakra-api-server`.

## Configure Redis

Install Redis on the same private network as the services. Bind it to localhost
or a private interface, enable authentication, and choose persistence and
memory policies appropriate for your operation.

Verify connectivity using the URL that will be in the Chakra configuration:

```bash
redis-cli -u 'redis://:YOUR_PASSWORD@127.0.0.1:6379/' PING
```

The password must be URL-encoded when it contains reserved URL characters.

## Configure Chakra

Create a private runtime configuration from the complete example:

```bash
cp Chakra-aggregator.toml aggregator.toml
chmod 600 aggregator.toml
```

Replace at least:

- `network.rpc_url` with the production Arc RPC.
- `redis.url` with the private Redis URL.
- `api.aggregator_contract` with the deployed Chakra Aggregator contract. Omit it
  only when the API should quote but never build transactions.

Both processes read the same file, so the network and Redis settings cannot
silently diverge. Validate the file before starting either process:

```bash
./Chakra-market-data-worker --config ./aggregator.toml --check-config
./Chakra-api-server --config ./aggregator.toml --check-config
```

See [Configuration Reference](aggregator-configuration.md)
for every supported parameter, its default, and the process that uses it.

## Run without a service manager

The binaries read the TOML file directly, so they are independent of systemd,
Docker, Kubernetes, or any other process manager:

```bash
./Chakra-market-data-worker --config ./aggregator.toml
```

After the worker publishes its first snapshot, start the API in another shell:

```bash
./Chakra-api-server --config ./aggregator.toml
```

Additional API replicas use the same config and a different `LISTEN_ADDR`:

```bash
./Chakra-api-server --config ./aggregator.toml --listen-addr 127.0.0.1:3101
```

In production, translate the same environment file into your chosen service
manager and configure restart, resource, logging, and secret policies there.

## Verify

Liveness confirms the process is running. Readiness confirms that a routing
graph has loaded:

```bash
curl -fsS http://127.0.0.1:3100/api/v1/health
curl -fsS http://127.0.0.1:3100/api/v1/ready
curl -fsS http://127.0.0.1:3101/api/v1/ready
curl -fsS http://127.0.0.1:3100/api/v1/tokens | jq
```

Use the [Integrator Guide](integrator-guide.md) to test `/quote` and
`/build_tx`. A load balancer should route traffic only to instances whose
`/api/v1/ready` endpoint succeeds.

## Optional systemd example

The release archive includes units under `systemd/`. They are examples, not a
required deployment model. Install the binaries and config at the paths used by
those units:

```bash
sudo useradd --system --home /var/lib/Chakra --shell /usr/sbin/nologin Chakra
sudo install -d -o root -g Chakra -m 0750 /etc/Chakra
sudo install -m 0755 Chakra-market-data-worker Chakra-api-server /usr/local/bin/
sudo install -m 0640 -o root -g Chakra aggregator.toml /etc/Chakra/aggregator.toml
sudo install -m 0644 systemd/*.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now Chakra-market-data-worker
sudo systemctl enable --now Chakra-api@3100 Chakra-api@3101
```

Inspect startup with `journalctl -u Chakra-market-data-worker -f` and
`journalctl -u 'Chakra-api@*' -f`.

## Scale and upgrade

API capacity scales horizontally by adding `Chakra-api-server` processes or
hosts connected to the same private Redis. Do not add a second worker to the
same namespace unless an external active/passive mechanism guarantees only one
is running.

For upgrades, retain the previous binaries for rollback, replace both binaries,
then restart the worker followed by API replicas. Confirm worker publication
and every API readiness endpoint before returning traffic to all replicas.
