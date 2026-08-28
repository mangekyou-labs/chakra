---
phase: testing
title: Testing Strategy
description: Define testing approach, test cases, and quality assurance
feature: chakra
date: 2026-08-20
---

# Testing Strategy

**Product:** Chakra  
**Feature key:** `chakra`  
**Aligned to:** reviewed requirements `docs/ai/requirements/2026-08-20-feature-chakra.md` (SC-1…SC-13, Phase 2 2026-08-20), reviewed design `docs/ai/design/2026-08-20-feature-chakra.md` (Phase 3 2026-08-20).

Browser QA uses **Playwright CLI** (`playwright-cli` or `npm run qa:cli`). Do **not** use Playwright MCP. Injected EIP-1193 smokes are **not** a substitute for the MetaMask extension harness.

## Test Coverage Goals

- **Routing math + split optimizer + decimal helpers:** ≥95% line coverage on `crates/router-engine` and adapter quote math. Target 100% of branches in SplitOptimizer skip/Brent/merge and in USDC 6-vs-18 guards.
- **Solidity aggregator + venues:** Foundry tests for happy path, `minAmountOut` revert, pause, deadline, Permit2 failure, leftover-token = 0, factory-allowlist drain attempt, V3 callback spoof, `msg.value != 0` revert, wrong-token hop. Compiler `solc 0.8.30`, `evm_version = prague`. Never use `block.prevrandao`. New aggregator code aims at 100% of public functions.
- **API handlers:** integration tests for quote / build_tx / tokens / balances / health / ready, envelope `{success,data,error}`, and error codes (`NO_ROUTE`, `NOT_READY`, `PAUSED`, `ROUTE_INVALID`, 400, 429). `/build_tx` does **not** re-quote.
- **TypeScript SDK:** unit tests for query encoding and response parsing (quote + build_tx).
- **UI:** critical-path e2e (Playwright CLI + MetaMask harness). Do not require 100% of React component lines; cover decimal formatters and chain-gate helpers with unit tests.
- **Excluded from 100%:** generated bindings, vendored Uniswap/Curve bytecode, Tailwind/CSS, snapshot fixtures.

Coverage commands (implementation will wire these):

```bash
cargo llvm-cov --workspace --fail-under-lines 90   # or crate-level thresholds
forge coverage
cd packages/sdk && npm test -- --coverage
cd packages/frontend && npm test
```

## Unit Tests

### EVM worker / Arc adapters (T3.3)

- [x] RPC URL policy: Canteen `$RPC` host (`rpc.testnet.arc-node.thecanteenapp.com`) rejected by config; documented public Arc + failovers (Blockdaemon HTTP; dRPC HTTP/WS; QuickNode HTTP/WS) accepted; invented Alchemy URLs rejected — `evm_rpc::tests::{rejects_canteen_proxy_rpc,allows_public_arc_and_documented_failovers,rejects_invented_alchemy_url,validate_lists_reject_canteen_and_accept_documented}` + `evm_watcher::tests::evm_config_rejects_canteen_rpc` (2026-08-25)
- [x] Worker `EvmConfig::from_env` maps `CHAKRA_RPC_HTTP/WS` + failovers, `CHAKRA_REDIS_URL` (with `SNAPSHOT_REDIS_URL` override), seed/discovery factory tuples, `CHAKRA_CHAIN_ID`, poll/WS knobs — `evm_config_from_env_reads_chakra_vars` + `worker::{worker_mode_defaults_to_arc_and_reads_chakra_redis,worker_mode_keeps_stellar_when_legacy_env_is_set}` (2026-08-25)
- [x] `eth_getLogs` poll path: fixture Swap log → touched pool `chakra:pool:xyk:*` (memory store) updated **< 5 s** after inclusion (SC-11 local) — `evm_watcher::tests::poll_refreshes_pool_store_after_fixture_swap_within_5s` (2026-08-25)
- [x] WS `eth_subscribe` + notification forwards the log — `evm_watcher::tests::ws_subscription_forwards_log_notification` + `parse_subscription_log_extracts_log` (2026-08-25)
- [x] Poll with empty topology does not RPC `eth_getLogs` — `evm_watcher::tests::poll_with_empty_topology_keeps_cursor_warm_without_logs` (2026-08-25)
- [x] Created-pool log (`PairCreated`) upserts topology; later `Swap` on the new pool resolves through the refreshed index — `created_pool_log_upserts_topology_and_later_swap_touches` (2026-08-25)
- [x] Discovery probes catalog pairs only (`getPair`/`getPool`), skips mBTC until `CHAKRA_MBTC_ADDRESS`, and never sweeps the market — `discovery_finds_catalog_xyk_pair_from_fixture_factory` / `discovery_without_mbtc_only_probes_usdc_eurc` (2026-08-25)
- [x] Never-call addresses are never in the WS watch list — `watch_addresses_filter_never_call_and_keep_0x` (2026-08-25)
- [x] Fetch pipeline coalesces `chakra-*` / `discovered:*` / `xylo` sources into EVM tasks — `fetch_pipeline::tests::coalesce_maps_evm_chakra_sources_to_evm_tasks` + `evm_watcher::tests::factory_tuple_parse_accepts_seed_and_discovery` (pins `source == "xylo"` for xylo seed and discovery) (2026-08-28)

### PathFinder (`crates/router-engine`)

