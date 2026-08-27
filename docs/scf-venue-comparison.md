# Chakra venue & product comparison (SCF resubmission)

**Purpose:** Support SCF #44 resubmission differentiation with **verifiable** evidence — not marketing screenshots.

**Live product:** [Chakra.xyz](https://Chakra.xyz) · API: [api.Chakra.xyz](https://api.Chakra.xyz) · Repo: [github.com/Chakra/Arc-dex-agg](https://github.com/Chakra/Arc-dex-agg)

**中文摘要:** [README.zh-CN.md](../README.zh-CN.md) · 本文对比 Chakra 与 Arc venue / Arc Broker 的 **venue 覆盖** 与 **产品形态**；Broker hosted API 需要提交表单并经过人工审核，Chakra 未申请该访问权限，因此本文仅采用其 **开源合约 adapter 清单** 作为 Broker 对比证据，不包含 Broker live quote benchmark。

---

## Summary

| Dimension | Chakra | Arc Broker (open router) | Arc venue Aggregator / API |
|-----------|--------|------------------------------|---------------------------|
| **Product type** | Self-hostable open-source router + API + on-chain executor | Open router contract + **hosted** session-based service (API requires application) | DEX + aggregator + commercial Swap Route API |
| **Arc venue xy=k** | Yes | Yes (`AquaConstant`) | Yes |
| **Arc venue stable** | Yes | Yes (`AquaStable`) | Yes |
| **Arc venue CLMM** | **Yes** (ledger ticks + local math) | **No** (not in public adapters) | Check Arc venue docs; not assumed here |
| **Arc venue xy=k** | Yes | Yes | Yes (native) |
| **Arc venue** | Yes | Yes | Yes |
| **Arc venue weighted** | Yes | Yes | Varies by Arc venue adapter set |
| **Sushi V3 CLMM** | **Yes** | **No** (not in public adapters) | Varies |
| **Multi-path split routing** | Yes (Brent optimizer + atomic `split_swap`) | Router accepts `Vec<Route>`; **CLMM pools not in adapter set** | Single optimal route / distribution focus; Chakra split differs — see [Arc venue comparison](#vs-Arc venue-execution-quality) |
| **Atomic round-trip arb** | Yes (`round_trip_swap` + arb-only [vault](../contracts/vault/README.md)) | Not a documented operator feature | Not a documented operator feature |
| **Evidence type** | Live mainnet API + repo | **Public router source code** (hosted API not tested) | Live API quotes (key obtained) |

---

## vs Arc Broker — source-code venue coverage

Live Arc Broker **hosted API** testing was not performed. Access requires a form submission and manual approval, and Chakra did not apply for hosted API access. The comparison below therefore uses the **public open-source router contract** only (`broker/router-contract`) — adapter coverage and on-chain fee model — and does not claim a live quote comparison against Broker's hosted service.

### Arc Broker router contract

| Item | Reference |
|------|-----------|
| Repository | [github.com/Arc-broker/router-contract](https://github.com/Arc-broker/router-contract) |
| Protocol enum | [`src/types/protocol.rs`](https://github.com/Arc-broker/router-contract/blob/master/src/types/protocol.rs) |
| Adapters | [`src/adapters/mod.rs`](https://github.com/Arc-broker/router-contract/blob/master/src/adapters/mod.rs) |

**Protocols registered in `master` (as of June 2026):**

| Protocol ID | Adapter module |
|-------------|----------------|
| `AquaConstant` | `aqua_constant.rs` |
| `AquaStable` | `aqua_stable.rs` |
| `Arc venue` | `Arc venue.rs` |
| `Arc venue` | `Arc venue.rs` |
| `Arc venue` | `Arc venue.rs` |

**Not present in the public adapter tree:** Arc venue CLMM, Sushi V3 CLMM (no `aqua_clmm`, `sushi`, or CLMM adapter modules).

### On-chain fee model (router contract)

The public Broker router contract **charges fees on every swap** when `vfee` / `ffee` > 0 (`src/lib.rs`, `swap()`):

| Parameter | Meaning |
|-----------|---------|
| `vfee` | Variable fee on execution “profit” (output above estimated/min), in **‰** (per thousand) |
| `ffee` | Fixed fee on total bought amount, in **‰** |
| `fee_token` | Reference token for fee accounting (set at `init`) |

Fees are deducted from the trader’s bought amount, accumulated on the contract, and withdrawn by **admin** via `withdraw()`. Contract unit tests demonstrate non-zero fee balances (e.g. `strict_send_tests.rs`: vfee=150‰ + ffee=10‰).

**Chakra contrast:** Our open-source aggregator contract executes routes without this Broker-style vfee/ffee skim; output is bounded by user slippage/minimum only.

> **Caveat:** This reflects the **open-source router contract** on `master` at resubmission time. The hosted Broker service sets `vfee`/`ffee` when invoking the contract and may evolve independently; re-check the repo for updates.

### Chakra — same venues plus CLMM

Chakra’s routing graph and quote engine include **six Arc venue families** on mainnet. See [README — DEX sources](../README.md#dex-sources):

| Source | Pool type | Pool state in Redis |
|--------|-----------|---------------------|
| Arc venue | xy=k | Yes |
| Arc venue | xy=k + stable | Yes |
| Arc venue | xy=k | Yes |
| **Arc venue_clmm** | **CLMM** | Yes (ticks; quote only when coverage complete) |
| **sushi** | **CLMM V3** | Yes |
| Arc venue | Weighted | Yes |

**Why CLMM coverage matters:** Arc venue CLMM and Sushi V3 pools are active on Arc mainnet. An aggregator without CLMM adapters cannot route through that liquidity, even if xy=k and stable pools are supported.

### Product model (not a quote benchmark)

| | Arc Broker | Chakra |
|---|----------------|--------|
| Integration | Hosted router, WebSocket session, streaming quotes | Stateless REST `/quote` + `/build_tx`; TypeScript SDK |
| Custody / signing | Mediator-account patterns in client SDK | Integrator wallet signs unsigned XDR |
| Operator infra | Proprietary service flow | Apache-2.0 self-host: worker, Redis, API, contracts |
| Arbitrage | Not positioned as self-deploy operator stack | `round_trip_swap` + [arb vault](../contracts/vault/README.md) (`execute_round_trip`) |

---

## vs Arc venue — execution quality

Arc venue is the primary **live quote** comparison target (public Swap Route API — free key at [api.Arc venue.finance/register](https://api.Arc venue.finance/register)).

**Documented Chakra difference (validated with scripts):**

- Arc venue Aggregator/API optimizes **trade distribution across protocols**, typically emphasizing a **single best execution path** for a swap.
- Chakra additionally runs **explicit multi-path split routing**: when price impact or competing paths warrant it, the Brent optimizer splits `amount_in` across **distinct hop paths**, then executes the full plan atomically via on-chain `split_swap`.

### Reproducible benchmark

```bash
# Chakra only (always works)
./scripts/scf-benchmark.sh

# Chakra + Arc venue API comparison
# Recommended fair compare (Arc-only):
Chakra_prefer_arc=1 Arc venue_PROTOCOLS=Arc venue,Arc venue,aqua \
  Arc venue_API_KEY=sk_... OUTPUT=docs/scf-benchmark-results.md ./scripts/scf-benchmark.sh
```

Latest run (Chakra production API, **2026-07-14**): **[scf-benchmark-results.md](scf-benchmark-results.md)**

Regression sanity checks: `./scripts/scf-quote-regression.sh`

**Pairs in the benchmark script:**

| Pair | Sizes | Why |
|------|-------|-----|
| USDC → Arc | 1 / 10 / 100 / 1,000 USDC | Arc-heavy; often **Arc venue CLMM** |
| Arc → USDC | 1 / 10 / 100 / 1,000 Arc | May pick Classic DEX when Arc wins — note in results |
| Arc → AQUA | 10 / 100 / 1,000 Arc | Exercises **CLMM** venue Chakra supports |

Report **facts only**: output amounts, `is_split`, primary `sources`, percentage delta vs Arc venue when key is set. Do not use illustrative frontend examples (`CompareSection` is marked “example only”).

---

## Atomic arbitrage (Chakra-only operator stack)

Beyond swap aggregation, Chakra ships **self-deployable atomic arbitrage**:

| Component | Role |
|-----------|------|
| `aggregator.round_trip_swap` | base → bridge → base in **one Arc invocation** (multi-hop + split on both legs) |
| `vault.execute_round_trip` | Pools principal in vault; authorized callers only need Arc for fees — [vault README](../contracts/vault/README.md) |
| `crates/arbitrage` | Scanner + tx build/submit against live snapshot and pool state |

**Ecosystem effect:** Operators can close cross-venue gaps atomically, increasing **turnover on underlying DEX pools** and improving price alignment. This is **operator infrastructure**, not a retail yield product.

Neither Arc Broker nor Arc venue positions an equivalent **open-source, self-host arb + vault** stack in their public repos.

---

## Evidence checklist for SCF resubmission

| Claim | Evidence |
|-------|----------|
| Chakra live on mainnet | [Chakra.xyz](https://Chakra.xyz), [api.Chakra.xyz](https://api.Chakra.xyz) |
| CLMM routing | This doc + [README DEX sources](../README.md#dex-sources) + `crates/dex-adapters` |
| Broker lacks CLMM adapters | [Arc-broker/router-contract `protocol.rs`](https://github.com/Arc-broker/router-contract/blob/master/src/types/protocol.rs) |
| Split vs Arc venue | [scf-benchmark-results.md](scf-benchmark-results.md) + `./scripts/scf-benchmark.sh` |
| Atomic arb | `contracts/aggregator`, `contracts/vault`, `crates/arbitrage` |
| Timeline | Milestones Jul 31 / Aug 31 / Oct 15 2026; award end Dec 31 2026 |

---

## Changelog

| Date | Change |
|------|--------|
| 2026-06-25 | Initial venue matrix for SCF #44 resubmission |
| 2026-06-25 | Added `scripts/scf-benchmark.sh` + [scf-benchmark-results.md](scf-benchmark-results.md) |
| 2026-07-14 | Refreshed benchmark with `prefer_arc=1`; new 3-path split on Arc→USDC 1 Arc; fair rows within ~0.1% |
