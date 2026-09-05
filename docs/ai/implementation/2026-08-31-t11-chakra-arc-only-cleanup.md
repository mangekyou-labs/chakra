# T11 implementation record

## Completed locally

- Added the literal-free lineage regression gate at
  `scripts/check-lineage.py`; it scans tracked paths, generated package roots,
  content, and commit metadata.
- Reduced the Cargo workspace to snapshot, worker, router, adapter, and API
  crates; removed obsolete workspace members and inactive contract surfaces.
- Removed alternate Redis environment fallbacks, source-string venue inference,
  snapshot field aliases, and retired transaction interfaces.
- Published interface names now use `ChakraClient`, `@chakra-ag/sdk`, and
  `@chakra-ag/frontend`; the SDK mapper consumes server-owned hop metadata.
- Applied the Sunset Trade tokens, DM Sans/JetBrains Mono, no-gradient rule,
  and split-ring SVG across the frontend.
- Consolidated current API, OpenAPI, integration, deployment, QA, manifest,
  evidence, and T11 records; removed stale product documentation.

## Fresh verification evidence (2026-08-31)

- `cargo fmt --all -- --check` passed (stable rustfmt reports only the existing
  nightly-option warnings).
- `cargo test --workspace` passed: 19 API unit, 17 API build, 12 API REST, 6
  venue, 10 snapshot lifecycle, 77 adapter, 28 worker, 33 snapshot, and 51
  router tests, plus doc tests.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo build --workspace --release` passed.
- SDK `npm test` passed (8 tests), `npm run build` passed, and
  `npm pack --dry-run` contains only README, package metadata, and the two
  distributable files.
- Frontend `npm test` passed (67 tests), `npm run typecheck`, `npm run lint`,
  `npm run format:check`, and `npm run build -- --webpack` passed.
- `python3 scripts/check-lineage.py files` passed with zero violations. The
  commit scan remains intentionally red until the isolated history rewrite.

## Remaining release gates

1. Fresh Rust, Foundry, SDK, and frontend verification.
2. Successful npm authentication, pack dry run, publish, and registry install.
3. Temporary-mirror history cleanup and fresh-clone scan.
4. Render redeploy, Vercel production deploy, and hosted smoke — done.
   Headed MetaMask wallet evidence (T11.10 / T11.11) — done, receipt
   `0xee7bc19a990ce6691a68e9b387585baee13edc846cbf3a43551ab3dd7cfcda6c`.
   T11.12 split-route live evidence is still open (`split_swaps` 0).