- [x] Direct USDC→EURC finds seeded xy=k and stable pools (SC-1) — `usdc_to_eurc_finds_both_seeded_xyk_and_stable_pools` (2 direct pools; sources `chakra-xyk` + `chakra-stable`) (2026-08-25, T4.1)
- [x] USDC→mBTC finds xy=k and CLMM (SC-1) — `usdc_to_mbtc_finds_xyk_and_clmm` (2 direct pools; CLMM via `clmm_pool_refs`) (2026-08-25, T4.1)
- [x] EURC→mBTC finds direct and/or 2-hop via USDC (SC-1) — `eurc_to_mbtc_finds_direct_and_two_hop_via_usdc` (direct xyk + 2-hop through ERC-20 USDC) (2026-08-25, T4.1)
- [x] `max_hops=1` excludes multi-hop — `max_hops_one_excludes_multi_hop` (2026-08-25, T4.1)
- [x] Unknown token or same in/out yields empty candidates — `unknown_token_or_same_in_out_yields_empty_candidates` (2026-08-25, T4.1)
- [x] Discovered pool whose token is outside {USDC, EURC, mBTC} is unused (catalog freeze) — `non_catalog_pool_is_unused` (graph token count 0) (2026-08-25, T4.1)
- [x] Native USDC is not a graph node (SC-12) — `graph_nodes` / `is_native_usdc_encoding` (T1.2) + **`native_usdc_encoding_is_not_a_graph_node`** (PathFinder drops `native_usdc` and `0x000…0` pairs) (2026-08-25, T4.1)
- [x] `PathFinderConfig::default()` is Chakra-correct: `max_hops=3`, bridge = ERC-20 USDC `0x3600…0000` (`TokenId::Contract`), never XLM/Classic — `default_config_is_chakra_arc_three_hops_with_erc20_usdc_bridge` (2026-08-25, T4.1)
- [x] Snapshot loader honors the catalog freeze — `pairs_from_chakra_snapshot` filter + `update_from_chakra_snapshot` used by every case above (2026-08-25, T4.1)

### QuoteEngine / AMM math

- [x] xy=k constant-product matches a hand-computed fixture — `evm_quote_math::xyk_matches_aggregator_997_formula` pins the exact `Aggregator._xykFormula` expression (2026-08-25, T3.2)
- [x] Stableswap (`A=100`) USDC/EURC low-impact quote vs xy=k higher impact (feeds SC-2) — `evm_quote_math::stable_deeper_than_xyk_for_low_impact_swap` (2026-08-25, T3.2)
- [x] QuoteEngine hydrates `chakra-stable` from Redis and quotes with `evm_quote_math::stable_quote` (never generic 9970/10000 / xy=k) — `quote_hydrates_chakra_stable_and_uses_evm_math` pins the T3.2 on-chain vector `999_550_535` at 1_000e6, single `chakra-stable` route, `protocol_fee_bps=0` (2026-08-25, T4.2)
- [x] `chakra-xyk` quotes use the EVM 997/1000 `xyk_quote` + integer `price_impact_bps` — same test covers dispatch; `usdc_to_mbtc_output_is_in_mbtc_8dp_atomic_units` pins the exact venue output (2026-08-25, T4.2)
- [x] CLMM quote with complete tick coverage — `chakra_clmm_quotes_when_complete_and_skips_when_incomplete` (`chakra-clmm` allowlisted at QuoteEngine; `amount_out > 0` with complete coverage) (2026-08-25, T4.2)
- [x] CLMM hop skipped when `coverage.is_complete=false` — enforced at Redis publish (`should_publish_clmm_to_redis`) + bootstrap incomplete-CLMM test + **QuoteEngine `chakra-clmm` skip** (same test, 2026-08-25, T3.1/T4.2)
- [x] Token decimals 6 vs 8 applied correctly (USDC vs mBTC) — `usdc_to_mbtc_output_is_in_mbtc_8dp_atomic_units`: 1_000e6 USDC → `xyk_quote(50_000e6, 1e8, 1_000e6)` exact atomic mBTC output (~1.95e6, 8 dp range, never 18 dp wei) (2026-08-25, T4.2)
- [x] Mixing native 18 dp into `amount_in` is rejected (SC-12) — `native_usdc_encoding_is_rejected_as_swap_amount`: `native_usdc` / `0x000…0` as token_in **or** token_out → empty route, zero output, `protocol_fee_bps=0` (2026-08-25, T4.2)
- [x] XyloNet stableswap (`A=200`, 4 bps fee-on-output) quote matches live RPC `calculateSwap` vectors — `evm_quote_math::tests::xylo_matches_live_rpc_calculate_swap_vectors` (pins `1e6 USDC→EURC = 865542` and `1e6 EURC→USDC = 1154419`) + `xylo_quote_guards_bad_inputs_and_fee_is_on_output` (2026-08-28, T-XYLO)
- [x] QuoteEngine routes small size to `chakra-stable` and capacity size to `xylo` — `quote_engine::tests::{xylo_loses_to_chakra_stable_at_small_size,xylo_wins_at_chakra_capacity_sizes}` (2026-08-28, T-XYLO)

### SplitOptimizer

- [x] Impact below `SPLIT_THRESHOLD_BPS` and uncompetitive 2nd path → `is_split=false` — `test_no_split_when_only_one_path_exists` + `test_split_skipped_when_competitive_but_zero_impact` + the Cycle-3 control (`1_000e6` single `chakra-stable`) (2026-08-25, T4.2)
- [x] Documented size on USDC/EURC (deep stable + thin xy=k) → split attempt and honest outcome — `sc2_180k_split_is_refused_and_single_stable_wins`: **documented deviation** — at `180_000e6` the plan-computed ~7 bps split gain is rejected by the locked `max_leg_rate_deviation_bps=500` filter (xy=k leg is ~0.7% of the trade; its marginal rate is ~17× the catastrophically diluted full-size quote) plus the 5 bps improvement floor + dust filter. The engine returns the single `chakra-stable` route (best execution). A real split needs a relaxed rate filter or deeper xy=k seeds — **out of T4.2 scope** (plan: no `SplitConfig` default or seed-depth changes). Tracked for T4.3/T9.2 follow-up (2026-08-25, T4.2)
- [x] Two-path Brent improves vs 50/50 seed — `test_brent_amm_split` + `test_brent_quadratic` (2026-08-25, T4.2)
- [x] Three-path pairwise merge — `test_three_path_split_can_beat_rest_best_approximation` (2026-08-25, T4.2)
- [x] Legs below `MIN_SPLIT_FRACTION_BPS` dropped — `test_filters_split_legs_below_min_fraction_bps` + `test_filters_split_legs_with_fantasy_rate_and_dust_input` (2026-08-25, T4.2)
- [x] `max_splits=1` forces single path — `max_splits_override_one_forces_single_path`: two competitive paths, `Some(1)` → `is_split=false`, one sub-order, debug `max_splits_1`, `protocol_fee_bps=0` (2026-08-25, T4.2)
- [x] `protocol_fee_bps` always 0 in optimizer output (SC-13) — `protocol_fee_bps_is_always_zero`: empty / single / split routes all report `0` (2026-08-25, T4.2)

