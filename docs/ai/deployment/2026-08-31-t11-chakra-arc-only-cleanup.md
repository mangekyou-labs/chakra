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
`d9fcec57125a10af1df76c1fb73b5353af320754` before this evidence update.

The SDK is published as `@chakra-ag/sdk@0.3.0`. Registry identity is
`zerefwtf`; a clean registry install and live quote/build smoke passed. The
client constructor uses the documented `{ apiUrl }` option.

Render health, readiness, tokens, quote, and build-transaction checks passed.
The repository CORS configuration includes the active Vercel aliases, but the
running Render service still serves its previous CORS allow-list. No Render
deployment credential is available in this environment, and the dashboard
redirects to login, so that configuration remains an external deployment
follow-up.

Vercel production deployment `dpl_Hvi7vc41FvPkGoKRXPAVdu8H9Cu5` is ready and
the active aliases serve the rebrand, `/docs`, `/docs/api`, metadata, links,
and the split-ring SVG. The documented `chakra-arc-dex.vercel.app` alias is
outside the available Vercel team and still serves stale content; it must be
reassigned or updated by its owner before it can be called the canonical alias.

The authenticated wallet path was not run because `QA_WALLET_SECRET` is not
present. No transaction evidence was fabricated. The separate legacy package
lookup returned not found, so no deprecation mutation was performed.
