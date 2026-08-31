# T11 planning record: Chakra identity and Arc-only cleanup

## Scope and source records

This plan reconciles the T11 requirements, design, implementation, testing,
and deployment records. The objective is complete for the code, package,
history, infrastructure, and frontend identity work; wallet settlement remains
an explicitly tracked provider-dependent follow-up.

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
| T11.10 | Exercise the CLI-first MetaMask wallet harness through chain add/connect, quote, approve/sign, submit, and confirmation. | T11.4; headed browser; disposable QA wallet; live API | Wallet setup/validate/cleanup artifacts, expected Arc chain, screenshots, receipt, and secret-free artifact scan | Blocked |
| T11.11 | Retain authenticated production evidence for the healthy 1 USDC to EURC route. | T11.10; funded disposable wallet and provider confirmation | Quote, approval/sign, submit, and confirmed receipt | Blocked by T11.10 |
| T11.12 | Revisit split-route and cirBTC scenarios only when live reserves provide multiple healthy routes. | External liquidity and adequate reserves | Honest quote/route evidence; no manufactured liquidity | Follow-up |

## Current progress summary

T11 implementation and public release work is complete through container and
production deployment. Both Chakra branches are synchronized at the latest
evidence checkpoint; the Vercel production alias is assigned to the Ready
deployment, and the clean Docker retry passed. No secrets are stored in this
repository; the operator-provided `RENDER_API_KEY` and disposable
`QA_WALLET_SECRET` remain local environment inputs only.

The wallet runner now reaches the current Chakra UI, wallet connection, Arc
chain state, and the healthy 1 USDC to EURC quote, but the provider exposes no
transaction notification page for approval. The app remains at the swap state
with no submitted transaction or fabricated receipt. The package lookup for
the separate retired SDK returned not found, so no deprecation mutation was
performed.

## Next actions

1. Resolve the headed MetaMask network-add confirmation in the disposable QA
   session, then rerun the CLI-first wallet setup, validation, and cleanup
   artifacts without exposing credentials.
2. Run the production 1 USDC to EURC wallet path and retain the real
   transaction confirmation evidence.
3. Monitor live reserves and add split-route/cirBTC evidence only after the
   required healthy liquidity exists.

## Risks and sequencing notes

- T11.10 and T11.11 are provider/browser execution blockers, not code or
  credential blockers; do not claim settlement evidence until a receipt exists.
- Split-route and cirBTC validation depends on external liquidity and must not
  be forced with synthetic balances.
- Any future branch rewrite requires a fresh remote-head check, a local-only
  bundle outside the repository, and explicit `--force-with-lease` values.
- Release evidence must continue to avoid printing or committing local
  credentials.