### Decimal / catalog helpers

- [x] ERC-20 USDC 6 dp parse/format (`decimals.rs`, 2026-08-20)
- [x] Native USDC 18 dp parse/format never used as swap amount (SC-12) — native is not a graph node / catalog token
- [x] mBTC 8 dp parse/format (catalog decimals = 8)
- [x] Reject float / scientific notation on the wire

### Solidity Aggregator (Foundry)

- [x] Single-hop xy=k swap succeeds; user receives ≥ `minAmountOut` — `test_single_hop_xyk_success` (exact 997/1000 venue output, `Swap` event `isSplit=false`) (2026-08-25)
- [x] Multi-hop EURC via USDC succeeds atomically — `test_multi_hop_eurc_via_usdc_success` (EURC→USDC→mBTC chained xyk) + atomic revert `test_multi_hop_min_revert_is_atomic` (2026-08-25)
- [x] Split across stable + xy=k succeeds; both pools’ reserves change (SC-4 unit analog) — `test_split_thin_xyk_plus_deep_stable` (700e6 stable + 300e6 thin xyk; both reserve sets move; `isSplit=true`) (2026-08-25)
- [x] `minAmountOut` too high reverts; no reserve change — `test_minAmountOut_too_high_reverts` (reserves + user balances unchanged) (2026-08-25)
- [x] Paused aggregator reverts — `test_owner_pause_blocks_splitSwap` (+ `test_owner_unpause_restores`) (2026-08-25)
- [x] Permit2 missing / bad signature reverts — `test_permit2_bad_signature_reverts` (`InvalidSignature`); `test_permit2_spender_mismatch_reverts` (2026-08-25)
- [x] ABI round-trip: API-style hex feed with correct selector + Permit2Pull encoding — `test_api_hex_empty_sig_succeeds` (low-level `agg.call` with `abi.encodeWithSelector(0x2e3be0c1, ...)`, empty-signature path; verifies Rust encoder output decodes on-chain) (2026-08-26, T4.4)
- [x] Permit2 `signature.length == 0` succeeds when AllowanceTransfer allowance is already sufficient — `test_permit2_empty_signature_skips_permit` (`permitCalls == 0`); signed path `test_permit2_signature_grants_allowance` (`permitCalls == 1`) (2026-08-25)
- [x] Aggregator token balances return to 0 after success and after revert (leftover sweep) — `_assertCatalogZero` on every success + revert path (USDC/EURC/mBTC) (2026-08-25)
- [x] `msg.value` non-zero reverts in v1 (SC-12) — `test_splitSwap_rejects_value` (low-level call + `!ok`); `test_receive_eth_reverts`/`test_fallback_eth_reverts` decode `DirectEth` (2026-08-25)
- [x] `deadline` in the past reverts — `test_deadline_past_reverts` (2026-08-25)
- [x] Protocol fee not taken (output matches quote math minus venue fees only) (SC-13) — xyk single-hop `amountOut == 997/1000 formula` exactly; split bounded by `xyk leg + ≥699e6 stable leg` (2026-08-25)
- [x] Non-owner cannot pause; owner can pause/unpause — `test_nonOwner_cannot_pause`/`test_nonOwner_cannot_unpause`/`test_owner_*` (2026-08-25)
- [x] Hop to a pool **not** from an allowlisted factory reverts (fake-pair drain attempt) — `test_hop_to_fake_pool_reverts` / `test_hop_without_allowlisted_factory_reverts` / `test_removeFactory_gates_hops` (2026-08-25)
- [x] Seed/discovery factory lists and aggregator allowlist **never** include CCTP V2, Gateway, USYC/Entitlements/Teller, FxEscrow, Memo, or Multicall3From addresses from `contract-addresses.md` — `test_never_call_addresses_not_allowlisted`, `test_never_call_hop_reverts_empty_allowlist`, `test_never_call_hop_reverts_after_allowlisting` (12 addresses × empty and populated allowlists) (2026-08-25)
- [x] `uniswapV3SwapCallback` from a non-allowlisted sender reverts — `test_clmm_callback_sender_mismatch_reverts` / `test_clmm_callback_non_allowlisted_pool_reverts` (FakePool) / `test_clmm_callback_random_eoa_reverts` (2026-08-25)
- [x] Per-hop min is 0; only total `minAmountOut` is checked — no per-hop min in Hop/SubRoute or execution; single total check at settle (2026-08-25)
- [x] Owner `rescueTokens` works; non-owner cannot — `test_rescueTokens_owner`/`test_rescueTokens_non_owner_reverts` (2026-08-25)
- [x] `addFactory` / `removeFactory` gate hops — `test_addFactory_onlyOwner`, fake-pool/removeFactory reverts (2026-08-25)
- [x] mBTC / venues never read `block.prevrandao` (always 0 on Arc) — `grep -R prevrandao src venues`: no hits (2026-08-24); re-checked with `test script` added: no hits (2026-08-25).
- [x] Mixed solc routing: `auto_detect_solc` + `compilation_restrictions` — `src/**`, `test/**`, `script/**` = `0.8.30`/`prague`; `venues/uniswap-v2/**` = `0.5.16`/`istanbul`; `venues/uniswap-v3/**` = `0.7.6`/`istanbul` (replaces the single global `solc = "0.8.30"` from T1.1). Tests/scripts never import V2/V3 sources; they deploy via `VendorDeployer` hex bytecodes and talk through 0.8.30 interfaces (2026-08-24).

### Seeded venues

