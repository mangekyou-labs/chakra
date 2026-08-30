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

Implementation is local in granular commits. The read-only
remote check confirmed `chakra/feature-chakra` at `671f478` and `chakra/main` at
`208d5ff`; a complete pre-rewrite bundle was created and verified outside the
repository. The public mutation gates are pending. The fresh
credential check returned npm HTTP 401 from `npm whoami`; `npm view
@chakra-ag/sdk version` also cannot authorize the package lookup. The local
SDK pack dry run succeeds, but no package publish, deprecation, branch rewrite,
force-push, Render deploy, Vercel deploy, or wallet run is attempted until npm
authentication is repaired and the requested public-mutation sequence can
start safely. Render, Vercel, and wallet checks remain after the cleaned
feature head is verified.
