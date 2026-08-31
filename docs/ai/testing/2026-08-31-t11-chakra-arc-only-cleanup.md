# T11 testing record

## Local gates

| Area | Command | Result |
| --- | --- | --- |
| Rust format | `cargo fmt --all -- --check` | passed |
| Rust tests | `cargo test --workspace` | passed |
| Rust lint | `cargo clippy --workspace --all-targets -- -D warnings` | passed on synchronized Chakra head |
| Rust release | `cargo build --workspace --release` | passed on synchronized Chakra head |
| Contracts | `cd contracts/evm && forge test` | blocked: installed Foundry binary crashes in macOS `system-configuration` before test execution |
| SDK | `npm test`, `npm run build`, `npm pack --dry-run` | passed (8 tests; `chakra-ag-sdk-0.3.0.tgz`) |
| SDK registry smoke | clean install, quote, build transaction | passed; published package imports `ChakraClient` |
| Frontend | unit, typecheck, lint, format, production build | 67 unit tests passed; all listed gates passed |
| Lineage | `python3 scripts/check-lineage.py all` | passed in a fresh clone of the synchronized remote |
| Docker | `docker buildx build --no-cache --file Dockerfile .` | passed; clean builder completed release compilation and exported the image (digest `sha256:b503d97edba5d6443dd094cf1d2dc1d9161428cbfcffcab6962e0849c44885bc`) |

## Production gates

Render redeployment `dep-daagnntg1s2s73d4rh70` is live from commit `d339f2b`.
Health/readiness/tokens/quote/build responses passed. The live 1 USDC
to EURC quote returned one `xylo` route, expected output `805774`, and the
build response returned chain `5042002`, zero native value, calldata, and no
required approvals. CORS preflight passed for both active Vercel aliases.
Vercel production inspect reported Ready for deployment
`dpl_4SDwHo26oWHSfy118cRD1wjAunYJ`, now assigned to
`https://chakra-ag.vercel.app`; metadata, favicon, links, docs routes, and
responsive browser review passed on the active aliases. CORS preflight for
`https://chakra-ag.vercel.app` now returns the matching allow-origin header.

The authenticated wallet run used the existing local `QA_WALLET_SECRET` but
the headed Chromium/MetaMask bootstrap closed before the wallet initialized.
The earlier run reached MetaMask's network-add risk confirmation; no swap was
submitted and there is no transaction evidence. Split-route and thin-pool
scenarios remain honest follow-up checks.