- [x] V2 pair swap/mint/burn — `XykFactory.t.sol`, 8 cases: createPair for all three pairs, token0<token1 sort, mint→reserves, transfer-in then `swap(..., "")`, burn, 30 bps fee vs no-fee counterfactual (2026-08-24). Seeded fixtures: USDC/EURC 10_000e6 each, USDC/mBTC 50_000e6/1e8 (conceptually; tests mint per-case), EURC/mBTC pair created.
- [x] Stableswap 2-token exchange — `StableSwap.t.sol`, 16 cases
- [x] StableSwap custody: deposit proof + index bounds + reserve tracking — `StableSwap.t.sol` 6 new cases (2026-08-26, T5.1/T2.3): `test_exchange_without_deposit_reverts` (no-deposit drain reverts), `test_exchange_rejects_index_out_of_range` (i/j ≥ 2 → IndexOutOfRange), `test_exchange_reverts_when_declared_amount_exceeds_actual_deposit` (50e6 in, declare 100e6 → InsufficientInput), `test_reserves_updated_after_exchange` (reserve0/1 track swap), `test_reserves_updated_after_remove_liquidity` (reserve0/1 track remove), `test_exchange_excess_deposit_not_consumed` (2000e6 in, declare 1000e6 → reserve tracks declared). `seedLiquidity` stores reserve0/1; `removeLiquidity` decrements. `exchange` reverts IndexOutOfRange / InsufficientInput. Full suite: `forge test -vv` → 73/73 pass, exit 0.
 (10 original + 6 custody): `createPool`/`getPool` both orderings, duplicate-pool revert, `exchange` 0→1 and 1→0, `minDy` revert, same-index/zero-amount revert, 4 bps fee, and 1_000e6 USDC on the 200_000e6-per-side stable pool > same swap on the 10_000e6 xy=k pair (2026-08-24).
- [x] CLMM swap in-range — `ClmmPool.t.sol`, 5 cases: `createPool(USDC, mBTC, 3000)` + `initialize` + `slot0`, in-range `mint` on tick-multiple-of-60 full range (L=1e12, both tokens owed), `swap` zeroForOne and oneForZero with callback, 5 bps pool absent (2026-08-24).
- [x] mBTC ERC-20 8 decimals — Foundry `MockBtcTest` (2026-08-20). Live Arcscan deploy still pending operator broadcast.

**Local seeded-venue suite (2026-08-24, worktree):** `forge test -vv` → 29 passed / 0 failed (Placeholder 1, MockBtc 5, XykFactory 8, StableSwap 10, ClmmPool 5). Live Arc seed (readable on-chain reserves) remains **blocked** — no operator key in this environment; same reason as T2.1.

**Local aggregator suite added (2026-08-25, worktree, T5.1; updated 2026-08-28 for T-XYLO):** `forge test -vv` → **81 passed / 0 failed, exit 0** (Aggregator 45 incl. 5 Xylo tests: approve+`swap` happy path, unknown factory revert, USYC pair block, not usable as stable hop, `removeFactory` gating + MockBtc 5 + XykFactory 8 + StableSwap 16 + ClmmPool 5 + LiquiditySeeder 2). `forge build` exit 0 (incl. `DeployAggregator.s.sol`). `grep -R prevrandao src venues test script` → no hits.

**Aggregator redeployment (2026-08-28, Arc testnet, T5.2):** broadcast via `scripts/arc-operator.sh` to `0xEa1b2C24bd41163590960F8e40afe6cb4CC92006` (tx `0x4cef6ba6e6d7132a7517666b2ce6c1ab7f5ae882ca9c80bb82ad9658ab71a22d`). Codesize 22258 hex chars, `paused=false`, owner `0x12E266744f6d25D372000e066eCc0DF5a752276d`, `factoryDexType` allowlists: Xyk=0, Stable=1, Clmm=2, Xylo=3.
**T3.1 snapshot/Redis suite added (2026-08-25, worktree):** `cargo test -p market-snapshot` → **36 passed / 0 failed** — new `ready::tests` (2 cluster against a spawned local `redis-server`, 2 memory: false with snapshot-only, true with snapshot+pool, stable pools counted), `bootstrap::tests` (3: memory publish reads snapshot/pools/factories + ready, incomplete CLMM skipped, Redis publish writes keys + events + `cluster_ready`), key-shape + legacy-default + stable/factory round-trip unit tests. `cargo test --workspace` → all suites 0 failed; `cargo check --workspace` exit 0. Redis tests skip gracefully when `redis-server` is unavailable.

**T3.2 EVM venue quote math suite added (2026-08-25, worktree):** `cargo test -p dex-adapters evm_quote_math` → **6 passed / 0 failed** — `xyk_quote` pins the exact `Aggregator._xykFormula` 997/1000 expression; `stable_quote` matches **on-chain `StableSwap.sol` vectors** captured this session via a temporary `forge script` probe (200_000e6 seed, 3× 1_000e6 USDC→EURC: `999550535 / 999451582 / 999352602`, reproduced exactly including inter-swap reserve drift); stable-deeper-than-xyk at 1_000e6 (SC-2 analog); integer `price_impact_bps`; zero/same-index/range guards. CLMM skip-if-incomplete re-verified (T3.1 bootstrap test). `cargo test --workspace` → all suites 0 failed; lint clean.

**T4.2 QuoteEngine EVM wiring + SplitOptimizer fee/split tests added (2026-08-25, worktree):** `cargo test -p router-engine` → **44 passed / 0 failed** — `protocol_fee_bps_is_always_zero`, `max_splits_override_one_forces_single_path`, `quote_hydrates_chakra_stable_and_uses_evm_math` (vector pin `999_550_535`), `sc2_180k_split_is_refused_and_single_stable_wins` (**documented SC-2 deviation**: rate-deviation filter blocks the thin-pool split; single stable is best execution), `chakra_clmm_quotes_when_complete_and_skips_when_incomplete`, `native_usdc_encoding_is_rejected_as_swap_amount`, `usdc_to_mbtc_output_is_in_mbtc_8dp_atomic_units`. `cargo test -p api-server` → 40 passed / 0 failed (`QuoteHydration.stable_pools` + `chakra-stable` classification). `cargo test --workspace` → all suites 0 failed; `cargo build --workspace` exit 0.

### TypeScript SDK

