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
and verified outside the repository before rewriting. The cleaned mirror passed
the full lineage scan, and both public branches are synchronized at
`63f690c` before this evidence update. The wallet QA runner's MetaMask warning
handling is included in that head.

The SDK is published as `@chakra-ag/sdk@0.3.0`. Registry identity is
`zerefwtf`; a clean registry install and live quote/build smoke passed. The
client constructor uses the documented `{ apiUrl }` option.

Render deployment `dep-daa7nb67bikc73fleun0` is live from commit `efaf383`.
Health, readiness, tokens, quote, and build-transaction checks passed. The
1 USDC to EURC quote returned one healthy `xylo` route with expected output
`805774`; build transaction returned chain `5042002`, `value: "0"`, calldata,
and no required approvals. CORS preflight passed for both active Vercel
aliases. The operator-provided Render credential is available from the local
worktree `.env` as `RENDER_API_KEY` and is never committed or printed.

Vercel production deployment `dpl_9M23MoL5QT7hi9u4CQnyWeWzsHth` is ready at
`https://chakra-arc-62ly0jgt0-gadillacers-projects.vercel.app`. Active aliases
are `https://frontend-ruddy-two-90.vercel.app` and
`https://chakra-arc-dex-gadillacers-projects.vercel.app`; they serve the
rebrand, `/docs`, `/docs/api`, metadata, links, and split-ring SVG. The old
unmanaged alias is not treated as canonical.

The disposable wallet credential is available from the local worktree `.env`
as `QA_WALLET_SECRET` and is never committed or printed. The authenticated
wallet run reached MetaMask's Arc network-add warning but did not complete the
confirmation, leaving the app on `Switch to Arc Testnet`; no transaction was
submitted and no receipt was fabricated. The separate legacy package lookup
returned not found, so no deprecation mutation was performed.
