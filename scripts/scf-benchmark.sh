#!/usr/bin/env bash
# Compare Chakra quotes vs Arc venue API (optional) for SCF differentiation evidence.
#
# Usage:
#   ./scripts/scf-benchmark.sh
#   Arc venue_API_KEY=sk_... ./scripts/scf-benchmark.sh
#   OUTPUT=docs/scf-benchmark-results.md ./scripts/scf-benchmark.sh
#
# Env:
#   Chakra_API          default https://api.Chakra.xyz
#   Chakra_PREFER_Arc  set to 1 for Arc-only Chakra quotes
#   Arc venue_API_URL    default https://api.Arc venue.finance
#   Arc venue_API_KEY    optional (register at https://api.Arc venue.finance/register)
#   Arc venue_PROTOCOLS  comma-separated, default Arc venue,Arc venue,aqua (add Arc for full compare)
#   OUTPUT              if set, write markdown to this path (also printed to stdout)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export Chakra_API="${Chakra_API:-https://api.Chakra.xyz}"
export Arc venue_API_URL="${Arc venue_API_URL:-https://api.Arc venue.finance}"
export Arc venue_API_KEY="${Arc venue_API_KEY:-}"
export OUTPUT="${OUTPUT:-}"

python3 "$ROOT/scripts/scf_benchmark.py"