- [x] `quote()` encodes query params (`token_in`, `amount_in`, `slippage_bps` — 0.5% → `50`), never `prefer_soroban`/percent `slippage` — `ChakraClient.quote encodes token_in/token_out/amount_in/slippage_bps…` (2026-08-25, T7.1)
- [x] Parses `is_split`, `sub_routes`, `protocol_fee_bps`, `price_impact_bps`, `fraction_bps` — `parses price_impact_bps, protocol_fee_bps, is_split, fraction_bps, sub_routes` (2026-08-25, T7.1)
- [x] `buildTx()` POST body matches OpenAPI (`user`, `token_in`, `amount_in`, `min_amount_out`, `sub_routes[].steps`; no `from`/`user_public_key`) — `POSTs user + token_in + amount_in + min_amount_out + sub_routes[].steps` (2026-08-25, T7.1)
- [x] `quoteSubRoutesToSteps` maps a two-hop `chakra-xyk → chakra-clmm` source into `xyk` then `clmm` steps — `maps a two-hop chakra-xyk → chakra-clmm source…` (2026-08-25, T7.1)
- [x] Surfaces envelope `error.code` (`NO_ROUTE`, `NOT_READY`, `PAUSED`) — `throws an error whose .code is NO_ROUTE / NOT_READY / PAUSED` via `ChakraApiError` (2026-08-25, T7.1)
- [x] `isHealthy()` uses `/api/v1/health` — `uses /api/v1/health` (2026-08-25, T7.1)
- [ ] OpenAPI example (or SDK example script) completes quote + build_tx against a local API (SC-6) — T7.1 SDK done; example ran with **API not up** (no local API running this session), live run deferred to a session with the local binary up

### Frontend helpers

- [x] Chain gate: `5042002` allowed, others blocked — `chain.test.ts::isArcTestnet` (5042002 true; 1/14/114/undefined false) (2026-08-25, T6.1)
- [x] `wallet_addEthereumChain` nativeCurrency USDC 18 dp, chainId `0x4CEF52` — `ARC_ADD_CHAIN_PARAMS` pinned (chainName `Arc Testnet`, rpcUrls public Arc, blockExplorerUrls Arcscan) (2026-08-25, T6.1)
- [x] If the injected wallet labels native as ETH, on-screen gas copy still says USDC (connect-to-arc caveat) — `nativeGasSymbol('ETH' | 'native' | undefined)` always `'USDC'` (2026-08-25, T6.1)
- [x] Amount chips 25/50/75/MAX against ERC-20 balance (not native) — implemented in `SwapCard::applyBalancePercent` (uses `getErc20Balance`); unit-locked at the behaviors level (`swap-tokens` native-encoding reject + decimals port) (2026-08-25, T6.2)
- [x] USDC MAX uses `ceil(gas_wei / 1e12)` × 1.25 with a 100_000 (0.10 USDC) floor; cannot drain native gas (SC-12) — `decimals.test.ts::usdcMaxAtomic` 5 vectors ported verbatim from Rust `decimals.rs::usdc_max_atomic` (0 wei → floor; 2e12; 1 wei over 1e12; 1.25× dominates; balance 50_000 → 0) (2026-08-25, T6.2)
- [x] Slippage default 0.5% → `minimum_output` — `swap-settings.test.ts` (default 0.5, key `chakra:swap-settings`, load default) (2026-08-25, T6.2)
- [x] Native gas estimate formatted with 18 dp; swap amount with token decimals (SC-12) — `decimals.test.ts::formatNativeUsdc` (18 dp) + `formatErc20` (6 dp), no scientific notation (2026-08-25, T6.2)
- [x] Impact formatter: `price_impact_bps` 12 → `0.12%`; protocol fee always `0` — `quote-format.test.ts` (2026-08-25, T6.2)
- [x] Token list rejects native encodings (`native_usdc`, `0x0`, `eth`) — `swap-tokens.test.ts` + `isNativeSwapToken` (2026-08-25, T6.2)
- [x] Quote debounce 250 ms; refresh 5 s does not double-submit — `quote-scheduler.test.ts` (fake timers: burst → single fetch; refresh skipped while in-flight) (2026-08-25, T6.2)
- [x] `localStorage` recent-swaps key `chakra:recent-swaps:5042002:{address}`, max 20 — `lib/recent-swaps.ts` (7 tests: add/get, newest-first, max 20, case-insensitive address, chain-id scoped) (2026-08-26, T6.3 local)
- [x] Unaudited-ack key `chakra:unaudited-ack:v1` shown once — `lib/unaudited-ack.ts` (6 tests: hasAck/recordAck, missing localStorage handled) (2026-08-26, T6.3 local)
- [x] `paused()` true disables confirm — `isPausedEnvelope` in `lib/swap-send.ts` (16 tests incl. `isPausedEnvelope`, `isChainAllowed`, `minFeePerGas`, `buildSendParams`, `spliceSignature`, `encodeApproveCalldata`, `encodePermitCalldata`) (2026-08-26, T6.3 local)
- [x] Swap success treats **first receipt** as final (`confirmations: 1`); no extra confirmation wait — `waitForTransactionReceipt(1)` in SwapCard send pipeline (2026-08-26, T6.3 local)

## Integration Tests

