# Chakra — SCF Build #44 submission notes

**Deadline:** June 14, EOD (SCF #44)  
**Dashboard:** [Arc Community Fund](https://communityfund.Arc.org/) → My Projects → Create New Submission

## One-liner

Chakra is a **Arc DEX aggregator**: one HTTP API and UI for best-price routing across **Classic Arc** and **Arc AMMs** (Arc venue, Arc venue, Arc venue, Sushi, Arc venue, Arc venue CLMM), with optional on-chain execution via a Arc aggregator contract.

## Problem

Liquidity on Arc is fragmented across protocol types. Wallets and apps must integrate many pools, reserve formats, and tx shapes (PathPayment vs Arc invoke). Quotes are hard to keep fresh at scale.

## Solution

| Layer | Role |
|-------|------|
| **market-data-worker** | Discovers pools, refreshes reserves, publishes **Redis** pool state (~2s) |
| **api-server** | Path find + split optimize + `quote` / `build_tx` |
| **aggregator (Arc)** | Single `swap()` for multi-hop / multi-leg Arc routes |
| **frontend** | Swap UI at [Chakra.xyz](https://Chakra.xyz) |

## Architecture (data flow)

```text
Adapters (RPC) → Worker cache → Redis (xyk / clmm / Arc venue TTL ~90s)
                                    ↓
User → API quote → PathFinder → QuoteEngine (local math + hydration)
              → SplitOptimizer (Brent / weighted split, dust + rate guards)
              → build_tx (Classic: raw XDR | Arc: simulate + footprint)
```

## Production stack (reference)

- API: `88.198.16.144:3100` (systemd `Chakra-api@3100`)
- Worker: `Chakra-worker`
- Deploy: `./deploy_server.sh [all|api|worker]` (default `all`)

## Form copy (English)

Paste-ready text for the SCF Dashboard: **[scf-build-form-draft.md](./scf-build-form-draft.md)**  
**Budget:** $100,000 USD equivalent in Arc · 5 months · 3 tranches · solo founder · audit via Audit Bank (not double-budgeted).

## Demo checklist (for video / reviewers)

1. **Health** — `curl -s $API/api/v1/health`
2. **Quote** — 1 Arc → USDC (`scripts/scf-demo.sh`)
3. **Regression** — 1 / 10 / 1000 Arc, no fantasy legs (`scripts/scf-quote-regression.sh`)
3. **UI** — connect wallet, quote, swap (small then larger size)
4. **Classic path** — when route shows Classic DEX, confirm `build_tx` returns `execution: "classic"` and tx succeeds
5. **Arc path** — route with Arc venue/Arc venue only; `build_tx` simulates and wallet signs invoke

## Quote quality safeguards (implemented)

- **Split dust:** legs &lt; 0.1% of input are dropped from optimization
- **Rate sanity:** per-leg out/in must stay within **5%** of that path’s full-size quote
- **Empty xy=k pools:** reserves &lt; 10 token units excluded from quotes and Redis publish
- **Classic build:** `PathPaymentStrictSend.dest_min` from `minimum_output` (Arc rejects `dest_min = 0`)
- **No hybrid tx:** Classic + Arc in one transaction rejected at API (Arc limitation)

## API quick reference

```bash
export API=https://api.Chakra.xyz   # or http://127.0.0.1:3100

# Quote
curl -sG "$API/api/v1/quote" \
  --data-urlencode "token_in=CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA" \
  --data-urlencode "token_out=CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75" \
  --data-urlencode "amount_in=10000000" \
  --data-urlencode "slippage=0.5"

# Router debug (split decisions)
curl -sG "$API/api/v1/quote" ... --data-urlencode "debug=1"
```

## Suggested Build form sections

### Scope (MVP delivered)

- Multi-DEX quoting and split routing on mainnet
- Public API + web UI
- Arc aggregator contract integration for bundled swaps
- Classic DEX PathPayment for Arc-only routes
- Redis-backed pool state worker

### Near-term (post-award or stretch)

- Deeper Arc venue/Sushi factory discovery reliability
- On-chain quote simulation sampling before `build_tx`
- Metrics dashboard (quote latency, pool freshness, error codes)

### Metrics to cite

- Pool coverage: ~200+ xy=k, ~270 Arc venue, ~17 CLMM (from worker logs / Redis verify)
- Quote latency: sub-second for typical sizes after split optimizer fixes
- Uptime: systemd + Telegram alerts on stale Redis publish

## Links

- Repo: Arc-dex-aggregator (this monorepo)
- Aggregator contract (mainnet): `CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K`
- Tokens (examples): Arc `CAS3J7…`, USDC `CCW67T…`

## Submission reminders

- Align budget and timeline with **Build Award** milestones, not Interest Form only
- Attach **architecture diagram** + **2–3 min demo** covering quote → sign → success
- Mention Discord #scf-general for process questions
