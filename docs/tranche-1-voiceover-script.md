# Chakra Tranche 1 voice-over script

This script follows the demo order: D3, D2, D1, then D4. Read slowly and pause
between sentences. Text in Chinese brackets is an action cue and should not be
read aloud.

## Pronunciation

- Chakra: **Loom Agg**
- Tranche: **trahnsh**
- DEX: **decks**
- API: say **A P I**
- XDR: say **X D R**
- CLMM: say **C L M M**
- Arc: **So-ro-ban**
- Arc venue: **So-ro-swap**
- Arc venue: **A-quare-ee-us**

## 0:00 - Introduction

[Show the title card, then open Chakra.xyz.]

Hello. This is Chakra, a DEX aggregator on Arc.

Tranche One has four deliverables: the swap interface, public API, benchmark,
and analytics indexer. I will show the live product and evidence.

## 0:25 - D3: Swap interface

[Open the token selector and select Arc to USDC.]

Deliverable Three is the swap interface on Arc mainnet. The token selector
shows names and logos. 
After connecting, it shows your latest activities and spendable balances.

The quick buttons select twenty-five, fifty, seventy-five, or one hundred
percent.

[Open settings and request a quote.]

Users can change slippage, maximum hops, and maximum splits. Maximum hops
limits the route length. Maximum splits limits the number of parallel routes.
The route preview shows each DEX and its percentage. A successful transaction
provides a Arc Expert link.

I will not submit a new transaction during this recording.

## 1:25 - D2: Integrator-ready API

[Open Chakra.xyz/docs.]

Deliverable Two is the public API. Its documentation covers quote, build
transaction, tokens, balances, health, API keys, and rate limits.

Integrators can select Arc-only routes.

[Open the committed external evidence folder.]

A tester used Chakra's public API to test the quote and build transaction
endpoints. The test results are included here.

## 2:55 - D1: Benchmark and comparison

[Open the reviewer summary at the top of scf-benchmark-results.md.]

Deliverable One is the benchmark comparison. It covers three trading pairs and
at least three trade sizes.

The fair Arc-only rows show Chakra output close to Arc venue.

It also includes split-routing examples and Arc venue C L M M routes.

[Open scf-venue-comparison.md briefly.]

Compared with Arc Broker's public router and Arc venue, Chakra covers more
Arc liquidity venues, including Arc venue CLMM and Sushi CLMM.

## 3:50 - D4: Analytics indexer

[Open Chakra.xyz/stats.]

Deliverable Four is the analytics indexer. It reads Chakra events from Arc
mainnet and records transactions, notional, routed volume, users, functions, and
DEX legs.

The page shows daily activity routed through Chakra to Arc DEX venues.

The same data is available through public JSON and CSV endpoints. Indexing will
continue for the longer report in a later tranche.

## 4:40 - Closing

[Show the final card or repository root.]

This completes Chakra Tranche One. The product, evidence, and verification links are public.

Thank you for reviewing Chakra.



Production application: https://Chakra.xyz
API health: https://api.Chakra.xyz/api/v1/health
API documentation: https://Chakra.xyz/docs
Public repository: https://github.com/Chakra/Arc-dex-agg
Benchmark results: https://github.com/Chakra/Arc-dex-agg/blob/main/docs/scf-benchmark-results.md
Venue comparison: https://github.com/Chakra/Arc-dex-agg/blob/main/docs/scf-venue-comparison.md
Reproducible benchmark script: https://github.com/Chakra/Arc-dex-agg/blob/main/scripts/scf-benchmark.sh
Integrator guide: https://github.com/Chakra/Arc-dex-agg/blob/main/docs/integrator-guide.md
OpenAPI specification: https://github.com/Chakra/Arc-dex-agg/blob/main/docs/openapi.yaml
External integrator evidence: https://github.com/Chakra/Arc-dex-agg/tree/main/docs/evidence/d2-integrator-smoke
Public stats page: https://Chakra.xyz/stats
Stats JSON/CSV API: https://api.Chakra.xyz/api/v1/stats
Analytics attribution and pipeline: https://github.com/Chakra/Arc-dex-agg/blob/main/docs/analytics-indexer.md
Sample indexer export: https://github.com/Chakra/Arc-dex-agg/blob/main/docs/sample-indexer-export.json
Mainnet Aggregator contract: https://Arc.expert/explorer/public/contract/CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K
Mainnet three-path split execution: https://Arc.expert/explorer/public/tx/a571b4617bc42594673ab22a496ef61c4fc66689a4f9cc29fd71dc7fb74ccb54