# T11 testing record

## Local gates

| Area | Command | Result |
| --- | --- | --- |
| Rust format | `cargo fmt --all --check` | pending fresh run |
| Rust tests | `cargo test --workspace` | pending fresh run |
| Rust lint | `cargo clippy --workspace --all-targets -- -D warnings` | pending fresh run |
| Rust release | `cargo build --workspace --release` | pending fresh run |
| Contracts | `cd contracts/evm && forge test` | pending fresh run |
| SDK | `npm test`, `npm run build`, `npm pack --dry-run` | pending fresh run |
| Frontend | unit, typecheck, lint, format, production build | pending fresh run |
| Lineage | `python3 scripts/check-lineage.py all` | intentionally red until history rewrite |

## Production gates

Validate API health, readiness, tokens, quote, and build transaction responses;
validate Vercel HTTP/CORS, metadata, favicon, links, and responsive layout;
then execute the healthy 1 USDC to EURC wallet flow with retained transaction
evidence. Split-route and thin-pool scenarios remain honest follow-up checks.
