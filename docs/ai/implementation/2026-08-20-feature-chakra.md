---
phase: implementation
title: Implementation Guide
description: Technical implementation notes, patterns, and code guidelines
feature: chakra
date: 2026-08-20
---

# Implementation Guide

**Product:** Chakra  
**Feature key:** `chakra`  
**Workspace:** `.worktrees/feature-chakra` (`feature-chakra`)  
**Phase:** 7 Check (2026-08-29) — **not aligned** with the 2026-08-29 curated rebaseline; return to Execute (do not start Review). Prior Phase 5 continuation notes retained. Canonical surface: "Canonical curated rebaseline (2026-08-29)" below.
**Rebaseline (2026-08-29):** canonical curated strategy — catalog is USDC/EURC/cirBTC; XyloNet/Presto/UnitFlow V2.5 manifest venues; mBTC and owned mocks are chain-31337 fixtures only; no reseeding gate. See the requirements/design/planning/testing amendments in the same pass.

## Development Setup

- Rust via `rust-toolchain.toml` (`1.88.0`) so Cargo is not the Solana default 1.79.
- Foundry: `forge` 1.5.x, `contracts/evm` with **mixed solc**: `auto_detect_solc = true` + `compilation_restrictions` — `src/**`/`test/**`/`script/**` = `0.8.30`/`prague`; `venues/uniswap-v2/**` = `0.5.16`/`istanbul`; `venues/uniswap-v3/**` = `0.7.6`/`istanbul`. `libs = ["lib", "venues"]`. There is no single global `solc` (V2 is `=0.5.16`, V3 is `=0.7.6`).
- `forge install foundry-rs/forge-std --no-git` into `contracts/evm/lib` (gitignored).
- Env: copy `.env.example`. Public RPC only (`https://rpc.testnet.arc.io` / `wss://rpc.testnet.arc.io`). Do not use Canteen `$RPC` or an invented Alchemy URL.
- Redis prefix constant: `market_snapshot::REDIS_PREFIX` = `chakra:`.

## Code Structure (M1)

Kept workspace members: `Chakra-config`, `market-snapshot`, `market-data-worker`, `router-engine`, `dex-adapters`, `api-server`, `Chakra-alerts`, `sdk`.

Excluded from default build: Arc `contracts/*`, `arbitrage`, `limit-keeper`, `analytics-indexer`, `Chakra-swap-api`.

Binaries: `chakra-market-data-worker`, `chakra-api-server`.

Foundry tree: `contracts/evm/{src,test,script}` plus Placeholder until T5.1.

## Implementation Notes

> **Historical note (2026-08-29):** T1.2–T2.4 below describe the pre-rebaseline mBTC/owned-liquidity implementation. The **canonical curated rebaseline supersedes them**: the catalog is USDC/EURC/cirBTC, mBTC and owned XYK/stable/CLMM deployments are chain-31337 fixtures only, and the Arc operator workflow never deploys them. See the "Canonical curated rebaseline (2026-08-29)" section at the end of this file for the normative current surface.

### T1.1 Workspace skeleton

- Changed Redis key prefixes from `Chakra:` to `chakra:` in `pool_state_store.rs` and default `redis.channel`.
- Removed indexer/stats/orders/swaps/arbitrage/Arc_price from `api-server` `lib.rs` so the crate compiles without `analytics-indexer`.
- Arc adapters still compile (T3.2 will replace them). Default workspace no longer builds WASM contracts.

### T1.2 Decimals

- `crates/market-snapshot/src/decimals.rs`: catalog USDC/EURC/mBTC, native-not-a-node, integer atomic parse, USDC MAX `ceil(wei/1e12)*1.25` with 100_000 floor.
- mBTC address is an argument until T2.1 deploy.

### T2.1 Mock BTC

- `contracts/evm/src/MockBtc.sol`: name `Mock BTC`, symbol `mBTC`, 8 decimals, `owner` = deployer, `mint` `onlyOwner`. No faucet.
- Tests: `test/MockBtc.t.sol` (5 cases). Script: `script/DeployMockBtc.s.sol` logs address/decimals/owner.
- Broadcast: `forge script DeployMockBtc --rpc-url $CHAKRA_RPC_HTTP --broadcast` with key from gitignored env/keystore — never `--private-key` on CI CLI.
- Live address not yet written to `CHAKRA_MBTC_ADDRESS` (no key in this environment).

### T2.2 xy=k (Uniswap V2, 30 bps hardcoded)

- **Vendor:** `venues/uniswap-v2/` from v2-core tag `v1.0.1` (GPL-3.0-or-later, upstream `LICENSE` kept). `contracts/` only (no upstream tests). Local `lib/` stays gitignored.
- **Deploy pattern:** V2 sources are `pragma =0.5.16` and must not be imported by 0.8.30 code. Factory is compiled **offline** (solc 0.5.16, istanbul) to `bytecodes/v2-factory.hex` / `v2-pair.hex`; tests/scripts deploy via `src/VendorDeployer.sol` (`_deployFromHexFile` / `_deployFromHexFileWithArgs`, `fs_permissions` read on `./bytecodes`) and talk to V2 through 0.8.30 ABI interfaces `src/interfaces/IUniswapV2{Factory,Pair}.sol`.
- **Constructor arg:** `UniswapV2Factory(feeToSetter)` — seed script/test pass `abi.encode(address(0))`.
- **Tests:** `test/XykFactory.t.sol` (8) — pairs USDC/EURC, USDC/mBTC, EURC/mBTC; `getPair` matches both orderings; `token0 < token1`; mint→reserves; transfer-in then `swap(amountOut, ..., "")`; LP burn; fee is 30 bps (997/1000 vs no-fee counterfactual). Uses `MockErc20` (6 dp) + `MockBtc` (8 dp) — no live Arc tokens.
- **Script:** `script/DeployXyk.s.sol` (compile-only; no broadcast this session).

### T2.3 Stableswap (USDC/EURC, A=100, 4 bps)

- **Original Apache-2.0 code** (not vendored): `src/stable/StableSwap.sol` + `StableSwapFactory.sol`.
- **Math:** invariant + Newton `_getD`/`_getDyFromOld` ported from `crates/dex-adapters/src/stable_math.rs` so T3.2 quote math can match on-chain. `A = 100`, `FEE_DENOMINATOR = 10_000`, fee **4 bps on input** (same as the Rust `get_dy`) — not Curve-vyper fee-on-output. Documented deviation.
- **No `transferFrom` in `exchange`:** the aggregator pre-transfers `tokenIn` to the pool; `exchange(i, j, amount, minDy)` computes output from stored reserves (not live balances). `actualIn = balanceOf(tokenIn) - reserveIn`; reverts `IndexOutOfRange` if i/j > 1, `SameIndex` if i==j, `ZeroAmount` if actualIn==0 or amount==0, `InsufficientInput` if actualIn < amount. After swap: stored reserves updated (reserveIn += amount, reserveOut -= dy). `removeLiquidity` decrements stored reserves. **Stored-reserve custody** (2026-08-26, T5.1/T2.3) prevents drain-by-declaration attack: without a real deposit, exchange reverts ZeroAmount.
- **Seeding:** `seedLiquidity(amount0, amount1)` — one-shot (reverts if already seeded), mints `sqrt(b0·b1)` LP to `msg.sender`, uses balance-delta of tokens already transferred in. Stores `reserve0 = b0`, `reserve1 = b1` (2026-08-26, T5.1/T2.3). `removeLiquidity` burns proportionally and decrements stored reserves. (Interface `IStableSwap` was aligned to `seedLiquidity` — the initial `addLiquidity()` 0-arg draft did not match the implementation.)
- **Factory:** `StableSwapFactory.createPool(tokenA, tokenB)` sorts tokens, stores both orderings in `getPool` (so T5.1 can membership-check without a Uniswap-style `getPair`), reverts `PoolExists` on duplicates.
- **Tests:** `test/StableSwap.t.sol` (16) — 10 original: createPool both orderings + duplicate revert, exchange 0→1 and 1→0, `minDy` too high reverts, same-index/zero-amount revert, 4 bps fee, depth > xy=k. 6 custody (2026-08-26, T5.1/T2.3): no-deposit drain reverts `ZeroAmount`, index ≥ 2 reverts `IndexOutOfRange`, declared amount > actual deposit reverts `InsufficientInput`, reserves track after exchange, reserves track after removeLiquidity, excess deposit not consumed by reserve tracking.
- **Script:** `script/DeployStable.s.sol` (compile-only).

### T2.4 CLMM (Uniswap V3, 30 bps required)

- **Vendor:** `venues/uniswap-v3/` from v3-core tag `v1.0.0` (upstream `LICENSE` = **Business Source License 1.1** — note the plan draft said "GPL-2.0-or-later"; that applies to later v3-core tags, not v1.0.0. We keep the true upstream license file).
- **Deploy pattern:** same `VendorDeployer` hex approach (`bytecodes/v3-factory.hex`, `v3-pool.hex`, solc 0.7.6/istanbul); 0.8.30 interfaces `src/interfaces/IUniswapV3{Factory,Pool}.sol`.
- **`int256` selector gotcha (fixed 2026-08-24):** V3 v1.0.0 `swap` is `swap(address,bool,int256,uint160,bytes)` — `amountSpecified` is `int256`. The initial interface declared `uint256`, producing a different selector so every swap reverted after ~226 gas (no source-level trace in opaque bytecode). `mint`'s `uint128 amount` was already correct. Tests now also use a real liquidity size: L=1 (full-range) makes oneForZero outputs round to 0 USDC (fee ≥ input), so `mint` uses **L = 1e12** — exactly funded by the 100_000e6 USDC + 100_000e8 mBTC the tests mint (amount0 = L/√P = 1e11, amount1 = L·√P = 1e13).
- **Price:** `sqrtPriceX96 = 10·2^96` (P = 100 raw mBTC-units per USDC-unit; "1 USDC = 1 mBTC nominal"). Pool fee 3000, tickSpacing 60.
- **Tests:** `test/ClmmPool.t.sol` (5) — `createPool(USDC, mBTC, 3000)` + `initialize` + `slot0`; in-range `mint` (ticks ±887220, multiples of 60, both tokens owed via `uniswapV3MintCallback`); `swap` zeroForOne (100 USDC in → ~9900 mBTC units) and oneForZero (1000 mBTC in → ~9 USDC units) via `uniswapV3SwapCallback`; 5 bps pool absent (`getPool(..., 500) == address(0)`). Callbacks cast `int256` deltas to `uint256` only when `> 0`.
- **Script:** `script/DeployClmm.s.sol` (compile-only). Tight-in-range seed (real spot) is a live-Arc step, out of scope here.
- **Seed sizes documented** (feeds SC-2 later): xy=k USDC/EURC 10_000e6 each (thin); stable USDC/EURC 200_000e6 each (**20×**); xy=k USDC/mBTC 50_000e6 / 1e8; xy=k EURC/mBTC 50_000e6 / 1e8; CLMM USDC/mBTC 30 bps tight in-range around spot.

### T5.1 Aggregator (splitSwap, Permit2, allowlist)

