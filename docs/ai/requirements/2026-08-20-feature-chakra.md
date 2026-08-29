---
phase: requirements
title: Requirements & Problem Understanding
description: Clarify the problem space, gather requirements, and define success criteria
feature: chakra
date: 2026-08-20
status: reviewed
---

# Requirements & Problem Understanding

**Product:** Chakra  
**Feature key:** `chakra`  
**Chain:** Arc testnet only (chain ID `5042002` / `0x4CEF52`)  
**Branch / worktree:** `feature-chakra` at `.worktrees/feature-chakra`  
**Status:** Phase 2 reviewed 2026-08-20. Ready for `dev-design`.

Sources: this repo’s LumAgg architecture (`README.md`), SCF #44 grant surface (`https://communityfund.stellar.org/submissions/recFRG56TbGtuXbMt`), Arc agent context (`~/.arc-canteen/context/AGENTS.md`, `circlefin-skills/use-arc.md`, `circlefin-skills/use-usdc.md`, Arc `contract-addresses.md`). Chakra is **not** a Stellar dual-chain product and **not** a Circle App Kit wrapper.

“Production” in this spec means **production-grade software on Arc testnet** (public UI/API, atomic execution, evidence pack). It does **not** mean Arc mainnet.

## Problem Statement

Arc liquidity, when it exists, is split across isolated AMM families. Today it is thin: Uniswap’s official `deployments.json` has no Arc entry, and Circle App Kit Swap is a closed 3-token API (USDC / EURC / cirBTC) that requires a kit key — not an open `quote` + wallet-ready calldata router. A trader who hits a single pool (or App Kit) takes avoidable slippage. Wallets and dApps have no public REST they can integrate in half an hour.

On Arc, USDC is **one economic balance** with two interfaces: native gas accounting at **18 decimals**, and an ERC-20 at `0x3600…0000` at **6 decimals** whose `transfer`/`approve`/`transferFrom` move that same native balance. Mixing those encodings silently mis-sizes swaps or gas. Circle’s guidance is to use the ERC-20 interface for application balances and transfers.

**Who is affected**

1. Retail traders on Arc testnet who want best execution across venues from a single swap UI.
2. Integrators (wallets, dApps, bots) who need an open REST + TypeScript SDK rather than a closed App Kit session.

**Current workaround**

- Swap in one pool, or through Circle App Kit’s closed swap.
- There is no event-driven multi-venue router, no split optimizer, and no atomic aggregator on Arc.

## Goals & Objectives

### Primary goals

- Best-execution swaps across multiple AMM types on Arc testnet (xy=k, stableswap, CLMM, Xylo, Presto).
- Sub-second-class quotes from Redis-hydrated local AMM/CLMM math (quote p95 &lt; 500 ms after warm Redis, measured at the API process).
- Atomic on-chain execution via a Solidity aggregator (`splitSwap` / multi-hop) with `minAmountOut`. No mid-route user-tx failure.
- Dense pro-terminal swap UX (Titan / Jupiter / 1inch class): balances, logos, % chips, route legs, impact, slippage, explorer link, recent swaps (this wallet, this browser).
- Integrator path: public REST, OpenAPI, TypeScript SDK, and a documented 30-minute walkthrough.
- Public Arc testnet deployment (UI + hosted API/worker/Redis + canonical venues).
- Grant-style differentiation evidence: venue matrix, split vs single-pool, on-chain **split** swap, Playwright MetaMask QA.

### Secondary goals

- **Canonical curated catalog** (supersedes the old three-token mBTC requirement and all live reseeding tasks): route only the canonical Arc USDC, EURC, and cirBTC tokens through the curated venues (XyloNet, Presto, UnitFlow V2.5) plus deterministic local fixtures. No Chakra-owned liquidity in the public release path.
- Event-driven pool freshness (WebSocket logs + short poll fallback) so a touched pool’s Redis state updates **≤ 5 s** after the swapping transaction is included.
- Zero protocol fee in v1 (venue LP fees + native USDC gas only).
- Permit2 token pull (`0x000000000022D473030F116dDEE9F6B43aC78BA3`).
- Rewrite in place on `feature-chakra`. `main` stays Stellar LumAgg until merge.
- Keep Lunex, UnitFlow V3, AchSwap/Arc Swap, and Synthra as explicit **promotion candidates** (watchlist) — never silently discovered or enabled.

