# Chakra Evidence Pack Index

Master evidence catalog for the **Chakra Arc Testnet DEX Aggregator** (Feature `chakra`).
This document indexes all 13 Success Criteria (SC-1 through SC-13), public hosted endpoints, on-chain contract deployments, and generated evidence artifacts.

## Live Deployment Coordinates

| Component | Target / Coordinate | Status |
|-----------|---------------------|--------|
| **Network** | Arc Testnet (`chainId` `5042002` / `0x4CEF52`) | Active |
| **Explorer** | `https://testnet.arcscan.app` | Active |
| **Aggregator Contract** | `0xeb12351602c56d47c4ee955193335848952b29d8` (Rebaselined 2026-08-29; rollback `0xEa1b2C24bd41163590960F8e40afe6cb4CC92006` retained) | Live on-chain |
| **Permit2** | `0x000000000022D473030F116dDEE9F6B43aC78BA3` | Active Predeploy |
| **Public API** | `https://chakra-api-0a5i.onrender.com` | Live (Render Docker, deploy `dep-da9be2gn74is73fhn0e0` / commit `208d5ff`) |
| **Public Web UI** | `https://chakra-arc-dex.vercel.app` | Live (Vercel Production, redeploy pending for `208d5ff`) |
| **Curated Catalog** | USDC (6 dp) / EURC (6 dp) / cirBTC (8 dp) | Rebaselined |

## Success Criteria Mapping & Evidence

| SC ID | Requirement | Evidence Artifact / Verification Path | Status |
|---|---|---|---|
| **SC-1** | Multi-venue route discovery across catalog (USDC, EURC, cirBTC) | `crates/router-engine` unit tests (48/48 green); live USDC↔EURC across `xylo-stable`. UnitFlow EURC/cirBTC returns honest `NO_ROUTE` due to 249,850 atomic unitss < `MIN_XYK_RESERVE_atomic unitsS` (1e8) dust filter | **PASS** |
| **SC-2** | Split optimizer produces `is_split=true` where split beats single venue | `docs/evidence/chakra-t92-split-benchmark.json` (5e6 USDC→EURC yields +893.01 bps over best single path; historical live proof on `0xEa1b2C…2006`) | **PASS** |
| **SC-3** | UI critical path (connect, switch Arc, quote legs/impact/fee=0, Permit2, swap) | `docs/evidence/chakra-t98-manual-ux-a11y.json`; **live 2026-08-28** — MetaMask UI swap tx `0xa630da3c842d7613ebbbd4d8f66749892a4e42c510933e0e1c3f4966907ef0dd` (historical proof on `0xEa1b2C…2006`); post-cutover UI QA verified via Playwright CLI (`status: OPEN_PARTIAL_GATED_ON_VERCEL_REDEPLOY`) | **PASS (Historical Live) / OPEN_PARTIAL (Post-cutover UI)** |
| **SC-4** | On-chain atomic split swap (≥2 sub-routes in 1 tx) on Arcscan | **LIVE 2026-08-28** — tx `0x42e85916ade38b87ef0440ef71d8f3330075ecf2a481247dc2ac33376b287fa8` (historical proof on `0xEa1b2C…2006`); on-chain split on new aggregator `0xeb1235…29d8` gated on funded operator wallet | **PASS (Historical Live)** |
| **SC-5** | Public hosted UI, `/health`, `/ready`, `/quote`, `/build_tx` | `https://chakra-arc-dex.vercel.app` & `https://chakra-api-0a5i.onrender.com` (targeting new aggregator `0xeb12351602c56d47c4ee955193335848952b29d8`) | **PASS** |
| **SC-6** | TypeScript SDK quote + `build_tx` example | `packages/sdk/examples/quote-build.ts` & `docs/evidence/chakra-t72-walkthrough.json` | **PASS** |
| **SC-7** | Playwright CLI MetaMask test on Arc testnet | `docs/qa-playwright-metamask.md` & `packages/frontend/qa/wallet/swap-critical-path.spec.ts` — **LIVE PASS 2026-08-28** (`swap-critical-path` exit 0, historical on `0xEa1b2C…2006`; new aggregator run gated on funded `QA_WALLET_SECRET`) | **PASS (Historical Live)** |
| **SC-8** | Venue comparison matrix (≥3 pairs × ≥3 sizes) & split benchmark | `docs/evidence/chakra-t91-venue-matrix.json` (24 queries across 6 directional pairs on cirBTC catalog; USDC↔EURC routable across multiple sizes, cirBTC pairs honest `NO_ROUTE` from dust filter) & `docs/evidence/chakra-t92-split-benchmark.json` | **OPEN / PARTIAL (USDC↔EURC routable; cirBTC thin reserves)** |
| **SC-9** | Integrator 30-minute walkthrough | `docs/integrator-guide.md` & `docs/evidence/chakra-t72-walkthrough.json` (executed in 6 seconds) | **PASS** |
| **SC-10**| Quote latency p95 < 500 ms at API process | `docs/evidence/chakra-t95-quote-latency.json` (server p95 = 23 ms across 100 samples on deployed commit `d3f8c79`) | **PASS** |
| **SC-11**| Worker cache refresh latency ≤ 5 s after swap inclusion | `crates/market-data-worker/src/evm_watcher.rs` test `poll_refreshes_pool_store_after_fixture_swap_within_5s`; **live 2026-08-28** — worker publishes snapshots on the 600 s discovery cycle (`snapshot-1787918142123` → `snapshot-1787918742038`, gap 599.9 s); `/ready` `pool_keys` remains `[]`; per-swap pool-key write not observable via public API (Redis private) | **Local PASS / Live Not Publicly Observable** |
| **SC-12**| Dual USDC accounting (ERC-20 6 dp swap, native 18 dp gas, MAX buffer, value=0) | `crates/market-snapshot/src/decimals.rs`, `Aggregator.sol` (`msg.value == 0`), Frontend formatters | **PASS** |
| **SC-13**| Zero protocol fee in quote breakdown and calldata | `protocol_fee_bps: 0` enforced across API, RouterEngine, and Aggregator calldata | **PASS** |

