# T11 requirements: Chakra identity and Arc-only cleanup

## Objective

Ship the approved Chakra identity and Arc-only product surface while preserving
the existing swap layout, quote flow, transaction builder, and responsive
information hierarchy.

## Required outcomes

- The Rust workspace contains only active snapshot, worker, router, adapter,
  and API crates for Arc EVM.
- Public names are `ChakraClient`, `@chakra-ag/sdk@0.3.0`, and
  `@chakra-ag/frontend`.
- The frontend uses the Sunset Trade palette, DM Sans, JetBrains Mono, semantic
  light/dark tokens, and a three-segment split-ring mark without gradients.
- Documentation, generated packages, tracked paths, and reachable commit
  metadata contain only approved Chakra/Arc terminology and URLs.
- Production checks cover npm, Render, Vercel, hosted API smoke, and the
  authenticated 1 USDC to EURC wallet path.

## Non-goals

Order-book features, analytics, alternate chain runtimes, unsupported venue
adapters, and artificial liquidity scenarios are outside this release.

## Constraints

Testnet only; preserve required third-party license notices and current AI
DevKit configuration. Public npm mutation requires a successful `npm whoami`
check before publishing or deprecating packages.
