# T11: Chakra identity and Arc-only cleanup

**Date:** 2026-08-30  
**Status:** approved design  
**Branch:** `feature-chakra`

## Objective

Finish the Chakra rebrand and reduce the published product to its active Arc architecture while preserving the existing frontend layout, swap flow, and useful granular Chakra implementation history. The work ends in a verified production release across npm, Render, and Vercel.

## Scope and boundaries

The work is one gated release train with four implementation boundaries:

1. **Lineage gate.** Add a regression checker before cleanup. It derives forbidden first-party vocabulary from encoded fragments so the checker does not contain the guarded literal terms. It scans tracked paths, tracked content, generated package output, documentation, and all reachable commit metadata.
2. **Arc runtime.** Keep only the active Rust workspace crates: market snapshot, market-data worker, router engine, dex adapters, and API server. Rename runtime mode/config identifiers to the Chakra names, remove legacy transaction/simulation/venue interfaces and limit-order surfaces, and use chain-neutral atomic-unit constants.
3. **Public interfaces and frontend.** Publish the JavaScript SDK as `@chakra-ag/sdk@0.3.0` with `ChakraClient`; rename the private frontend package to `@chakra-ag/frontend`; update repository metadata, examples, imports, lockfiles, and package contents. Rebrand the existing frontend in place with the Sunset Trade palette, existing DM Sans and JetBrains Mono typography, semantic light/dark tokens, no gradients, a code-native three-segment split-ring SVG, GitHub/docs links, and no Discord.
4. **Documentation and history.** Consolidate documentation to supported Chakra surfaces and retain legally required third-party licenses. Before history mutation, freeze updates, verify expected remote heads, and create a local-only bundle outside the repository. Use a temporary mirror and the confirmed `git-filter-repo` tool through `uvx` to remove obsolete paths/blobs/messages from every reachable commit, then point both Chakra branches at the cleaned feature result. The separate legacy public repository is out of scope.

## Design direction

Direction: **warm-monochrome industrial workstation**  
Density: **comfortable**, with compact numeric presentation where trading data requires it  
Surface: **defined dark panels on a warm dark base**  
Type mood: **precise, restrained, technical**  
Motion: **crisp ease-out, reduced-motion safe**

Use `#15100D` as the base, `#1F1712` for elevated surfaces, `#EC7A3A` for the primary action, `#F5A877` for soft emphasis, and `#F6F3EE` for foreground. Keep semantic tokens for both themes in `brand.md` and the frontend token layer. Avoid pure black, gradients, decorative competing hues, and layout restructuring. Preserve responsive breakpoints and information hierarchy at 375px, 768px, and 1440px.

## Release flow

Every stage produces fresh evidence and blocks later public mutation on failure:

```text
npm auth + whoami + package availability + pack dry-run
        ↓
T11 implementation + focused tests + full build matrix
        ↓
bundle + temporary mirror + all-history forbidden-term scan
        ↓
npm publish/install/smoke → deprecate old versions
        ↓
Render redeploy + health/readiness/token/quote/build smoke
        ↓
Vercel production deploy + HTTP/CORS/metadata/link/favicon/responsive smoke
        ↓
authenticated 1 USDC → EURC quote/approve/sign/submit/confirmation evidence
```

The original branch state remains recoverable through the local bundle and refs until the rewritten branches are verified. The real-wallet run may only use healthy existing liquidity; it must not manufacture liquidity for split-route or cirBTC claims.

## Error handling and rollback

The lineage checker is a hard gate. Focused checks run after each implementation slice; failures are recorded with the command and output before remediation. Public release commands are never retried blindly. npm publication is followed by clean-project installation and smoke usage before deprecations. Render and Vercel deployments are inspected before production claims are made. If a release gate fails, stop at that gate, preserve evidence, and use the pre-rewrite bundle or provider rollback facilities where applicable.

## Verification plan

- **Rust:** format check, workspace tests, all-target Clippy with denied warnings, release build, Docker build, and EVM contract tests when the toolchain is available.
- **Frontend:** unit tests, typecheck, lint, format check, production build, semantic-token/contrast tests, and link/logo/theme regressions.
- **SDK:** unit tests, TypeScript build, package-content inspection, publish dry-run, clean npm installation, quote/build smoke usage, and hosted API smoke.
- **Browser:** swap flow, docs, wallet states, keyboard focus, hover/touch states, SVG rendering, and WCAG AA at 375px, 768px, and 1440px.
- **History:** fresh clone, both rewritten branches, every reachable commit, forbidden paths/blobs/messages, remote URLs, and generated artifacts.
- **Production:** Render health/readiness/tokens/quote/build transaction, Vercel HTTP/CORS/metadata/favicon/link checks, and the authenticated healthy 1 USDC → EURC wallet path with transaction evidence.

## Known constraints

npm authentication is the only declared credential blocker and must be resolved before any public npm mutation. Split-route proof and cirBTC routing remain honest follow-up items when live reserves do not support them. Independent clones, caches, archives, npm history, and the separate legacy repository cannot be rewritten by this task.
