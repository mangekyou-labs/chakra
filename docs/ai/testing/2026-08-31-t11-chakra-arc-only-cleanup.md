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
| Docker | `docker buildx build --file Dockerfile .` | blocked: Docker Desktop daemon did not respond to the client ping; retry interrupted |

## Production gates

Render health/readiness/tokens/quote/build responses passed. The live 1 USDC
to EURC quote returned one `xylo` route, expected output `805774`, and the
build response returned chain `5042002`, zero native value, calldata, and no
required approvals. CORS preflight passed for both active Vercel aliases.
Vercel production inspect reported Ready; metadata, favicon, links, docs
routes, and responsive browser review passed on the active aliases.

The authenticated wallet run used the existing local `QA_WALLET_SECRET` but
stopped at MetaMask's network-add risk confirmation. The wallet remained off
Arc, so the swap was not attempted and there is no transaction evidence. The
QA runner now handles the warning screens and records the remaining provider
blocker. Split-route and thin-pool scenarios remain honest follow-up checks.
