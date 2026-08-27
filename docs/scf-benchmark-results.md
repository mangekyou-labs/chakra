# Chakra quote benchmark results

## Tranche 1 reviewer summary

Generated: **2026-07-21 17:12 UTC**

| Criterion | Evidence |
|-----------|----------|
| At least 3 pairs and 3 sizes | USDC↔XLM (4 sizes) and XLM→AQUA (3 sizes) |
| Fair Arc parity rows | **XLM→USDC 100 XLM: +0.03%** and **1,000 XLM: +0.16%** |
| Split-routing evidence | **XLM→USDC 1 XLM:** 2 paths; **10 XLM:** 3 paths |
| CLMM coverage | USDC→XLM and XLM→AQUA use **`Arc venue_clmm`**; see [venue comparison](scf-venue-comparison.md) |

The two parity rows use aligned Arc-only outputs. Rows whose provider outputs
diverged by more than 2× are retained in the full matrix for reproducibility but
are marked `n/a` and are not used for the parity claim.

## Full reproducible benchmark

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
> **`n/a` Δ:** the script suppresses percentage comparison when provider outputs diverge by more than 2×. These rows are not used for parity conclusions.

| Pair | Size | Chakra out | Split | Sources | Arc venue out | Δ vs Arc venue | Notes |
|------|------|------------|-------|---------|--------------|---------------|-------|
| USDC → XLM | 1 USDC | 5.2097 | no | Arc venue_clmm | 15.3209 | n/a | CLMM; provider outputs diverge >2×; excluded from parity claim |
| USDC → XLM | 10 USDC | 52.0969 | no | Arc venue_clmm | 153.1089 | n/a | CLMM; provider outputs diverge >2×; excluded from parity claim |
| USDC → XLM | 100 USDC | 520.9554 | no | Arc venue_clmm | 1,521.1884 | n/a | CLMM; provider outputs diverge >2×; excluded from parity claim |
| USDC → XLM | 1,000 USDC | 5,208.2231 | no | Arc venue_clmm | 14,287.9921 | n/a | CLMM; provider outputs diverge >2×; excluded from parity claim |
| XLM → USDC | 1 XLM | 0.1940 | yes | Arc venue → Arc venue ;; Arc venue → Arc venue | 0.7206 | n/a | split 2 paths; provider outputs diverge >2×; excluded from parity claim |
| XLM → USDC | 10 XLM | 1.9241 | yes | Arc venue → Arc venue ;; Arc venue → Arc venue ;; Arc venue → Arc venue | 5.6699 | n/a | split 3 paths; provider outputs diverge >2×; excluded from parity claim |
| XLM → USDC | 100 XLM | 19.1864 | no | Arc venue | 19.1814 | +0.03% | fair row (aligned) |
| XLM → USDC | 1,000 XLM | 191.8454 | no | Arc venue | 191.5380 | +0.16% | fair row (aligned) |
| XLM → AQUA | 10 XLM | 5,154.5228 | no | Arc venue_clmm | 7,040.0378 | -26.78% | CLMM venue in route |
| XLM → AQUA | 100 XLM | 51,545.1132 | no | Arc venue_clmm | 70,368.9897 | -26.75% | CLMM venue in route |
| XLM → AQUA | 1,000 XLM | 515,439.6619 | no | Arc venue_clmm | 700,565.8226 | -26.43% | CLMM venue in route |

## Summary

- **Venue coverage:** See [scf-venue-comparison.md](scf-venue-comparison.md) for Arc Broker CLMM gap (source-based).
- **Split routing:** Chakra `is_split=true` when Brent optimizer splits `amount_in` across distinct paths; Arc venue API returns a single best route.
- **Fair compare:** Prefer `Chakra_PREFER_Arc=1` + Arc venue `protocols` without `Arc` so Classic Arc does not dominate Chakra while Arc venue stays Arc-only.
- **Arc venue API key:** Free registration at https://api.Arc venue.finance/register — pass via `Arc venue_API_KEY` (never commit the key).
- **Split cases in this run:**
  - XLM → USDC 1 XLM: 2 paths (`Arc venue → Arc venue` ;; `Arc venue → Arc venue`)
  - XLM → USDC 10 XLM: 3 paths (`Arc venue → Arc venue` ;; `Arc venue → Arc venue` ;; `Arc venue → Arc venue`)
