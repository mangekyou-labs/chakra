# T11 testing record

## Local gates

| Area | Command | Result |
| --- | --- | --- |
| Rust format | `cargo fmt --all -- --check` | passed |
| Rust tests | `cargo test --workspace` | passed |
| Rust lint | `cargo clippy --workspace --all-targets -- -D warnings` | passed from the fresh remote clone |
| Rust release | `cargo build --workspace --release` | passed from the fresh remote clone |
| Contracts | `cd contracts/evm && forge test` | 88 passed in a dependency-equipped isolated checkout; fresh checkout lacks ignored Forge libraries |
| SDK | `npm test`, `npm run build`, `npm pack --dry-run` | passed |
| SDK registry smoke | clean install, quote, build transaction | passed |
| Frontend | unit, typecheck, lint, format, production build | 67 unit tests passed; all listed gates passed |
| Lineage | `python3 scripts/check-lineage.py all` | passed in a fresh checkout |
| Docker | `docker buildx build --file Dockerfile .` | blocked by Docker Desktop overlay metadata I/O during image commit |

## Production gates

Render health/readiness/quote/build responses passed. Vercel production HTTP,
metadata, favicon, links, docs routes, and responsive browser review passed on
the active aliases; browser API requests still expose the stale Render CORS
allow-list. The healthy 1 USDC to EURC wallet flow remains pending the
disposable QA wallet secret. Split-route and thin-pool scenarios remain honest
follow-up checks.
