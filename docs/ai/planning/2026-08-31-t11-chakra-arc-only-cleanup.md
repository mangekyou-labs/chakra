# T11 planning record: Chakra identity and Arc-only cleanup

## Scope and source records

This plan reconciles the T11 requirements, design, implementation, testing,
and deployment records. The objective is complete for the code, package,
history, infrastructure, frontend identity, and headed MetaMask settlement
work. Split-route live evidence remains an explicitly tracked follow-up.

- Requirements: `docs/ai/requirements/2026-08-31-t11-chakra-arc-only-cleanup.md`
- Design: `docs/ai/design/2026-08-31-t11-chakra-arc-only-cleanup.md`
- Implementation: `docs/ai/implementation/2026-08-31-t11-chakra-arc-only-cleanup.md`
- Testing: `docs/ai/testing/2026-08-31-t11-chakra-arc-only-cleanup.md`
- Deployment: `docs/ai/deployment/2026-08-31-t11-chakra-arc-only-cleanup.md`

## Milestones and task status

| ID | Outcome | Dependencies | Validation evidence | Status |
| --- | --- | --- | --- | --- |
| T11.1 | Add the literal-free lineage regression check across tracked paths, content, generated packages, documentation, and commit metadata. | Clean-check rules and package roots | `python3 scripts/check-lineage.py all`; fresh-clone scan | Done |
| T11.2 | Reduce the Rust workspace to the active Arc runtime, venues, discovery, routing, quote, transaction build, API, and worker paths. | T11.1; active Arc architecture | Format, workspace tests, all-target Clippy, release build | Done |
| T11.3 | Remove inactive interfaces, adapters, state variants, fixtures, constants, integrations, and unsupported product surfaces while preserving required licenses and DevKit configuration. | T11.2; history cleanup inventory | Lineage scan, package-content inspection, contract tests | Done |
| T11.4 | Rebrand the existing frontend in place with Sunset Trade tokens, semantic light/dark themes, DM Sans, JetBrains Mono, and the arrow-free split-ring SVG. | Existing responsive shell and swap flow | Frontend unit, type, lint, format, build, contrast, link, logo, theme, and responsive checks | Done |
| T11.5 | Publish the public interfaces as `@chakra-ag/sdk@0.3.0`, `ChakraClient`, and `@chakra-ag/frontend`; update examples, imports, lockfiles, metadata, and package contents. | T11.2; npm authentication | SDK tests/build, pack dry run, clean registry install, quote/build smoke | Done |
| T11.6 | Rewrite the Chakra branch history in a temporary mirror, remove obsolete reachable paths/blobs/messages, and point `main` and `feature-chakra` at the cleaned result. | T11.1 and bundle backup; frozen remote heads | Fresh-clone all-commit scan; explicit force-with-lease push; both refs synchronized | Done |
| T11.7 | Redeploy and validate the backend runtime. | T11.2; Render credential | Render health, readiness, tokens, quote, build transaction, and CORS checks | Done |
| T11.8 | Deploy the frontend to Vercel production and attach the requested public alias. | T11.4; linked Vercel project | Ready deployment `dpl_4SDwHo26oWHSfy118cRD1wjAunYJ`; `https://chakra-ag.vercel.app`; docs, metadata, links, favicon, and responsive review | Done |
| T11.9 | Build the production container from a clean Docker builder. | T11.2; Docker daemon | `docker buildx build --no-cache --file Dockerfile .`; exported image digest recorded in testing evidence | Done |
| T11.10 | Exercise the CLI-first MetaMask wallet harness through chain add/connect, quote, approve/sign, submit, and confirmation. | T11.4; headed browser; disposable QA wallet; live API | Wallet setup/validate/cleanup artifacts, expected Arc chain, screenshots, receipt, and secret-free artifact scan | Done |
| T11.11 | Retain authenticated production evidence for the healthy 1 USDC to EURC route. | T11.10; funded disposable wallet and provider confirmation | Quote, approval/sign, submit, and confirmed receipt | Done |
| T11.12 | Produce honest live split-route evidence. Local A-C are done: pool-spot impact plus a live-shaped Presto/Xylo optimizer-attempt fixture and lockstep docs. D-F remain approval/liquidity gated. | External liquidity, approved Render deploy, fundable split quote, explicit broadcast approval | Local red/green impact and optimizer tests; then live `is_split: true`, one Arcscan split receipt with at least two sub-routes, and `split_swaps` increment | Follow-up |

## Current progress summary

T11 implementation and public release work is complete through container and
production deployment. Both Chakra branches are synchronized at the latest
evidence checkpoint; the Vercel production alias is assigned to the Ready
deployment, and the clean Docker retry passed. No secrets are stored in this
repository; the operator-provided `RENDER_API_KEY` and disposable
`QA_WALLET_SECRET` remain local environment inputs only.

Headed MetaMask T11.10 / T11.11 settled on 2026-09-05: Connect → add-chain →
Permit2 signature → `splitSwap` confirm. Receipt
`0xee7bc19a990ce6691a68e9b387585baee13edc846cbf3a43551ab3dd7cfcda6c` (block
60563600, 1_000_000 USDC → 1_629_188 EURC via presto-hub). The 2026-09-04
viem CLI swap is a different evidence path. The package lookup for the
separate retired SDK returned not found, so no deprecation mutation was
performed.

## Next actions

1. T11.12-D/E are complete: Render deploy `dep-daenql8u01pc73f9prfg` is live
   from `d4c443a`; health/readiness and the quote matrix passed. The smallest
   observed split was 21,467 USDC, while 21,466 USDC remained single-route.
2. Obtain sufficient legitimate QA-wallet USDC or wait for a lower fundable
   split window; the current 1.949753-USDC balance cannot fund the quote.
3. Only with a fundable split quote and explicit broadcast approval, execute
   one atomic split and require an Arcscan receipt plus `split_swaps` increment
   before closing T11.12 (T11.12-F). Do not manufacture liquidity.

## Risks and sequencing notes

- T11.10 and T11.11 are closed with the headed MetaMask receipt above. Do not
  treat the viem CLI tx as a substitute for that path.
- Split-route and cirBTC split validation depends on external liquidity and
  must not be forced with synthetic balances. Live quotes at 1 / 100 / 1000 /
  10,000 USDC remain `is_split: false`; a two-pool split appears around
  21,467 USDC, but `split_swaps` remains 0 because it is unfundable.
- Any future branch rewrite requires a fresh remote-head check, a local-only
  bundle outside the repository, and explicit `--force-with-lease` values.
- Release evidence must continue to avoid printing or committing local
  credentials.

## Phase 6 update — T11.12 A-C (2026-09-06)

T11.12-A, B, and C are done locally. Presto, Xylo, and Chakra-stable impact is
now integer bps against hydrated reserve-ratio spot via the existing quote-math
helper. The live-shaped 1-USDC Presto/Xylo fixture reports 30 bps best-single
impact against the unchanged 5-bps threshold, attempts two-path Brent across
distinct pools, and honestly rejects with `no_improvement`. SC-2 remains green.
T11.12-D/E subsequently deployed `d4c443a` and proved a live Presto+Xylo split
at 21,467 USDC. The QA wallet holds only 1.949753 USDC, so T11.12-F remains
blocked without a broadcast and T11.12 remains Follow-up.
