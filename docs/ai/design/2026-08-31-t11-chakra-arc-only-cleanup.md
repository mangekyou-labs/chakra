# T11 design: Chakra Arc-only surface

## Runtime

The active data path is Arc EVM logs and RPC into one market-data worker, Redis
pool snapshots, stateless API hydration, local path finding and quote math, and
wallet-submitted aggregator calldata. The API never owns signing keys.

Runtime configuration is named `RuntimeMode`, `runtime_mode`, and
`CHAKRA_RUNTIME_MODE`. The workspace has no alternate runtime compatibility
surface.

## Public interfaces

`GET /api/v1/quote` returns explicit per-hop `dex_types`, `hop_fees`, and
`hop_factories`. `POST /api/v1/build_tx` validates those identities and emits
aggregator calldata plus optional Permit2 typed data. No deprecated request
fields or transaction formats are accepted.

## Frontend identity

Use the existing component tree and swap flow. Apply the warm dark-first
palette documented in `brand.md`; provide complete light and dark semantic
tokens. The split-ring SVG is code-native, asymmetric, and arrow-free. GitHub
points to the Chakra repository, documentation points to `/docs`, and the
social navigation contains no community-chat link.

## Release safety

Create a local-only bundle before history cleanup. Rewrite only a temporary
mirror with the confirmed filtering tool, verify both target branches and all
reachable commits, then force-push with explicit leases to the Chakra remote.
The separate historical public repository is not modified.
