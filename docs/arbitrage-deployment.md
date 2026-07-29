# Arbitrage Deployment

Chakra Arbitrage is an operator-run scanner and executor. It is not part of
`Chakra-swap-api` and it should run as its own native process.

For high-throughput mainnet operation, connect it to the
[production Aggregator topology](aggregator-deployment.md). A
`Chakra-swap-api` instance can be used for evaluation or a small private setup.

## Architecture

```text
quote API -> arb-scanner -> simulateTransaction -> optional transaction submit
                         -> vault -> Aggregator -> DEXes
```

The scanner requests outward and return routes, optimizes trade size, builds an
atomic round trip, and checks simulated profit after the estimated fee. Only
the final rollout stage signs and broadcasts transactions.

## Prerequisites

- One or more ready Chakra quote API endpoints.
- A low-latency Arc RPC with sufficient simulation and submission capacity.
- A deployed Chakra Aggregator contract compatible with the quote API.
- Base and bridge token contract IDs selected for the scanner.
- One or more funded caller accounts for fees and transaction submission.
- Optionally, a deployed and funded Chakra Vault with every caller allowlisted.

Without `ARB_VAULT_CONTRACT`, caller accounts must also hold the trading
principal. With a vault, principal remains in the vault and callers normally
hold only enough native XLM for fees.

## Build and Install

Build from a tagged revision of the monorepo:

```bash
git clone https://github.com/Chakra/Arc-dex-agg.git
cd Arc-dex-agg
git checkout <release-tag-or-commit>
cargo build --locked --release -p arbitrage --bin arb-scanner
```

Install the executable and public service files:

```bash
sudo useradd --system --home /var/lib/Chakra --shell /usr/sbin/nologin Chakra
sudo install -d -o Chakra -g Chakra -m 0750 /var/lib/Chakra
sudo install -d -o root -g Chakra -m 0750 /etc/Chakra
sudo install -m 0755 target/release/arb-scanner /usr/local/bin/
sudo install -m 0600 -o Chakra -g Chakra /dev/null \
  /etc/Chakra/arbitrage-callers
sudo install -m 0640 -o root -g Chakra \
  packaging/arbitrage.env.example /etc/Chakra/arbitrage.env
sudo install -m 0644 packaging/Chakra-arbitrage.service \
  /etc/systemd/system/
```

Skip `useradd` when the service account already exists. Put one Arc `S...`
secret per line in `/etc/Chakra/arbitrage-callers`; never commit that file.
Mnemonic-based callers are also supported through `ARB_MNEMONIC_PATH` and
`ARB_CALLER_INDICES`.

Edit `/etc/Chakra/arbitrage.env` and set the quote URLs, RPC, contract IDs, and
bridge tokens. Start with conservative trade limits. The amounts are integer
token units; XLM uses seven decimal places.

## Safe Rollout

Do not move directly from installation to live submission. Use these stages.

### 1. Quote-only scan

```ini
ARB_BUILD_TX=0
ARB_SUBMIT_TX=0
ARB_DRY_RUN=1
```

This validates quote connectivity, route discovery, concurrency, and sizing
without building or simulating transactions.

### 2. Build and simulate

```ini
ARB_BUILD_TX=1
ARB_SUBMIT_TX=0
ARB_DRY_RUN=1
```

This requires valid contracts and caller accounts. Confirm simulation success,
authorization, fees, vault balance, and post-fee profit behavior before
continuing.

### 3. Live submission

```ini
ARB_BUILD_TX=1
ARB_SUBMIT_TX=1
ARB_DRY_RUN=0
```

Enable this only after reviewing sustained simulation logs and funding limits.
Restart the service after each configuration change:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now Chakra-arbitrage
journalctl -u Chakra-arbitrage -f
```

## Profit and Failure Policy

Before submission, the scanner gates an opportunity using the simulated return
minus the estimated transaction fee and `ARB_MIN_PROFIT`. The transaction's
on-chain `min_amount_out` is intentionally only greater than the input amount,
instead of encoding the entire simulated profit target. This reduces avoidable
post-submission failures and fee loss when execution changes slightly, while
still requiring a positive round-trip token return.

This policy does not guarantee fiat profit and does not remove latency,
liquidity, RPC, contract, or fee risk. The operator controls account funding,
vault exposure, token selection, and submission settings.

## Operations

- Keep the quote API and RPC close to the scanner; latency directly affects
  opportunity validity.
- Keep `ARB_MAX_SPLITS=1` initially. More splits increase route complexity and
  Arc resource use.
- Use several callers only after confirming sequence management and fee
  funding for one caller.
- Monitor simulation failure rate, estimated fees, successful transactions,
  realized token deltas, RPC errors, and vault balance independently.
- To stop new submissions immediately, set `ARB_SUBMIT_TX=0` and restart, or
  stop `Chakra-arbitrage.service`.

The deeper contract and fund-flow description remains in
[Round-trip Arbitrage](round-trip-arb.md).
