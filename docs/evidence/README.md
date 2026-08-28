# Chakra Evidence Pack Index

Master evidence catalog for the **Chakra Arc Testnet DEX Aggregator** (Feature `chakra`).
This document indexes all 13 Success Criteria (SC-1 through SC-13), public hosted endpoints, on-chain contract deployments, and generated evidence artifacts.

## Live Deployment Coordinates

| Component | Target / Coordinate | Status |
|-----------|---------------------|--------|
| **Network** | Arc Testnet (`chainId` `5042002` / `0x4CEF52`) | Active |
| **Explorer** | `https://testnet.arcscan.app` | Active |
| **Aggregator Contract** | `0xEa1b2C24bd41163590960F8e40afe6cb4CC92006` (Codesize: 11,128 bytes, non-upgradeable) | Live on-chain |
| **Permit2** | `0x000000000022D473030F116dDEE9F6B43aC78BA3` | Active Predeploy |
| **Public API** | `https://chakra-api-0a5i.onrender.com` | Live (Render Docker, deploy `dep-da8jk6cs728c73bvdrb0`) |
| **Public Web UI** | `https://chakra-arc-dex.vercel.app` | Live (Vercel Production) |

## Success Criteria Mapping & Evidence

| SC ID | Requirement | Evidence Artifact / Verification Path | Status |
|---|---|---|---|
| **SC-1** | Multi-venue route discovery across catalog (USDC, EURC, mBTC) | `crates/router-engine` unit tests (48/48 green); live USDC↔EURC across `chakra-stable` and `xylo` | **PASS** |
| **SC-2** | Split optimizer produces `is_split=true` where split beats single venue | `docs/evidence/chakra-t92-split-benchmark.json` (5e6 USDC→EURC yields +893.01 bps over best single path) | **PASS** |
| **SC-3** | UI critical path (connect, switch Arc, quote legs/impact/fee=0, Permit2, swap) | `docs/evidence/chakra-t98-manual-ux-a11y.json` (UI layout & controls audited; on-chain wallet send pending funded MetaMask) | **UI AUDITED / Wallet Gated** |
| **SC-4** | On-chain atomic split swap (≥2 sub-routes in 1 tx) on Arcscan | Requires ≥5 USDC operator balance; live split route proven in `/quote` and `/build_tx` | **Gated (Operator Funds)** |
| **SC-5** | Public hosted UI, `/health`, `/ready`, `/quote`, `/build_tx` | `https://chakra-arc-dex.vercel.app` & `https://chakra-api-0a5i.onrender.com` | **PASS** |
| **SC-6** | TypeScript SDK quote + `build_tx` example | `packages/sdk/examples/quote-build.ts` & `docs/evidence/chakra-t72-walkthrough.json` | **PASS** |
| **SC-7** | Playwright CLI MetaMask test on Arc testnet | `docs/qa-playwright-metamask.md` & `packages/frontend/qa/wallet/swap-critical-path.spec.ts` (dAppwright harness code complete; live headed run gated on `QA_WALLET_SECRET`) | **Harness Implemented / Live Gated** |
| **SC-8** | Venue comparison matrix (≥3 pairs × ≥3 sizes) & split benchmark | `docs/evidence/chakra-t91-venue-matrix.json` & `docs/evidence/chakra-t92-split-benchmark.json` | **OPEN / PARTIAL (Gated on T2.1–T2.4 Re-seed)** |
| **SC-9** | Integrator 30-minute walkthrough | `docs/integrator-guide.md` & `docs/evidence/chakra-t72-walkthrough.json` (executed in 6 seconds) | **PASS** |
| **SC-10**| Quote latency p95 < 500 ms at API process | `docs/evidence/chakra-t95-quote-latency.json` (server p95 = 23 ms across 100 samples on deployed commit `d3f8c79`) | **PASS** |
| **SC-11**| Worker cache refresh latency ≤ 5 s after swap inclusion | `crates/market-data-worker/src/evm_watcher.rs` test `poll_refreshes_pool_store_after_fixture_swap_within_5s` | **Local PASS / Live Follows T9.3** |
| **SC-12**| Dual USDC accounting (ERC-20 6 dp swap, native 18 dp gas, MAX buffer, value=0) | `crates/market-snapshot/src/decimals.rs`, `Aggregator.sol` (`msg.value == 0`), Frontend formatters | **PASS** |
| **SC-13**| Zero protocol fee in quote breakdown and calldata | `protocol_fee_bps: 0` enforced across API, RouterEngine, and Aggregator calldata | **PASS** |

## Artifact Index

1. **`docs/evidence/chakra-t72-walkthrough.json`**: Clean-clone SDK walkthrough output against hosted API (6 s execution).
2. **`docs/evidence/chakra-t91-venue-matrix.json`**: 15-scenario route matrix across 5 pairs and 3 sizes, analyzing live routability and documenting the testnet mBTC liquidity gap.
3. **`docs/evidence/chakra-t92-split-benchmark.json`**: Split vs single-path optimization benchmark demonstrating +893.01 bps gain (+383,687 atomic units / ~+0.383687 EURC).
4. **`docs/evidence/chakra-t95-quote-latency.json`**: 100-sample latency benchmark demonstrating 23 ms p95 server-side API compute time on deployed revision `dep-da8jk6cs728c73bvdrb0`.
5. **`docs/evidence/chakra-t98-manual-ux-a11y.json`**: Desktop and mobile UI viewport audit with measured DOM contrast ratios and focus order (`status: OPEN_PARTIAL`).
6. **`docs/qa-playwright-metamask.md`**: Technical specification and operational guide for the automated dAppwright MetaMask E2E testing harness on Arc testnet.
7. **`docs/evidence/chakra-t98-desktop-audit.png`**: Desktop (1280×800) rendered UI audit snapshot with updated contrast.
8. **`docs/evidence/chakra-t98-mobile-audit.png`**: Mobile (375×667) responsive rendered UI audit snapshot with updated contrast.

## Concrete External Blockers

- **T6.3 / T9.4 (Live MetaMask E2E Swap):** Requires a funded MetaMask browser extension on Arc testnet (`5042002`) with $\ge 1$ USDC + native gas or `QA_WALLET_SECRET` in `.env`.
- **T9.3 (On-Chain Split Swap Broadcast):** Requires operator wallet with $\ge 5$ USDC (current balance is $\sim 1.036$ USDC) to broadcast the 5e6 split swap transaction to the live aggregator.
- **T2.1–T2.4 (Pool Deepening):** Re-seeding pools to target 200k/10k sizes requires Circle faucet inventory to enable live routing for mBTC pairs.