- [x] Worker bootstrap publishes snapshot + pool keys with `chakra:` prefix — `bootstrap::tests` (memory + real local `redis-server`): snapshot loads back, `chakra:pool:xyk|stable|clmm:{source}:{pool}` fetchable, `chakra:factories` readable, version event published, `cluster_ready` true after publish (2026-08-25, T3.1)
- [x] API `/ready` is false before snapshot; true after (SC-5) — `ready_is_503_until_snapshot_and_pool_exist` (2026-08-25, T4.3)
- [x] `/ready` predicate: 200 only when `chakra:snapshot:current` exists **and** ≥1 `chakra:pool:*` key — `ready::tests::cluster_ready_*` (real Redis) + `memory_ready_*` (2026-08-25, T3.1; HTTP handler shape still T4.3)
- [x] `/quote` after Redis hydrate returns routes for all three pairs (SC-1) — `quote_hydrates_chakra_snapshot_routes` (USDC→EURC via stable, USDC→mBTC via xyk; 1_000e6 vector `999_550_535` pinned) (2026-08-25, T4.3)
- [ ] `/quote` split case returns `is_split=true` at documented size (SC-2) — **documented deviation**: at `180_000e6` the engine honestly returns single `chakra-stable` (`sc2_180k_is_not_split_and_single_stable_wins` locks this); a real split case waits for the T9.2 rate-filter/seed decision
- [x] T4.4 selector fix: `selectors_match_contract_abis` now pins canonical 7-arg signature → `0x2e3be0c1` (was `0xcc03a3bc`); `encode_permit2_pull` emits 6-word PermitSingle struct + offset (was 20 zero words); `permit2_allowance` uses Permit2 `0x927da105` 3-arg selector (was ERC-20 `0xdd62ed3e`); fixtures updated to dispatch on `0x927da105` (2026-08-26, T4.4)
- [x] `/build_tx` calldata decodes to `splitSwap` with matching `minAmountOut` and sub-routes — `build_tx_encodes_split_swap_with_matching_route` (full ABI decode: head, routes, hops, pool/dexType/tokenIn/tokenOut/fee; selector `0x2e3be0c1`) (2026-08-25, T4.4)
- [x] `/build_tx` does **not** re-run PathFinder; mutated `sub_routes` with broken continuity → `ROUTE_INVALID` — `build_tx_rejects_broken_continuity_without_requoting` (continuity + amount sum + unknown pool; fixture would panic on any re-quote call) (2026-08-25, T4.4)
- [x] `/build_tx` omits Permit2 `typedData` when allowance is already sufficient — `build_tx_omits_typed_data_and_approvals_when_allowances_sufficient` (2026-08-25, T4.4)
- [x] `/build_tx` returns `PAUSED` when aggregator is paused — `build_tx_returns_paused_when_aggregator_paused` (2026-08-25, T4.4)
- [x] `/build_tx` emits `PermitSingle` typed data (AllowanceTransfer) with `verifyingContract` = Permit2 + spender = aggregator when the Permit2 allowance is insufficient — `build_tx_requires_typed_data_when_permit2_allowance_insufficient` (2026-08-25, T4.4)
- [x] `/build_tx` empty aggregator config → `NOT_READY` (503) — `build_tx_not_ready_when_aggregator_unconfigured` (2026-08-25, T4.4)
- [x] Envelope `{success,data,error}` on quote / build_tx / tokens / balances; `price_impact_bps` integer (no float `price_impact`) — `quote_errors_use_envelope_with_code_and_no_float_impact` (2026-08-25, T4.3)
- [x] `/tokens` lists **only** USDC, EURC, mBTC with correct decimals (catalog freeze; SC-1, SC-12) — `tokens_lists_frozen_catalog_only_with_decimals` (2026-08-25, T4.3)
- [x] `/balances` returns catalog ERC-20 via **Multicall3** and a separate `native_usdc` field; the two USDC encodings are **never summed**; swap USDC is the ERC-20 figure only — `balances_never_sum_erc20_and_native_usdc` (fixture Multicall3 aggregate3; 99e18 native) (2026-08-25, T4.3)
- [x] API/worker RPC is public `rpc.testnet.arc.io` (or documented failovers); tests **fail** if config points at Canteen `$RPC` / `rpc.testnet.arc-node.thecanteenapp.com` — **worker side covered** (2026-08-25, T3.3: `EvmConfig::from_env` + policy tests); API side wires the same policy in T4.3 — `config_rejects_canteen_and_invented_alchemy_urls` (2026-08-25, T4.3)
- [x] `/ready` is 200 only when `chakra:snapshot:current` exists **and** ≥1 `chakra:pool:*` key is present (SC-5) — superseded by the checked scenario above (2026-08-25, T3.1)
- [x] `/health` 200 while `/ready` 503 during empty Redis — `ready_is_503_until_snapshot_and_pool_exist` (2026-08-25, T4.3)
- [x] WS/log path: after a fixture Swap log, touched pool Redis key updates **≤ 5 s** after inclusion (SC-11) — `poll_refreshes_pool_store_after_fixture_swap_within_5s` writes the memory pool store inside the ≤ 5 s window (2026-08-25, T3.3). Live on-chain measurement stays **T9.6** (worker WS→Redis, operator-gated).
- [x] Poll fallback: with WS disabled, `eth_getLogs` catch-up still refreshes — same test drives `poll_once` with `ws_enabled=false` over a fixture server (2026-08-25, T3.3)
- [x] `QUOTE_RPC_HYDRATE_ENABLED=false` does not RPC on Redis hit — `quote_does_not_call_rpc_when_hydrate_disabled` (fixture panics if called) (2026-08-25, T4.3)
- [x] Rate limit returns 429 on `/quote` (10 req/s/IP); `/health` and `/ready` are exempt — `rate_limit_429_on_quote_but_health_and_ready_exempt` (non-loopback ConnectInfo) (2026-08-25, T4.3)
- [x] CORS rejects an origin not in `CHAKRA_CORS_ORIGINS` — `cors_rejects_unlisted_origin_and_allows_configured` (no allowlist header; tower-http behavior) (2026-08-25, T4.3)
- [ ] OpenAPI example (or SDK example script) completes quote + build_tx against a local API (SC-6) — T7.1 SDK once T4.4 lands
- [x] QuoteEngine skips pools whose factory is not in `chakra:factories` — `t45_allowlisted_stable_factory_still_quotes` + `t45_unlisted_factory_pool_is_skipped` + `t45_empty_factories_still_quotes_legacy_pools` (3 tests in `quote_engine.rs`) (2026-08-26, T4.5 local)

## End-to-End Tests

### Injected-provider / anvil-style (not sufficient for SC-7)

- [ ] Headless swap against local fork or mock EIP-1193: quote → build → send
- [ ] Wrong chain id blocks submit

### Playwright CLI + MetaMask harness (SC-3, SC-7) — required

Network: **Arc testnet** `chainId` 5042002 (`0x4CEF52`). Disposable persistent Chromium profile initialized with dAppwright (or equivalent). `DAPP_URL` is local preview or public UI.

