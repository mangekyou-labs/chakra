# Chakra — Circle Developer Grant Proposal

**Application draft · 30 September 2026**  
**Request: 24,000 USDC · four milestones · 20 sequential weeks**

[Product](https://chakra-ag.vercel.app) · [Source](https://github.com/mangekyou-labs/chakra) · [Founder](https://github.com/MrSufferer) · [Demo](https://youtu.be/12hvXfd5rsE)

[Editable deck](Chakra-Circle-Grant-Pitch-Deck.pptx) · [PDF deck](Chakra-Circle-Grant-Pitch-Deck.pdf) · [Word proposal](Chakra-Circle-Grant-Proposal.docx)

The existing Arc Testnet proof is separated from the proposed Circle integration work. Historical QA figures are dated. Applicant/legal confirmations and upload links remain explicit submission inputs.

## Applicant Details

**Primary contact first name:** Kyler  
**Primary contact last name:** Nguyen  
**Email address:** kylernguyeen@gmail.com **[confirm spelling]**  
**Company Legal Entity Name:** Mangekyou Labs **[confirm against incorporation status]**  
**Company Doing-Business-As (DBA) name:** Chakra **[confirm whether this is a registered DBA; otherwise use it as the project name per form instructions]**

**Co-founder information (one line per founder):**  
Kyler Nguyen | CEO | LinkedIn: [add link or state unavailable] | GitHub: https://github.com/MrSufferer | Technical founder responsible for Chakra's Solidity execution contract, Rust routing/API, and TypeScript integration surface; public GitHub: https://github.com/MrSufferer

**Project website:** https://chakra-ag.vercel.app  
**Project X handle:** @chakraaggregator **[confirm]**  
**Project GitHub URL:** https://github.com/mangekyou-labs/chakra  
**Founder locations:** Kyler Nguyen, CEO, Da Nang, Vietnam **[confirm]**  
**Business location:** Vietnam **[confirm legal location]**  
**Is your business incorporated?:** [answer]

## Project Abstract

**Project Name (80-character limit):** Chakra

**One-line description (200-character limit):** Chakra is an open-source Arc DEX aggregator that compares supported USDC/EURC venues and returns wallet-ready swap transactions through one API and web app.

**What problem are you solving, why is it important, and what has kept it from being solved so far? (750-character limit):**

Arc has USDC and EURC liquidity spread across DEX venues with different pool math and swap interfaces. A wallet or payments app offering a USDC/EURC swap must select a venue, build its own route adapter, and arrange gas for the user. Circle user-controlled wallets and Gas Station can handle authorization and eligible sponsorship, but they still need a reliable Arc quote and transaction payload. Chakra connects venue-aware same-chain routing to that wallet path, with an honest no-route result when liquidity is unavailable.

**What is your solution to that problem? (750-character limit):**

Chakra's live Arc Testnet proof of concept quotes supported USDC/EURC pools and returns unsigned `build_tx` calldata for an external wallet to authorize. Its factory-gated aggregator settles atomic routes; unsupported or empty venues fail closed. This grant proposes a new SDK release with a Circle SCA adapter, distinct mined Arc Testnet SCA and Gas Station-sponsored swaps, mainnet integration readiness against a self-funded live route, and a CCTP V2 inbound USDC handoff. Arc mainnet deployment and an external-wallet proof are self-funded prerequisites, not Circle-funded deliverables.

**Team track record of acquiring these users/customers, with verifiable numbers (500-character limit):** Chakra has shipped an Arc Testnet app, quote/build API and wallet execution flow. A 30 September 2026 API fetch reports 5 indexed swaps, 5 USDC notional and 2 addresses, with about 44 hours of index freshness lag. These are testnet activity, not verified customers. Next: secure one Arc-supporting wallet integrator and publish its first swap receipt. Founder: https://github.com/MrSufferer

## Product Alignment Track

**Is your project currently live in production?:** [verify the current Arc mainnet deployment, live route, and transaction evidence at submission, then answer according to the form's definition; the hosted Arc Testnet proof is evidenced]

**Are you live on Arc?:** [select Arc Testnet based on existing evidence; also select mainnet if a verified Chakra deployment and currently quoteable route are public at submission]

**Other production chains:** None for Chakra **[confirm]**

**Circle products currently integrated (video must validate each):** USDC and EURC token usage on Arc Testnet **[select only exact applicable form options]**. Do not select Circle Wallets, Gas Station, or CCTP as currently integrated without working code and a demonstrable flow.

**Circle products planned:** Circle user-controlled Wallets / smart contract account **[match exact form options]**.  
**Other Circle products planned:** Gas Station, policy-bound on the Circle SCA path; CCTP V2 inbound USDC handoff.

## Milestones and Timelines

**Proposed Circle request:** 24,000 USDC over 20 sequential weeks after an agreed kickoff, led by one engineer. At Kyler's planning cost of 40 hours per week and 30 USDC per hour, this is 800 engineer hours × 30 USDC/hour. No other expense or markup is included. These are application estimates, not an award or guaranteed payout schedule. Portal dates should come from the agreed schedule. The budget covers new Circle integration, verification, and documentation; already shipped Chakra work and the self-funded mainnet proof are excluded.

Each milestone's deliverable, budget basis, metrics, and acceptance criteria below belong in its **Details** field. Replace evidence placeholders with direct public links before submission.

### 01 — Publish a Circle adapter for the Chakra SDK

**4,800 USDC; 4 weeks. Details (2,048-character limit):**

**Deliverable:** Publish a Circle user-controlled smart contract account (SCA) adapter in a new `@chakra-ag/sdk` release. It converts Chakra's existing unsigned `build_tx` response into Circle authorization data.

**Budget basis:** 160 hours × 30 USDC/hour = 4,800 USDC. Estimate: adapter and mapping 80 hours; examples and instructions 40; release and public proof 40.

**Metrics:**
- One public npm version newer than `0.3.0` [add link] and matching source tag [add link].
- Two runnable examples with commands and expected output [add links]; at least one builds Circle authorization data rather than stopping at a quote.
- One saved `build_tx` input, resulting authorization fields, and steps to reproduce the conversion [add link].

**Acceptance criteria:** All three metrics must be publicly checkable and the conversion must reproduce. An unpublished adapter, republished `0.3.0`, or quote-only examples fail. The existing quote/build client, OpenAPI, and guide are baseline work. A mined SCA swap belongs to milestone 02.

### 02 — Complete a swap with a Circle smart account

**8,400 USDC; 7 weeks. Details (2,048-character limit):**

**Deliverable:** Add a Circle SCA path for Chakra USDC/EURC swaps and configure it for the separately funded Chakra contract on Arc mainnet. Keep the external-wallet path working.

**Budget basis:** 280 hours × 30 USDC/hour = 8,400 USDC. Estimate: authorization path 120 hours; Testnet swap and debugging 80; external-wallet check 20; mainnet configuration 40; public proof 20.

**Metrics:**
- One mined, successful Arc Testnet USDC/EURC swap authorized by a Circle SCA: explorer transaction [add link] and matching quote, `build_tx`, and Circle authorization data [add links].
- One linked check that an external-wallet swap still works [add link]; a new receipt is not required.
- One dated Arc mainnet (chain 5042) SCA configuration and reproducible quote → `build_tx` → authorization payload for the self-funded contract and a USDC/EURC route live at review time [add links].

**Acceptance criteria:** The Testnet links must identify the SCA account, Chakra contract, tokens, chain, and successful result. The mainnet links must identify the deployed contract and live route. All three metrics must pass. A simulation, an external-wallet swap presented as SCA execution, or mainnet `NO_ROUTE` fails. The mainnet payload proves readiness; a mined mainnet SCA swap is not required. If the self-funded contract or route is unavailable, ask Circle to change this milestone date.

### 03 — Pay gas for a separate Circle smart account swap

**4,800 USDC; 4 weeks. Details (2,048-character limit):**

**Deliverable:** Sponsor eligible Circle SCA swaps with Gas Station and configure a bounded policy for the self-funded Arc mainnet contract. External-wallet swaps remain unsponsored.

**Budget basis:** 160 hours × 30 USDC/hour = 4,800 USDC. Estimate: integration and policy 60 hours; sponsored Testnet swap 40; failure checks 20; mainnet policy and checks 20; public proof 20.

**Metrics:**
- One mined, successful Arc Testnet USDC/EURC SCA swap, different from milestone 02's transaction [add explorer link], plus a redacted Circle record tying Gas Station sponsorship to that transaction [add link].
- One Testnet policy naming allowed chain, contract, transaction type, and spend limit; show reproducible ineligible and unavailable outcomes [add links].
- One dated active mainnet policy for the self-funded contract, with eligible, ineligible, and unavailable checks against a USDC/EURC route live at review time [add links].

**Acceptance criteria:** The explorer transaction and Circle sponsorship record must match. All policy limits and outcomes must be checkable. All three metrics must pass. Reusing milestone 02's transaction, showing only a policy, or mainnet `NO_ROUTE` fails. Mainnet policy checks prove readiness; a sponsored mainnet swap is not required. If the contract or route is unavailable, ask Circle to change this milestone date.

### 04 — Bring USDC to Arc with CCTP V2 and try a Chakra swap

**6,000 USDC; 5 weeks. Details (2,048-character limit):**

**Deliverable:** Publish a runnable CCTP V2 USDC handoff from a supported source chain to Arc mainnet, then try to route the received USDC through Chakra.

**Budget basis:** 200 hours × 30 USDC/hour = 6,000 USDC. Estimate: runnable handoff 80 hours; confirmed transfer and debugging 60; post-receive Chakra check 20; instructions and public proof 40.

**Metrics:**
- One confirmed source-chain USDC burn [add link] and one confirmed Arc mainnet receive [add link], with matching transfer/message ID and amount.
- One dated Chakra quote request using that Arc USDC after the receive [add link]. If it finds a route, save the `build_tx` payload [add link].
- One confirmed Chakra swap if a live route exists [add link]; otherwise one dated venue scan and Chakra `NO_ROUTE` response showing that no swap could be built or sent [add links].
- One public runnable handoff with commands, expected output, and steps to inspect the receipts and Chakra result [add link].

**Acceptance criteria:** A reviewer can run the handoff and match both transaction links to the same transfer. The Chakra attempt must follow the receive. All four metrics must pass, using the applicable third outcome. Dry runs, unrelated transactions, and pre-receive quotes fail. A general cross-chain swap interface is outside this milestone.

**Total:** 24,000 USDC; 20 weeks; 800 engineer hours at 30 USDC/hour. A mined two-venue split is outside these paid milestones. Do not seed liquidity to create evidence. Mainnet SCA or sponsored swap transactions, if achieved, are extra evidence rather than payment conditions.

**Budget explanation for reviewers:** One engineer plans 40 hours a week for 20 sequential weeks at 30 USDC per hour. The 800 hours cost 24,000 USDC. Each milestone's weeks times 1,200 USDC per week gives its requested amount: 4,800 / 8,400 / 4,800 / 6,000 USDC. The hours cover implementation, debugging, public evidence, and documentation. There is no separate expense or markup line. Chakra does not charge Circle for the existing router, quote/build API, or self-funded Arc mainnet proof. These are estimated work weeks, not recorded labor or a market-rate claim. If a milestone needs fewer weeks, reduce its price before submission. Circle agrees on final milestones and payments after selection.

## Project Traction and Roadmap

**Where can we verify traction? (300-character limit):** Arc Testnet controlled-QA swap: https://testnet.arcscan.app/tx/0xee7bc19a990ce6691a68e9b387585baee13edc846cbf3a43551ab3dd7cfcda6c ; product stats: https://chakra-ag.vercel.app/stats . Single-route testnet activity, not organic customer volume.

**Nonpublic verification folder:** [Google Drive folder with view/download access if supporting documents exist; otherwise leave blank as the form permits]  
**Are you funded?:** [answer, including any Arc Microgrant award if one exists at submission]

**Technical Roadmap (2–4 lines; 1,250-character limit; replace relative dates for final portal entry):**

Publish an SDK version with a Circle SCA adapter | Circle Wallets | kickoff + 4 weeks | npm version, source tag, two examples, saved input and output  
Complete an SCA swap on Arc Testnet; configure the mainnet path | Circle Wallets | kickoff + 11 weeks | Testnet transaction, authorization data, mainnet configuration and live route  
Use Gas Station for a separate Testnet swap; set a mainnet policy | Gas Station | kickoff + 15 weeks | sponsored transaction, policy limits, rejected cases, mainnet checks  
Transfer USDC to Arc mainnet through CCTP V2 and try a Chakra swap | CCTP | kickoff + 20 weeks | source and Arc transactions, Chakra quote and swap or `NO_ROUTE`

**Seeking funding/support for an audit or security review?:** No audit is priced in these four milestones **[confirm before selecting]**.  
**Prior internal or third-party audit findings (500-character limit):** [write N/A if none; otherwise identify actual scope, date, and findings]

**Distribution and reporting for narrative fields or deck:** Target one Arc-supporting wallet integrator as the first adopter, not a signed or paying customer. Report its dated time to first live swap if achieved; weekly npm downloads with version/date range; public API rate-limit, observable 429s, and stated-interval uptime; monthly Chakra swap count, volume, and pair mix; and Circle policy plus sponsored receipt. With fewer than ten swaps, link the transaction list instead of a percentage. Independent SDK consumption and two-venue route comparison are reporting goals, not payment gates.

**Revenue path, separate from the grant budget:** Chakra proposes a disclosed 2 bps (0.02%) protocol fee on successful Arc mainnet Chakra swaps, shown separately from pool fees and gas. At 1 million USDC of routed volume, that rate would yield 200 USDC before operating costs; do not portray it as enough to sustain the protocol at launch or as revenue already earned. Current Testnet quotes show 0 bps.

**Funding boundary for narrative fields or deck:** The Arc mainnet contract, token/venue configuration, scanner, single-route API/UI, and external-wallet receipt are self-funded proof work. Apply to the separate 500 USDC Arc Microgrant only if a mined mainnet swap and live route exist before its deadline. Disclose any award and exclude separately funded work from Circle's request. The funded Circle-wallet and Gas Station gates require two distinct mined Arc Testnet receipts plus mainnet readiness checks; verify any claim that a Circle path is live against dated receipts.

**Outside this request:** Existing testnet router, quote/build API, in-repo SDK methods, OpenAPI, and guide; self-funded mainnet proof; liquidity seeding; two-venue comparison and independent integrator run as paid gates; split receipt; independent audit; operator vault; multi-chain expansion beyond the bounded CCTP inbound handoff; general cross-chain routing; Paymaster, Gateway, StableFX, Agent Stack, x402, USYC, and Bridge Kit.

## Existing Arc Testnet evidence

The previous grant draft and Chakra testing record establish the following dated baseline. Historical figures are retained as historical evidence; they are not a claim about today's live totals.

- **Wallet execution:** 5 September 2026, 10:24:14 UTC; block `60563600`; successful MetaMask-authorized `splitSwap`; **1 USDC → 1.629188 EURC**, via Presto. [Public receipt](https://testnet.arcscan.app/tx/0xee7bc19a990ce6691a68e9b387585baee13edc846cbf3a43551ab3dd7cfcda6c).
- **Network:** Arc Testnet, chain `5042002`. [Aggregator](https://testnet.arcscan.app/address/0xeb12351602c56d47c4ee955193335848952b29d8): `0xeb12351602c56d47c4ee955193335848952b29d8`. [Public manifest](https://github.com/mangekyou-labs/chakra/blob/feature-chakra/docs/arc-testnet-manifest.json).
- **Controlled QA snapshot, 5 September 2026:** 3 confirmed swaps; 3 USDC notional; 1 unique test trader; 0 mined split swaps. [Stats UI](https://chakra-ag.vercel.app/stats) and [stats API](https://chakra-api-0a5i.onrender.com/api/v1/stats?range=all) are inspection links, not archived proof of those historical totals.
- **Indexed snapshot fetched 30 September 2026:** 5 confirmed swaps; 5 USDC notional; 2 unique addresses; 0 split swaps. The API reports `freshness_secs: 157662` (about 44 hours). This is indexed testnet activity, not real-time freshness or verified organic adoption. [Saved JSON](https://github.com/mangekyou-labs/chakra/blob/feature-chakra/docs/evidence/grant-testnet/stats-2026-09-30.json).
- **Execution boundary:** The receipt is a single-route swap through the `splitSwap` entry point. The historical two-venue harness at block `60763000` is simulation evidence; it created neither a mined transaction nor user volume. No Circle SCA, Gas Station, CCTP or mainnet execution is established by this receipt.
- **Developer surface:** [Source](https://github.com/mangekyou-labs/chakra), [OpenAPI](https://github.com/mangekyou-labs/chakra/blob/feature-chakra/docs/openapi.yaml), [integrator guide](https://github.com/mangekyou-labs/chakra/blob/feature-chakra/docs/integrator-guide.md), and [SDK source](https://github.com/mangekyou-labs/chakra/tree/feature-chakra/packages/sdk). These source links do not establish independent adoption or a fresh npm release.
- **Evidence provenance:** Existing `feature-chakra` grant proposal and `docs/ai/testing/2026-09-03-feature-chakra.md`, headed MetaMask and T11.12 records. Public URLs must remain reviewer-accessible at submission. The new Circle milestone links above are future acceptance evidence, not gaps in the existing testnet proof.

## Team capability

Kyler Nguyen leads Chakra's contract, routing, API, integration and release work. The implementation spans a Solidity aggregator, Rust services, a TypeScript SDK and a hosted web app. The dated wallet receipt connects this engineering surface to an executed testnet swap. Public profile: https://github.com/MrSufferer; project: https://github.com/mangekyou-labs/chakra. Public GitHub contribution history also records work on [Kamui](https://github.com/mangekyou-labs/kamui), [MGK Protocol](https://github.com/mangekyou-labs/mgk-protocol) and [Oreka](https://github.com/mangekyou-labs/oreka), spanning VRF, perpetuals and prediction-market infrastructure. These are contributions, not a claim of sole authorship. The proposal is scoped to one engineer and four sequential milestones. Public repositories demonstrate engineering work; they do not establish customers, revenue, awards or professional employment history.

## Deck and Demo

**Video demo (public/unlisted, no longer than five minutes):** [Recorded Chakra demo](https://youtu.be/12hvXfd5rsE), supplied by Kyler on 30 September 2026. Check duration, access and product claims before submission. The existing receipt is supporting evidence, not a replacement video. Current USDC/EURC use must be distinguished from proposed Circle Wallets and Gas Station work.

**Transcript folder:** [add Google Drive link with view/download access]. Name each Circle product discussed, its code location, and whether it is current or planned.  
**Code screenshots:** [add Google Drive link to PNG/JPG files for products actually integrated when recording].  
**Investor deck:** Updated editable PowerPoint and PDF in the `feature-chakra` worktree: `docs/grants/Chakra-Circle-Grant-Pitch-Deck.pptx` and `.pdf`. Upload either supported format and add the shareable link; the local file path is not a submission URL.

## Conflict of Interest

**Conflict of interest:** No **[confirm this remains accurate for the applicant and key people at submission]**.

## Remaining submission facts

Applicant legal/incorporation status, private contact confirmations, funding and audit declarations, the existing video URL, and viewable upload links remain submission-time inputs. Future milestone receipts remain future work. No invented team members, customers, volume, partners or awards are supplied.
