# Chakra quote benchmark results

Generated: **2026-07-14 00:03 UTC**

- Chakra API: `https://api.Chakra.xyz` (`prefer_Arc=1`)
- Arc venue API: `https://api.Arc venue.finance` protocols=`Arc venue,Arc venue,aqua` (key provided)

Reproduce:

```bash
./scripts/scf-benchmark.sh
# Arc-only fair compare:
Chakra_PREFER_Arc=1 Arc venue_PROTOCOLS=Arc venue,Arc venue,aqua Arc venue_API_KEY=sk_... ./scripts/scf-benchmark.sh
# Full compare (include Arc on both sides when Chakra omits prefer_Arc):
Arc venue_PROTOCOLS=Arc venue,Arc venue,aqua,Arc Arc venue_API_KEY=sk_... ./scripts/scf-benchmark.sh
OUTPUT=docs/scf-benchmark-results.md ./scripts/scf-benchmark.sh
```

> **Interpretation:** Use `Chakra_PREFER_Arc=1` + Arc venue without `Arc` for Arc-only rows. Include `Arc` in `Arc venue_PROTOCOLS` when comparing full aggregation. Positive Δ = Chakra higher output for same `amount_in`.

| Pair | Size | Chakra out | Split | Sources | Arc venue out | Δ vs Arc venue | Notes |
|------|------|------------|-------|---------|--------------|---------------|-------|
| USDC → XLM | 1 USDC | 5.5343 | no | sushi | 39.8184 | n/a | CLMM venue in route; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| USDC → XLM | 10 USDC | 55.3429 | no | sushi | 397.5412 | n/a | CLMM venue in route; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| USDC → XLM | 100 USDC | 553.3874 | no | sushi | 3,912.2814 | n/a | CLMM venue in route; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| USDC → XLM | 1,000 USDC | 5,531.8914 | no | Arc venue_clmm | 33,761.4350 | n/a | CLMM venue in route; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| XLM → USDC | 1 XLM | 0.1856 | yes | Arc venue → Arc venue ;; Arc venue → Arc venue ;; Arc venue → Arc venue | 0.6976 | n/a | split 3 legs; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| XLM → USDC | 10 XLM | 1.8039 | no | Arc venue_clmm | 5.5136 | n/a | CLMM venue in route; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| XLM → USDC | 100 XLM | 18.0387 | no | Arc venue_clmm | 18.0492 | -0.06% | CLMM venue in route |
| XLM → USDC | 1,000 XLM | 180.3834 | no | Arc venue_clmm | 180.4751 | -0.05% | CLMM venue in route |
| XLM → AQUA | 10 XLM | 5,094.0778 | no | Arc venue → Arc venue | 14,645.6265 | n/a | ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| XLM → AQUA | 100 XLM | 50,913.5404 | no | Arc venue_clmm | 146,350.5060 | n/a | CLMM venue in route; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |
| XLM → AQUA | 1,000 XLM | 509,107.7012 | no | Arc venue_clmm | 1,453,011.7464 | n/a | CLMM venue in route; ⚠️ outputs not comparable (>2× gap — check venue / token mismatch) |

## Summary

- **Venue coverage:** See [scf-venue-comparison.md](scf-venue-comparison.md) for Arc Broker CLMM gap (source-based).
- **Split routing:** Chakra `is_split=true` when Brent optimizer splits `amount_in` across distinct paths; Arc venue API returns a single best route.
- **Fair compare:** Prefer `Chakra_PREFER_Arc=1` + Arc venue `protocols` without `Arc` so Classic Arc does not dominate Chakra while Arc venue stays Arc-only.
- **Arc venue API key:** Free registration at https://api.Arc venue.finance/register — pass via `Arc venue_API_KEY` (never commit the key).
- **Split cases in this run:**
  - XLM → USDC 1 XLM: 3 paths (`Arc venue → Arc venue` ;; `Arc venue → Arc venue` ;; `Arc venue → Arc venue`)

## What's new vs 2026-06-25 resubmission snapshot

| Item | 2026-06-25 | **2026-07-14 (this run)** |
|------|------------|---------------------------|
| Chakra mode | all venues (Classic often won USDC↔XLM) | **`prefer_Arc=1`** (fair vs Arc venue Arc protocols) |
| Fair rows (Δ) | XLM→USDC 100 / 1,000 XLM (−2.92% / +0.02%) | XLM→USDC 100 / 1,000 XLM (**−0.06% / −0.05%**, Arc venue CLMM) |
| New split case | XLM→USDC 1 XLM: 2 paths (Arc venue + Arc venue) | XLM→USDC 1 XLM: **3 paths** (Arc venue×3) — **new** vs prior split shape |
| USDC→XLM | Classic DEX | **Sushi / Arc venue CLMM** (no Classic under prefer_Arc) |
| XLM→AQUA | Arc venue CLMM; delta n/a | Arc venue xy=k / CLMM; Arc venue still >2× → **n/a** |

**SCF takeaway:** At 100–1,000 XLM→USDC, Chakra tracks Arc venue within **0.1%** on Arc-only quotes while additionally demonstrating **multi-path `is_split`** and **Sushi + Arc venue CLMM** venue coverage.