- [x] `qa:wallet:validate` / setup / cleanup logs exist and contain no seed, password, or private key (verified 2026-08-28; validate prints only mnemonic word count)
- [x] Extension-loaded snapshot; provider chain ID `0x4CEF52` (verified 2026-08-28 — header menu switch + MetaMask notification popup Confirm; dAppwright `addNetwork`/`confirmNetworkSwitch` broken on MetaMask 13.17)
- [x] Connect-approval screenshot / snapshot
- [x] Critical path: connect → add/switch Arc testnet → select USDC/EURC → amount → quote visible (legs, impact, fee 0) → Permit2 if needed → swap → success → Arcscan link (SC-3, SC-13; **live PASS 2026-08-28** — `swap-critical-path` exit 0, tx `0xa630da3c842d7613ebbbd4d8f66749892a4e42c510933e0e1c3f4966907ef0dd`)
- [x] At least one run uses a size expected to split when practical; if e2e uses a smaller size, SC-4 is still proven by a dedicated on-chain **split** tx (cast or UI), not by multi-hop alone (SC-4 proven separately: tx `0x42e85916ade38b87ef0440ef71d8f3330075ecf2a481247dc2ac33376b287fa8`)
- [ ] Mobile viewport (stacked layout) does not hide confirm CTA — partial: verified 375×812 via Playwright CLI without MetaMask (CTA "Connect Wallet" visible; rerun with the harness at T9.4)
- [ ] Named-session isolation (`playwright-cli -s=chakra-wallet`)
- [x] Artifact scan: no mnemonic / private key in traces or screenshots (scan 2026-08-28 — no seed words in `output/playwright/`)
- [x] Do not treat `test:e2e:injected*` as a substitute for this harness

### Public / on-chain evidence (manual + scripted)

- [x] Public `/health`, `/ready`, `/quote`, and `/build_tx` succeed (SC-5, verified 2026-08-28 on `https://chakra-api-0a5i.onrender.com`):
  - `/health` → 200 `{"status":"ok"}`
  - `/ready` → 200 `{"status":"ready","ready":true,"snapshot_id":"snapshot-…"}`
  - `/quote` (1e6 USDC→EURC) → 200 `expected_output: 996915` via `chakra-stable` (`dex_types: ["stable"]`)
  - `/quote` (5e6 USDC→EURC) → 200 split execution (`is_split: true`, 4680269 total, xylo legs + `chakra-stable`)
  - `/quote` (1e6 USDC→mBTC) → 200 honest `NO_ROUTE` error
  - `/build_tx` (1e6 & 5e6) → 200 with `to: "0xea1b2c24bd41163590960f8e40afe6cb4cc92006"` targeting new aggregator
- [x] On-chain **split** (≥2 sub-routes in one tx) on `testnet.arcscan.app`; multi-hop single-path is extra, not a substitute (SC-4; **live 2026-08-28** — tx `0x42e85916ade38b87ef0440ef71d8f3330075ecf2a481247dc2ac33376b287fa8`, 3 sub-routes 2×xylo+1×stable, `isSplit=1`, 5e6 USDC→4,674,618 EURC)
- [ ] Venue matrix ≥3 pairs × ≥3 sizes checked in (SC-8, partial: USDC↔EURC pairs routable across stable and xylo in `docs/evidence/chakra-t91-venue-matrix.json`; live 3-pair routing open pending T2.1–T2.4 re-seed)
- [x] Split vs single-path benchmark checked in (SC-2, SC-8, verified 2026-08-28 in `docs/evidence/chakra-t92-split-benchmark.json`; +893.01 bps gain)
- [x] Integrator 30-min walkthrough followed once by a clean environment (SC-6, SC-9, verified 2026-08-28 in `/tmp/chakra-clean-clone-t72` in 6 seconds against hosted API; evidence in `docs/evidence/chakra-t72-walkthrough.json`)
- [x] Quote p95 &lt; 500 ms after warm Redis, measured **at the API process** (exclude client RTT) and checked in (SC-10, verified 2026-08-28 in `docs/evidence/chakra-t95-quote-latency.json`; server-side API-process p95 = 23 ms)
- [ ] Worker refresh latency after a live swap: Redis write **≤ 5 s** after inclusion (SC-11, local test verified in `evm_watcher::poll_refreshes_pool_store_after_fixture_swap_within_5s`; **live 2026-08-28 finding** — worker publishes snapshots on the 600 s discovery cycle (`snapshot-1787918142123` → `snapshot-1787918742038`, gap 599.9 s) and `/ready` always reports `pool_keys: []`; per-swap pool-key write is not observable via the public API because Redis is private — needs a metrics endpoint or Redis access to close)

## Test Data

- Foundry fork or Arc testnet: ERC-20 USDC, EURC, deployed mBTC, seeded pools with **documented** depths (thin xy=k USDC/EURC vs deep stable USDC/EURC).
- Redis fixture snapshot for API tests (no live RPC).
- Quote fixtures: small size (no split), documented split size, dust size (no route or min out 0 rejected).
- MetaMask harness wallet: funded from Circle faucet + mBTC mint; keys **never** committed. Prefer env `WALLET_PRIVATE_KEY` injected into dAppwright at setup, gitignored.
- OpenAPI examples use the three catalog tokens only.

Seed documentation (implementation fills exact amounts):

| Pool | Tokens | Intent |
|------|--------|--------|
| xy=k | USDC/EURC | Thin — high impact |
| stable | USDC/EURC | Deep — low impact; enables SC-2 |
| xy=k | USDC/mBTC | Volatile direct |
| clmm | USDC/mBTC 30 bps | Required second venue for splits; 5 bps optional |
| xy=k | EURC/mBTC | Hop / direct |

## Test Reporting & Coverage

- CI: `cargo test` (ported workspace), `forge test`, SDK tests, frontend unit tests. E2E MetaMask harness is **opt-in** (`qa:wallet:*`) because it needs a headed/persistent profile and secrets.
- Coverage artifacts under `coverage/` (gitignored) or CI upload.
- Evidence pack under `docs/evidence/` (quotes JSON, Arcscan URLs, p95 table, Playwright snapshots **without** secrets).
- Gaps: full UI line coverage, third-party factory adapters until a live factory is found **and owner-allowlisted**, optional 5 bps CLMM pool, load testing beyond p95 quotes.