- **Deps:** OZ 5.7.0 — `forge install OpenZeppelin/openzeppelin-contracts --no-git` into gitignored `contracts/evm/lib/` (same pattern as forge-std); remap `@openzeppelin/=lib/openzeppelin-contracts/` (root, not `contracts/`). Fallback tiny `Auth.sol` was **not** needed.
- **`src/Aggregator.sol`** (Apache-2.0, `pragma ^0.8.30`): `Ownable + Pausable + ReentrancyGuard`; **not upgradeable**. Constructor `(permit2, usdc, eurc, mbtc)` — catalog = leftover-sweep set.
- **ABI (per design):** `enum DexType { Xyk, Stable, Clmm }`, `Hop{pool,dexType,tokenIn,tokenOut,fee}`, `SubRoute{amountIn,hops[]}`, `Permit2Pull{permitSingle,signature}`; `splitSwap(...) external nonReentrant whenNotPaused`.
- **Execution order:** `_validate` (deadline, non-zero/different tokens, amount>0, ≥1 routes, per-route continuity + first/last token match, sum(amountIn)==amountIn) → `_verifyPools` (per-hop allowlist BEFORE any external call) → `_pull` (Permit2; empty signature skips `permit()`, `spender` must be `address(this)`) → hops (recipients = aggregator; per-hop computed outputs chain into the next hop's input) → `amountOut = balanceDelta(tokenOut)`, `>= minAmountOut` → send all `tokenOut` to `msg.sender` → sweep leftover catalog balances → `Swap(...)` with `isSplit = routes.length > 1`.
- **xyk hop:** compute 997/1000 output from `getReserves` (pre-transfer), `safeTransfer(tokenIn, pair)`, then `pair.swap(amount0Out, amount1Out, address(this), "")` — empty data, no callback. **Do not** call `IUniswapV2Pair.getAmountOut` — the real V2 pair has no such function (the 0.8.30 interface in `src/interfaces/` lists it; the aggregator never uses it).
- **stable hop:** membership = `allowedStablePools[pool]` **or** allowlisted stable factory `getPool(tokenIn, tokenOut) == pool`; `safeTransfer` then `IStableSwap(pool).exchange(i, j, amount, 0)` (pool has no `transferFrom`; i/j from `token0()`).
- **clmm hop:** allowlisted Clmm factory `getPool(tokenIn, tokenOut, fee) == pool`; `pool.swap(address(this), zeroForOne, int256(amountIn), sqrtLimit, abi.encode(pool, tokenIn))` — **`int256` selector** (T2.4 gotcha); output from negative delta. Callback pays the pool; **not** `nonReentrant` (swap re-enters while `splitSwap` holds the guard). `uniswapV3SwapCallback` requires `msg.sender == decoded pool` AND `getPool(token0, token1, fee) == msg.sender` for an allowlisted Clmm factory → `CallbackSenderMismatch` / `CallbackPoolNotAllowlisted`.
- **Admin:** `pause`/`unpause`, `addFactory(factory, DexType)`/`removeFactory` (swap-and-pop list + index map), `addStablePool`/`removeStablePool`, `rescueTokens` (owner-only, not on the swap path). No events on admin functions (TDD-minimal); `Swap` event is the locked ABI.
- **receive/fallback:** non-payable `splitSwap` plus explicit `receive()/fallback() external payable { revert DirectEth(); }`. Never `block.prevrandao`.
- **`test/MockPermit2.sol`:** AllowanceTransfer test double — `approve(user, token, spender, amount, expiration)` simulates an existing allowance; `permit` accepts any 65-byte signature (grants + records, `permitCalls` observation port), rejects other lengths `InvalidSignature`; `transferFrom` checks allowance/expiry then ERC-20 pulls. Tests approve the mock contract directly as spender (no EIP-712 verification in the mock).
- **`test/Aggregator.t.sol` — 39 cases across the 12 locked behaviors** (pause & owner-only, deadline/token/route/amount validation, ETH rejection via low-level call + revert-data decode, factory allowlist gating incl. fake-pool + removeFactory, Permit2 empty-sig skip vs signed pull vs bad sig vs spender mismatch, single-hop xyk with exact venue-only output + `Swap` event, minAmountOut revert with leftover-0 + unchanged reserves/balances, multi-hop EURC→USDC→mBTC atomic success + atomic revert, split thin-xyk + deep-stable (both reserves move, venue-fee-only bound, `isSplit=true`), CLMM callback hop + 3 spoof variants (sender mismatch / non-allowlisted FakePool / random EOA), owner rescue + non-owner revert, never-call table = 12 Arc `contract-addresses.md` addresses rejected with empty and populated allowlists).
- **Fixtures:** reuse `VendorDeployer` hex factories; seed sizes from implementation notes (xyk USDC/EURC 10_000e6 thin, stable 200_000e6 deep, xyk USDC/mBTC 50_000e6/1e8, CLMM full-range L=1e12 at `sqrtPriceX96 = 10·2^96`, fee 3000); V3 mint callback lives on the test contract; mint-before-test; no live Arc.
- **`script/DeployAggregator.s.sol`** (compile-only): Permit2 defaults to the Arc predeploy `0x0000...c78BA3`; USDC/EURC default to the frozen catalog addresses; `CHAKRA_MBTC_ADDRESS` + factory placeholders from env; `addFactory` only for non-empty placeholders. `.env.example` gained `CHAKRA_PERMIT2` / `CHAKRA_USDC_ADDRESS` / `CHAKRA_EURC_ADDRESS`. No broadcast this session, no `--private-key`.
- **Placeholder removed:** `src/Placeholder.sol` + `test/Placeholder.t.sol` deleted once `Aggregator` compiled (skeleton obsolete).

### T3.1 Snapshot schema + `chakra:` Redis store (2026-08-25)

- **Key constants** (`crates/market-snapshot`):
  - `store.rs`: `DEFAULT_REDIS_EVENTS_CHANNEL = "chakra:snapshot:events"`; new `SNAPSHOT_CURRENT_KEY = "chakra:snapshot:current"`; `RedisSnapshotStore` key prefix flipped `Chakra:snapshot` → `chakra:snapshot`. Versioned payloads remain `chakra:snapshot:data:{version}` (+ `:meta:{version}` + `:versions` index) so `current`/`events`/`versions` cannot collide. **Deviation vs the design table's `chakra:snapshot:{version}`** — documented here and in the planning Phase 6 summary.
  - `pool_state_store.rs`: `STABLE_KEY_PREFIX` is now used (`chakra:pool:stable`), new `pub const FACTORIES_KEY = "chakra:factories"`; pool keys keep EX=86400.
- **Schema:**
  - `TradingPairSnapshot` + `dex_type: String` (`"xyk"`/`"stable"`/`"clmm"`, serde default `"xyk"`) and `factory: String` (serde default empty) — old JSON still deserializes.
  - `XykPoolStateValue` and `ClmmPoolSnapshot` + `factory: String` (serde default empty).
  - New `StablePoolStateValue` (source, pool_address, token_a/b, balance_a/b, `a`, `fee_bps`, factory, `updated_at_ms`) with `redis_key`/`pool_key`/`new`.
  - New `FactoryRecord { address, dex_type, source }` with `new`.
- **`PoolStateStore` trait:** `set_stable_batch`/`fetch_stable` and `set_factories`/`fetch_factories` implemented on `MemoryPoolStateStore` (stable map + factories vec, `pool_count()` helper) and `RedisPoolStateStore` (set_ex/mget + JSON `chakra:factories`). `publish_pool_state` writes stable (currently empty — Arc venue/Arc venue types kept until T3.2 adapter replacement). `RedisPoolStateStore::snapshot_exists` uses `SNAPSHOT_CURRENT_KEY`.
- **Ready helper** (`src/ready.rs`): `cluster_ready(redis_url)` = EXISTS `chakra:snapshot:current` AND `COUNTKEYS chakra:pool:*` ≥ 1; `memory_ready(&MemorySnapshotStore, &MemoryPoolStateStore)` = `has_snapshot()` (new) AND `pool_count()` (new) ≥ 1. HTTP `/ready` handler shape untouched (T4.3 wires it per plan).
- **Bootstrap publisher** (`src/bootstrap.rs`): `BootstrapPublish { snapshot, xyk_pools, stable_pools, clmm_pools, factories }`; `publish_bootstrap` (Redis: snapshot → pool keys → factories; pool keys EX default 86400) and `publish_bootstrap_memory` (embedded). No RPC anywhere. CLMM write path still gated by `should_publish_clmm_to_redis` (complete coverage only). Worker will call this in T3.3; T3.1 tests call it directly with fixture data (mock catalog USDC `0x3600…0000` / EURC `0x89B5…D72a` / placeholder `0xMBTC`).
- **Defaults flipped:** `DEFAULT_REDIS_EVENTS_CHANNEL` now `chakra:snapshot:events` — consumed by api-server `AppConfig`, worker `WorkerConfig` (both read `SNAPSHOT_REDIS_CHANNEL` override), and api-server tests updated. Remaining `Chakra:` strings live only in Arc-era bins (`dex-adapters/src/bin/{dump,audit}_ledger_events.rs`), untouched.
- Caller updates for the new required fields: `market-data-worker` (`trading_pair_snapshot` sets `dex_type: "xyk"`, `factory: ""`; test literals), `api-server` (`snapshot_loader`, tests), `dex-adapters` (`clmm_math::clmm_pool_from_snapshot` sets `factory: ""`; `pool_index` test literals). All existing tests updated atomically.

### T3.2 EVM venue quote math (2026-08-25)

- New `crates/dex-adapters/src/evm_quote_math.rs` (pure local math, no RPC; re-exported from `dex_adapters::`):
  - `xyk_quote(reserve_in, reserve_out, amount_in)` — Uniswap V2 997/1000: `in_after_fee * r_out / (r_in + in_after_fee)` with `in_after_fee = amount_in * 997 / 1000`. Identical to `Aggregator._xykFormula` (test pins the same expression as `Aggregator.t.sol::_xykFormula`).
  - `stable_quote(&StablePoolStateValue, i, j, amount_in)` — 2-token equal-decimals port of `StableSwap.sol`: 4 bps **fee-on-input** (`fee = ceil(amount * FEE_BPS / 10000)`), invariant `D` from **old** balances (`Ann = A*2`, A=100), Newton solve `y` from `c = D^3/(4·xNew·Ann)`, `b = xNew + D/Ann`, `dy = oldBalJ - y - 1` (rounding-safety `-1`). **Byte-exact vs on-chain:** validated with a temporary `forge script` probe run against the real `StableSwap.sol` this session — 200_000e6 seed, three sequential 1_000e6 USDC→EURC exchanges produced `999550535 / 999451582 / 999352602`; `stable_quote` reproduces all three exactly including the reserve drift between swaps. Probe script deleted after capture.
  - `price_impact_bps(reserve_in, reserve_out, amount_in, amount_out)` — integer bps vs spot (`12` = 0.12%).
  - Guards: zero input / empty reserves / same index / out-of-range index → 0.
  - CLMM: no new math in T3.2 — the existing fixed-point `clmm_math` engine already quotes V3, and the skip-if-incomplete policy is enforced at Redis publish (`should_publish_clmm_to_redis`) and in QuoteEngine. Fetch adapters (RPC reads, WS/poll) are T3.3.

### T3.3 Arc RW client + log decoder + WS/poll watcher + fetch pipeline (2026-08-25)

Worker default path on `feature-chakra` is now Arc. Deps `CHAKRA_RPC_HTTP/WS` + documented failovers are enforced (never Canteen `$RPC`, never an invented Alchemy URL); `CHAKRA_REDIS_URL` maps to the snapshot store (`SNAPSHOT_REDIS_*` keep working as overrides); `CHAKRA_SEED_FACTORIES` / `CHAKRA_DISCOVERY_FACTORIES` are `address:xyk|stable|clmm` tuples (`discovered:*` sources are **not** auto-allowlisted on the aggregator); `CHAKRA_CHAIN_ID`, `CHAKRA_EVM_WS_ENABLED`, `CHAKRA_EVM_POLL_INTERVAL_MS`, `CHAKRA_EVM_MAX_CATCHUP_BLOCKS`, `CHAKRA_DISCOVERY_INTERVAL_SECS`.

Everything runs on fixture RPC/WS servers and a **memory store** in tests (SC-11's ≤ 5 s proven locally); **live on-chain swap → Redis measurement is T9.6** (no operator key here, never Canteen `$RPC`).

**New in `crates/dex-adapters`:**
- `evm_rpc.rs` — `EvmRpcClient` (reqwest JSON-RPC `eth_blockNumber` / `eth_call` / `eth_getLogs`, ordered URL failover, 10 s timeout, JSON-RPC error surfacing), `EvmLog` (+ `EvmLog::from_json` used by the WS notification path), `LogFilter`, hex helpers (`word_to_u128`/`word_to_i32`/`word_to_u256_limbs`/`parse_hex_u64`), and the **RPC URL policy**: only `rpc.testnet.arc.io` + Blockdaemon HTTP / dRPC HTTP+WS / QuickNode HTTP+WS; Canteen `rpc.testnet.arc-node.thecanteenapp.com` and Alchemy hosts rejected (`evm_http_url_allowed` / `evm_ws_url_allowed` / `validate_http_urls` / `validate_ws_urls`). Fixture JSON-RPC HTTP server is a `#[cfg(test)] pub(crate) mod fixture` on its own std thread + tokio runtime.
- `evm_logs.rs` — keccak256 topic0/selector helpers; venue event signatures (V2 `PairCreated`/`Swap`/`Sync`/`Mint`/`Burn`, V3 `PoolCreated`/`Swap`/`Mint`/`Burn`, stableswap `PoolCreated`/`Swapped`/`LiquidityAdded`/`LiquidityRemoved`); `touched_pools_from_evm_logs` (index lookup, **ERC-20 `Transfer` and native USDC sends are never touches**); `created_pools_from_evm_logs` → `DecodedCreated::{Xyk,Stable,Clmm}` (tokens+pool decoded from topics/data, canonical sorted); **12-address never-call table** (mirrors `Aggregator.t.sol::neverCall`), `filter_subscribe_addresses` drops never-call entries; `normalize_evm_address` (lowercase `0x`-padded). Topic0 hashes pinned against well-known Uniswap values in tests.
- `evm_fetch.rs` — eth_call hydrators for **touched pools only**: `fetch_xyk_state` (`getReserves` → `XykPoolStateValue`), `fetch_stable_state` (`balanceOf` both tokens → `StablePoolStateValue`, `A = CHAKRA_STABLE_A = 100`), `fetch_clmm_state` (`slot0` + `liquidity` merged over the existing snapshot — ticks/bitmaps/coverage carried through; Redis publish remains gated by `should_publish_clmm_to_redis`). Factory discovery: `factory_has_xyk_pair` / `factory_has_stable_pool` (`getPair`/`getPool` mapping getter) / `factory_has_clmm_pool` (`getPool(token0, token1, fee)`).
- `pool_index.rs` — `KnownPoolIndex` now indexes both Arc `C…` ids **and EVM `0x…` addresses** (lowercased keys) so Arc pools are never dropped.

**New/edited in `crates/market-data-worker`:**
- `evm_watcher.rs` — `FactoryConfig::parse`, `EvmConfig::from_env` (URL policy enforced — Canteen fail), `EvmRunner`: `discover_once` (~600 s; `getPair`/`getPool` over the catalog pairs only — **no full-market sweep on the hot path**; `catalog_pairs` skips mBTC pairs until `CHAKRA_MBTC_ADDRESS` is set), `publish_bootstrap` (`market_snapshot::publish_bootstrap` reuse — empty factories allowed; `/ready` stays false until ≥1 pool key), `poll_once` (blockNumber → `eth_getLogs` window with catch-up cap, ~0.5 s default), `ingest_logs` (created-pool upsert into `shared.sources`/`clmm_pools` + index refresh + touch enqueue), `ws_watch_loop` (`eth_subscribe "logs"` with watched address filter that **reconnects when the address list grows**; WS failover list; ~30 s idle reconnects), `run_arc` (event-channel loop: WS logs / poll ticks / discovery ticks all funnel into one `&mut self`). Arc adapters are **not constructed** on this path — `spawn_arc_pipeline` passes only an EVM client + never-contacted stub RPCs.
- `fetch_pipeline.rs` — same one pipeline, extended queue shape: `FetchTask::{EvmXyk,EvmStable,EvmClmm}`, `PoolStateUpdate::Stable` → `set_stable_batch`, EVM execution arms read topology from `shared` (`find_evm_pair`) and hydrate via `evm_fetch`; coalesce maps `chakra-xyk|discovered:xyk` → EvmXyk etc. CLMM still skips Redis when coverage incomplete.
- `worker.rs` — `WorkerMode::{Arc,Arc}`; `from_env` defaults to **Arc** (Arc loop only when `RPC_URL`/`SNAPSHOT_REDIS_URL` set and no `CHAKRA_*`); `run()` dispatches to `evm_watcher::run_arc`.
- Workspace deps added: `tiny-keccak`, `url`, `tokio-tungstenite 0.24` (no alloy/ethers).

**Edge cases handled:** odd-length hex; word >32 bytes; getReserves with <2 words; non-whole-word eth_call responses rejected; empty mBTC address never probes `getPair(…, "")`; never-call addresses excluded from watch list and created-pool upserts; created pools on unconfigured factories ignored; WS channel closed → runner exits. `sqrt_price_x96` decoded as big-endian 256-bit word → little-endian `[u64;4]` limbs (limb `i` from the rightmost 8-byte window — this is the representation `clmm_math::U256` stores).

### T4.2 QuoteEngine EVM wiring + SplitOptimizer fee/split (2026-08-25)

- **`OptimalRoute.protocol_fee_bps: u32`** (SC-13) — new field, set to `0` on every `OptimalRoute` literal in `split_optimizer.rs` (7 sites incl. `empty_route`) and `quote_engine.rs` (4 sites). Locked by `protocol_fee_bps_is_always_zero` (empty / single / split) and asserted in every T4.2 engine test.
- **`QuoteHydration.stable_pools: HashMap<String, StablePoolStateValue>`** keyed by `StablePoolStateValue::pool_key(source, pool)` (= `chakra-stable:{pool}`). `api-server::pool_hydrate` now collects `chakra-stable` refs into a stable bucket (`fetch_stable`), reports `redis_miss_stable`, and fills the new field. `chakra-clmm` added to the CLMM refs bucket.
- **EVM hop dispatch in `quote_path`:** `chakra-stable` → `local_stable_quote` (uses `evm_quote_math::stable_quote`, token index from hydrated `token_a/b`, **no** `MIN_XYK_RESERVE_atomic unitsS` on stable balances, impact = `(spot - out)*10000/spot` vs the 1:1 spot — the 4 bps venue fee is always paid); `chakra-xyk` → `local_evm_xyk_quote` (`evm_quote_math::xyk_quote` 997/1000 + integer `price_impact_bps` on hydrated reserves, dust-reserve guard kept). Arc generic 9970/10000 / Arc venue / Arc venue paths are untouched.
- **`local_clmm_quote` allowlist** extended to `chakra-clmm` (keep skip-if-incomplete policy: `clmm_swap_allowed` + coverage).
- **`QuoteEngine::update_from_chakra_snapshot`** — snapshot → `pairs_from_chakra_snapshot` → per-source `update_pairs_from_cache` (keeps real source names so dispatch + hydration keys match). Test helper; API wiring is T4.3.
- **SC-12 guard** at the top of `get_route_with_paths`: `decimals::is_native_usdc_encoding` on `token_in` **or** `token_out` (`native_usdc` / `eth` / `0x000…0`) → empty route with `total_expected_out=0`.
- **SC-2 documented deviation (important):** at the plan's `180_000e6` size the xy=k leg (~0.7% of the trade, ≈1.22e9 in) genuinely improves 100% stable by ~7 bps **in isolation**, but the locked `max_leg_rate_deviation_bps=500` filter compares the leg's marginal rate (~0.89) against the **catastrophically diluted full-size xy=k quote rate** (~0.0526) → 17× deviation → leg rejected; combined with the 5 bps improvement floor and dust filters, the engine returns the single `chakra-stable` route. Verified: no size exists where the split both passes the rate filter and beats single by ≥5 bps on these seeds (brute-force scan to 2e12). Per plan constraints (no `SplitConfig` default changes, no seed-depth changes), the test **locks the honest behavior** (`sc2_180k_split_is_refused_and_single_stable_wins`) and this doc records the deviation; the split becomes reachable if the rate filter is relaxed for chakra-xyk or the xy=k seed is deepened (T4.3/T9.2 follow-up).
- Tests (7 new): fee=0 (3 shapes), `max_splits=1` lock, stable hydration + vector pin `999_550_535`, SC-2 refusal + control, chakra-clmm complete/incomplete, native-encoding rejection (both encodings × both directions), USDC→mBTC 8 dp atomic pin (`xyk_quote(50_000e6, 1e8, 1_000e6)` exact).

### T4.1 PathFinder BFS on the Arc graph (2026-08-25)

- **`PathFinderConfig::default()` fixed for Chakra:** `max_hops = 3` (unchanged), `bridge_tokens = [TokenId::Contract { address: USDC_ERC20 (0x3600…0000) }]` — the old default bridged **Arc Native + Classic USDC**, which was wrong on Arc. (The BFS itself explores all edges; the bridge list is compatibility — asserted by the default-config test.)
- **Snapshot loader** (`path_finder.rs`): `pairs_from_chakra_snapshot(snapshot, mbtc_address)` maps `MarketSnapshot` sources + `clmm_pool_refs` into router `TradingPair`s, **filtering with `decimals::graph_nodes(mbtc)`** (catalog freeze: pools whose tokens are outside {USDC, EURC, mBTC} are unused; native USDC encodings never become nodes). `PathFinder::update_from_chakra_snapshot` groups pairs by source and replaces each source's edges.
- **No changes to SplitOptimizer / QuoteEngine** (T4.2 onward).
- Tests (8 new): both-venues direct routes for USDC→EURC (xyk + stable) and USDC→mBTC (xyk + clmm), EURC→mBTC direct + 2-hop via ERC-20 USDC, `max_hops=1` exclusion, unknown/same-token empty, non-catalog pool unused (graph token count 0), native-encoding not a node, Chakra default config. The fully-connected catalog also produces legitimate multi-hop candidates (e.g. USDC→mBTC via EURC); the "finds both venues" assertions scope to direct (1-hop) paths.

### T4.3 Chakra REST + OpenAPI (2026-08-25)

- **Envelope** (`envelope.rs`): `{success, data, error:{code,message}}`; codes `INVALID_PARAMS`, `ZERO_AMOUNT`, `SAME_TOKEN`, `UNKNOWN_TOKEN`, `NO_ROUTE`, `RATE_LIMITED`, `ROUTE_INVALID`, `PAUSED`, `NOT_READY`, `RPC_ERROR`. `error` is `null` on success.
- **`handlers.rs` rewritten** to the Chakra surface:
  - `/quote` — tokens validated against the frozen catalog (native USDC encodings → `UNKNOWN_TOKEN`, SC-12); missing/zero/same/unknown → 400 with code; integer `price_impact_bps` + `protocol_fee_bps: 0` + `max_splits` (default 5, clamped to server max); hydration via `hydrate::hydrate_for_quote` (Redis `chakra:pool:*` or memory store) — **quotes never hit RPC**.
  - `/tokens` — frozen catalog only (USDC 6 dp, EURC 6 dp, mBTC 8 dp); native USDC absent. mBTC address from `CHAKRA_MBTC_ADDRESS` via state (not env read at call time).
  - `/balances` — Multicall3 `aggregate3` (`0xcA11bde05977b3631167028862bE2a173976CA11`, selector `0x82ad56cb`) batch `balanceOf` for catalog tokens + separate `native_usdc` via `eth_getBalance` (18 dp). **Never summed** (SC-12). u128-safe hex parse (native balances exceed u64; odd-length hex accepted).
  - `/health` — `{status: ok}`; `/ready` — `{ready, snapshot_id, pool_keys}` via `ready::{cluster_ready,memory_ready}` (snapshot current AND ≥1 pool key).
  - `/build_tx` (T4.3 stub) — catalog validation, `to` from `CHAKRA_AGGREGATOR` (empty → 503 `NOT_READY` until T5.2); no encode yet (T4.4).
- **`rate_limit.rs`** — 10 req/s/IP sliding window; `/health` + `/ready` exempt; partner keys removed; loopback + `QUOTE_RATE_LIMIT_BYPASS_IPS` exempt (loopback kept for local curl, documented).
- **CORS** — `CorsLayer` with `AllowOrigin::list(CHAKRA_CORS_ORIGINS)` (default `http://localhost:3000`); unlisted origin gets no allowlist header (tower-http behavior).
- **`config.rs`** — `parse_chakra_rpc_http` (RPC policy: Canteen `rpc.testnet.arc-node.thecanteenapp.com` + invented Alchemy URLs rejected via `dex_adapters::evm_rpc::validate_http_urls`; public Arc + documented failovers only), `chakra_aggregator`, `chakra_cors_origins`.
- **`path_finder.rs` lowercase normalization** — `pairs_from_chakra_snapshot` now stores token addresses **lowercased** (EVM RPC addresses are lowercase; the API normalizes request tokens to lowercase). Test helpers in `path_finder.rs` / `quote_engine.rs` updated to lowercase lookups.
- **`dex-adapters`** — `evm_rpc::fixture` gated behind new `test-fixture` feature (integration-test server); `eth_get_balance` added. `api-server` gained `test-fixture` feature + `hex` dep.
- **`market-snapshot`** — `MemoryPoolStateStore::pool_keys` for `/ready` embedded pool listing.
- **OpenAPI** (`docs/openapi.yaml`) — retitled Chakra; envelope schemas + error codes; dropped all Arc paths (`/orders*`, `/stats`, `/prices*`, `/submit_tx`, `/tx_status`, `/account`, `/classic_asset`, `/ledger/latest`, `prefer_arc`); `/ready` predicate text; balances never-sum; **two quote examples**: (a) honest 100 USDC single `chakra-stable`, (b) 70/30 split labeled **illustrative / not current engine output** (doc deviation, see below). `docs/api-reference.md` rewritten.
- **Legacy removed** — Arc-era integration tests (`snapshot_quote_test`, `redis_snapshot_smoke_test`, `build_tx_simulate_test`, `decode_user_tx`) + `verify_split_quote` example (referenced deleted Arc handler symbols). Arc modules still compile but are not constructed on the Arc path.
- Tests: `tests/chakra_rest_test.rs` — 10 integration tests (envelope codes + no float impact, catalog freeze, hydrate routes + `999_550_535` pin + zero-RPC proof, SC-2 refusal + control, ready/health lifecycle, balances never-sum with fixture Multicall3, 429 + exempt paths, CORS allowlist, RPC policy).

### T4.4 `build_tx` splitSwap encoder + Permit2 typed data (2026-08-25, selector + Permit2 fixes 2026-08-26)

- **`abi.rs` (new)** — keccak256 + ABI word helpers. Selectors pinned: `splitSwap(...)` → `0x2e3be0c1` (canonical 7-arg with nested Permit2Pull tuple), `paused()` → `0x5c975abb`, `allowance(address,address)` → `0xdd62ed3e` (same tiny-keccak stack as the worker's log decoder; no alloy/ethers).
- **`build_tx.rs` (new)** — encoder + validator:
  - **Not a re-quoter**: continuity (first/last step + adjacent steps), per-leg amount sum == `amount_in`, snapshot pool membership per `dex_type`, `chakra:factories` allowlist (when factories configured; legacy unstamped pools accepted only when the factory list is empty).
  - Calldata: `splitSwap(tokenIn, tokenOut, amountIn, minAmountOut, deadline, SubRoute[]{amountIn, Hop[]{pool, dexType, tokenIn, tokenOut, fee}}, Permit2Pull{permitSingle, signature})` — ABI offsets are relative to the argument encoding (the 4-byte selector is not counted); dynamic-array element offsets are relative to the start of each element-offset list (the classic two-level offset bug cost several test iterations — the integration test decodes the full structure to catch it).
  - **T4.4 fixes (2026-08-26):** (a) `SPLIT_SWAP_SIGNATURE` corrected from 8-arg flattened to 7-arg canonical: extra `()` wrapper around `Permit2Pull` struct type → selector `0x2e3be0c1` (compiled `forge inspect`); (b) `encode_permit2_pull` now emits 6-word PermitSingle struct (token, amount, expiration, nonce, spender, sigDeadline) + offset(224) + signature (was 20 zero words + raw sig); (c) `permit2_allowance` uses Permit2 `allowance(address,address,address)` selector `0x927da105` (was ERC-20 `0xdd62ed3e`); (d) Foundry round-trip test `test_api_hex_empty_sig_succeeds` feeds `abi.encodeWithSelector(0x2e3be0c1, ...)` to the Aggregator via low-level call.
  - RPC (fixture): `paused()` → 503 `PAUSED`; ERC-20 `allowance(user→Permit2)` sufficient → `required_approvals: []`; Permit2 `allowance(user, tokenIn→aggregator)` sufficient + unexpired → `typed_data: null`; else `PermitSingle` typed data (AllowanceTransfer only; `verifyingContract` = Permit2 predeploy; spender = aggregator; `value` always `"0"`; `deadline = now + 120 s`; `chain_id 5042002`).
- Handler: `BuildTxRequest` gained `user`; envelope includes `{to, data, chain_id, value, deadline, typed_data, required_approvals}` (required_approvals always present, empty array when none).
- Tests: `tests/chakra_build_tx_test.rs` — 6 integration tests incl. full ABI decode of the calldata (head, routes, hops, pool/dexType/tokenIn/tokenOut/fee) and the allowance-skip matrix.


### T4.5 QuoteEngine factory skip (2026-08-26, local)

- **`QuoteHydration.factories`** — field added; the legacy `pool_hydrate.rs` loads `chakra:factories`, but that module is not exported on the active Chakra API path (caught by the Phase 7 check below).
- **`factory_allows_pool`** — empty factories → accept all (legacy); non-empty → source must have a matching factory record.
- **Factory gate in `quote_path`** — `chakra-*` sources only; non-Chakra sources are not gated.
- **Tests:** `t45_allowlisted_stable_factory_still_quotes`, `t45_unlisted_factory_pool_is_skipped`, and `t45_empty_factories_still_quotes_legacy_pools`.

### T9.2 Split-filter semantic change (2026-08-26, local)

- **`split_optimizer.rs`:** `leg_rate_matches_full_quote` → `leg_rate_matches_alloc_quote` (async, re-quotes at allocated leg size via `quote_fn`). The old filter compared leg rate against the full-size quote rate, which punished thin pools for AMM convexity (better small-size rates). The new filter re-quotes at the allocated leg size and checks consistency within `max_leg_rate_deviation_bps=500`.
- **`filter_dust_split_legs`:** now uses `for` loop with `.await` (was `.filter()` with `.await` — compile error).
- **`quote_engine.rs`:** EURC address casing fix in test fixtures — `EURC.to_lowercase()` to match pathfinder normalization (pre-existing latent bug: xyk paths were silently un-quotable in tests due to mixed-case EURC vs lowercased path tokens).
- **Tests:** `sc2_180k_split_is_refused_and_single_stable_wins` removed; `sc2_180k_split_beats_single_stable` added (T2.3 seeds, `180_000e6` USDC→EURC, `is_split=true`, ≥5 bps, 2 sub-orders). `leg_rate_rejects_fantasy_micro_quote` rewritten to async with `Arc<Path>`.
- **api-server test:** `sc2_180k_is_not_split_and_single_stable_wins` → `sc2_180k_is_split_and_beats_single_stable`. `seed_pool_state` uses lowercased EURC.
- **`180_000e6` is now the documented split size** (was refused before filter change).

### T6.1 wagmi/viem `arcTestnet` + EIP-6963 + chain gate (2026-08-25)

- `packages/frontend` deps: removed `@creit.tech/Arc-wallets-kit`, `@Arc/wallet-api`, `@Arc/Arc-sdk`; added `wagmi@3.7.6`, `viem@2.55.19`, `@tanstack/react-query@5.102.3`.
- `src/lib/chain.ts` — `ARC_CHAIN_ID` (5042002), `ARC_CHAIN_ID_HEX` (`0x4CEF52`), `ARC_RPC_URLS` (`https://rpc.testnet.arc.io`), `ARC_BLOCK_EXPLORER_URLS` (`https://testnet.arcscan.app`), `ARC_ADD_CHAIN_PARAMS` (chainId hex, chainName `Arc Testnet`, nativeCurrency USDC 18 dp, rpcUrls, blockExplorerUrls — matches viem `arcTestnet`), `isArcTestnet` (uses `arcTestnet.id` from `wagmi/chains` — **no `defineChain`**), `nativeGasSymbol` always `'USDC'`.
- `src/lib/wagmi-config.ts` — `createConfig({ chains: [arcTestnet], connectors: [injected()], transports: { [arcTestnet.id]: http('https://rpc.testnet.arc.io') } })`. **`injected` must be imported from the wagmi root** — `wagmi/connectors` in 3.7.6 pulls the tempo connector chain which fails webpack with a bare `accounts` module-resolution error (module not installed; the `@solana/*` tree it leaks is extraneous).
- `src/lib/wallet-context.tsx` — wagmi v3 hooks: `useConnection` (deprecated alias `useAccount`), `useConnect` (variables require a `connector` — `connectors[0]` from the hook), `useDisconnect`, `useSwitchChain` (`switchChainAsync({ chainId: 5042002 })` → catch → `provider.request({ method: 'wallet_addEthereumChain', params: [ARC_ADD_CHAIN_PARAMS] })` on `window.ethereum`). Exposes `{ address, chainId, onArcTestnet, connecting, connect, disconnect, switchToArc }` + `AccountBalancesProvider`.
- `src/app/providers.tsx` — `WagmiProvider` + `QueryClientProvider` (client-only; `QueryClient` in a `useState` initializer). Arc dynamic `WalletProviderInner` dropped.
- `HeaderWallet` — Connect / `0x…` truncate / wrong-chain amber dot + "Switch to Arc Testnet" CTA / "Gas: USDC" label. `WalletButton` (wallet) deleted (was never imported).
- `next.config.ts` — `transpilePackages: ['@creit.tech/Arc-wallets-kit']` removed (webpack fallbacks kept).
- `layout.tsx` — metadata `Chakra — Arc Testnet DEX Aggregator`; header brand Chakra; footer "Aggregated routing across Arc Testnet DEXs".
- **frontend `tsconfig.json` target ES2017 → ES2020** (BigInt literals). `.next`/`tsconfig.tsbuildinfo` had to be cleared after the change (stale incremental cache kept reporting ES2017).


### T6.3 Permit2 approve + sign + send + Arcscan + recent swaps + unaudited warning (2026-08-26, local complete / live blocked)

- **Pure modules (vitest, 29 new tests):**
  - `recent-swaps.ts` (7 tests): address- and chain-scoped local storage, newest-first, maximum 20 entries; Arcscan URL derived rather than stored.
  - `unaudited-ack.ts` (6 tests): versioned acknowledgement with an ISO timestamp and graceful missing-localStorage behavior.
  - `swap-send.ts` (16 tests): minimum 20 gwei fee, fee fallback, send parameters, paused/chain gates, signature splicing, and approval/permit calldata helpers.
- **UI components:**
  - `UnauditedModal.tsx` — one-time acknowledgement modal.
  - `RecentSwaps.tsx` — empty state plus token pair, split badge, and Arcscan link rows.
  - Settings gear icon wired to `SwapSettingsModal`.
- **SwapCard rewrite:** CTA states cover connect, chain switch, amount/route readiness, approval, Permit2 signing, send, pause, pending, and confirmation. Intended send pipeline: `buildSwapTx` → ERC-20 approval when required → typed-data signature when required → splice signature → send → wait one confirmation → save recent swap.
- **Live swap not claimed** (T5.2 deploy blocked). Planning box stays  with **local complete / live blocked**.

### T7.1 TypeScript SDK `quote` + `buildTx` (2026-08-25)

- `packages/sdk/src/index.ts` rewritten: `ChakraClient` → `ChakraClient`; Arc surface dropped (orders/DCA/stats/submit/XDR/trustlines/prices/account/classic_asset/ledger).
- `ChakraApiError extends Error { code }` — thrown on envelope `success:false`; code is `error.code` (NO_ROUTE / NOT_READY / PAUSED / INVALID_PARAMS / ZERO_AMOUNT / SAME_TOKEN / UNKNOWN_TOKEN / ROUTE_INVALID / RATE_LIMITED / RPC_ERROR; defaults `RPC_ERROR` when missing). Never stringifies the whole body.
- `quote(params)` — query `token_in`, `token_out`, `amount_in`, `slippage_bps` (via `slippageToBps(slippage?, slippageBps?)`; `slippage` percent → `Math.round(slippage * 100)`), optional `max_hops`/`max_splits`. Never sends `prefer_arc` or percent `slippage`. Maps response to camelCase (`priceImpactBps`, `protocolFeeBps`, `fractionBps`, `subRoutes`).
- `buildTx(params)` — POST body `{ user, token_in, token_out, amount_in, min_amount_out, sub_routes: [{ amount_in, steps }] }`; `quoteSubRoutesToSteps` maps `source.split(' → ')` → dex_type (`chakra-stable`/`stable` → `stable`, `chakra-clmm`/`clmm` → `clmm`, else `xyk`), `path[i]`/`path[i+1]` → token_in/out, `pool_addresses[i]` → pool_address. Result camelCased `{ to, data, chainId, value, deadline, typedData, requiredApprovals }`.
- `getBalances({ account })` → `{ erc20: {usdc, eurc, mbtc}, nativeUsdc }` — API shape as-is, native never summed.
- `listTokens`, `isHealthy` (`/api/v1/health`), `isReady` (`/api/v1/ready`).
- `examples/quote-build.ts` — USDC→EURC, `slippage: 0.5` → 50 bps; prints quote + `to`/`data`/`chain_id`/`value`/`deadline`/`typed_data`; `example not executed — API not up` when `/health` is down, `NOT_READY` handled as "aggregator unconfigured".
- `README.md` rewritten; `package.json` description updated; old examples deleted (`basic-usage.ts`, `stats.ts`, `browser-swap/`).
- `docs/openapi.yaml` — `BuildTxRequest.required` now includes `user` (+ description "0x EOA address (the `from` of the built transaction)"). Docs-only; no Rust handler change.
- `tsconfig.json` excludes `src/**/*.test.ts` from the build (tests run under vitest); vitest added to devDependencies.

### T6.2 swap workspace (quote-only, no send) (2026-08-25)

- **Deleted Arc-only frontend files** (see `Changed files` below). `lib/balance.ts`, `lib/routeDisplay.ts`, `lib/swap-selection.ts` removed in favor of `lib/decimals.ts`.
- `src/lib/decimals.ts` — Rust `usdc_max_atomic` port: `raw = ceil(gas_wei / 1e12)`, `with_margin = ceil(raw * 1.25)`, `buffer = max(with_margin, 100_000)`, `saturating_sub` (pure bigint, no floats); `formatErc20` (6 dp), `formatNativeUsdc` (18 dp), `isNativeSwapToken` (`native_usdc` / `eth` / `0x0`), `slippageToBps`, catalog constants (`USDC_ERC20_ADDRESS`, `EURC_ADDRESS`, `NATIVE_USDC_KEY`).
- `src/lib/swap-tokens.ts` — `filterSwapTokens` (drops native encodings, dedupes by address, falls back to `FALLBACK_SWAP_TOKENS` USDC+EURC), `SwapToken`.
- `src/lib/swap-settings.ts` — storage key `chakra:swap-settings` (old `Chakra.swapSettings` unread), `DEFAULT_SWAP_SETTINGS.slippage = 0.5`, `maxHops 3`, `maxSplits 5`.
- `src/lib/quote-format.ts` — `formatImpactPercent(bps)` (`12` → `0.12%`), `formatProtocolFeePercent` (always `0%`).
- `src/lib/quote-scheduler.ts` — `createQuoteScheduler({ debounceMs: 250, refreshMs: 5000, fetch })`: debounced `schedule()` + interval refresh that **skips when a fetch is in flight** (`inFlight` guard).
- `src/lib/aggregator.ts` — thin Chakra wrapper: `CHAKRA_API_URL` (`NEXT_PUBLIC_CHAKRA_API_URL`), `getQuote` (`slippage_bps`), `buildSwapTx` (`user` + `sub_routes[].steps` via `quoteSubRoutesToSteps`), `QuoteData`/`SubRoute` types (`price_impact_bps`, `fraction_bps`, no `dex_types`/`percentage`).
- `src/lib/account-balances-context.tsx` — `GET /api/v1/balances?account=`: `balances` keyed lowercase token address (usdc→USDC_ERC20_ADDRESS, eurc→EURC_ADDRESS), `nativeBalance` separate, never summed; `getErc20Balance`.
- `src/components/SwapCard.tsx` — single-column; catalog load (fallback on error); amount input (regex `^\d*\.?\d*$`); 25/50/75/Max chips (Max = `eth_gasPrice` × 400_000 gas wei → `usdcMaxAtomic`, floor-only when RPC fails); Circle faucet link when ERC-20 USDC/EURC balance is 0; quote via `getQuote` (slippage_bps from `slippageToBps`), 250 ms debounce + 5 s refresh (no overlap), fingerprint guard; CTA = Connect Wallet / Switch to Arc Testnet / Swap (coming soon) — **send disabled (T6.3)**; gas row from `nativeBalance` (USDC 18 dp label).
- `src/components/TokenSelector.tsx` — catalog from `/tokens` with hardcoded fallback, balance display via `getErc20Balance`, mBTC empty note "Buy mBTC via swap" (never a faucet).
- `src/components/RouteDisplay.tsx` — impact/fee rows (`price_impact_bps`, `protocol_fee_bps`), route summary (`chakra-stable` or `N paths`), expandable tabular legs (`source` venues from `source.split(' → ')`, `%` = `fraction_bps/100`, amounts in token decimals).
- `src/components/DisclaimerBanner.tsx` — "Arc Testnet · Contracts unaudited · …".
- `src/components/BuildTxCodeSample.tsx` — Chakra quote → build_tx sample (no Arc imports); `docs/ApiReference.tsx` + `docs/page.tsx` rewritten to the Chakra surface (`user`, `slippage_bps`, no deleted endpoints).

## Integration Points

- UI will use `NEXT_PUBLIC_CHAKRA_API_URL` (T6).
- Worker/API RPC env: `CHAKRA_RPC_HTTP` / `CHAKRA_RPC_WS`.
- API env: `CHAKRA_CORS_ORIGINS`, `CHAKRA_AGGREGATOR` (empty until T5.2), `CHAKRA_MBTC_ADDRESS`.

## Error Handling / Security

- `.env*` gitignored except `.env.example`.
- Never `--private-key` as a CI flag.

## Changed files (M1 + T2.1–T2.4 + T5.1 + T3.1–T3.3 + T4.2 + T4.3 + T4.4)

- `Cargo.toml`, `rust-toolchain.toml`, `.env.example`, `.gitignore`
- `crates/market-snapshot/src/{lib.rs,pool_state_store.rs,store.rs,decimals.rs,ready.rs,bootstrap.rs}` — `ready.rs` + `bootstrap.rs` new (T3.1); `pool_keys()` (T4.3)
- `crates/Chakra-config/src/aggregator.rs`
- `crates/api-server/{Cargo.toml,src/lib.rs,src/main.rs,src/config.rs,src/snapshot_loader.rs}` + `tests/{redis_snapshot_smoke_test,snapshot_quote_test}.rs` (T3.1 schema/defaults)
- `crates/api-server/` (T4.3 rewrite): `src/{lib.rs,main.rs,config.rs,state.rs,rate_limit.rs,handlers.rs}` rewritten; `src/{envelope.rs,catalog.rs,hydrate.rs,evm_balances.rs}` new; `src/{abi.rs,build_tx.rs}` new (T4.4); `src/{pool_hydrate.rs,snapshot_loader.rs,Arc_prepare.rs,orders.rs,stats.rs,swaps.rs,price_*.rs,prices.rs,arbitrage.rs,Arc_price.rs}` remain on disk but are **no longer wired** (dead modules; Arc surface dropped); `tests/chakra_rest_test.rs` + `tests/chakra_build_tx_test.rs` new; `tests/{redis_snapshot_smoke_test,snapshot_quote_test,build_tx_simulate_test,decode_user_tx}.rs` + `examples/verify_split_quote.rs` **removed** (Arc-era)
- `crates/market-data-worker/{Cargo.toml,src/main.rs,src/worker.rs,src/clmm_metrics.rs}` (T3.1 schema) + `src/{evm_watcher.rs,fetch_pipeline.rs,lib.rs}` (T3.3; `evm_watcher.rs` new)
- `crates/router-engine/src/path_finder.rs` (T4.1: Chakra default config + `pairs_from_chakra_snapshot` + `update_from_chakra_snapshot` + 8 new tests; T4.3: lowercase token normalization)
- `crates/router-engine/src/types.rs` (T4.2: `OptimalRoute.protocol_fee_bps`)
- `crates/router-engine/src/split_optimizer.rs` (T4.2: `protocol_fee_bps=0` on all literals + fee=0 and `max_splits=1` tests)
- `crates/router-engine/src/quote_engine.rs` (T4.2: `QuoteHydration.stable_pools`, `chakra-stable`/`chakra-xyk` EVM dispatch, `chakra-clmm` allowlist, SC-12 native guard, `update_from_chakra_snapshot` helper + 6 new engine tests; T4.3: test helper lowercase)
- `crates/api-server/src/pool_hydrate.rs` (T4.2: stable refs bucket + `fetch_stable` + `stable_pools` in `QuoteHydration`; `chakra-clmm` in CLMM bucket)
- `crates/dex-adapters/src/{clmm_math.rs,pool_index.rs,evm_quote_math.rs}` — `evm_quote_math.rs` new (T3.2); `clmm_math`/`pool_index` T3.1 schema
- `crates/dex-adapters/src/{evm_rpc.rs,evm_logs.rs,evm_fetch.rs}` — **new (T3.3)**; `Cargo.toml` + `lib.rs` for the new modules (T4.3: `test-fixture` feature + `eth_get_balance`)
- `contracts/evm/foundry.toml` (mixed solc + `libs = ["lib", "venues"]` + `bytecodes` fs read), `remappings.txt` (`forge-std/`, `@openzeppelin/`, `uniswap-v2/`, `uniswap-v3/`), `.gitignore` (`out/`, `cache/`, `broadcast/`, `lib/`)
- `contracts/evm/venues/` — vendored v2-core `v1.0.1` (GPL-3.0, `contracts/` + `LICENSE`) and v3-core `v1.0.0` (BSL-1.1, `contracts/` + `LICENSE`), `venues/README.md` (license table + solc routing)
- `contracts/evm/bytecodes/{v2-factory,v2-pair,v3-factory,v3-pool}.hex` — offline-compiled V2/V3 creation bytecode, committed (only `out/`/`cache/`/`lib/` are ignored)
- `contracts/evm/src/` — `Aggregator.sol` (T5.1), `MockErc20.sol`, `MockBtc.sol`, `VendorDeployer.sol`, `stable/{StableSwap,StableSwapFactory}.sol`, `interfaces/{IERC20Minimal,IStableSwap,IUniswapV2Factory,IUniswapV2Pair,IUniswapV3Factory,IUniswapV3Pool,IAllowanceTransfer,IStableSwapFactory}.sol`. `Placeholder.sol` **removed** (T5.1).
- `contracts/evm/test/` — `Aggregator.t.sol` (39 cases, T5.1), `MockPermit2.sol` (T5.1), `MockBtc.t.sol`, `XykFactory.t.sol`, `StableSwap.t.sol`, `ClmmPool.t.sol`. `Placeholder.t.sol` **removed** (T5.1).
- `contracts/evm/script/` — `DeployAggregator.s.sol` (T5.1), `DeployMockBtc.s.sol`, `DeployXyk.s.sol`, `DeployStable.s.sol`, `DeployClmm.s.sol` (all compile-only this session)
- `.env.example` — added `CHAKRA_PERMIT2`, `CHAKRA_USDC_ADDRESS`, `CHAKRA_EURC_ADDRESS` (T5.1); `CHAKRA_AGGREGATOR` (T4.3)
- `docs/openapi.yaml` (T4.3 rewrite: Chakra title, envelope, dropped Arc paths, two quote examples; T7.1: `BuildTxRequest.user` required), `docs/api-reference.md` (T4.3 rewrite)
- **T4.5 (`crates/router-engine`):** `pool_hydrate.rs` (factories field), `quote_engine.rs` (factory gate + 3 tests).
- **T6.1/T6.2 (`packages/frontend`):** `package.json` (deps wagmi/viem/react-query, removed Arc wallet deps, `test` script), `next.config.ts` (transpilePackages removed), `tsconfig.json` (ES2020), `vitest.config.ts` (new, `@` alias), `src/lib/{chain.ts,wagmi-config.ts,decimals.ts,swap-tokens.ts,quote-format.ts,quote-scheduler.ts}` (new), `src/lib/{wallet-context.tsx,aggregator.ts,account-balances-context.tsx,swap-settings.ts}` (rewritten), `src/app/{providers.tsx,layout.tsx,page.tsx}` (rewritten), `src/app/docs/{page.tsx}` + `src/components/docs/ApiReference.tsx` (rewritten to Chakra), `src/components/{SwapCard.tsx,TokenSelector.tsx,RouteDisplay.tsx,HeaderWallet.tsx,DisclaimerBanner.tsx,BuildTxCodeSample.tsx,HeaderNav.tsx}` (rewritten), tests `src/lib/{chain.test.ts,decimals.test.ts,swap-settings.test.ts,quote-format.test.ts,quote-scheduler.test.ts,swap-tokens.test.ts}` (new). **Deleted:** `app/{portfolio,stats,arbitrage}/`, `components/{LimitCard,DcaCard,OrderTypeRail,OpenOrders,HoldingsSummary,SwapHistory,SubmitViaToggle,WalletButton,CompareSection,FaqSection,Sparkline}.tsx`, `components/portfolio/*`, `lib/{trustline,limit-orders,rpc,wallet,swaps,useSwapHistory,prices,tokenDisplay,routeDisplay,balance,swap-selection}.ts` (+ `.test.ts`).
- **T6.3 (`packages/frontend`):** `lib/recent-swaps.ts` (new), `lib/unaudited-ack.ts` (new), `lib/swap-send.ts` (new), `components/UnauditedModal.tsx` (new), `components/RecentSwaps.tsx` (new), `components/SwapCard.tsx` (rewritten send pipeline), `components/SwapSettingsModal.tsx` (wired).
- **T7.1 (`packages/sdk`):** `src/index.ts` (rewritten `ChakraClient`), `src/client.test.ts` (new), `package.json` (vitest + test script + description), `tsconfig.json` (exclude tests), `README.md` (rewritten), `examples/quote-build.ts` (rewritten). **Deleted:** `examples/{basic-usage.ts,stats.ts}`, `examples/browser-swap/`.
- planning / testing / this file

## Deviations

- Dex adapters still Arc-named; compiling keep-set, not yet EVM (T3.2 — **T3.3 supersedes this for the worker path**: the Arc path never constructs Arc adapters, though the crates still compile).
- forge-std not vendored in git (`lib/` gitignored).
- **T3.1: versioned snapshot payload keys are `chakra:snapshot:data:{version}`**, not `chakra:snapshot:{version}` as drawn in the design Redis table — the `:data:` infix keeps `current`/`events`/`versions` from colliding with version strings. `chakra:snapshot:current`, `chakra:pool:*`, `chakra:factories`, `chakra:snapshot:events` match the design exactly.
- **T5.1: OpenZeppelin added** (`lib/openzeppelin-contracts` 5.7.0, gitignored like forge-std) for Ownable/Pausable/ReentrancyGuard/SafeERC20 — the Auth.sol fallback was not needed.
- **T3.3: pool index + watch lists accept EVM `0x` addresses** (lowercased) in addition to Arc `C…` ids — the Arc `is_contract_address` guard alone would have dropped every Arc pool.
- **T3.3: `spawn_arc_pipeline` constructs never-contacted Arc adapter stubs** (cheap constructors, no I/O) so the single fetch pipeline serves both paths without Option-wrapping its whole context; no Arc task is ever enqueued on the Arc path.
- **Uniswap V3 license:** plan draft said GPL-2.0-or-later, but upstream v3-core `v1.0.0` ships **BSL 1.1** — the vendored `LICENSE` is the true upstream file. (The GPL relicensing starts with later tags; if compliance ever requires GPL, vendor a GPL tag instead.)
- **Mixed solc required** (deviation from single global `solc = "0.8.30"`): V2 `=0.5.16`, V3 `=0.7.6`, our code `0.8.30` — enforced by `compilation_restrictions`; cross-version calls use `deployCode`/hex + 0.8.30 ABIs, never source imports.
- CLMM test seeds a **full-range** position (L=1e12) to verify mint/swap mechanics; the *tight in-range around spot* seed is a live-Arc step (T2.4 validate), not yet implemented.
- Stableswap fee is **on input** (ported Rust `get_dy` style), not Curve-vyper fee-on-output; `exchange` does not `transferFrom`.
- `IStableSwap` uses `seedLiquidity` (not an `addLiquidity()` 0-arg draft) to match `StableSwap.sol`.
- **T4.2 SC-2 deviation:** the plan's documented split size `180_000e6` is **refused** by the engine. The xy=k leg (~0.7% of the trade) fails the locked `max_leg_rate_deviation_bps=500` check (marginal rate ~0.89 vs full-size diluted quote rate ~0.0526 = 17×) and the 5 bps improvement floor; the engine returns the single `chakra-stable` route (correct best execution). No size passes both the rate filter and the improvement floor on the T2.3 seeds (brute-force verified to 2e12). Fix options (deferred): relax the rate filter for chakra-xyk, or deepen the xy=k seed — both out of T4.2 scope per the plan (no `SplitConfig` default / seed-depth changes). The OpenAPI 70/30 example at 100 USDC is a doc deviation for T4.3.
- **T4.2 `update_from_chakra_snapshot` on `QuoteEngine`:** the PathFinder already had one; the engine-level helper groups pairs by real source name (`chakra-xyk`, `chakra-stable`, …) so hop dispatch + hydration keys resolve — a naive single-source insert breaks both.
- **T4.3 OpenAPI 70/30 quote example is a doc deviation:** the plan-era sketch showed a 70/30 xy=k/stable split at 100 USDC; the honest engine returns a single `chakra-stable` route on the T2.3 seeds (SC-2 deviation). The OpenAPI keeps the split example but labels it **illustrative / not current engine output**; the honest example is the single-stable quote.
- **T4.3: `pairs_from_chakra_snapshot` normalizes token addresses to lowercase.** EVM RPC addresses are lowercase; the API lowercases request tokens. Mixed-case test constants (e.g. `EURC = 0x89B5…D72a`) no longer leak into the graph — without this, `/quote` looked up `0x89b5…` against graph nodes `0x89B5…` and found nothing.
- **T4.3: legacy Arc-era integration tests removed** (`snapshot_quote_test`, `redis_snapshot_smoke_test`, `build_tx_simulate_test`, `decode_user_tx`, `examples/verify_split_quote.rs`) — they referenced the deleted Arc handler surface. Arc modules (`Arc_prepare`, `orders`, `stats`, `swaps`, `prices`, …) stay on disk, compiling, but are not wired into the router (plan: leave unused modules compiling).
- **T4.3: loopback IP stays rate-limit exempt** (with `QUOTE_RATE_LIMIT_BYPASS_IPS` for extras) so local curl works; the 429 test injects a non-loopback `ConnectInfo`.
- **T4.3 (2026-08-26):** `AppState::from_env` now creates `RedisSnapshotStore` in cluster mode and loads the existing snapshot into `QuoteEngine` at startup (best-effort; empty Redis = engine starts empty). `load_snapshot` reads from `snapshot_store` in cluster mode. Config: `CHAKRA_REDIS_URL` → `snapshot_redis_url` (falls back to `SNAPSHOT_REDIS_URL`); `CHAKRA_LISTEN_ADDR` → `listen_addr` (falls back to `LISTEN_ADDR`); `max_splits` default 3→5. `/ready` in cluster mode now also checks engine graph is non-empty.
- **T4.3: `build_tx` is a stub until T4.4** — route + envelope + catalog validation, `to` from `CHAKRA_AGGREGATOR`, 503 `NOT_READY` when empty (T5.2 deploy pending). No `splitSwap` encode in T4.3.
- **T6.1: wagmi 3.7.6 `injected` must be imported from the `wagmi` root.** `wagmi/connectors` re-export resolves to the tempo connector chain whose `@wagmi/core/tempo` imports a bare `accounts` package that is never installed (the `@solana/*` tree it drags in is extraneous) — webpack fails `Module not found: Can't resolve 'accounts'`. The plan's "wagmi hooks" surface is unchanged; only the import path differs. `useConnection` is the wagmi v3 name for `useAccount`.
- **T6.1: frontend `tsconfig.json` target bumped ES2017 → ES2020** — BigInt literals (raw `400_000n`-style constants in `decimals`/`SwapCard`). Not a deviation from the plan (no target was specified), recorded for the diff.
- **T6.1: Arc wallet dep removal pulled the Arc-UI deletions forward.** Removing `@Arc/*` deps (T6.1) forces every importer handled in the same pass, so the T6.2 "delete Arc-only files" list landed with T6.1's green pass; T6.2's own red/green gate is the unit behaviors (decimals port, slippage 0.5, bps formatters, debounce, native-encoding filter). Both boxes are flipped with the full evidence.
- **T6.2: `SwapSettingsModal` retained but unwired** — the swap card shows the slippage preset inline ("Slippage 0.5%") and the modal is not opened from `SwapCard` in T6.2 (settings editing + maxSplits UI belongs to T6.3 polish). The modal still compiles and uses the Chakra settings module.
- **T6.2: `docs/ApiReference.tsx` + `docs/page.tsx` rewritten beyond the plan's file list** — they referenced deleted Arc endpoints (`/swaps`, `/prices`, `/submit_tx`, `user_public_key`) and would have sent dead calls from the try-it UI; rewritten to the shipped Chakra surface (`user`, `slippage_bps`, `/health`, `/ready`, `/tokens`, `/balances`, `/quote`, `/build_tx`).
- **T7.1: OpenAPI `user` field added docs-only.** The shipped handler already requires `user` (T4.3/T4.4); the OpenAPI schema was the only place missing it. No Rust changes.
- **T4.3: `cargo test -p api-server` requires `--features test-fixture`** (unlocks `dex_adapters::evm_rpc::fixture` for the balances test; never touches live Arc).

## Phase 7 Implementation Check (2026-08-26)

**Verdict:** **not aligned with the approved requirements/design; return to Phase 5 implementation.** This check reviewed the dirty `feature-chakra` worktree in place and intentionally made no production-code fixes. Existing local tests exercise useful units and fixtures, but several fixtures assert an implementation-private protocol that does not match the compiled Solidity/Permit2 interfaces.

### Blocking findings

| Severity | Area | Finding and impact |
|----------|------|--------------------|
| **Critical** | Stableswap custody | `StableSwap.exchange` derives a supposed pre-swap balance as `currentBalance - callerSuppliedAmount` but never proves the caller transferred that amount. An arbitrary caller can call `exchange` without a deposit and withdraw the other reserve from seeded pool liquidity. Indices are also not bounded to 0/1. The venue must track reserves or authenticate the actual balance delta before deployment, with an attacker-without-transfer regression test. |
| **Critical** | API production topology | `AppState::from_env` constructs an empty `QuoteEngine` and never loads `chakra:snapshot:current` or starts snapshot reload/poll/pub-sub. The old `snapshot_loader.rs` remains on disk but is no longer exported. `/ready` can return ready from Redis keys while the engine has no graph, so production `/quote` returns `NO_ROUTE`. Fixture tests inject a populated engine and mask this. |
| **Critical** | Cluster `/build_tx` | `build_tx::load_snapshot` only supports `memory_snapshot`; `AppState::from_env` sets that field to `None`, and the Redis branch always falls through to `no snapshot available for build_tx validation`. A deployed cluster cannot build a transaction. |
| **Critical** | Contract ABI | The API selector constant described eight arguments by flattening `Permit2Pull`; the compiled `Aggregator` accepts seven arguments with the outer tuple. `forge inspect Aggregator methodIdentifiers` gives **`0x2e3be0c1`**, while the API/UI/tests pinned **`0xcc03a3bc`**. **Fixed 2026-08-26:** canonical 7-arg signature, selector now `0x2e3be0c1`, Foundry round-trip test added. Calls generated by `/build_tx` hit the reverting fallback. The remaining routes/hops/Permit2 offsets are also non-canonical: static `Hop` elements are encoded as dynamic, extra offset words are inserted, multi-route offsets ignore earlier tails, and the six-word `PermitSingle` plus signature offset is emitted as twenty zero words. |
| **Critical** | Permit2 state and calldata | The API called Permit2's three-argument `allowance(user,token,spender)` with ERC-20 selector **`0xdd62ed3e`** instead of **`0x927da105`**. **Fixed 2026-08-26:** `permit2_allowance` now uses `0x927da105`., reads only amount, ignores expiration and nonce, and signs nonce 0. It also left the calldata `PermitSingle` as 20 zero words (non-canonical). **Fixed 2026-08-26:** `encode_permit2_pull` now emits the 6-word struct + offset. The unused frontend Permit2 helper also pins a wrong selector. |
| **Critical** | Token approval UI | `/build_tx` correctly returns `required_approvals[].spender = Permit2`, but `SwapCard` discards `spender` and encodes `approve(approval.token, amount)` on the token contract. First-time users approve the token address itself, not Permit2, so the required allowance is never granted. |
| **High** | Quote allowlist | The active `api-server::hydrate` never fetches `chakra:factories`; the fetch exists only in dead `pool_hydrate.rs`. Even if populated, `QuoteHydration::factory_allows_pool` checks only whether any record has the source name, not whether the current pool's stamped factory address matches that record. The test named `t45_unlisted_factory_pool_is_skipped` varies only a record address that production logic never reads, so it is a false positive. |
| **High** | Deployment config | `.env.example` supplies `CHAKRA_REDIS_URL` and `CHAKRA_LISTEN_ADDR`, while the API reads only `SNAPSHOT_REDIS_URL` and `LISTEN_ADDR`. Using the shipped example leaves the API disconnected from Redis and listening on the default `0.0.0.0:3100`, while the UI points to port 8080. |
| **High** | Acceptance/config | Locked `MAX_SPLITS=5` defaults to 3 in `AppConfig`. More importantly, the documented seed/config combination never produces a real router split, so SC-2 and the downstream on-chain split SC-4 remain unmet. |
| **High** | Frontend dependencies | Fresh `npm audit` reports six high-severity advisories, including installed Next.js 15.5.18 with a patched release available at 15.5.21 or newer. Resolve before a public deployment. |
| **Medium** | Public API memory | The per-IP sliding-window limiter prunes timestamps only when the same key returns and never removes empty IP buckets. A public service can accumulate unbounded map keys from rotating source addresses. |
| **Medium** | Baseline verification | The default `cargo test --workspace` command no longer compiles the API integration tests because they import `dex_adapters::evm_rpc::fixture` without enabling `api-server/test-fixture`; the feature-enabled workspace suite is green. `forge fmt --check` also reports diffs in 17 Solidity files, and `git diff --check` reports trailing whitespace in `docs/integrator-guide.md`. The historical green baseline claims below therefore do not describe the current worktree's default verification path. |

### Test gaps that allowed the blockers

- Decode `/build_tx.data` with the compiled `Aggregator` ABI or a standard ABI library; do not maintain a matching handwritten decoder for the handwritten encoder.
- Add a real contract-call integration test that sends API-produced calldata to a deployed `Aggregator` with both the existing-allowance and signed-Permit2 paths.
- ~~Make Permit2 RPC fixtures require selector `0x927da105`~~ **Done 2026-08-26.**
- Add first-time approval coverage that asserts the token transaction's calldata spender equals Permit2.
- Add a production-construction API test: publish a snapshot/pools/factories to Redis, call `AppState::from_env`, assert `/ready`, quote, then build a transaction.
- Add the no-transfer stable-pool drain regression before changing the venue accounting.
- Add a per-pool factory mismatch test where source names are identical but stamped factory addresses differ.

### Open requirements and evidence gates

- T2.1–T2.5 and T5.2 live deployments/discovery/seed state remain open; no Arc addresses or readable live reserves are recorded.
- SC-2 has no router split for the current seeds/config, and SC-4 has no on-chain transaction containing at least two sub-routes.
- SC-3/SC-7 MetaMask QA, SC-5 public health/ready/quote, SC-6 API/SDK smoke, SC-8 venue matrix, SC-9 clean-environment walkthrough, SC-10 API-process p95, and SC-11 live inclusion-to-Redis measurement remain open.
- Coverage/evidence-pack reporting and public UI/API/worker/Redis deployment remain open.

### Required correction order

1. Redesign `StableSwap.exchange` input accounting and add the exploit regression.
2. Replace handwritten transaction encoding with canonical ABI-generated encoding; repair Permit2 allowance decoding, nonce/expiration use, embedded `PermitSingle`, and frontend approval/signature handling.
3. Restore production snapshot bootstrap/reload and cluster snapshot access; make readiness reflect a usable engine.
4. Wire exact per-pool factory membership into quote hydration and align API env names/default `MAX_SPLITS` with the locked docs.
5. Add the missing cross-boundary tests, resolve dependency/hygiene findings, and rerun Phase 7 before Phase 8 testing or any live deployment.

## Phase 7 Implementation Re-check (2026-08-27)

**Verdict:** **not aligned with the approved requirements/design; return to Phase 5 implementation.** This re-check inspected the dirty `feature-chakra` worktree in place and made no production-code changes. The earlier StableSwap custody, production Redis startup/reload, cluster snapshot access, exact factory hydration, Permit2 allowance/packing, UI approval-spender, discovery scanner, split fixture, rate-limiter memory, formatting, and dependency-audit findings are resolved. The selector portion of T4.4 is fixed, but its nested ABI payload is not.

### Current findings

| Severity | Area | Finding and impact |
|----------|------|--------------------|
| **Critical** | T4.4 `/build_tx` ABI | `crates/api-server/src/build_tx.rs:76-99,188-201` encodes `SubRoute[]` and `Hop[]` with non-canonical extra offset words. A dynamic array starts with its length; the encoder instead writes `32`, so the contract reads 32 routes. `Hop` is a static five-word tuple and must be inline, but the encoder emits per-hop offsets. Multi-route offsets also ignore preceding route tails. The Rust test at `crates/api-server/tests/chakra_build_tx_test.rs:346-358` uses the same private layout, while `contracts/evm/test/Aggregator.t.sol:345-383` constructs fresh canonical data with `abi.encodeWithSelector` rather than executing Rust-emitted bytes. The selector `0x2e3be0c1` is correct, but transactions built by the API should fail Solidity ABI decoding before `splitSwap` executes. This blocks T4.4, T6.3, SC-3, SC-4, and SC-6. |
| **High** | Production CLMM routing/build | Worker discovery stores CLMM only in `clmm_pool_refs` (`crates/market-data-worker/src/evm_watcher.rs:347-376,436-446`), but `build_engine_from_snapshot` iterates only `snapshot.sources` (`crates/api-server/src/snapshot_loader.rs:30-50`), so a production-loaded engine never receives discovered CLMM edges. `ClmmPoolRefSnapshot` drops the factory (`crates/market-snapshot/src/lib.rs:75-105`), and `/build_tx` looks up factories only in `sources.pairs` (`crates/api-server/src/build_tx.rs:331-339`), so CLMM validation fails once `chakra:factories` is populated. Discovery probes both 30 bps and optional 5 bps pools, while `BuildTxStep` carries no fee and `step_fee_bps` always encodes CLMM as 30 bps. The required third venue cannot complete production quote-to-build flow; a discovered 5 bps route cannot be encoded correctly. |
| **High** | Frontend release build | `npm run typecheck` and `npm run build` fail: `qa.wallet.config.ts:24` uses unsupported Playwright `use.screenshotDir`, and `SwapCard.tsx:166` multiplies a possibly undefined `gasPriceWei`. The latter is also a runtime MAX-button bug because `undefined !== null` is true before the query resolves. Unit tests and ESLint pass, but the production bundle is not releasable. |
| **Medium** | T7.2 local harness | `cargo run -p api-server --example local_harness --features test-fixture` and feature-enabled all-target verification fail at `crates/api-server/examples/local_harness.rs:148`: the example still constructs the removed `AppState.engine` field instead of the current constructor/state shape. This contradicts the planning claim that the local quote + `build_tx` harness is green and blocks the local SC-6 walkthrough. |
| **Medium** | Route schema and validation | The locked quote shape carries per-hop `dex_types`/fee metadata, but `SubRouteData` exposes only a joined source string (`crates/api-server/src/handlers.rs:81-89`) and the UI/SDK reconstruct dex types heuristically (`packages/frontend/src/lib/aggregator.ts:63-79`). `/build_tx` also does not verify that submitted hop tokens are the tokens of the referenced snapshot pool, and `snapshot_has_pool` accepts a CLMM ref regardless of the submitted dex type (`crates/api-server/src/build_tx.rs:311-320`). Invalid or ambiguous client routes can pass API validation and become reverting contract calls. |
| **Medium** | MetaMask QA gate | `packages/frontend/qa/wallet/swap-critical-path.spec.ts` performs HTTP requests through Playwright's request client only. It never opens the dApp, loads/connects MetaMask, switches Arc, approves Permit2, signs EIP-712, submits a swap, waits for a receipt, verifies Arcscan/recent swaps, or sanitizes evidence. The file name and comments overstate coverage; SC-3/SC-7 remain wholly open. |
| **Medium** | EIP-1559 fee suggestion | `fetchSuggestedFee` returns only `feeHistory.reward[0][0]` as `maxFeePerGas` (`packages/frontend/src/lib/swap-send.ts:39-59`), omitting the base fee. With a 20 gwei base and nonzero priority fee this is floored back to 20 gwei, which can be below the inclusion price. Use a valid total fee suggestion (at least base + priority, with the locked 20 gwei floor) or `eth_gasPrice`. |
| **Low** | Repository hygiene | `scripts/__pycache__/test_discovery_scan.cpython-314.pyc` is untracked. Ignore/remove generated Python bytecode before preparing a commit. |

### Test gaps that still allow false-green claims

- Add one cross-language contract test that feeds exact bytes returned by Rust `encode_split_swap` into a deployed compiled `Aggregator` for single-hop, multi-hop, split, empty-signature, and signed-Permit2 cases. Decode with the compiled ABI or a standard ABI implementation, not another handwritten mirror.
- Build the production engine from a worker-shaped snapshot containing CLMM only in `clmm_pool_refs`; require a CLMM quote, exact factory membership, and correct 5/30 bps build encoding.
- Add `/build_tx` rejection tests for mismatched pool tokens, wrong dex type, wrong fee tier, and same-source/different-factory pools.
- Include `--all-targets --features api-server/test-fixture` in the Rust release gate so examples cannot silently rot.
- Make the frontend production build/typecheck a required gate, then implement the actual MetaMask browser flow rather than API-only checks.

### Open requirements and evidence gates

- T2.1-T2.4 live venue/token deployment and seeds, T5.2 Aggregator deployment, and T6.3 live send remain open.
- SC-2 lacks a checked-in benchmark evidence artifact; SC-4 lacks a live on-chain split transaction.
- SC-5 public UI/API, SC-7 MetaMask QA, SC-8 venue matrix, SC-9 clean/timed walkthrough, SC-10 API-process p95, and SC-11 live inclusion-to-Redis evidence remain open.
- These external gates are not evidence of code defects by themselves, but they prevent completion of the feature and any transition to final testing/review.

### Required correction order

1. Replace or repair the handwritten Rust ABI encoder and prove exact Rust bytes against the compiled Aggregator.
2. Preserve CLMM factory/fee topology through worker snapshot → production engine → quote → `/build_tx`, and validate exact pool token/type/factory/fee membership.
3. Restore green frontend typecheck/build and the local API/SDK harness.
4. Replace the API-only wallet spec with the locked MetaMask critical path; correct the total fee suggestion.
5. Rerun Phase 7. Only after local alignment should Phase 8 and the live/evidence gates proceed.

## Phase 5 execute batch (2026-08-28)

### T4.7 — explicit quote hop metadata (done)

- `router_engine::types::Path` gained per-hop `dex_types[]` / `fee_bps[]` / `factories[]` (serde-defaulted); `TokenGraph::Edge` carries `dex_type` + `factory`; `add_pair_meta` added (legacy `add_pair` keeps empty metadata). `TradingPair` gained `dex_type`.
- `pairs_from_chakra_snapshot` stamps `dex_type` from the snapshot (source-derived fallback for legacy JSON); `snapshot_loader::snapshot_pair_to_trading` same; CLMM refs always `clmm`.
- `SubRouteData` now emits `dex_types`, `hop_fees`, `hop_factories` (length == `pool_addresses`); `source` stays as a joined display string (deprecated for DEX inference).
- SDK + UI `quoteSubRoutesToSteps`: server `dexTypes` takes precedence; joined-source fallback for in-flight clients; `fee_bps` carried into `BuildTxStep` so `/build_tx` encodes the snapshot fee. `venueToDexType` understands `xylo`.
- `BuildTxCodeSample.tsx` and `qa/wallet/swap-critical-path.spec.ts` now consume `dex_types[]`/`hop_fees[]`.
- OpenAPI `SubRoute`/`BuildTxStep` schemas + `docs/api-reference.md` "Quote hop metadata (T4.7)" section; `dex_types` enum includes `xylo` (schema extensible without reopening).
- Tests: graph `test_paths_carry_per_hop_dex_type_fee_factory` / `test_legacy_edges_yield_empty_hop_metadata`; API `quote_emits_explicit_per_hop_dex_type_fee_factory` (stable/x yk/xyk legs, lengths, fees); SDK mapper tests (server-precedence, legacy fallback, fee passthrough).

### T4.6 remainder — snapshot-fee encoding + 5 bps tier (done)

- `encode_sub_route` now takes the snapshot: omitted `fee_bps` resolves **snapshot** fee first, then the venue default. `encode_split_swap` and `build_tx_data` thread the loaded snapshot through.
- 5 bps CLMM tier is representable end-to-end: `build_tx_omit_fee_encodes_snapshot_clmm_fee_not_default` (omitted → encodes 5, not 30) and `build_tx_encodes_and_validates_5bps_clmm_tier` (explicit 5 accepted/encoded; 30 rejected vs the 5 bps snapshot).
- Production `build_engine_from_snapshot` already consumes `clmm_pool_refs` (T4.3 reconciliation) — CLMM topology survives worker → snapshot → engine.

### T6.3 local release gates (done)

- `package.json` test script pins `NODE_ENV=development` — the session shell exports `NODE_ENV=production`, which loads react's production build (no `React.act`); testing-library `renderHook` crashed with `React.act is not a function` under production. With the pin: 66/66 tests.
- `fetchSuggestedFee`: **base + priority** from `eth_feeHistory` (was priority-only), `eth_gasPrice` fallback, 20 gwei floor — 3 new tests.
- Transaction-level `encodeApproveCalldata` test pins spender = Permit2 (not the token).
- Frontend `tsc` clean, `npm run build` exit 0, lint 0 problems. `qa.wallet.config.ts` screenshotDir issue did not reproduce (no `use.screenshotDir` in the config — the earlier finding was resolved by the config rewrite).

### T7.2 local harness (done)

- `local_harness.rs` fixture RPC gained the real ERC-20 `0xdd62ed3e` allowance arm (and the 3-word zeroed Permit2 allowance) — `/build_tx` previously failed with `unexpected eth_call selector 0xdd62ed3e`.
- Full SDK walkthrough against the harness: quote (T4.7 `dexTypes`/`hopFees`) → `buildTx` → calldata `0x2e3be0c1`, Permit2 typed data, `required_approvals`. Clean-clone timed walkthrough remains open (SC-6/SC-9).

### T-XYLO — scoped XyloNet hop (local code done; live redeploy operator-gated)

- **Solidity:** `interfaces/IXyloNet.sol` (`IXyloFactory.getPool(address,address)`, `IXyloPool.swap(tokenIn,tokenOut,amountIn,minOut,to,deadline)`); `Aggregator.sol` — `DexType.Xylo` appended (value 3), `_assertPool` Xylo arm, `_xyloOut` (forceApprove → `swap(..., address(this), block.timestamp)` → allowance reset to 0). `MockXylo.sol` test double; **gotcha**: during `createPool` the pool constructor's `msg.sender` is itself, so the factory forwards seed balances after CREATE.
- **Foundry:** 5 new Aggregator tests (happy path + allowance reset, unknown factory, USYC pool never matches, not usable as Stable hop, removeFactory gates) — 81/81 total.
- **Quote math:** `evm_quote_math::xylo_quote`/`xylo_gross` — exact `_getD`/`_getY` port from `Panchu11/xylonet-public` (raw amp ann=40000, `A_PRECISION=100` in the c/b terms, per-coin `dP` loop, `dy - 1`, 4 bps fee on output). Pinned to **same-block** live RPC vectors (getReserves + amp + both `calculateSwap` in one batch): 1e6 USDC→EURC 865542 (Rust 865543), 1e6 EURC→USDC 1154419 (Rust 1154420) — ±1 unit.
- **Worker:** `FetchTask::EvmXylo` + `coalesce` (`xylo`/`discovered:xylo`) + `fetch_xylo_state` (getReserves → stored reserves, A=200, fee 4); `FactoryConfig::parse` accepts `xylo`; `discover_once` Xylo arm (catalog pairs only — USDC/USYC never discovered).
- **Engine/API:** `local_xylo_quote` dispatch (`source == "xylo"`, stable bucket state); `hydrate.rs` collects `xylo` into the stable refs; `build_tx` `DexType::Xylo` (u8 3, fee 4, factory source `xylo`).
- **Router behavior tests:** small size (1e6) prefers `chakra-stable` (999599); Chakra-capacity size (4.5e6 USDC) routes `xylo` (deeper A=200 curve).
- **Operator-gated live steps:** aggregator redeploy (bytecode change), `addFactory(xylo)`, worker `CHAKRA_*` factory config, hosted smoke. `DeployAggregator.s.sol` gained `CHAKRA_XYLO_FACTORY`.

### Fresh verification (2026-08-28, nested repo)

- `cargo test -p market-snapshot -p market-data-worker -p router-engine -p api-server --lib --tests`: **153 tests, 0 failed** (17+12+14+11+10+17+36+48 across suites; api-server integration incl. build_tx 14).
- `cargo test -p dex-adapters --lib evm_quote_math`: **8 passed** (xylo vector pins + guards).
- `forge test` (contracts/evm): **81 passed / 0 failed** (Aggregator 45 incl. 5 Xylo, Stable 16, Xyk 8, Clmm 5, MockBtc 5, MockXylo 2).
- `cargo fmt --all` clean; `npx ai-devkit@latest lint --feature chakra`: **passed** (only the pre-existing `feature-chakra` branch-name check misses — the nested repo tracks `main` per the worktree layout).
- SDK: 14 tests + build. Frontend: **66 tests**, `tsc` clean, `npm run build` exit 0, lint 0 problems.
- Full workspace `cargo test --workspace` still fails to compile the legacy Arc bins/tests (`dex-adapters/src/bin/*`, `tests/Arc venue_3token_stableswap.rs`) — pre-existing excluded targets (Arc compile strip; the documented gate is the kept-crate set above).

## Evidence

### Phase 7 fresh verification (2026-08-27)

- `npx ai-devkit@latest lint` and `npx ai-devkit@latest lint --feature feature-chakra`: **passed**, exit 0 (the feature command resolves to `chakra`).
- `cargo check --workspace`: **passed**, exit 0.
- `cargo test --workspace --all-targets`: **passed**, exit 0. `cargo test --workspace --features api-server/test-fixture --lib --tests`: **passed**, exit 0; API 16 unit + 28 integration tests passed. These selectors intentionally exclude examples.
- `cargo test --workspace --all-targets --features api-server/test-fixture`, `cargo test -p api-server --features test-fixture`, and `cargo run -p api-server --example local_harness --features test-fixture`: **failed to compile**, exit 101, because `local_harness.rs:148` references removed `AppState.engine`.
- `forge fmt --check` and `forge build`: **passed**, exit 0. `forge test --offline`: **74 passed / 0 failed**, exit 0. The ordinary online `forge test -vv` process hit a local Foundry macOS system-proxy crash before tests; offline execution avoids that environment failure.
- `forge inspect Aggregator methods` confirms selector **`0x2e3be0c1`**. A canonical `cast calldata` reference places the `SubRoute[]` length at the array offset and inlines static `Hop` tuples, unlike the Rust encoder. The existing Rust test explicitly reads the length one word late, confirming it mirrors the private format.
- `cd packages/frontend && npm test`: **62 passed / 0 failed** across 10 files. `npm run lint`: **passed** with two QA warnings. `npm run typecheck`: **failed**, exit 1, at `qa.wallet.config.ts:24` and `SwapCard.tsx:166`. `npm run build`: compiled application code, then **failed**, exit 1, on the same TypeScript errors. `npm audit --audit-level=high`: **0 vulnerabilities**, exit 0.
- `cd packages/sdk && npm test && npm run build`: **12 passed / 0 failed**, TypeScript build passed.
- `python3 scripts/test_discovery_scan.py`: **8 passed / 0 failed**. `bash -n scripts/discovery_scan.sh`: passed.
- `git diff --check`: **passed**. The worktree remains intentionally dirty with 120 tracked paths changed plus untracked feature files; the generated `scripts/__pycache__/` artifact remains untracked.

### Phase 7 fresh verification (2026-08-26)

- `cargo test --workspace`: **failed to compile**, exit 101 — unresolved `dex_adapters::evm_rpc::fixture` imports in `chakra_rest_test.rs` and `chakra_build_tx_test.rs` because the required feature is disabled by the default command.
- `cargo test --workspace --features api-server/test-fixture`: **passed**, exit 0; `cargo test -p api-server --features test-fixture`: **25 passed / 0 failed**; `cargo build --workspace`: **passed**, exit 0.
- `forge test -vv`: **67 passed / 0 failed**, exit 0; `forge build`: **passed**, exit 0; `forge fmt --check`: **failed**, exit 1, with diffs in 17 source/interface/test/script files.
- `cd packages/frontend && npm test && npm run lint && npx tsc --noEmit && npm run build`: **passed**, exit 0 — 53 tests across 9 files, lint/typecheck/build green.
- `cd packages/sdk && npm test && npm run build`: **passed**, exit 0 — 12 tests across 2 files and TypeScript build green.
- `cd packages/frontend && npm audit --audit-level=high`: **failed**, exit 1 — 6 high-severity vulnerabilities, 0 critical.
- `git diff --check`: **failed**, exit 2 — trailing whitespace in `docs/integrator-guide.md:5-6`.
- `npx ai-devkit@latest lint` and `npx ai-devkit@latest lint --feature chakra`: passed after this document reconciliation.

- `cargo check --workspace` exit 0 (worktree, rustc 1.88.0).
- `cargo test -p market-snapshot redis_key_prefix_is_chakra` pass.
- `cargo test -p market-snapshot decimals` 4 passed.
- **`cargo test -p market-snapshot` (T3.1, 2026-08-25): 36 passed / 0 failed** — includes new `ready::tests` (2 cluster tests against a spawned local `redis-server`, 2 memory), `bootstrap::tests` (3 incl. real-Redis), stable/factory round-trips, legacy JSON defaults.
- `cargo test --workspace` (T3.1, 2026-08-25): all suites 0 failed; `cargo check --workspace` exit 0.
- `npx ai-devkit@latest lint --feature chakra` (T3.1): all checks passed.
- **`cargo test -p dex-adapters evm_quote_math` (T3.2, 2026-08-25): 6 passed / 0 failed** — xyk formula pin, xyk guards, integer price impact, **stable on-chain vector match (999550535/999451582/999352602)**, stable-deeper-than-xyk (SC-2 analog), stable bad-input guards. `cargo test --workspace` green; lint clean.
- `forge test -vv` (worktree, 2026-08-24): **29 passed, 0 failed** — Placeholder 1, MockBtc 5, XykFactory 8, StableSwap 10, ClmmPool 5. Exit 0.
- `forge test -vv` (worktree, 2026-08-25, T5.1): **67 passed, 0 failed, exit 0** — Aggregator 39 (new), MockBtc 5, XykFactory 8, StableSwap 10, ClmmPool 5. Placeholder removed.
- `forge build` exit 0 (includes `script/DeployAggregator.s.sol`).
- `grep -R prevrandao src venues test script` → no hits (original code clean).
- **`cargo test -p dex-adapters evm` (T3.3, 2026-08-25): 36 passed / 0 failed** — URL policy (Canteen/Alchemy reject, public+failover allow), fixture-server client (blockNumber/eth_call/eth_getLogs/failover/jsonrpc-error), log decoder (topic0 pins vs well-known Uniswap values, V2 Swap touch, Transfer never-touch, Sync/Mint/Burn touch, V2/V3/stable created-pool decodes, canonical sort, never-call × 12, selector pins), 0x pool index, fetchers (getReserves → xyk value incl. factory stamp, balanceOf → stable value, slot0+liquidity → clmm with preserved coverage + skip-if-incomplete, getPair/getPool discovery). Full crate: **116 passed / 0 failed**.
- **`cargo test -p market-data-worker` (T3.3, 2026-08-25): 23 passed / 0 failed** — incl. **`poll_refreshes_pool_store_after_fixture_swap_within_5s`** (fixture `eth_getLogs` Swap → EvmXyk fetch → memory store update, elapsed < 5 s = SC-11 local validation), **`ws_subscription_forwards_log_notification`** (real tokio-tungstenite server: `eth_subscribe` + notification → EvmLog), poll-with-empty-topology cursor warm, created-pool → topology upsert → later Swap touch, discovery fixture (catalog-only), config env mapping + Canteen reject, never-call watch filtering, coalesce EVM tasks.
- `cargo test --workspace` (T3.3, 2026-08-25): **all suites 0 failed** (market-snapshot 35, dex-adapters 116, market-data-worker 23, router-engine 29, api-server 5, Chakra-* …).
- `npx ai-devkit@latest lint --feature chakra` (T3.3, 2026-08-25): all checks passed.
- **`cargo test -p router-engine` (T4.1, 2026-08-25): 37 passed / 0 failed** — 8 new PathFinder cases (USDC→EURC xyk+stable, USDC→mBTC xyk+clmm, EURC→mBTC direct+2-hop via USDC, max_hops=1, unknown/same-token, catalog freeze, native-encoding, Chakra default config). `cargo test --workspace` green; lint clean.
- **`cargo test -p router-engine` (T4.2, 2026-08-25): 44 passed / 0 failed** — 7 new: `protocol_fee_bps_is_always_zero`, `max_splits_override_one_forces_single_path`, `quote_hydrates_chakra_stable_and_uses_evm_math` (vector `999_550_535`), `sc2_180k_split_is_refused_and_single_stable_wins` (documented deviation), `chakra_clmm_quotes_when_complete_and_skips_when_incomplete`, `native_usdc_encoding_is_rejected_as_swap_amount`, `usdc_to_mbtc_output_is_in_mbtc_8dp_atomic_units`. `cargo test -p api-server` 40/0 (hydrate struct change). `cargo test --workspace` all suites 0 failed; `cargo build --workspace` exit 0.
- **`cargo test -p api-server --features test-fixture` (T4.3, 2026-08-25): 17 passed / 0 failed** — 7 unit (envelope/rate-limit/config) + 10 integration (`tests/chakra_rest_test.rs`: envelope codes + no float impact, catalog freeze with decimals, hydrate routes + `999_550_535` pin + zero-RPC proof via panicking fixture, SC-2 refusal + control, ready/health lifecycle incl. snapshot-id + pool_keys, balances never-sum with fixture Multicall3 aggregate3 + native 99e18, 429 + exempt paths with non-loopback IP, CORS unlisted-origin no-allowlist, RPC policy Canteen/Alchemy reject + public/failover allow).
- **`cargo test --workspace` (T4.3, 2026-08-25): all suites 0 failed** (market-snapshot 39, dex-adapters 116, router-engine 44, market-data-worker 23, api-server 17 + …); `cargo build --workspace` exit 0; `npx ai-devkit@latest lint --feature chakra` all checks passed.
- **`cargo test -p api-server --features test-fixture` (T4.4, 2026-08-25): 25 passed / 0 failed** — 9 unit (incl. 2 abi selector pins) + 6 `chakra_build_tx_test` (selector + full ABI decode, ROUTE_INVALID × 3 no-re-quote, PAUSED, typed-data omitted when fully approved, PermitSingle when Permit2 allowance insufficient, NOT_READY) + 10 `chakra_rest_test`. `cargo test --workspace` all suites 0 failed; `cargo build --workspace` exit 0; `npx ai-devkit@latest lint --feature chakra` all checks passed.
- **`cd packages/frontend && npm test` (T6.1/T6.2, 2026-08-25): 24 passed / 0 failed** — chain 5 (arcTestnet gate, add-chain params, ETH→USDC copy), decimals 9 (Rust `usdc_max_atomic` vectors ×5, 6/18 dp formats ×2, native-encoding reject ×1, slippageToBps), swap-settings 3 (0.5 default, chakra key, load default), quote-format 2 (12 bps → 0.12%, fee 0), quote-scheduler 2 (250 ms debounce single fetch, 5 s refresh no in-flight overlap), swap-tokens 2 (native rows dropped, empty fallback), swap-selection 2 (kept).
- **frontend (T6.1/T6.2, 2026-08-25):** `npm run lint` 0 problems; `npx tsc --noEmit` 0 errors; `npm run build` exit 0 (static `/`, `/docs`, `/docs/api`).
- **Playwright CLI (frontend, 2026-08-25):** desktop 1280×800 header = Connect Wallet, Swap + Docs nav, Chakra brand; `find` on wallet/Portfolio/Limit/DCA/Arbitrage/ETH → no matches; mock routes (`/tokens`, `/quote`, `/balances`) → typed 1 USDC, quote panel shows impact `0.12%`, protocol fee `0%`, minimum `0.994552 EURC`, route `chakra-stable`; expanded legs show `Stable · 100%`; mobile 375×812 keeps the CTA visible.
- **`cd packages/sdk` (T7.1, 2026-08-25): `npm test` 12 passed / 0 failed** (6 new client tests: quote params incl. no `prefer_arc`/percent slippage, bps parse, buildTx `user` + steps body, 2-hop step mapper, envelope `.code` error, isHealthy path); `npm run build` (tsc) exit 0; `npx tsx examples/quote-build.ts` → `example not executed — API not up` (no local API running — no SC-6 live claim).
- `npx ai-devkit@latest lint --feature chakra` (2026-08-25, after T6.1/T7.1/T6.2): all checks passed.
- Live Arc broadcasts for T2.1–T2.5/T5.2 all **blocked** (no operator key in this environment; never `--private-key` on CLI); live SC-11 WS→Redis proof is **T9.6**.

## Canonical curated rebaseline (2026-08-29)

### Catalog: USDC / EURC / cirBTC

- `mBTC` is removed from the public release path. The frozen catalog is exactly ERC-20 USDC (`0x3600…0000`, 6 dp), EURC (`0x89B5…D72a`, 6 dp), and canonical **cirBTC** (8 dp).
- `MockBtc.sol` (mBTC) and all Chakra-owned XYK/stable/CLMM deployments remain in the repo **only** as deterministic chain-31337 fixtures for local Foundry/engine tests. The Arc operator workflow (`Deploy.s.sol`, `Seed.s.sol`, `arc-operator.sh`) can no longer deploy them; deploy scripts are restricted to the aggregator + venue registration.
- The aggregator's `mbtc` immutable and sweep target are replaced by canonical `cirbtc` (constructor arg + `_sweepCatalogTo`). The Foundry invariant `_assertCatalogZero` covers USDC/EURC/cirBTC.
- `/tokens` and `/balances` serve cirBTC at 8 decimals; PathFinder graph nodes are {USDC, EURC, cirBTC}; native USDC encodings never become nodes.
- cirBTC has no faucet and no Chakra mint: acquire it via the route itself (e.g. USDC → EURC → cirBTC). Canaries do not require a prefunded target-token balance.

### Venue manifest (default-on)

| Source id | Venue | Addresses | Scope |
|-----------|-------|-----------|-------|
| `xylo-stable` | XyloNet | factory `0x60EDeFB094B84BBC6430cc130B358A43Ba1979e2`, router `0x73742278c31a76dBb0D2587d03ef92E6E2141023`, pool `0x3DF3966F5138143dce7a9cFDdC2c0310ce083BB1` | USDC/EURC stable pool |
| `presto-hub` | Presto | hub `0x5794a8284A29493871Fbfa3c4f343D42001424D6` | USDC/EURC discovery only |
| `unitflow-v25` | UnitFlow V2.5 | factory `0xd67F63A4F26a497b364d1C82e6747Aec8B5743a5`, pair `0x268DC75517EaFc6e0D52666639529e5DAB8c9200` | EURC/cirBTC, 30 bps |

Watchlist (promotion candidates, never silently enabled): Lunex, UnitFlow V3, AchSwap/Arc Swap, Synthra. Excluded: LiftUp (artificial), Curve WUSDC wrapper, LI.FI (meta-aggregator). No healthy direct USDC/cirBTC venue exists — Chakra routes that pair atomically as USDC → EURC → cirBTC.

### Solidity aggregator surface

- `enum DexType { Xyk, Stable, Clmm, Xylo, Presto }` — **Xylo=3 and Presto=4 appended**; Xyk=0, Stable=1, Clmm=2 preserved. The Rust ABI encoder emits raw uint8 indices.
- `Hop{pool, dexType, tokenIn, tokenOut, fee}` shape preserved (static 5-word tuple → 160 bytes/hop in the tail).
- Owner config additions:
  - `setXyloRouter(address factory, address router)` — atomic Xylo factory/router pair; Xylo hops execute via the router's exact-input `swapExactTokensForTokens(amountIn, 0, path, address(this), deadline)` with the **request deadline**, aggregator recipient, then a post-call balance delta (no `calculateSwap`-style pool ABI in the execution path).
  - `addPrestoHub(address hub)` / `removePrestoHub` — hub allowlist; Presto hops call `swap(tokenIn, tokenOut, amountIn, 0, deadline)` with exact temporary approval + allowance reset + post-call balance delta.
  - `setFactoryFee(address factory, uint24 feeBps)` — per-XYK-factory fee (UnitFlow V2.5 = 30 bps); `_xykOut` uses the factory's configured fee instead of a hardcoded 997/1000.
- Sweep: `_sweepCatalogTo` covers USDC, EURC, cirBTC. New non-upgradeable deploy (constructor drops `mbtc` for `cirbtc`).

### Quote math / adapters

- `xylo_quote`: keep the `calculateSwap` port but read the **amplification from hydrated on-chain pool parameters** (fetch `amp`/`A` from the pool at hydrate time) rather than hardcoding A=200 from the documentation.
- `presto_quote`: Presto's published **normalized hub formula** (normalized balances × hub invariant), new in `evm_quote_math`.
- `unitflow-v25`: reuse existing XYK state/math with the factory fee (30 bps); parity-pinned against `getAmountsOut`.
- Split optimizer: reject any split plan whose sub-routes **share a pool** (prevent two paths from independently overestimating the same downstream UnitFlow liquidity).

### Worker / discovery / API

- Worker manifest sources: `xylo-stable`, `presto-hub`, `unitflow-v25` (stable ids through snapshot → API → SDK → UI).
- Startup discovery verifies each manifest venue: bytecode presence, canonical token endpoints, factory membership, nonzero reserves, and a successful probe quote. Failed venues become unavailable → `NO_ROUTE`; **never auto-reseeded**.
- `/tokens`, `/balances`, OpenAPI, SDK types, and UI route legs all use cirBTC (8 dp) and the stable source ids; REST request shapes are preserved.
- WUSDC and any other wrapper identity are excluded from v1.

### Verification gates (fresh, 2026-08-29)

- `npx ai-devkit@latest lint --feature chakra`
- `forge test -vv` (contracts/evm)
- `cargo test --workspace` (+ `--features api-server/test-fixture` for API integration)
- Frontend `npm test` / `npm run typecheck` / `npm run build`
- SDK `npm test` / `npm run build`
- `git diff --check`

### Phase 5 Xylo + Aggregator Redeploy + Hosted Verification (2026-08-28)

- **XyloNet Integration (T-XYLO):**
  - `contracts/evm/src/Aggregator.sol`: `DexType.Xylo` (3), `_xyloOut` (`forceApprove`, `swap(..., address(this), deadline)`, reset allowance), `_assertPool` (`getPool(in,out) == pool`).
  - `contracts/evm/src/interfaces/IXyloNet.sol` and `contracts/evm/src/MockXylo.sol` added.
  - `contracts/evm/test/Aggregator.t.sol`: 5 new Xylo tests (happy path swap, unknown factory revert, USYC pair block, not usable as stable hop, `removeFactory` gating). Forge suite: **81 passed / 0 failed**.
  - `crates/dex-adapters/src/evm_quote_math.rs`: `xylo_quote` (A=200, 4 bps fee on output) pinned to live same-block RPC vectors (`calculateSwap(1e6 USDC→EURC) = 865542`).
  - `crates/market-data-worker/src/evm_watcher.rs`: `FactoryConfig::parse` fixed so `dex_type == "xylo"` maps to `source: "xylo"` for both seed and discovery.
  - `crates/market-data-worker/src/fetch_pipeline.rs`: Added `chakra-xylo` coalesce fallback to `EvmXylo`.
  - `crates/router-engine/src/quote_engine.rs`: Dispatches `source == "xylo"` to `local_xylo_quote`.
  - `crates/api-server/src/build_tx.rs`: `DexType::Xylo` mapped to enum value 3 with 4 bps fee.
  - Workspace tests: `market-snapshot` 36 passed, `market-data-worker` 17 passed, `router-engine` 48 passed, `dex-adapters --lib` 78 passed, `api-server` tests 25 passed.
  - SDK (14 passed) and Frontend (66 passed) tests green.
- **Aggregator Redeployment (T5.2):**
  - Broadcast to Arc testnet via `scripts/arc-operator.sh --broadcast script script/DeployAggregator.s.sol`.
  - New Aggregator address: `0xEa1b2C24bd41163590960F8e40afe6cb4CC92006` (Tx hash: `0x4cef6ba6e6d7132a7517666b2ce6c1ab7f5ae882ca9c80bb82ad9658ab71a22d`).
  - On-chain properties verified:
    - Codesize: 22,258 hex chars (11,128 bytes, was 10,139 bytes).
    - Owner: `0x12E266744f6d25D372000e066eCc0DF5a752276d`.
    - Paused: `false`.
    - `factoryDexType` allowlists: Xyk (`0x0c81...`)=0, Stable (`0x77Ce...`)=1, Clmm (`0xf6dE...`)=2, Xylo (`0x60ED...`)=3.
- **Hosted Deployment & Verification (Render):**
  - Commits `1e810b6` and `6d9cc78` pushed to `chakra/main`.
  - Render service `srv-da8g4non74is73ds1jgg` updated with new `CHAKRA_AGGREGATOR` and `CHAKRA_SEED_FACTORIES`/`CHAKRA_DISCOVERY_FACTORIES` containing `0x60EDeFB094B84BBC6430cc130B358A43Ba1979e2:xylo`.
  - Deploy `dep-da8j4g8ae00c73d3j4cg` successfully rolled out to `live`.
  - Live API smoke tests (`https://chakra-api-0a5i.onrender.com`):
    - `GET /api/v1/health`: 200 OK
    - `GET /api/v1/ready`: 200 ready:true
    - `GET /api/v1/quote` (1e6 USDC→EURC): 200 OK, returns 996,915 via `chakra-stable` (`dex_types: ["stable"]`)
    - `GET /api/v1/quote` (5e6 USDC→EURC): 200 OK, returns 4,680,042 routing `xylo` (`dex_types: ["xylo"]`)
    - `GET /api/v1/quote` (1e6 USDC→mBTC): 200 OK, returns `NO_ROUTE` error
    - `POST /api/v1/build_tx`: 200 OK, returns calldata with `to: "0xea1b2c24bd41163590960f8e40afe6cb4cc92006"` matching the newly deployed Aggregator contract.

### Clean-Clone SDK Walkthrough & UI Verification (2026-08-28)

- **Clean-Clone SDK Walkthrough (T7.2 / SC-6 / SC-9):**
  - Executed in `/tmp/chakra-clean-clone-t72` cloning `https://github.com/mangekyou-labs/chakra.git`.
  - Tested hosted API liveness and readiness (`https://chakra-api-0a5i.onrender.com`).
  - Installed `@Chakra/sdk` dependencies, compiled with TypeScript (`npm run build`).
  - Executed `npx tsx examples/quote-build.ts` with `API_URL=https://chakra-api-0a5i.onrender.com`.
  - Quote received: 1e6 USDC → 996,915 EURC via `chakra-stable` (`dex_types: ["stable"]`, `pool: 0xe4a881f4211b5cc11d8298032136a0d72e93cb02`).
  - `build_tx` output verified: `to: "0xea1b2c24bd41163590960f8e40afe6cb4cc92006"`, calldata selector `0x2e3be0c1`, `chainId: 5042002`, `value: "0"`, PermitSingle EIP-712 typed data present.
  - Total clean-clone elapsed time: **6 seconds** (well within the ≤30 minute gate).
  - Evidence logged in `docs/evidence/chakra-t72-walkthrough.json`.
- **Frontend UI Verification (T6.3 / T8.2):**
  - Verified `https://chakra-arc-dex.vercel.app` using `playwright-cli`.
  - Tested desktop (1280×800) and mobile (375×667) viewports.
  - Verified banner, unaudited disclaimer, SwapCard layout, responsive menu on mobile, and live API connection.
  - Full on-chain wallet swap pending MetaMask extension connection.

### T9 Grant-Style Evidence Pack (2026-08-28)

- **Venue Routing Matrix (T9.1 / SC-8 — Partial):**
  - Evaluated 15 matrix points across 5 pairs (USDC↔EURC, USDC↔mBTC, EURC↔mBTC, mBTC↔USDC) and 3 sizes each on live hosted API (`https://chakra-api-0a5i.onrender.com`).
  - Reclassified envelope errors: USDC↔EURC pairs are routable across `chakra-stable` and `xylo`; mBTC pairs return `NO_ROUTE` due to under-seeded testnet pool balances and incomplete CLMM ticks. Live 3-pair routing remains open gated on T2.1–T2.4 liquidity re-seed.
  - Evidence logged in `docs/evidence/chakra-t91-venue-matrix.json`.
- **Split vs Single-Path Benchmark (T9.2 / SC-2 / SC-8):**
  - Live benchmark of 5e6 USDC→EURC (5 USDC input) proves split optimizer outputs 4,680,269 atomic units (~4.68 EURC) vs 4,296,582 atomic units (~4.30 EURC) for best single venue (`xylo`), delivering **+383,687 atomic units (+0.3837 EURC / +893.01 bps / +8.93%)** improvement for the user.
  - Evidence logged in `docs/evidence/chakra-t92-split-benchmark.json`.
- **Quote Latency p95 Benchmark & API-Boundary Instrumentation (T9.5 / SC-10):**
  - Added API-boundary timer (`start_time.elapsed().as_millis()`) in `crates/api-server/src/handlers.rs` (commit `d3f8c79`), deployed to Render (`dep-da8jk6cs728c73bvdrb0`), and verified live.
  - Measured 100 consecutive requests to `GET /api/v1/quote` on hosted API. Server-side `compute_time_ms` p95 = **23 ms** (min 9 ms, avg 13.16 ms, median 11 ms, p90 18 ms, max 74 ms; gate < 500 ms).
  - Evidence logged in `docs/evidence/chakra-t95-quote-latency.json`.
- **UX & Accessibility Audit (T9.8 / SC-3 UX — Partial):**
  - Audited desktop (1280×800) and mobile (375×667) viewports on live Vercel UI.
  - Measured real DOM contrast ratios: Heading 'Swap' (16.62:1) and Amount Input (15.03:1) pass WCAG AA/AAA; Status Banner (4.07:1) and Sell Label (3.42:1) fall below standard 4.5:1 threshold; Disabled CTA (2.64:1) is exempt.
  - Captured real browser `Tab` (14 focusable steps) and `Shift+Tab` keyboard traversal sequence via `playwright-cli press`.
  - Second-wallet (Rabby/Coinbase) EIP-6963 spot check and live on-chain confirm UI state remain open pending browser wallet extensions.
  - Evidence logged in `docs/evidence/chakra-t98-manual-ux-a11y.json`.
- **Master Evidence Pack Index (T9.7 — Partial):**
  - Compiled grant catalog `docs/evidence/README.md` indexing all 13 Success Criteria (SC-1 through SC-13), public URLs, on-chain contract deployments, and artifact files with explicit open/gated criteria tracking.

### Phase 5 Continuation Tasks (2026-08-28)

- **Production CLMM Loader & Quoter (T4.3):**
  - `build_engine_from_snapshot` verified consuming `snapshot.clmm_pool_refs` (T4.6).
  - Added production-path integration test `ready_and_clmm_only_snapshot_quotes_and_builds` in `crates/api-server/tests/chakra_rest_test.rs`:
    - Verifies `/api/v1/ready` returns 200 OK with `ready: true` and snapshot ID for worker CLMM-only snapshots.
    - Verifies `/api/v1/quote` quotes CLMM pool with `dex_types: ["clmm"]`, `hop_fees: [30]`, and `hop_factories: [CLMM_FACTORY]`.
    - Verifies `/api/v1/build_tx` encodes calldata targeting `Aggregator` (`0xEa1b2C24bd41163590960F8e40afe6cb4CC92006`) with `value: "0"`.
  - All 53 tests in `api-server` and 196 tests across active workspace crates passing.
- **Factory Discovery Scanner (T2.5):**
  - Updated `scripts/discovery_scan.sh` to auto-window `fromBlock` to latest 10,000 blocks (`CHAKRA_SCAN_FROM_BLOCK`) to comply with public Arc RPC node `eth_getLogs` block range limits.
  - Fixed bash log count extraction.
  - Verified unit test suite: 8/8 tests pass in `scripts/test_discovery_scan.py`.
  - Executed live scan on Arc testnet across seeded/discovery factories: read-only, 0 errors, no allowlist mutations.
- **Theme WCAG AA Contrast Fix & Re-Audit (T9.8):**
  - Added red unit test `packages/frontend/src/lib/contrast.test.ts` asserting >= 4.5:1 contrast against `--bg-0` (`#0a0b0d`) and `--surface-raised` (`#1a1f27`).
  - Raised `--text-muted` in `packages/frontend/src/app/globals.css` from `#6b7280` to `#848fa0`.
  - Unit tests green (67/67 passing in frontend vitest; TypeScript clean).
  - Re-audited via `playwright-cli`: Status Banner contrast raised from 4.07 to 6.02 (PASS); Sell/Buy Label contrast raised from 3.42 to 5.06 (PASS); Slippage Header 5.59 (PASS). Captured desktop and mobile audit screenshots.
  - Updated `docs/evidence/chakra-t98-manual-ux-a11y.json`.
- **MetaMask E2E Harness Implementation (T9.4):**
  - Rewrote `packages/frontend/qa/wallet/swap-critical-path.spec.ts` using `@tenkeylabs/dappwright` to automate real MetaMask extension interactions on Chromium.
  - Supports mnemonic seed phrases and raw private keys, adds/switches to Arc Testnet (`5042002`), connects, quotes, handles Permit2 approvals and EIP-712 PermitSingle signing, confirms swap tx with `value = 0n`, and verifies Arcscan link and `localStorage` recent swaps.
  - Automatically skips cleanly when `QA_WALLET_SECRET` is unset (`1 skipped`, exit 0).
  - Added MetaMask extension pre-caching to `scripts/qa-wallet-setup.mjs`.
  - Updated default endpoints in `scripts/qa-wallet-validate.mjs`.
  - Created `docs/qa-playwright-metamask.md`.
  - ESLint (0 errors, 0 warnings) and TypeScript typecheck clean across frontend.

### Phase 5 Live Execution — T9.3 / T9.4 / T9.6 + QA harness fixes (2026-08-28)

- **T9.3 on-chain split swap (LIVE):** Operator executed the hosted `/build_tx` calldata (5e6 USDC→EURC) → `0x42e85916ade38b87ef0440ef71d8f3330075ecf2a481247dc2ac33376b287fa8` (block 59271873, status 1). 3 sub-routes in one tx (2× xylo + 1× chakra-stable), aggregator `Swap` event `isSplit=1`, 4,674,618 EURC credited. **Findings:**
  - `forge script --simulate` (and the dry-run implied by `--broadcast`) reverts locally on Arc's USDC system precompile `0x1800…0000` (native precompile stack-underflow; `vm.mockCall` intercepts direct calls but not the USDC proxy's delegatecall path) — broadcast required `--skip-simulation`.
  - `/build_tx` omits `typed_data` once the Permit2 allowance is pre-set (empty-signature permit path); `ExecuteSplitSwap` sets ERC-20 `approve(Permit2)` + `Permit2.approve(USDC, aggregator)` first. The 120 s `sigDeadline` window means calldata must be broadcast promptly.
- **T9.4 MetaMask swap (LIVE):** `swap-critical-path` exit 0 (38.3 s) → tx `0xa630da3c842d7613ebbbd4d8f66749892a4e42c510933e0e1c3f4966907ef0dd` (block 59292405, status 1): 1e6 USDC → 864,471 EURC via xylo. Sequence: mnemonic bootstrap → connect → header-menu switch → quote → Swap → unaudited ack → swap confirm → `Swap confirmed!` + Arcscan link + `chakra:recent-swaps:5042002:{addr}`.
- **QA harness / UI fixes (T9.4 prep):**
  - **UI token-default race (real bug):** `SwapCard` defaults ran on the FALLBACK catalog render (mixed-case `EURC_ADDRESS`), then the lowercased API catalog made `tokenOut`'s exact-match `find` fail → Buy token selector unmounted (`tokenOut` null) → "No route available". Fixed with case-insensitive address matching in `SwapCard` (tokenIn/tokenOut) and `TokenSelector` (`exclude`).
  - **dAppwright 2.13.12 + MetaMask 13.17:** `confirmNetworkSwitch` is a no-op stub; `addNetwork` drives stale extension UI selectors and can close the context. Network switch now drives the MetaMask `notification.html` popup Confirm directly via `context.pages()`.
  - `qa:wallet:validate/setup` moved to `packages/frontend/qa/wallet/` (npm scripts were resolving `../scripts/` to `packages/scripts/`); `qa.wallet.config.ts` loads `packages/frontend/.env` via `process.loadEnvFile`; validator loads the same file and prints only the secret shape.
  - Disposable QA wallet `0xc603C3…dCE76` funded from operator (6e6 ERC-20 USDC + 6 native USDC) via keystore-backed `cast send` (key never on argv); `FundQaWallet.s.sol` helper added.
- **T9.6 (live finding):** Worker publishes snapshots on the 600 s discovery cycle (two consecutive snapshot IDs 599.9 s apart); `/ready` always reports `pool_keys: []` despite working quotes; per-swap pool-key refresh is **not observable via the public API** (Redis is private). SC-11 live gate needs a metrics endpoint or Redis access.
- **Vercel:** `vercel --prod` upload repeatedly failed with `AbortError` (CLI network issue); the T9.4 run used the locally served fixed build (`DAPP_URL=http://localhost:3000`, hosted API baked via `NEXT_PUBLIC_CHAKRA_API_URL=https://chakra-api-0a5i.onrender.com npm run build`). The UI fix is **not yet deployed to Vercel** — needs a `vercel --prod` retry or push-driven deploy.

## Phase 7 Implementation Check (2026-08-29)

**Verdict:** **not aligned** with the approved 2026-08-29 requirements/design (canonical curated catalog + XyloNet / Presto / UnitFlow V2.5). Return to **Execute Plan** for the Major venue-discovery, source-id, operator-script, and public-doc leftovers. Do **not** start `dev-testing` / `dev-review` until those are closed or the design is explicitly reconciled. This check made **no production-code changes** and did **not** flip planning T2.1–T2.4 / T3.3 checkboxes.

**Workspace:** `.worktrees/feature-chakra` (`feature-chakra`), HEAD `208d5ff` `feat(chakra): rebaseline audit fixes — canonical source ids, operator deploy, cirBTC cleanup`. Dirty tree at check time: only uncommitted `docs/ai/deployment/2026-08-20-feature-chakra.md` (authorized hosted cutover notes) — **not reverted**. Task CLI is unavailable (`npx ai-devkit@latest task` → `unknown command`); no tracing events emitted.

The 2026-08-26 / 2026-08-27 **Critical** blockers (StableSwap custody, production Redis snapshot load, cluster `/build_tx`, ABI selector/packing, Permit2 selector/PermitSingle, UI approve-spender) are **resolved in code** at this HEAD. This check is against the **2026-08-29 rebaseline**, not those earlier Crits.

### What shipped and matches design

| Area | Evidence |
|------|----------|
| Catalog | Runtime freeze is ERC-20 USDC `0x3600…0000` (6 dp), EURC `0x89B5…D72a` (6 dp), cirBTC `0xf0C4…32BF` (8 dp). `market-snapshot` `graph_nodes()` excludes native USDC. OpenAPI `/tokens` and UI fallback catalog match. Aggregator constructor + `_sweepCatalogTo` sweep usdc/eurc/cirbtc. `splitSwap` is not payable; `receive`/`fallback` revert DirectEth. |
| Aggregator surface | `enum DexType { Xyk, Stable, Clmm, Xylo, Presto }` with Xylo=3, Presto=4. Hop is the static 5-word tuple. `_xyloOut` is router `swapExactTokensForTokens` + temp approve + reset + balance delta. `_prestoOut` is hub `swap(..., 0, deadline)` + temp approve + reset + delta. `_rejectSharedPools` present; Foundry `test_split_reuses_pool_reverts` passes. `_xykFeeFor` default 30 bps; `test_xyk_factory_fee_is_configurable` passes. |
| Split optimizer | `optimize` drops candidates that share a pool with a kept better-output path (`split_optimizer.rs`); unit `test_shared_pool_paths_are_reduced_to_best_single`. |
| Xylo quote | `fetch_xylo_state` hydrates `getAmplificationParameter()` / `A_PRECISION=100` into `state.a`; live hop uses `xylo_quote_with_a(..., state.a)`. Amp fail → `a=0` (quote 0), not a silent A=200 fallback. |
| Snapshot / ABI / Permit2 | `AppState::from_env` loads Redis via `snapshot_loader`; `/quote` reloads on version pointer change. Hosted `/build_tx` selector `0x2e3be0c1`. Permit2 AllowanceTransfer `0x000000000022D473030F116dDEE9F6B43aC78BA3`. |
| No auto-reseed | Failed venue is skipped / `NO_ROUTE`; worker never auto-reseeds. |
| Live catalog + encoder pin | Hosted API after cutover serves cirBTC catalog; `/build_tx.to` = `0xeb12351602c56d47c4ee955193335848952b29d8`. |

### Findings

| Severity | Area | Finding and impact |
|----------|------|--------------------|
| **Major** | Worker Presto discovery | `discover_once` (`evm_watcher.rs` ~341–447) match arms are `xyk` / `stable` / `xylo` / `clmm` only. Seeded `:presto` can pass bytecode verification and publish to `chakra:factories` with **zero pools**. Live USDC→EURC is **Xylo-only**; Presto never enters topology. |
| **Major** | UnitFlow source id | `FactoryConfig::parse` maps seeded `:xyk` → `chakra-xyk` (test at `evm_watcher.rs:1182` pins this). Comment claims `unitflow-v25`; code never emits it. Quote engine accepts both ids; `/build_tx` `step_factory_source("xyk")` → `chakra-xyk`. Design/manifest id `unitflow-v25` is not the worker stamp. |
| **Major** | Venue verification vs T3.3 | Startup check is **bytecode-only** (`eth_getCode`). Design wants bytecode, canonical endpoints, factory membership, nonzero reserves, and a probe quote. Failed venues become unavailable (match); the four extra checks are missing. |
| **Major** | Operator leftovers | `Deploy.s.sol` / `Seed.s.sol` still `require` chain `5042002` and can mint/deploy mBTC + owned factories. `DeployMockBtc.s.sol` still broadcasts `new MockBtc()`. `scripts/arc-operator.sh` has **no script allowlist** — any `script <path>` is runnable. Policy is docs-only. Planning local-release-gate overclaimed "operator workflow restricted to `DeployAggregator.s.sol`". |
| **Major** | Public integrator docs | `README.md` still lists `CHAKRA_MBTC_ADDRESS`. `docs/integrator-guide.md` still teaches mBTC as a catalog token. Runtime `/tokens` is correct; public docs are not. |
| **Major** | `render.yaml` | Still pins aggregator `0xEa1b2C24bd41163590960F8e40afe6cb4CC92006`. Hosted Render env was updated to `0xeb1235…29d8`; a yaml-driven redeploy would roll the encoder target back. |
| **Major** | Quote factory gate | `quote_engine.rs` ~555–568 gates **only** `source.starts_with("chakra-")`. Curated ids `xylo-stable` / `presto-hub` / `unitflow-v25` skip the factory membership check at quote time. `/build_tx` still checks snapshot + factory membership. |
| **Major** | SDK/UI dex-type fallback | `venueToDexType` maps `xylo-stable` / `xylo` → `'stable'` (`packages/sdk/src/index.ts` 147–148, `packages/frontend/src/lib/aggregator.ts` 99–100). Live quotes send `dex_types: ["xylo"]` so the server wins today; a client reconstructing hops from `source` alone would encode a Stable hop against a Xylo pool. |
| **Medium** | Unused `IXyloPool` | `Aggregator.sol` imports `IXyloPool` and never uses it. Execution is router-only. |
| **Medium** | `setXyloRouter` | Mapping-only (`xyloRouterForFactory[factory] = router`). Hop-time still requires factory allowlisted as Xylo **and** a router set (comment calls that atomic). Design bullet also still lists pool `IXyloPool.swap` as an alternate path; code is router-only. |
| **Medium** | Presto hydrator | Fetch pipeline maps `presto-hub` → `FetchTask::EvmStable`; `find_evm_pair(..., "stable")` will miss pairs stamped `dex_type: "presto"` even if discovery is fixed. |
| **Medium** | PathFinder legacy map | `dex_type_for_source("xylo-stable")` → `"stable"`. Discovery stamps `dex_type: "xylo"` so live hops are correct; snapshots without a stamp would mis-type Xylo as Stable. |
| **Medium** | `xyk_quote` | Adapter math is still hardcoded 997/1000. Aggregator `_xykOut` uses per-factory fee (UnitFlow 30 bps default). |
| **Medium** | `/build_tx` shared-pool | Encoder does not re-check that submitted sub-routes share a pool. SplitOptimizer + Solidity `_rejectSharedPools` cover the happy path; a hand-built body can still reach the contract revert. |
| **Medium** | Presto pair scope | Contract hub allowlist does not restrict USDC/EURC. Catalog freeze is the three tokens; design "USDC/EURC discovery only" is worker-side (and currently a no-op because Presto is never discovered). |
| **Medium** | Evidence pack | T7.2 / T9.1–T9.5 / `docs/evidence/README.md` still record mBTC pairs and aggregator `0xEa1b2C…2006`. Stale vs current catalog and `0xeb1235…29d8`. |
| **Medium** | OpenAPI examples | Still `chakra-stable` / `chakra-xyk`. Live source is `xylo-stable`. |
| **Medium** | Circle faucet CTA | Nested inside `balanceFor > 0` in `SwapCard.tsx` (~455 vs ~474), so the empty-balance CTA never renders. |
| **Medium** | Deployment-doc header | File still opens with "hosted stack still runs the pre-rebaseline revision" while the uncommitted cutover section (and this check's live smoke) show the rebaselined catalog + new aggregator. Cutover section also claimed `/quote` `is_split`; this check observed `is_split: false`. |
| **Low** | A=200 wrappers | `xylo_quote` / `xylo_gross` / `xylo_invariant_d` still hardcode A=200. Production hop uses `xylo_quote_with_a(state.a)`. |
| **Low** | `ExecuteSplitSwap.s.sol` | Default aggregator still `0xEa1b2C…2006`. |
| **Low** | Collapsed Route pill | Shows raw `source` (`xylo-stable`) rather than the display map used in expanded legs. |
| **Low** | TokenSelector mBTC hint | Intentional fixture-only search copy; not a runtime catalog leak. |
| **Low** | Testing-doc parse pin | Testing doc EVM-worker bullet was stale (`source == "xylo"`); Check leftover note now records that parse pins `xylo-stable`. |
| **Low** | Stale cargo feature flag | Planning/impl still mention `cargo test -p api-server --features test-fixture`. The feature lives on `dex-adapters` and is already enabled in `crates/api-server/Cargo.toml`; `cargo test -p api-server` is sufficient (60 passed this session). |

### Live hosted smoke (this session, 2026-08-30)

API `https://chakra-api-0a5i.onrender.com`, UI `https://chakra-arc-dex.vercel.app`.

- `GET /api/v1/health` → 200 `{"status":"ok"}`.
- `GET /api/v1/ready` → 200 `ready:true`, `snapshot_id=snapshot-1788081330115`, `pool_keys:[]` (known T9.6 cluster-mode note).
- `GET /api/v1/tokens` → USDC (6) / EURC (6) / cirBTC (8).
- `GET /api/v1/quote` 1e6 USDC→EURC → 200, `source: "xylo-stable"`, `dex_types: ["xylo"]`, hop factory `0x60ed…e2`, pool `0x3df3…bb1`, `expected_output: "803990"`, `minimum_output: "799970"`, `hop_fees: [4]`, **`is_split: false`**, `protocol_fee_bps: 0`.
- EURC→cirBTC and USDC→cirBTC → honest `NO_ROUTE` (UnitFlow cirBTC reserve 249,850 < 1e8 dust filter; no reseed).
- `POST /api/v1/build_tx` with `min_amount_out: "799970"` + quoted xylo step → 200; **`to: "0xeb12351602c56d47c4ee955193335848952b29d8"`**, calldata selector **`0x2e3be0c1`**, `value: "0"`, `chain_id: 5042002`, Permit2 `typed_data.domain.verifyingContract` = `0x000000000022D473030F116dDEE9F6B43aC78BA3`, `typed_data.message.spender` = new aggregator. First attempt with `slippage_bps` instead of `min_amount_out` returned 422 (API contract; not a regression).
- CORS `access-control-allow-origin: https://chakra-arc-dex.vercel.app`. UI HTTP 200.

Working live pair: Xylo USDC↔EURC only.

### Fresh verification (this session, HEAD `208d5ff`)

- `npx ai-devkit@latest lint --feature chakra` (from the worktree): **all checks passed**. (Same command from repo root fails because the five feature docs live in the worktree.)
- `forge test -vv` in `contracts/evm`: **88 passed / 0 failed** — Aggregator 52 (incl. `test_xylo_hop_succeeds_via_router`, `test_presto_hop_succeeds_via_hub`, `test_split_reuses_pool_reverts`, `test_xyk_factory_fee_is_configurable`, `test_catalog_enforcement_rejects_out_of_catalog_token`), StableSwap 16, Xyk 8, Clmm 5, MockBtc 5, LiquiditySeeder 2. Planning local-release-gate said MockXylo 2; the suite is LiquiditySeeder 2.
- `cargo test --workspace --all-targets`: **245 passed / 0 failed**.
- `cargo test -p api-server`: **60 passed / 0 failed** (do not pass `--features test-fixture` on this package).
- Frontend: vitest **67/67**, `npx tsc --noEmit` clean, `npm run build` exit 0.
- SDK: vitest **14/14**, `npx tsc --noEmit` clean.
- `git diff --check`: pass.

Local gates are green. They do **not** prove venue-discovery or source-id alignment; several Majors are untested paths (no Presto discovery arm, parse pins `chakra-xyk`, bytecode-only verification).

### Prior Criticals (08-26 / 08-27) — status at this HEAD

Resolved in code: StableSwap deposit accounting, production snapshot bootstrap/reload, cluster `/build_tx` Redis load, selector `0x2e3be0c1` + canonical ABI packing, Permit2 `0x927da105` + PermitSingle, UI approve spender = Permit2, CLMM snapshot → engine, frontend typecheck/build, MetaMask harness (live evidence still vs old aggregator `0xEa1b2C…2006`).

### Open requirements / evidence

- Planning T2.1–T2.4 and T3.3 remain `[ ]` (Check did not flip them). Catalog/runtime work exists; discovery/verification/source-id gaps above are why they stay open.
- SC-1 live three-pair routing: USDC↔EURC only; cirBTC legs honest `NO_ROUTE`.
- SC-2 / SC-4 live split evidence is 2026-08-28 vs the old aggregator and `chakra-stable`+xylo; not re-proven on `0xeb1235…29d8`.
- SC-6/SC-9 clean-clone walkthrough evidence still names `0xEa1b2C…2006`.
- SC-11 still not observable via public `/ready` (`pool_keys: []`).
- Watchlist (Lunex, UnitFlow V3, AchSwap/Arc Swap, Synthra) is docs-only — correctly **not** in code/env/manifest.

### Required correction order (Execute, not this check)

1. Discover Presto hubs into topology (`discover_once` arm + hydrator that finds `dex_type: "presto"`), or design-reconcile Presto as deploy-time allowlist only with a different discovery contract.
2. Stamp UnitFlow as `unitflow-v25` end-to-end (worker parse, snapshot, quote gate, `step_factory_source`), or reconcile design to `chakra-xyk`.
3. Implement the T3.3 five-check venue verification (or narrow the design to bytecode-only).
4. Enforce `arc-operator.sh` script allowlist (`DeployAggregator.s.sol` only on 5042002); keep `Deploy`/`Seed`/`DeployMockBtc` chain-31337 / fixture-marked.
5. Align public README + integrator-guide + `render.yaml` + OpenAPI examples + evidence pack with cirBTC + `0xeb1235…29d8`.
6. Gate quote-time factory membership for curated source ids; map SDK/UI `xylo*` → `xylo` not `stable`.

### Next

`dev-execute` for the Majors above (or `dev-design` if product wants to drop Presto / keep `chakra-xyk` as the UnitFlow id). **Do not** run `dev-testing` or `dev-review` on this verdict.

**Phase 6 (2026-08-30):** Planning reconciled. T2.1–T2.4 / T3.3 stay `[ ]` with partial/blocked notes. Leftover Execute queue is **T10.1–T10.6** in `docs/ai/planning/2026-08-20-feature-chakra.md`. Next invocation: `dev-implementation` at T10.1.

### T10.1 Presto discovery into topology (Done 2026-08-30)
- `crates/market-data-worker/src/evm_watcher.rs`: added `discover_once` `"presto"` arm restricted to USDC/EURC discovery over catalog pairs, publishing `presto-hub` pairs with `dex_type: "presto"` and pool address = hub address.
- `crates/dex-adapters/src/evm_fetch.rs`: added `token_reserves_selector()`, `path_reserves_selector()`, and `fetch_presto_state()` querying `tokenReserves(spoke)` and `pathReserves(spoke)` on the hub with directional token mapping.
- `crates/market-data-worker/src/fetch_pipeline.rs`: added `FetchTask::EvmPresto`, mapped `"presto-hub" | "discovered:presto"` in `coalesce_touched_into_tasks` to `FetchTask::EvmPresto`, implemented `execute_fetch_task` delegating to `fetch_presto_state()`, and updated `find_evm_pair` to support `"presto"`.
- Tests: `discovery_finds_presto_hub_pair_from_seeded_hub`, `coalesce_maps_evm_chakra_sources_to_evm_tasks` with Presto, and `execute_fetch_task_hydrates_presto_pool_reserves_with_distinct_getters` asserting distinct `pathReserves`/`tokenReserves` getters and directional mapping. T2.3 remains partial pending live pool publication.
