# Circle 2026 Cohort 2 — Submit Proposal Workbook

**Program:** Circle Developer Grants — Circle 2026 Cohort 2
**Portal:** [circle.questbook.app](https://circle.questbook.app/)
**RFP:** [circle.com/grant](https://www.circle.com/grant)
**Use case alignment:** Stablecoin FX on Arc (not Circle's StableFX product)
**Evidence checked:** 4 September 2026
**Companion architecture:** [Chakra Technical Architecture](../chakra-architecture.md)

This file is a paste workbook for the Questbook **Submit Proposal** form. Each
heading matches a portal section. Each field has a labeled **Paste** or
**Select** block in portal order. Copy only the fenced block (or the Select
line) into the form.

Unknown legal and identity values are `TO_BE_FILLED`. Do not invent them at
submit time. Character counts on limited fields were measured on 4 September
2026.

The 60,000 USDC request and 16-week plan live inside milestone details and the
roadmap answers. The portal has no standalone budget field.

---

## How to submit

1. Complete every `TO_BE_FILLED` value in the checklist below.
2. Paste Applicant Details through Conflict of Interest in portal order.
3. Tick Circle product boxes from the Checked / Unchecked lists. If a live
   label differs, tick the closest honest match and keep the prose the same.
4. Record the 5-minute video from the shot list, then paste the unlisted URL.
5. Paste the investor-deck URL if you have one; otherwise leave that field
   empty and do not invent a link.
6. Do not file until fill-ins, video, and conflict-of-interest confirmation
   are real.

This pass does not record the video, build a deck, or submit Questbook.

---

## Fill-ins required before Questbook submit

Replace every `TO_BE_FILLED` below. The rewrite can be used for every other
field today.

| Portal field | Status |
| --- | --- |
| Email address | `TO_BE_FILLED` |
| Company Legal Entity Name | `TO_BE_FILLED` (GitHub org `mangekyou-labs` is not a legal entity) |
| Project X handle | `TO_BE_FILLED` |
| Founder / team locations (City, State/Province, Country) | `TO_BE_FILLED` |
| Business location (country) | `TO_BE_FILLED` |
| Is your business incorporated? | `TO_BE_FILLED` (Yes / No) |
| Are you funded? | `TO_BE_FILLED` (Yes / No) |
| Conflict of interest | `TO_BE_FILLED` (recommended **No** only if you confirm there is no Circle relationship) |
| Video demo URL | `TO_BE_FILLED` |
| Investor deck URL | `TO_BE_FILLED` |

---

## Submit Proposal

### Circle 2026 Cohort 2

### Grant Program Details

Click to view program RFP. Not a paste field.

**RFP URL:** https://www.circle.com/grant

Chakra's honest fit is the RFP **Stablecoin FX** use case on Arc, with USDC and
EURC live today and Circle Wallets plus Gas Station as grant work. Do not
reframe this application around CCTP, Gateway, Nanopayments, Agent Stack, or
Circle's StableFX product.

---

## Applicant Details

Provide personal or organizational details, including contact information and
company details.

### Primary contact first name

**Select / paste:**

```
Ligang
```

### Primary contact last name

```
Zhou
```

### Email address

```
TO_BE_FILLED
```

Use a monitored address. Do not invent one.

### Company Legal Entity Name

```
TO_BE_FILLED
```

`mangekyou-labs` is the GitHub organization. It is not proof of incorporation
or a legal entity name.

### Company Doing-Business-As (DBA) name

Portal hint: if you do not have a DBA name, provide your project name.

```
Chakra
```

### Founder names, roles, bios

Jackson is core team, not a founder. Keep that distinction in the paste.

```
Ligang Zhou — Founder and Project Lead. Ligang owns Chakra's Solidity aggregator, Rust market-data worker, routing engine, REST API, TypeScript SDK, release operations, and the planned Circle Wallets / Gas Station backend. He previously shipped LumAgg, a multi-venue DEX aggregator on Stellar covering worker ingestion, Redis snapshots, pathfinding, split optimization, API, SDK, frontend, and on-chain execution. His background includes DEX routing and atomic execution across Flare, Stellar, Solana, Sui, Aptos, and Fuel, including price discovery, slippage controls, and production bot operations. GitHub: https://github.com/ligulfzhou — LinkedIn: https://www.linkedin.com/in/ligangzhou/

Jackson (Jianhao Bi) — Frontend Engineer (core team; not a founder). Jackson owns the Next.js swap interface, wallet connection and transaction-state UX, responsive layout, and SDK integration examples. He will implement the user-facing Circle Wallets and Gas Station flows while keeping the existing external-wallet path. GitHub: https://github.com/Billshimmer — LinkedIn: https://www.linkedin.com/in/%E5%BB%BA%E6%B5%A9-%E6%AF%95-467967415/
```

### Project website

```
https://chakra-ag.vercel.app
```

### Project X handle

```
TO_BE_FILLED
```

### Project GitHub URL

Portal hint: optional, if publicly available.

```
https://github.com/mangekyou-labs/chakra
```

### Where are you and your founders located?

Portal format, one line per person: Full Name, Title, Location (City,
State/Province, Country).

```
Ligang Zhou, Founder and Project Lead, TO_BE_FILLED (City, State/Province, Country)
Jackson (Jianhao Bi), Frontend Engineer, TO_BE_FILLED (City, State/Province, Country)
```

### Where is your business located?

**Select country:** `TO_BE_FILLED`

### Is your business incorporated?

**Select:** `TO_BE_FILLED` (Yes / No)

---

## Project Abstract

### Project Name

Limit 80 characters. Recommended paste is 6 characters.

```
Chakra
```

If the portal wants a descriptive name, use this 32-character alternative
instead, not both:

```
Chakra: Arc Stablecoin FX Router
```

### Please provide a one liner description of your project (Limit to one sentence)

Limit 200 characters. This paste is 144 characters, one sentence.

```
Chakra is a non-custodial aggregator that compares executable USDC and EURC routes across Arc venues and returns wallet-ready swap transactions.
```

### What problem are you solving and why is it important?

```
Stablecoin liquidity on Arc is split across venues that use different pool mathematics, factories, and swap interfaces. A wallet or payment application that talks to each venue itself has to rebuild market observation, quote validation, and transaction construction. Users then see one venue's price instead of an executable comparison, and integrators cannot share a common USDC/EURC routing layer.

That gap matters because Arc is being positioned as a settlement network for real-world financial flows. If exchanging USDC and EURC on Arc requires a custom adapter per venue, those assets stay harder to embed in wallets, payments, and treasury workflows. A shared, non-custodial comparison layer is infrastructure for the rest of the Arc application surface, not a single-app feature.
```

### What is your solution to that problem?

```
Chakra is an open-source, self-hostable aggregator for Arc. A market-data worker observes supported DEX pools from Arc logs and RPC state. A local router compares healthy constant-product, stable-swap, and concentrated-liquidity routes, including split orders when more than one healthy path exists. The REST API returns the best valid quote plus unsigned transaction data for a Solidity aggregator that supports Permit2. The user authorizes the swap in their own wallet. Chakra never holds keys and never submits on the user's behalf.

Integrators can use the hosted REST API, the published TypeScript SDK (@chakra-ag/sdk), or a self-hosted stack. The current hosted product is live on Arc Testnet (chain ID 5042002) at https://chakra-ag.vercel.app with API https://chakra-api-0a5i.onrender.com.

The grant does not rebuild that router. It adds a Circle user-controlled smart contract wallet on ARC-TESTNET and Circle Gas Station sponsorship for eligible transactions, so a user can approve a compared USDC/EURC route in an embedded wallet with policy-bound fee sponsorship.
```

### Why hasn’t this problem been solved yet? What are the barriers (regulatory, technical, etc.) that previously existed?

```
Arc is early. Venue interfaces are not standardized, live testnet reserves are thin, and an honest router has to fail closed when pool state is stale or incomplete. Seeding liquidity would manufacture a demo without proving the product.

Quoting and custody are different problems. Comparing routes is not the same as issuing a user-controlled smart contract account or sponsoring gas under a policy. Those Circle Developer Platform pieces were not required to ship a first quote, and they are not present in the current codebase.

Aggregators on other chains do not give Arc applications a native USDC/EURC execution layer. Cross-chain products such as CCTP or Gateway also do not solve same-chain venue comparison. This grant therefore stays on Arc-native routing plus Wallets and Gas Station, rather than expanding into a cross-chain story that the product does not need yet.
```

### Why are you and your team uniquely suited to solve this problem?

```
Ligang has already shipped a multi-venue DEX aggregator, LumAgg on Stellar, with the same operational shape Chakra uses: a market-data worker, Redis snapshots, pathfinder, split optimizer, REST API, TypeScript SDK, frontend, and on-chain aggregator. Chakra is that architecture on Arc, rewritten for Solidity, Permit2, and Arc's native USDC/EURC. It is not a slide-deck router. The hosted Arc Testnet API already returns USDC/EURC quotes from live Presto and Xylo venues.

Jackson owns the wallet-facing interface, which is the surface Circle Wallets and Gas Station have to land on without breaking the existing external-wallet path.

The team is two people who already operate the Arc stack. The grant is asked to fund confirmed execution evidence, Circle product integration, integrator validation, and independent security review — not to invent the aggregator from zero.
```

LumAgg is prior shipping evidence for this answer only. Do not list Stellar as
a live chain in Product Alignment.

---

## Product Alignment Track

Tell us about your product's current status, Arc integration, and Circle
product usage.

### Is your project currently live in production?

**Select:** Yes

Qualify in the chains field and traction answer: this means a public hosted
Arc Testnet product (frontend, API, SDK, aggregator). It does not mean Arc
mainnet.

### Are you live on Arc?

**Select:** Yes

Arc Testnet, chain ID `5042002`. Aggregator
`0xeb12351602c56d47c4ee955193335848952b29d8`. Native USDC is gas and ERC-20 at
`0x3600000000000000000000000000000000000000`. EURC is
`0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a`.

### Which other chain(s) are you currently live on?

```
None. Chakra is Arc-only.
```

Do not list Stellar, Flare, Solana, Sui, Aptos, or Fuel. Those appear only as
Ligang's prior routing background.

### Which Circle products are currently integrated into your project?

Portal hint: the video must validate this.

Tick only products the current codebase and live demo can show. Native Arc
USDC (gas + ERC-20) and EURC in the token catalog, quotes, and aggregator
calldata count. Circle Wallets, Gas Station, CCTP, and Gateway are not in the
codebase.

**Checked:**

- USDC
- EURC

**Unchecked (do not tick, even if the label is tempting):**

- Circle Wallets / Programmable Wallets / user-controlled wallets
- Circle Gas Station
- Circle Paymaster
- CCTP
- Bridge Kit
- Circle Gateway
- Nanopayments / x402
- Circle Contracts (Chakra ships its own Solidity aggregator; it does not use
  Circle contract templates as a product integration)
- StableFX (the RFP use case is Stablecoin FX; StableFX is a different Circle
  product and is not integrated)
- Agent Stack
- Circle Payments Network
- App Kits
- USYC

If the portal uses a combined "USDC / EURC" box, tick that single box. If a
label differs slightly, tick the closest honest match and leave the rest
unchecked.

### Which Circle products do you plan to integrate into your project?

**Checked:**

- Circle Wallets (user-controlled smart contract account on `ARC-TESTNET`;
  email-OTP; server-only API credentials)
- Circle Gas Station (policy-bound sponsorship for eligible Circle
  smart-account transactions)

**Unchecked:**

- CCTP
- Bridge Kit
- Circle Gateway
- Nanopayments / x402
- Circle Paymaster
- StableFX
- Agent Stack
- Circle Payments Network
- App Kits
- USYC
- Circle Contracts as a Circle product

USDC and EURC remain in use; they are current integrations, not merely planned.
Do not add CCTP or Gateway to look more aligned. Architecture already excludes
them from this grant.

---

## Milestones and Timelines

The portal repeats milestone title (limit 1,024 characters) and details (limit
2,048 characters). Add six milestones in this order. Budget slices are inside
the details because the form has no amount field. Combined request: **60,000
USDC over 16 weeks from kickoff**.

Do not group these under older SCF-style phase labels.

### Milestone 01 — title

Limit 1,024. This paste is 73 characters.

```
Confirmed Arc Testnet execution baseline (kickoff + 4 weeks; 13,000 USDC)
```

### Milestone 01 — details

Limit 2,048. This paste is 1,947 characters.

```
Chakra is already hosted on Arc Testnet. This milestone does not rebuild the router, worker, API, SDK, or frontend. It establishes a confirmed execution baseline.

Work:
1. Execution evidence. Complete at least one USDC-to-EURC swap from genuine Arc Testnet reserves using Chakra quote and build_tx. Publish a public explorer receipt linked to the same-day quote, route identity, built transaction, hash, receipt status, and observed token balance change. Document remaining wallet-provider limits without treating incomplete attempts as successful swaps.
2. Quote integrity. Add automated tests for stale or missing pool state, mutated pair, venue, fee, or factory metadata, expired deadlines, and minimum-output enforcement. Keep /build_tx rejection of routes that do not match the market snapshot. Pass constant-product, stable-swap, concentrated-liquidity, and Xylo fixtures with documented tolerances. Keep existing API and SDK contracts backward compatible.
3. Swap UX recovery. Distinguish quote loading, authorization, submission, confirmation, rejection, and failure. Link successful transactions to Arc's testnet explorer. Provide specific recovery for chain mismatch, wallet rejection, expired quote, rate limit, and unavailable route. Pass desktop and mobile quote-to-wallet regression.
4. Telemetry v0. Aggregate daily counts and success rates for quote, build, submit, and confirm stages, plus no-route, stale-state, invalid-route, wallet-rejection, RPC, and confirmation failures. Never record private keys, Circle access tokens, authorization material, or personal login data. Document fields, retention, and measurement limits.

Acceptance: a reviewer can repeat quote and build_tx from published instructions; one public USDC-to-EURC receipt exists; integrity tests pass; UX recovery states are visible; telemetry documentation is published.

Timing: grant kickoff + 4 weeks.
Budget slice: 13,000 USDC of the 60,000 USDC request.
```

### Milestone 02 — title

Limit 1,024. This paste is 78 characters.

```
Circle user-controlled Wallets on Arc Testnet (kickoff + 8 weeks; 14,000 USDC)
```

### Milestone 02 — details

Limit 2,048. This paste is 1,194 characters.

```
Add an embedded Circle Wallets option using a user-controlled smart contract account on ARC-TESTNET. Users authenticate by email one-time passcode, retain authority over their wallet, review the Chakra route, and authorize the final transaction. Circle API credentials and short-lived session issuance remain on the server. Chakra never receives or stores user private keys, PINs, one-time passcodes, or signing material.

The Circle wallet consumes the same to, data, chain_id, value, deadline, and authorization information returned by the existing /quote and /build_tx path. No Circle-specific quote format is introduced. User and session creation is idempotent. The existing external-wallet flow remains available and must pass regression testing.

Acceptance:
- A new user can authenticate, create or recover an Arc Testnet wallet, and view its address and supported token balances.
- The Circle wallet completes quote, transaction build, user authorization, submission, and confirmation for USDC-to-EURC.
- Credentials stay server-only; no key material is logged.
- External-wallet regression passes.

Timing: grant kickoff + 8 weeks.
Budget slice: 14,000 USDC of the 60,000 USDC request.
```

### Milestone 03 — title

Limit 1,024. This paste is 63 characters.

```
Circle Gas Station sponsorship (kickoff + 10 weeks; 8,000 USDC)
```

### Milestone 03 — details

Limit 2,048. This paste is 1,001 characters.

```
Configure and integrate Circle Gas Station for eligible Circle smart-account transactions on Arc Testnet. Show sponsorship eligibility before authorization where the Circle interface permits it, record the result, and return a clear error when a transaction falls outside the policy or the sponsorship service is unavailable. Do not submit a misleading or partially formed transaction in those cases.

The application must explain that sponsorship is subject to policy and is not a permanent promise of free transactions. The policy limits supported network, contracts, transaction types, and spend.

Acceptance:
- A documented testnet sponsorship policy is published.
- At least 10 confirmed sponsored Chakra swaps are documented with transaction hashes and aggregate Gas Station evidence.
- Ineligible, policy-limited, and unavailable-service cases fail honestly.
- The UI does not claim unconditional free gas.

Timing: grant kickoff + 10 weeks.
Budget slice: 8,000 USDC of the 60,000 USDC request.
```

### Milestone 04 — title

Limit 1,024. This paste is 63 characters.

```
Two-path integrator validation (kickoff + 12 weeks; 7,000 USDC)
```

### Milestone 04 — details

Limit 2,048. This paste is 853 characters.

```
Validate Chakra through two distinct integration paths. Path A is the in-repository reference application using Circle Wallets and Gas Station. Path B is an independent developer or reviewer using Chakra's REST API or TypeScript SDK with either a self-hosted or public API deployment.

Acceptance:
- Both paths have reproducible quote and transaction-build instructions.
- Path A includes a confirmed user-controlled Circle Wallets swap with the sponsorship result shown.
- Path B is completed from the public documentation, with public or anonymized role-based feedback recorded.
- At least one documented walkthrough reaches a valid transaction build in under 30 minutes from a clean project.
- Feedback is incorporated into the SDK examples and integrator guide.

Timing: grant kickoff + 12 weeks.
Budget slice: 7,000 USDC of the 60,000 USDC request.
```

### Milestone 05 — title

Limit 1,024. This paste is 87 characters.

```
Public routing report and independent security review (kickoff + 15 weeks; 15,000 USDC)
```

### Milestone 05 — details

Limit 2,048. This paste is 1,134 characters.

```
Publish a public routing report and complete an independent security review. The report is built from telemetry, not from seeded volume.

Report: public daily totals for quotes, valid transaction builds, confirmed swaps, failures by category, and sponsorship outcomes, covering at least 30 days. Identify controlled QA transactions separately from external activity when that distinction is known. State collection limits and that the report is not a revenue or user-growth claim.

Security: commission a scoped independent review of the aggregator contract, route-to-calldata validation, Permit2 authorization path, Circle Wallets integration, and Gas Station policy boundaries. Disclose reviewer and scope. Publish a final report or public summary. Resolve all critical and high-severity findings before any mainnet deployment; document dispositions for medium and low findings. Include remediation regression tests and remaining operational assumptions in the launch checklist.

Timing: grant kickoff + 15 weeks.
Budget slice: 15,000 USDC of the 60,000 USDC request (public report 3,000; independent review and remediation 12,000).
```

### Milestone 06 — title

Limit 1,024. This paste is 69 characters.

```
Grant close-out and launch-readiness (kickoff + 16 weeks; 3,000 USDC)
```

### Milestone 06 — details

Limit 2,048. This paste is 1,066 characters.

```
Produce the grant close-out and launch-readiness package. A mainnet deployment occurs only if Arc mainnet, the required Circle products, suitable liquidity, and the project's security gate are available during the grant period. Otherwise completion is the security, deployment, operations, and handoff evidence needed for a later launch.

Acceptance:
- A final report links each milestone to code, documentation, tests, and public evidence.
- A short public demo covers quote, route review, Circle wallet authorization, sponsorship result, submission, and confirmation.
- The self-host guide covers API, worker, Redis, frontend, configuration, monitoring, and recovery.
- A six-month maintenance plan assigns responsibility for RPC health, Circle service changes, venue allowlists, dependency updates, and incident response.
- Any mainnet deployment includes public addresses and a confirmed transaction; otherwise the report states which external or security condition remains.

Timing: grant kickoff + 16 weeks.
Budget slice: 3,000 USDC of the 60,000 USDC request.
```

---

## Project Traction and Roadmap

### Tell us about your current traction and success already achieved (transaction volume, project growth, MAU, AUM, etc.)

This paste is 1,616 characters. The portal does not publish a limit here; keep it short anyway.

```
Chakra is a hosted Arc Testnet product, not an Arc mainnet launch. It has no MAU, AUM, revenue, or organic volume to report.

Live on 4 September 2026:
- Frontend https://chakra-ag.vercel.app (HTTP 200).
- API https://chakra-api-0a5i.onrender.com: /api/v1/health ok; /api/v1/ready ready true.
- SDK @chakra-ag/sdk and repo https://github.com/mangekyou-labs/chakra.
- Aggregator 0xeb12351602c56d47c4ee955193335848952b29d8 on Arc Testnet (chain ID 5042002).
- 1 USDC → EURC: expected_output 823,415 via Presto hub, price_impact_bps 1765.
- 1 EURC → USDC: expected_output 1,232,732 via Xylo stable pool, price_impact_bps 0.
- /api/v1/stats?range=all: lag_blocks 0, freshness about 4 seconds; six catalog directions (USDC, EURC, cirBTC) report usable Presto, Xylo, or UnitFlow pools.

Not claimed:
- No organic public Chakra user volume. One controlled QA swap is confirmed;
  stats show 2 confirmed swaps and 2,000,000 stablecoin-notional micros total,
  including the pre-existing unattributed 2026-08-30 row and the new
  Presto/UnitFlow-attributed QA receipt.
- No Dune dashboard. https://chakra-ag.vercel.app/stats is 404; the public analytics surface is the API stats endpoint.
- /api/v1/ready returns pool_keys [] while /quote still succeeds, so readiness is not a complete pool inventory.
- Split routes and cirBTC stay follow-ups until genuine reserves support them. The project will not seed liquidity to manufacture those cases.

Repository evidence includes the September 4 controlled QA USDC→EURC→cirBTC
swap, its explorer receipt, and post-confirmation stats attribution. This single
wallet-controlled transaction is execution evidence, not organic traction.
Manifest deployment hashes are not swap evidence.
```

### Please share your Dune Analytics or any other public analytics dashboard link (if you have one)

There is no Dune dashboard. Paste the live API stats endpoint, not the 404
`/stats` page.

```
https://chakra-api-0a5i.onrender.com/api/v1/stats?range=all
```

If the portal requires a Dune URL and rejects a non-Dune link, leave the field
empty rather than inventing a dashboard.

### Are you funded?

**Select:** `TO_BE_FILLED`

Recommended **No** only after you confirm there is no outside capital to
disclose. Do not select No without that confirmation.

### Technical Roadmap: Timeline and grant milestones. Provide a high-level technical plan that includes Circle product integration timelines.

```
Four-month Arc-first plan from grant kickoff. Combined funding request: 60,000 USDC, paid against the six form milestones.

Weeks 0–4 — Execution baseline (13,000 USDC). One public USDC-to-EURC swap with quote/build/receipt linkage; quote-integrity tests; swap UX recovery states; aggregate telemetry v0. Circle products in this window remain the current USDC and EURC catalog.

Weeks 4–8 — Circle user-controlled Wallets (14,000 USDC). Email-OTP SCA on ARC-TESTNET; server-only credentials; same /quote and /build_tx path; external-wallet regression.

Weeks 8–10 — Circle Gas Station (8,000 USDC). Policy-bound sponsorship; at least 10 documented sponsored swaps; honest ineligible/unavailable errors.

Weeks 10–12 — Two-path integrator validation (7,000 USDC). Path A: in-repo app with Wallets and Gas Station. Path B: independent REST/SDK walkthrough to a valid transaction build in under 30 minutes.

Weeks 12–15 — Public routing report and independent security review (15,000 USDC). Thirty days of aggregate metrics without claiming organic growth; scoped review of aggregator, route validation, Permit2, Wallets, and Gas Station; critical and high findings fixed before any mainnet.

Week 16 — Close-out and launch-readiness (3,000 USDC). Final report, demo, self-host/runbook, six-month maintenance plan.

Conditional Arc mainnet only if, during the grant period, Arc mainnet is available (Circle has pointed to 16 September 2026), Circle Wallets and Gas Station support that network, genuine liquidity exists, and the security gate is green. CCTP, Gateway, Paymaster, Nanopayments, and StableFX stay out of this grant. Split routes and cirBTC are reported only when live reserves support them.
```

### How will this grant support your technical roadmap?

```
The grant funds the work that is not yet shipped: a confirmed public swap receipt, quote-to-transaction integrity hardening, swap recovery UX, operational telemetry, Circle user-controlled Wallets on ARC-TESTNET, Circle Gas Station sponsorship, two integrator paths, an independent security review, and a close-out package.

It does not rebuild the existing router, worker, API, SDK, frontend, or aggregator. Those are already hosted on Arc Testnet and already quote USDC and EURC from live venues. The grant turns that proof of concept into a reviewable Circle-integrated product: an embedded wallet path, sponsored eligible fees, documented integrator adoption, and a security bar before any mainnet discussion.

Funding is requested as 60,000 USDC over 16 weeks, released against the six milestones above.
```

---

## Deck and Demo

Technical video requirements (no longer than 5 minutes):

**Codebase walkthrough (required).** Show the actual code where Circle
technologies are implemented. Current code can show native USDC (gas + ERC-20
`0x3600…0000`), EURC `0x89B5…D72a`, Permit2, Arc chain ID `5042002`, `/quote`,
and `/build_tx`. Circle Wallets and Gas Station are not in the repository yet;
say that clearly and point at the planned architecture, not at fictional
source files.

**Integration demonstration (required).** Show the user flow where Circle
infrastructure powers the product. Today that is a USDC/EURC quote and a
wallet-ready transaction on Arc Testnet. If some integrations are planned,
describe the intended approach.

Upload to a private link (Google Drive, YouTube unlisted, or similar).

### Video demo of the product

```
TO_BE_FILLED
```

### Video shot list (do not paste into Questbook; use while recording)

Keep the video at or under 5 minutes. Suggested order:

1. **Open (15s).** Product name, Arc Testnet only, non-custodial. Live URLs:
   https://chakra-ag.vercel.app and https://chakra-api-0a5i.onrender.com.
2. **Catalog / USDC / EURC (45s).** `docs/arc-testnet-manifest.json`: chain ID
   `5042002`, USDC `0x3600000000000000000000000000000000000000`, EURC
   `0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a`, Permit2
   `0x000000000022D473030F116dDEE9F6B43aC78BA3`, aggregator
   `0xeb12351602c56d47c4ee955193335848952b29d8`. Note that USDC is native gas
   on Arc and an ERC-20 at that address.
3. **Quote path (45s).** `crates/api-server` quote handler and
   `packages/sdk/src/index.ts` `/api/v1/quote`. Show a live USDC→EURC quote
   (Presto) and EURC→USDC quote (Xylo) with venue, pool, and price-impact
   fields.
4. **Build / Permit2 (45s).** `crates/api-server/src/build_tx.rs` Permit2
   allowance and typed data; `contracts/evm/src/Aggregator.sol` constructor
   immutables and Permit2 `permit` call. Explain that the wallet, not Chakra,
   submits.
5. **Live UI (60–90s).** Connect an external wallet, switch to Arc, USDC→EURC
   quote, route disclosure in `packages/frontend/src/components/RouteDisplay.tsx`.
   Call `build_tx` if the quote is still valid. Submit only if a real receipt
   exists that day; otherwise stop at the unsigned transaction and say so.
6. **Planned Circle products (45s).** `docs/chakra-architecture.md` Circle
   Wallets and Gas Station sections. State that these are grant milestones 2
   and 3, not shipped code: user-controlled SCA on `ARC-TESTNET`, email-OTP,
   server-only credentials, policy-bound Gas Station. Explicitly not CCTP or
   Gateway.
7. **Close (15s).** What grant money buys: confirmed execution, Wallets, Gas
   Station, integrator paths, independent review.

### Please upload your investor deck

```
TO_BE_FILLED
```

Do not invent a Drive link. If you later attach a deck, cover: problem, Arc
stablecoin FX, what is live, Circle products current vs planned, traction with
the caveats above, six milestones totaling 60,000 USDC / 16 weeks, team. This
pass does not create a slide file.

---

## Conflict of Interest

Do you, your organization, or any key individuals involved in this application
currently have, or have had, any actual, potential, or perceived conflict of
interest in relation to Circle or this grant? This includes, but is not limited
to:

- A financial, business, or advisory relationship with Circle or any of its
  subsidiaries
- A family, personal, or close personal relationship with a current Circle
  employee, officer, director, or contractor
- Any role, interest, or relationship that could reasonably be seen as giving
  you an unfair advantage or improperly influencing Circle’s decision

### Conflict of interest

**Select:** `TO_BE_FILLED`

Recommended **No** only if Ligang, Jackson, and the legal entity have no
Circle employment, investment, advisory, family, or other covered
relationship. Do not select No without that confirmation. If a relationship
exists, select Yes and attach a factual description.

---

## Operator notes (not portal fields)

- The September 4, 2026 live API check supersedes the earlier `NO_ROUTE`
  snapshot: the guarded 1-USDC probe returned canonical USDC→EURC→cirBTC via
  UnitFlow at 27 bps impact (363 cirBTC atomic output, 361 minimum output). The
  explicitly authorized transaction confirmed in block `60438104` (explorer:
  https://testnet.arcscan.app/tx/0x2df6e81aa9ff0805aad7d49241ccdd9e979dd7c0dae1b261c51ed469542236c5),
  and `/api/v1/stats?range=all` recorded one attributed swap, one additional
  confirmed swap, and 2,000,000 stablecoin-notional micros total, with Presto
  and UnitFlow attribution.
- `/api/v1/ready` returning `pool_keys: []` while `/quote` succeeds is an
  honesty note, not a reason to claim the service is down.
- Do not check StableFX because the RFP use case is named Stablecoin FX.
- Do not implement Wallets or Gas Station in order to tick them as current.
- Circle Wallets docs list `ARC-TESTNET` for user-controlled accounts; reconfirm
  product availability before recording the video and before submit.
- Arc mainnet timing (Circle public comments around 16 September 2026) is an
  external gate, not a Chakra commitment.

## Reference sources

- [Circle Developer Grants](https://www.circle.com/grant)
- [Circle Wallets supported blockchains](https://developers.circle.com/wallets/supported-blockchains)
- [Circle user-controlled wallet application guide](https://developers.circle.com/wallets/user-controlled/build-a-wallet-app)
- [Circle Gas Station policy management](https://developers.circle.com/wallets/gas-station/policy-management)
- [Arc Testnet manifest](../arc-testnet-manifest.json)
- [Chakra Technical Architecture](../chakra-architecture.md)
