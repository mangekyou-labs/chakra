# SCF Build - Tranche 1 Completion Form Draft

Prepared for the **July 31, 2026** deadline. Replace every `TODO` before
submitting. Keep the video public or unlisted and verify all links in a private
browser window.

## Form selections

- **Submission:** Chakra - Arc DEX Aggregator (SCF #44)
- **Ready to submit:** Yes, after the pre-submit checklist below is complete
- **Project stage:** Pre-Launch #1 - MVP
- **Telegram username:** `@ligulfzhou` (use the exact Telegram handle only)

## Tranche Deliverables

Chakra completed all four approved Tranche 1 deliverables:

**1. Execution evidence and venue differentiation pack.** We published a
reproducible benchmark covering at least three pairs and three sizes, including
fair Arc-only comparisons using `prefer_arc=1`, Arc venue CLMM routes,
and documented split-routing cases. The benchmark script can be rerun locally,
and the comparison document cites the public Arc Broker adapter coverage.

**2. Integrator-ready API.** We published an OpenAPI specification and a
docs-only integrator guide covering `/quote`, `/build_tx`, `/tokens`, balances,
health, partner API keys, and rate limits. The live `/quote` endpoint supports
`prefer_arc=1`. A non-founder external tester used their own funded public
G-address and ran the documented one-command smoke test
(`OUT=./output USER_G=G... ./scripts/integrator-smoke.sh`). The script completed
the health, quote, and unsigned transaction-build flow and saved the request and
responses. The captured quote is a split Arc venue plus Arc venue CLMM route, and
the `build_tx` response contains a valid unsigned transaction XDR. No secret key,
signature, or on-chain submission was required.

**3. Completed swap UI.** The production UI now provides token metadata and
self-hosted logos with deterministic fallbacks, connected-wallet spendable
balances, 25% / 50% / 75% / 100% quick amount controls with an Arc reserve,
configurable slippage, Max hops and Max splits routing controls, and a Arc
Expert transaction link after submission. Max hops limits route length, while
Max splits limits the number of parallel routes considered. The live token API
currently returns metadata/logo coverage for more than 50 routable tokens, and
the responsive quote-to-build-to-sign flow remains live on mainnet.

**4. On-chain analytics indexer v0.** A production indexer ingests Chakra
Aggregator contract events from Arc mainnet and stores daily time-series
for transaction count, entry notional, unique users, function breakdown,
split/round-trip activity, and per-leg DEX attribution across Arc venue,
Arc venue, Arc venue, Sushi, and Arc venue. We published the attribution specification,
sample export, public `/api/v1/stats` endpoint, CSV export, and public stats page
for continued data collection toward the Tranche 3 30-day report.

## Deliverable Verification - Video

`TODO: paste public or unlisted YouTube/Loom URL`

## Additional Deliverable Verification

- Production application: https://Chakra.xyz
- API health: https://api.Chakra.xyz/api/v1/health
- API documentation: https://Chakra.xyz/docs
- Public repository: https://github.com/Chakra/Arc-dex-agg
- Benchmark results: https://github.com/Chakra/Arc-dex-agg/blob/main/docs/scf-benchmark-results.md
- Venue comparison: https://github.com/Chakra/Arc-dex-agg/blob/main/docs/scf-venue-comparison.md
- Reproducible benchmark script: https://github.com/Chakra/Arc-dex-agg/blob/main/scripts/scf-benchmark.sh
- Integrator guide: https://github.com/Chakra/Arc-dex-agg/blob/main/docs/integrator-guide.md
- OpenAPI specification: https://github.com/Chakra/Arc-dex-agg/blob/main/docs/openapi.yaml
- External integrator evidence: https://github.com/Chakra/Arc-dex-agg/tree/main/docs/evidence/d2-integrator-smoke
- Public stats page: https://Chakra.xyz/stats
- Stats JSON/CSV API: https://api.Chakra.xyz/api/v1/stats
- Analytics attribution and pipeline: https://github.com/Chakra/Arc-dex-agg/blob/main/docs/analytics-indexer.md
- Sample indexer export: https://github.com/Chakra/Arc-dex-agg/blob/main/docs/sample-indexer-export.json
- Mainnet Aggregator contract: https://Arc.expert/explorer/public/contract/CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K
- Mainnet three-path split execution: https://Arc.expert/explorer/public/tx/a571b4617bc42594673ab22a496ef61c4fc66689a4f9cc29fd71dc7fb74ccb54

## Support Needed

We would appreciate introductions to Arc wallets and dApps interested in
testing Chakra's REST API or TypeScript SDK, and guidance on engaging a
Arc/Arc-focused auditor for the approved Tranche 3 Aggregator and Vault
audit. No blocker currently prevents continued development.

## Product Testing

- **Product URL:** https://Chakra.xyz
- **Documentation:** https://Chakra.xyz/docs
- **Network:** Arc Public Network (mainnet)
- **Credentials:** No credentials are required for normal public API testing.
  Partner API keys only increase rate limits.
- **Suggested test:** Connect wallet, select Arc to USDC, enter a small
  amount, inspect the returned route, and build/sign the transaction. A USDC
  trustline is required before receiving classic-backed USDC.
- **Non-signing API test:** Run the repository's `scripts/integrator-smoke.sh`
  with a funded public G-address. It builds an unsigned XDR and does not require
  a secret key or transaction submission.

## Tranche 1 Video Script (5 minutes maximum)

### 0:00-0:25 - Scope

- Show the Chakra home/swap page.
- State that Tranche 1 covers differentiation evidence, integrator API
  readiness, swap UI completion, and analytics indexer v0.

### 0:25-1:15 - Benchmark and differentiation

- Open `docs/scf-benchmark-results.md`.
- Show the three pairs and multiple sizes.
- Highlight one fair Arc-to-USDC comparison, one `is_split=true` result, and an
  Arc venue CLMM route.
- Briefly show `scripts/scf-benchmark.sh` to establish reproducibility.

### 1:15-2:25 - Integrator-ready API

- Open https://Chakra.xyz/docs and show `/quote`, `/build_tx`, API keys/rate
  limits, and `prefer_arc`.
- In a terminal, show the external tester's `quote.json` fields: `is_split`,
  `sub_routes`, Arc venue, and Arc venue CLMM.
- Show `build_resp.json` with `success=true` and the beginning of
  `unsigned_tx_xdr`; do not scroll through the full XDR.

### 2:25-3:40 - Swap UI

- Connect wallet or use a prepared connected session.
- Show token logos, spendable balance, and 25% / 50% / 75% / 100% controls.
- Open swap settings and show slippage, Max hops, and Max splits.
- Request a small Arc-to-USDC quote and show split-route percentages if the
  current market returns a split.
- Show an existing successful transaction's Arc Expert link if you do not
  want to submit a new transaction during recording.

### 3:40-4:35 - Analytics indexer v0

- Open https://Chakra.xyz/stats.
- Show daily notional, transaction count, function breakdown, and DEX legs.
- Open `/api/v1/stats?format=csv` and mention that indexing continues toward
  the Tranche 3 30-day report.

### 4:35-5:00 - Verification summary

- Show the GitHub repository and the four evidence links.
- State that production UI/API and the mainnet Aggregator contract are public.
- End with `Chakra.xyz`, `api.Chakra.xyz`, and the repository URL.

## Pre-submit Checklist

- [x] Capture the external non-founder smoke run, command, public G-address, and
  quote/build artifacts in `docs/evidence/d2-integrator-smoke/`.
- [x] Commit and push `docs/evidence/d2-integrator-smoke/` to the public
  repository.
- [ ] Commit and push the final form and video evidence documentation.
- [ ] Open every GitHub and production link in a private browser window.
- [ ] Record a video no longer than five minutes and add its URL above.
- [ ] Confirm the video sharing setting is public or unlisted.
- [ ] Use the exact Telegram username in the form.
- [ ] Submit no later than July 31, 2026 and retain a screenshot/PDF of the
  completed submission.
