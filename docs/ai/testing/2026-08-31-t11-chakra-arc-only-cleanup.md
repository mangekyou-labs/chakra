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

The authenticated wallet run used the existing local `QA_WALLET_SECRET` and
the canonical `https://chakra-ag.vercel.app` alias. An earlier harness run
reached the UI, connected, switched to Arc, and quoted, then blocked at the
MetaMask notification with no fabricated receipt.

## Headed MetaMask settlement (2026-09-05)

`packages/frontend` `npm run qa:wallet` (headed Chromium, dappwright MetaMask
13.17.0): 1 passed, 39.1s. The spec keeps `wallet.page` on extension home and
opens the DApp in `context.newPage()`. `QA_WALLET_SECRET` was not printed
(`qa:wallet:validate` reported a 24-word mnemonic only).

On-chain receipt, Arcscan
`0xee7bc19a990ce6691a68e9b387585baee13edc846cbf3a43551ab3dd7cfcda6c`:
status ok, block 60563600, 2026-09-05T10:24:14Z, method `0x2e3be0c1`
(`splitSwap`), value 0, from `0xc603C39102b84c101f21F3b9723780F8F84dCE76` to
aggregator `0xeb12351602c56D47c4EE955193335848952b29d8`. Transfers:
1_000_000 USDC in → pool `0x5794a8284A29493871Fbfa3c4f343D42001424D6` →
1_629_188 EURC back. UI quote on that run was 1.0 USDC → 1.627293 EURC
(single-path `presto-hub`). T11.10 and T11.11 are closed.

Split-route live evidence remains an honest follow-up: hosted `split_swaps`
is 0 and probed catalog quotes return `is_split: false`. Do not manufacture
liquidity.