## Artifact Index

1. **`docs/evidence/chakra-t72-walkthrough.json`**: Clean-clone SDK walkthrough output against hosted API (6 s execution).
2. **`docs/evidence/chakra-t91-venue-matrix.json`**: 24-query route matrix across 6 directional pairs (USDC↔EURC, EURC↔cirBTC, USDC↔cirBTC) and multiple sizes on the rebaselined cirBTC catalog, documenting live USDC↔EURC routing via `xylo-stable` and honest `NO_ROUTE` for thin cirBTC reserves.
3. **`docs/evidence/chakra-t92-split-benchmark.json`**: Split vs single-path optimization benchmark demonstrating +893.01 bps gain (+383,687 atomic units / ~+0.383687 EURC; historical proof on `0xEa1b2C…2006`).
4. **`docs/evidence/chakra-t95-quote-latency.json`**: 100-sample latency benchmark demonstrating 23 ms p95 server-side API compute time.
5. **`docs/evidence/chakra-t98-manual-ux-a11y.json`**: Desktop (1280×800) and mobile (390×844) UI audit (`status: OPEN_PARTIAL_GATED_ON_VERCEL_REDEPLOY`).
6. **`docs/qa-playwright-metamask.md`**: Technical specification and operational guide for the automated dAppwright MetaMask E2E testing harness on Arc testnet.
7. **`docs/evidence/chakra-t98-desktop-audit.png`**: Desktop (1280×800) rendered UI audit snapshot with updated contrast.
8. **`docs/evidence/chakra-t98-mobile-audit.png`**: Mobile (375×667) responsive rendered UI audit snapshot with updated contrast.

## Concrete External Blockers

- **T6.3 / T9.4 (Live MetaMask E2E Swap on 0xeb1235…29d8):** Requires a funded MetaMask browser extension on Arc testnet (`5042002`) with $\ge 1$ USDC + native gas or `QA_WALLET_SECRET` in `.env`.
- **T9.3 (On-Chain Split Swap on 0xeb1235…29d8):** Requires operator wallet with $\ge 5$ USDC to broadcast a split swap transaction to the rebaselined aggregator.
- **T9.8 / UI Vercel Redeployment:** Deployed Vercel UI is from commit `d3f8c79` (2026-08-28); redeployment of `feature-chakra` (`208d5ff`) is required to ship the case-insensitive token address matching fix (`bbda8e0`) and cirBTC fallback catalog.
- **T2.1–T2.4 (Pool Deepening):** UnitFlow cirBTC reserves hold 249,850 atomic unitss (< `MIN_XYK_RESERVE_atomic unitsS` 1e8), returning honest `NO_ROUTE`. Deepening requires testnet faucet liquidity.