### Non-goals (v1)

- Arc mainnet (does not exist; Circle skill: never target mainnet).
- Atomic arb vault / bot.
- Limit orders, DCA, portfolio, stats dashboard.
- Circle Modular Wallets / passkeys / Account Abstraction.
- Weighted (Comet / Balancer-style) pools.
- Circle App Kit Swap as a routed venue; App Kit `cirBTC` as a catalog token.
- USYC or other permissioned Circle assets.
- Protocol fee (0 bps).
- Third-party contract audit (testnet; document as follow-on).
- Dual-chain Stellar + Arc product.
- Playwright MCP for browser QA (use `playwright-cli` / project `qa:cli` only).
- Native USDC as `msg.value` swap input. There is no wrap/unwrap: ERC-20 USDC already moves the native balance.
- Arbitrary token import / user-pasted addresses in the v1 UI.
- Partner API keys as a v1 deliverable (public + rate limit is enough).
- Global swap indexer or public leaderboard (recent swaps are local).
- Firm/RFQ quotes. Quotes are indicative; `minAmountOut` is the on-chain guarantee.

## User Stories & Use Cases

### Retail trader

- As a trader, I want to connect an injected EIP-6963 wallet (MetaMask, Rabby, Coinbase Wallet, Rainbow) so I can swap without a custodial or passkey wallet. **QA of record is MetaMask;** other EIP-6963 wallets are best-effort.
- As a trader, I want the app to prompt `wallet_addEthereumChain` / switch to Arc testnet (`5042002`) so I cannot submit on the wrong chain.
- As a trader, I want to swap only among the v1 catalog — USDC (ERC-20 interface) ↔ EURC, USDC ↔ cirBTC, EURC ↔ cirBTC (direct or via hop).
- As a trader, I want a quoted expected out, min out, price impact, protocol fee 0, and visible route legs (including split percentages) so I can judge execution quality before signing.
- As a trader, I want 25 / 50 / 75 / MAX amount chips and a default 0.5% slippage setting so I can trade quickly. **MAX on USDC reserves a gas buffer** so a swap cannot drain the native balance needed to pay the tx.
- As a trader, I want Permit2 approve-once (allowance to Permit2) + a per-swap Permit2 signature so I do not grant unlimited `approve` to the aggregator itself.
- As a trader, I want an atomic swap: either the full route (including splits) settles or the transaction reverts.
- As a trader, I want an Arcscan link after success and a **recent-swaps list for this connected address in this browser** so I can verify settlement.
- As a trader, I want swap amounts and USDC **swap** balances shown via the ERC-20 6 dp interface, with native gas cost shown separately in USDC using 18 dp accounting, so decimals are never mixed on screen.
- As a trader with empty testnet funds, I want a clear link to the [Circle faucet](https://faucet.circle.com) (Arc Testnet, USDC and EURC) so I can get tokens; cirBTC is acquired through the route itself (e.g. USDC → EURC → cirBTC), not prefunded.
- As a trader, I want to see **venue names** (Xylo, Presto, UnitFlow, etc.) and a **persistent Arc Testnet liquidity warning** so I know testnet liquidity may deplete or reset and unavailable routes are never hidden behind an artificial pool.

### Integrator

- As an integrator, I want `GET /api/v1/tokens` → `GET /api/v1/quote` → `POST /api/v1/build_tx` so I can build a swap without holding user keys.
- As an integrator, I want OpenAPI plus a TypeScript SDK with a short example that completes quote + `build_tx` in about 30 minutes.
- As an integrator, I want public `/health` and `/ready` so I can gate traffic on process liveness vs routing-data readiness.

### Operator

- As an operator, I want a single Redis writer (worker) and a stateless API so I can scale quote serving independently.
- As an operator, I want bootstrap + discovery + event-driven refresh so I am not full-sweeping the market on a timer.
- As an operator, I want Ownable + pausable (non-upgradeable) aggregator control so I can halt swaps on testnet without a proxy.

### Key workflows

1. **Connect and gate:** open UI → EIP-6963 connect → ensure chain `5042002` → show ERC-20 catalog balances (USDC/EURC 6 dp, cirBTC 8 dp) and native USDC gas (18 dp). Empty USDC/EURC points at the Circle faucet; cirBTC has no faucet (acquired via swap route).
2. **Quote:** enter amount → debounce → `/quote` → render expected out, impact, route/split legs, protocol fee = 0. Quote auto-refreshes; it is not a firm fill.
3. **Execute:** `/build_tx` → wallet signs Permit2 typed data (if needed) + swap tx → submit → wait for ~0.5s finality → Arcscan link → recent-swaps row.
4. **Integrator:** copy OpenAPI/SDK example → quote a pair → build calldata → sign with own wallet tooling. Gate on `/ready`.

### Edge cases

- Redis miss / worker not ready → `/ready` false; UI shows “routing data warming”, no stale invented quote.
- No route (zero liquidity, empty venue, venue paused/stale/removed, incomplete CLMM tick coverage) → explicit no-route error (`NO_ROUTE`), not a zero output and never a fallback to an artificial pool.
- Failed venue discovery (bytecode mismatch, non-canonical token endpoints, missing factory membership, zero reserves, failed probe quote) → venue marked unavailable; aggregator **never auto-reseeds** a failed venue.
- Price impact below split threshold and paths not competitive → single path, `is_split=false`.
- Documented size where split beats single path → `is_split=true` with ≥2 sub-routes that **never share a pool** (shared-pool split rejected to prevent double-counting downstream liquidity).
- Slippage / `minAmountOut` breach on-chain → full revert.
- Aggregator paused → swap tx reverts; UI surfaces pause.
- Wallet on wrong chain → block submit until switch.
- Native 18 dp encoding used as if it were ERC-20 USDC 6 dp → must be impossible in quote math, UI parse, and contracts (`msg.value` = 0 for v1 swaps).
- USDC MAX with no gas reserve → rejected; chip/button leaves a buffer.
- Unaudited venue contracts → UI warning before first interaction.
- Permit2 allowance missing → guided approve-to-Permit2, then swap.
- Rate limit `429` on API → UI retry / backoff, no double-submit.
- Testnet liquidity depletes or resets → quotes degrade honestly (NO_ROUTE / thinner routes); large reseeding is **not** a release gate (Circle guidance: small test amounts, realistic slippage).

## Success Criteria

v1 is done only when **all** of the following are true and evidenced:

| ID | Criterion |
|----|-----------|
| SC-1 | Quote returns a route for USDC↔EURC and USDC↔cirBTC, and EURC↔cirBTC (direct or via hop, e.g. USDC → EURC → cirBTC). Catalog tokens only (USDC, EURC, cirBTC). |
| SC-2 | Split optimizer produces `is_split=true` on a documented size where it beats single-path and no sub-route reuses a pool; evidence checked into the repo (deterministic local execution proof, plus faucet-sized external canaries). |
| SC-3 | UI critical path: connect injected wallet → auto-add/switch Arc `5042002` → quote (legs, impact, fee 0) → Permit2 → swap → Arcscan link → recent-swaps row for this wallet. Gas shown separately from swap amounts. |
| SC-4 | At least one **on-chain split** (≥2 sub-routes executed atomically in one tx) verified on `https://testnet.arcscan.app` (faucet-sized canary). A multi-hop single-path swap is extra evidence, not a substitute. |
| SC-5 | Public UI URL and public `/api/v1/quote` + `/api/v1/health` + `/api/v1/ready`. |
| SC-6 | OpenAPI + TypeScript SDK example completes quote + `build_tx`. |
| SC-7 | Playwright CLI + MetaMask harness on Arc testnet for the critical path (not injected-provider-only). |
| SC-8 | Venue comparison matrix (≥3 pairs × ≥3 sizes) and split-vs-single benchmark in repo. |
| SC-9 | Integrator 30-minute walkthrough in docs. |
| SC-10 | Quote p95 &lt; 500 ms after warm Redis, measured **at the API process** (exclude client RTT). |
| SC-11 | Worker writes the touched pool’s Redis key **≤ 5 s** after the swapping transaction is included (WS or poll fallback). |
| SC-12 | Native 18 dp vs ERC-20 6 dp USDC never mixed in quotes, UI, or contracts. Swap USDC uses the ERC-20 interface. USDC MAX reserves a gas buffer. Aggregator `msg.value` is 0. |
| SC-13 | Zero protocol fee in quote breakdown and on-chain output. |
| SC-14 | No Chakra-owned liquidity in the public release path: `mBTC` and all Chakra-deployed/owned pools exist only as deterministic chain-31337 local fixtures; the public catalog is exactly USDC/EURC/cirBTC. |
| SC-15 | Startup discovery verifies each manifest venue (bytecode, canonical token endpoints, factory membership, nonzero reserves, probe quote); a failed venue becomes unavailable and yields `NO_ROUTE`, and the aggregator never automatically reseeds it. |
| SC-16 | Lunex, UnitFlow V3, AchSwap/Arc Swap, and Synthra are recorded as explicit promotion candidates (watchlist) in the manifest/docs; none is silently enabled. |

Grant-style evidence pack (done bar): venue matrix, split vs single-pool, 30-min integrator walkthrough, on-chain split swap (faucet-sized), Playwright MetaMask QA, API/SDK smoke, coverage + latency.

## Constraints & Assumptions

### Technical constraints

- Arc testnet only. RPC `https://rpc.testnet.arc.io`, WS `wss://rpc.testnet.arc.io`, explorer `https://testnet.arcscan.app`.
- Dual USDC: native gas **18 decimals**; ERC-20 USDC `0x3600000000000000000000000000000000000000` **6 decimals** — **same underlying balance**. EURC `0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a` **6 decimals**.
- Application USDC balances and swap amounts use the ERC-20 interface (Circle recommendation). Gas estimates use native 18 dp and are labeled as gas.
- Arc testnet minimum base fee 20 gwei; UI/wallet must not submit below that.
- Permit2 predeploy `0x000000000022D473030F116dDEE9F6B43aC78BA3`.
- ~0.5 s deterministic finality. Foundry / viem / wagmi. Use viem `arcTestnet`; do not invent a custom chain definition.
- Always verify wallet chain ID `5042002` before submit.
- Never commit private keys. Foundry `--private-key` only for local testing. `arc-canteen` keys stay in `~/.arc-canteen/wallet.yaml`. Secrets in env / gitignored `.env`.
- Warn before interacting with unaudited venue contracts.
- Playwright CLI only for browser QA (no Playwright MCP).
- Keep LumAgg principles: topology vs state, single Redis writer, event-driven freshness, local quote math, split only when warranted, atomic aggregator execution.

### Product / UX constraints

- Focused swap app only (no portfolio / stats / limit / DCA chrome).
- Visual: dense pro terminal, desktop-primary; mobile stacked layout must not hide the confirm CTA.
- EIP-6963 injected wallets first. MetaMask is the harness wallet of record.
- Default UI slippage 0.5%.
- v1 UI token list is the catalog of three tokens only.

### Business constraints

- License Apache-2.0.
- 0 protocol fee.
- Public testnet deploy; no mainnet.

### Time / budget

- Planning estimate after design approval: ~11–16 focused implementation days (not a calendar SLA).
- No budget cap specified. Hosting is a small Vercel + one API/worker/Redis host.

### Locked assumptions (accepted, not open)

- Redis key prefix `chakra:`.
- Split defaults port LumAgg: `PATH_FINDER_MAX_HOPS=3`, `MAX_SPLITS=5`, `SPLIT_THRESHOLD_BPS=5`, `SPLIT_COMPETITIVE_DELTA_BPS=50`. **Split plans whose sub-routes reuse a pool are rejected** (a shared downstream pool cannot be quoted independently twice).
- Aggregator is **Ownable, pausable, non-upgradeable** in v1 (redeploy on testnet if needed).
- **v1 routable catalog is exactly** ERC-20 USDC, EURC, **cirBTC** (canonical Arc tokens). WUSDC or any other wrapper identity is excluded from v1. mBTC and all Chakra-owned pools are removed from the public release path and exist only as deterministic chain-31337 local fixtures.
- cirBTC has **no faucet and no Chakra mint**: acquire it through the route itself (e.g. USDC → EURC → cirBTC). Do not require a prefunded target-token balance for canaries.
- Venue strategy (canonical curated, 2026-08-29 venue decision):
  - **Default-on:** XyloNet (factory `0x60EDeFB094B84BBC6430cc130B358A43Ba1979e2`, router `0x73742278c31a76dBb0D2587d03ef92E6E2141023`, verified stable pool `0x3DF3966F5138143dce7a9cFDdC2c0310ce083BB1`, documented aggregator interface, 4 bps fee), Presto (normalized hub `0x5794a8284A29493871Fbfa3c4f343D42001424D6`, USDC/EURC discovery only), and UnitFlow V2.5 (factory `0xd67F63A4F26a497b364d1C82e6747Aec8B5743a5`, canonical EURC/cirBTC pair `0x268DC75517EaFc6e0D52666639529e5DAB8c9200`, 30 bps XYK).
  - **Watchlist (promotion candidates, never silently enabled):** Lunex, UnitFlow V3, AchSwap / Arc Swap, Synthra.
  - **Excluded from v1:** LiftUp (explicitly artificial local/community fallback), Curve wrapper pool (WUSDC only), LI.FI (meta-aggregator, not independent liquidity).
  - **Not presently deployable:** Uniswap, Aerodrome/Velodrome, Fluid, Euler (Arc announcements only, no current public testnet pools).
  - No healthy direct canonical USDC/cirBTC venue exists; Chakra routes that pair atomically as USDC → EURC → cirBTC.
  - Testnet liquidity may deplete or reset; large reseeding is **not** a release gate. Use small test amounts and realistic slippage (Circle guidance).
- Recent swaps: local to the connected address + this browser. No global indexer in v1.
- Quotes are point-in-time; UI auto-refreshes; `minAmountOut` is the settlement protection.
- USDC MAX reserves gas using a buffer derived from current fee data (`eth_gasPrice` / fee history, min 20 gwei) × estimated gas.
- UI host: Vercel. API/worker/Redis: a single small host or compose on a VPS (exact vendor chosen in implementation).
- Foundry for Solidity; keep Rust for worker/router/API; Next.js + wagmi/viem + Tailwind for UI.
- Docs live under configured `paths.docs` (`docs/ai`); `docs init-feature chakra` paths are authoritative.
- Task tracing via `ai-devkit task` was unavailable (`unknown command 'task'`); continue without it.

## Questions & Open Items

No unresolved product questions. Phase 2 review converted remaining gaps into named assumptions or non-goals:

| Item | Disposition |
|------|-------------|
| Exact API/worker/Redis host vendor | Deferred to implementation / deployment. |
| Which third-party factories exist on Arc at deploy time | Curated manifest (XyloNet, Presto, UnitFlow V2.5) + discovery scan; watchlist venues recorded but never silently enabled. Extra tokens stay out of the v1 catalog. |
| Stableswap `A` and CLMM fee tiers for seed pools | **Fixture-only** (chain-31337): stable `A=100` for USDC/EURC; CLMM 5 bps and/or 30 bps; xy=k 30 bps. Not deployed to Arc by the operator workflow. |
| Mock BTC vs App Kit cirBTC | **Superseded:** mBTC removed from the public path; canonical **cirBTC** (8 dp) is the catalog token, acquired via the route itself. |
| Wrap native USDC ↔ ERC-20 | Not applicable. Same balance, two encodings. Use ERC-20 for swaps; reserve gas on MAX. |
| Recent swaps storage | Locked: local to wallet + browser. |
| Quote firmness | Locked: indicative + `minAmountOut`. |
| SC-11 SLA | Locked: ≤ 5 s after inclusion. |
| Partner API keys | Non-goal for v1. |
| Third-party audit | Explicit non-goal; follow-on. |
| Protocol fee, Modular Wallets, arb, limit/DCA, USYC | Explicit non-goals. |
| UnitFlow V3, Lunex, AchSwap/Arc Swap, Synthra | Watchlist / promotion candidates after v1; not silently enabled. |

## Approaches considered

Phase 2 re-validated the architecture and two catalog/USDC choices that were implicit in Phase 1.

| Approach | Trade-off | Decision |
|----------|-----------|----------|
| **A. Port LumAgg worker/Redis/router/API + Solidity aggregator + focused Next.js UI** | Highest fidelity to proven split math; more port work | **Chosen** |
| **B. Wrap Circle App Kit Swap** | Fast, closed 3-token API, kit key, not a DEX aggregator | Rejected |
| **C. TypeScript-only router** | Faster to ship a demo; throws away PathFinder / Brent splitter / local math | Rejected |
| **D. Route App Kit cirBTC / closed liquidity instead of seeding mBTC** | Looks more “official”; depends on a closed API we already rejected as a venue; no public factory to split across | Rejected |

| Catalog | Trade-off | Decision |
|---------|-----------|----------|
| Freeze 3 tokens (USDC, EURC, mBTC) | Demoable, matches locked Q&A; mBTC is Chakra-owned and requires reseeding; may ignore organic pools | **Superseded 2026-08-29** by the canonical curated strategy |
| **Canonical curated: USDC, EURC, cirBTC** | Matches the Arc canonical token set; no Chakra-owned liquidity; cirBTC acquired via route; no reseeding gate | **Chosen** |
| Grow `/tokens` from every discovered pool | More “aggregator-like”; thin/unknown tokens, UI clutter, faucet story breaks | Rejected for v1 |
| Include cirBTC if an ERC-20 appears | Closer to App Kit surface; still no open venue math unless we seed around it | Rejected for v1 (superseded — cirBTC is now canonical in the catalog) |

| Venue strategy | Trade-off | Decision |
|----------------|-----------|----------|
| **Canonical curated** (XyloNet, Presto, UnitFlow V2.5 default-on; watchlist for Lunex/V3/AchSwap/Synthra) | Real external liquidity; no Chakra-owned venues; documented integration packs | **Chosen** (2026-08-29 venue decision) |
| Chakra-owned seed + discovery hybrid | Deterministic splits; but requires operator reseeding, mBTC mint, and Chakra liquidity on Arc | Rejected — mock pools stay chain-31337 fixtures only |

| USDC handling | Trade-off | Decision |
|---------------|-----------|----------|
| ERC-20 interface for swap amounts/balances; native 18 dp for gas only; MAX reserves gas | Matches Circle docs; one balance | **Chosen** |
| Accept `msg.value` native USDC as swap input | Mixes encodings; aggregator payable path | Rejected |
| Separate wrap/unwrap token | Implies two balances; Arc does not work that way | Rejected |

## Phase 2 review notes

Reviewed against the requirements README template (all six sections present; time/budget added). Cross-checked design and testing docs, Arc contract-addresses / gas docs, and `use-usdc` dual-balance note.

**Validated:** problem, users, goals/non-goals, stories, SC-1…13, LumAgg-port approach, hybrid venues, Permit2, 0 fee, EIP-6963, dense terminal, public REST+SDK, rewrite on `feature-chakra`, testnet-only.

**Clarified in this review:** Arc USDC is one balance / two encodings; catalog freeze; SC-4 requires a real split; SC-5 includes `/ready`; SC-10 is server-side; SC-11 is ≤ 5 s; USDC MAX gas buffer; local recent swaps; MetaMask as QA wallet of record; “production” = production-grade on testnet.

**Not changed:** architecture choice A; token set USDC+EURC+mBTC; Permit2; 0 fee; non-goals for arb/limit/DCA/AA/App Kit/mainnet.

Template compliance: Problem Statement, Goals & Objectives, User Stories & Use Cases, Success Criteria, Constraints & Assumptions, Questions & Open Items — all filled. No open product questions remain.
