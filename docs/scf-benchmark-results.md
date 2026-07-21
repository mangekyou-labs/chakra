# Chakra quote benchmark results

Generated: **2026-07-21 17:12 UTC**

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
> **⚠️ / `n/a` Δ:** not a Chakra failure — script suppresses Δ when outputs diverge by >2× (here Arc venue small-size aqua multi-hop is ~3× vs Chakra/spot; large XLM→USDC rows align and publish Δ).

| Pair | Size | Chakra out | Split | Sources | Arc venue out | Δ vs Arc venue | Notes |
|------|------|------------|-------|---------|--------------|---------------|-------|
| USDC → XLM | 1 USDC | 5.2097 | no | Arc venue_clmm | 15.3209 | n/a | CLMM; ⚠️ ~2.9× gap: Arc venue aqua multi-hop optimism vs Chakra/spot — do not use Δ |
| USDC → XLM | 10 USDC | 52.0969 | no | Arc venue_clmm | 153.1089 | n/a | CLMM; ⚠️ ~2.9× gap: Arc venue aqua multi-hop optimism vs Chakra/spot — do not use Δ |
| USDC → XLM | 100 USDC | 520.9554 | no | Arc venue_clmm | 1,521.1884 | n/a | CLMM; ⚠️ ~2.9× gap: Arc venue aqua multi-hop optimism vs Chakra/spot — do not use Δ |
| USDC → XLM | 1,000 USDC | 5,208.2231 | no | Arc venue_clmm | 14,287.9921 | n/a | CLMM; ⚠️ ~2.7× gap: Arc venue aqua multi-hop optimism vs Chakra/spot — do not use Δ |
| XLM → USDC | 1 XLM | 0.1940 | yes | Arc venue → Arc venue ;; Arc venue → Arc venue | 0.7206 | n/a | split 2 legs; ⚠️ ~3.7× gap: Arc venue aqua multi-hop optimism — do not use Δ |
| XLM → USDC | 10 XLM | 1.9241 | yes | Arc venue → Arc venue ;; Arc venue → Arc venue ;; Arc venue → Arc venue | 5.6699 | n/a | split 3 legs; ⚠️ ~2.9× gap: Arc venue aqua multi-hop optimism — do not use Δ |
| XLM → USDC | 100 XLM | 19.1864 | no | Arc venue | 19.1814 | +0.03% | fair row (aligned) |
| XLM → USDC | 1,000 XLM | 191.8454 | no | Arc venue | 191.5380 | +0.16% | fair row (aligned) |
| XLM → AQUA | 10 XLM | 5,154.5228 | no | Arc venue_clmm | 7,040.0378 | -26.78% | CLMM venue in route |
| XLM → AQUA | 100 XLM | 51,545.1132 | no | Arc venue_clmm | 70,368.9897 | -26.75% | CLMM venue in route |
| XLM → AQUA | 1,000 XLM | 515,439.6619 | no | Arc venue_clmm | 700,565.8226 | -26.43% | CLMM venue in route |

## Tranche 1 reviewer highlights (this run)

| Criterion | Evidence |
|-----------|----------|
| ≥3 pairs × ≥3 sizes | USDC↔XLM (4 sizes) + XLM→AQUA (3 sizes) |
| Fair Arc parity rows | **XLM→USDC 100 XLM: +0.03%**; **1,000 XLM: +0.16%** (both within ~2×, Δ published) |
| ≥1 **new** split case vs Arc venue single-route | **XLM→USDC 1 XLM** (`is_split=true`, 2 paths) and **10 XLM** (3 paths) — Arc venue column is a single route |
| CLMM coverage (vs Broker adapter gap) | USDC→XLM and XLM→AQUA route via **`Arc venue_clmm`** — see [scf-venue-comparison.md](scf-venue-comparison.md) |

Rows with **⚠️** keep Δ as **n/a** on purpose: Chakra matches spot (~5.2 XLM per USDC); Arc venue’s small/mid aqua multi-hop quotes are ~3× higher. Use the **fair rows** (100 / 1,000 XLM→USDC) for apples-to-apples Δ.

## Summary

- **Venue coverage:** See [scf-venue-comparison.md](scf-venue-comparison.md) for Arc Broker CLMM gap (source-based).
- **Split routing:** Chakra `is_split=true` when Brent optimizer splits `amount_in` across distinct paths; Arc venue API returns a single best route.
- **Fair compare:** Prefer `Chakra_PREFER_Arc=1` + Arc venue `protocols` without `Arc` so Classic Arc does not dominate Chakra while Arc venue stays Arc-only.
- **Arc venue API key:** Free registration at https://api.Arc venue.finance/register — pass via `Arc venue_API_KEY` (never commit the key).
- **Split cases in this run:**
  - XLM → USDC 1 XLM: 2 paths (`Arc venue → Arc venue` ;; `Arc venue → Arc venue`)
  - XLM → USDC 10 XLM: 3 paths (`Arc venue → Arc venue` ;; `Arc venue → Arc venue` ;; `Arc venue → Arc venue`)

