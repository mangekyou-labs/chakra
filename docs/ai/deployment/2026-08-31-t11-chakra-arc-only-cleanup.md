# T11 deployment record

## Order of operations

1. Confirm npm identity, package availability, and SDK pack dry run.
2. Freeze updates, fetch the Chakra remote, verify expected heads, and create
   a bundle outside the repository.
3. Rewrite the temporary mirror, scan both branches and all reachable commits,
   and force-push with `--force-with-lease`.
4. Publish `@chakra-ag/sdk@0.3.0`, install it in a clean temporary project,
   and run quote/build smoke usage.
5. Redeploy Render and validate health, readiness, tokens, quote, and build.
6. Deploy Vercel production and validate alias, API origin, CORS, metadata,
   favicon, documentation link, and responsive presentation.
7. Run the authenticated 1 USDC to EURC wallet path and retain the receipt.

## Current status

The implementation was completed in granular commits. A bundle was created
and verified outside the repository before rewriting. A fresh clone of the
cleaned mirror passes the full lineage scan, and both public branches are
synchronized at `d339f2b`.

The SDK is published as `@chakra-ag/sdk@0.3.0`. Registry identity is
`zerefwtf`; a clean registry install and live quote/build smoke passed. The
client constructor uses the documented `{ apiUrl }` option.

Render deployment `dep-daagnntg1s2s73d4rh70` is live from commit `d339f2b`.
Health, readiness, tokens, quote, and build-transaction checks passed. The
1 USDC to EURC quote returned one healthy `xylo` route with expected output
`805774`; build transaction returned chain `5042002`, `value: "0"`, calldata,
and no required approvals. CORS preflight passed for both active Vercel
aliases. The requested `https://chakra-ag.vercel.app` origin is now included
in the live Render CORS allowlist. The operator-provided Render credential is
available from the local worktree `.env` as `RENDER_API_KEY` and is never
committed or printed.

Vercel production deployment `dpl_4SDwHo26oWHSfy118cRD1wjAunYJ` is ready at
`https://chakra-arc-5l68aer41-gadillacers-projects.vercel.app`. The requested
production alias `https://chakra-ag.vercel.app` now points to that deployment;
the compatibility alias `https://chakra-arc-dex.vercel.app` was also repointed
to the same current deployment. The project aliases `https://frontend-ruddy-two-90.vercel.app` and
`https://chakra-arc-dex-gadillacers-projects.vercel.app` remain active. They
serve the rebrand, `/docs`, `/docs/api`, metadata, links, and split-ring SVG.
The deployment was created from the synchronized `d8dd20b` head and inspected
with Vercel as `Ready`.

The disposable wallet credential is available from the local worktree `.env`
as `QA_WALLET_SECRET` and is never committed or printed. The latest headed
wallet run used the canonical alias, reached the current Chakra UI and live
quote, then blocked because no MetaMask transaction notification appeared;
no transaction was submitted and no receipt was fabricated. The separate
legacy package lookup returned not found, so no deprecation mutation was performed.