## Manual Testing

- [ ] Dense pro-terminal visual check desktop 1280+ and mobile 390 (SC-3 UX, partial: layout/contrast verified in `docs/evidence/chakra-t98-manual-ux-a11y.json`; second-wallet spot-check open)
- [x] Token logos, % chips, route legs, impact, slippage, explorer, recent swaps present (verified in `docs/evidence/chakra-t98-manual-ux-a11y.json`)
- [x] Unaudited-contract warning before first swap (verified in `docs/evidence/chakra-t98-manual-ux-a11y.json`)
- [x] Keyboard: connect, amount, token switch, confirm (verified in `docs/evidence/chakra-t98-manual-ux-a11y.json`)
- [ ] MetaMask, and spot-check one additional EIP-6963 wallet (Rabby or Coinbase) if available (gated on browser wallet extension)
- [ ] Faucet empty / insufficient gas (native USDC) error is readable
- [ ] Pause aggregator → UI error; unpause → swap works
- [x] Accessibility: labels, contrast on terminal palette, not color-only impact (verified in `docs/evidence/chakra-t98-manual-ux-a11y.json`)

## Performance Testing

- [ ] Warm Redis quote p95 &lt; 500 ms for USDC→EURC and USDC→mBTC at 3 sizes, measured at the API process (SC-10)
- [ ] Split quote still under p95 (Brent ~10 extra math evals, no extra RPC)
- [ ] Worker: time from included swap tx to Redis key update **≤ 5 s** (SC-11)
- [ ] No full-market sweep on the hot path (assert discovery interval ≠ hot path)

Load/stress beyond p95 is not a v1 gate.

## Bug Tracking

- File issues against `feature-chakra` with scenario IDs from this doc (e.g. `SC-12`, `SplitOptimizer`).
- Severity: S1 wrong decimals / funds stuck / non-atomic split; S2 wrong quote vs on-chain; S3 UX; S4 docs.
- Regression: any S1/S2 gets a Foundry or cargo test before close.
- Re-run MetaMask harness after wallet/chain/Permit2 changes.

## Scenario → requirements map

| Scenario group | Success criteria |
|----------------|------------------|
| PathFinder + quote integration | SC-1 |
| SplitOptimizer documented size | SC-2 |
| MetaMask Playwright critical path | SC-3, SC-7, SC-13 |
| On-chain split (≥2 sub-routes; multi-hop not a substitute) | SC-4 |
| Public health/ready/quote + UI URL | SC-5 |
| SDK + OpenAPI example | SC-6, SC-9 |
| Venue matrix + benchmark files | SC-8 |
| Quote p95 | SC-10 |
| Worker WS/poll refresh | SC-11 |
| Decimal unit + Foundry value=0 + MAX `/1e12` | SC-12 |
| Fee=0 unit + UI | SC-13 |
| Factory allowlist + V3 callback spoof + leftover | Aggregator security (design Phase 3) |
| Envelope / `price_impact_bps` / skip-if-allowance | API (design Phase 3) |
| localStorage recent-swaps + unaudited-ack | UI (design Phase 3) |
| Multicall3 balances never-sum; 1-conf; public RPC not Canteen `$RPC`; ETH-display copy; prague/`prevrandao`; never-call CCTP/Gateway/USYC/FxEscrow/Memo/Multicall3From | Arc Canteen full-index addendum |

## Verification Evidence (Phase 5 Continuation, 2026-08-28)

- **T4.3 Production CLMM Snapshot Quoter:**
  - Integration test `ready_and_clmm_only_snapshot_quotes_and_builds` in `crates/api-server/tests/chakra_rest_test.rs`.
  - Confirmed `/ready` returns `200 OK` (`ready: true`) on worker CLMM snapshots.
  - Confirmed `/quote` quotes CLMM pool with `dex_types: ["clmm"]`, `hop_fees: [30]`, `hop_factories: [CLMM_FACTORY]`.
  - Confirmed `/build_tx` succeeds with `to: AGGREGATOR`, `value: "0"`, and valid calldata targeting deployed contract.
  - All 53 tests in `api-server` and 196 tests across active workspace crates passing.

- **T2.5 Factory Discovery Scanner:**
  - Unit tests: `scripts/test_discovery_scan.py` (8/8 passing).
  - Public Arc RPC block windowing added to `scripts/discovery_scan.sh` (latest 10,000 blocks / `CHAKRA_SCAN_FROM_BLOCK`) to comply with RPC getLogs range limits.
  - Live execution against seeded/discovery factories verified (read-only, 0 errors, no allowlist changes).

- **T9.8 WCAG AA Theme Contrast:**
  - Unit test `packages/frontend/src/lib/contrast.test.ts` (vitest) tests contrast against actual backgrounds.
  - Raised `--text-muted` in `globals.css` from `#6b7280` to `#848fa0`.
  - DOM measurements in `docs/evidence/chakra-t98-manual-ux-a11y.json` updated: Status Banner (6.02:1, PASS), Sell Label (5.06:1, PASS), Buy Label (5.06:1, PASS), Slippage Header (5.59:1, PASS). Captured desktop and mobile audit screenshots.
  - Frontend unit suite: 67/67 tests passing, TypeScript clean.

- **T9.4 MetaMask E2E Harness (Playwright + dAppwright):**
  - Rewrote `packages/frontend/qa/wallet/swap-critical-path.spec.ts` with real `@tenkeylabs/dappwright` MetaMask automation for headed Chromium.
  - Tested graceful skip behavior when `QA_WALLET_SECRET` is unset (`1 skipped`, exit 0).
  - Updated `scripts/qa-wallet-setup.mjs` with MetaMask extension pre-downloading.
  - Updated `scripts/qa-wallet-validate.mjs` with live defaults.
  - Added `docs/qa-playwright-metamask.md`.
  - ESLint 0 errors, 0 warnings.
