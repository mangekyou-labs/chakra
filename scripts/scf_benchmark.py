#!/usr/bin/env python3
"""SCF quote benchmark: Chakra vs optional Arc venue API."""

from __future__ import annotations

import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any

Chakra_API = os.environ.get("Chakra_API", "https://api.Chakra.xyz").rstrip("/")
Chakra_PREFER_Arc = os.environ.get("Chakra_PREFER_Arc", "").strip() in ("1", "true", "yes")
Arc venue_API_URL = os.environ.get("Arc venue_API_URL", "https://api.Arc venue.finance").rstrip("/")
Arc venue_API_KEY = os.environ.get("Arc venue_API_KEY", "").strip()
Arc venue_PROTOCOLS = os.environ.get(
    "Arc venue_PROTOCOLS", "Arc venue,Arc venue,aqua"
).strip()
OUTPUT = os.environ.get("OUTPUT", "").strip()

XLM = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"
USDC = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"
AQUA = "CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK"

# (label, token_in, token_out, amount_in atomic unitss, size_label)
CASES: list[tuple[str, str, str, int, str]] = [
    ("USDC → XLM", USDC, XLM, 10_000_000, "1 USDC"),
    ("USDC → XLM", USDC, XLM, 100_000_000, "10 USDC"),
    ("USDC → XLM", USDC, XLM, 1_000_000_000, "100 USDC"),
    ("USDC → XLM", USDC, XLM, 10_000_000_000, "1,000 USDC"),
    ("XLM → USDC", XLM, USDC, 10_000_000, "1 XLM"),
    ("XLM → USDC", XLM, USDC, 100_000_000, "10 XLM"),
    ("XLM → USDC", XLM, USDC, 1_000_000_000, "100 XLM"),
    ("XLM → USDC", XLM, USDC, 10_000_000_000, "1,000 XLM"),
    ("XLM → AQUA", XLM, AQUA, 100_000_000, "10 XLM"),
    ("XLM → AQUA", XLM, AQUA, 1_000_000_000, "100 XLM"),
    ("XLM → AQUA", XLM, AQUA, 10_000_000_000, "1,000 XLM"),
]


@dataclass
class ChakraQuote:
    amount_out: int
    is_split: bool
    legs: int
    sources: list[str]
    compute_ms: int | None
    error: str | None = None


@dataclass
class Arc venueQuote:
    amount_out: int | None
    error: str | None = None


def http_get_json(url: str, timeout: float = 60.0) -> Any:
    req = urllib.request.Request(
        url,
        headers={
            "Accept": "application/json",
            "User-Agent": "Chakra-scf-benchmark/1.0 (SCF evidence script)",
        },
    )
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode())


def http_post_json(url: str, body: dict[str, Any], headers: dict[str, str], timeout: float = 60.0) -> Any:
    data = json.dumps(body).encode()
    req = urllib.request.Request(url, data=data, headers=headers, method="POST")
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode())


def Chakra_quote(token_in: str, token_out: str, amount_in: int) -> ChakraQuote:
    query = {
        "token_in": token_in,
        "token_out": token_out,
        "amount_in": str(amount_in),
        "slippage": "0.5",
        "debug": "1",
    }
    if Chakra_PREFER_Arc:
        query["prefer_Arc"] = "1"
    params = urllib.parse.urlencode(query)
    url = f"{Chakra_API}/api/v1/quote?{params}"
    try:
        raw = http_get_json(url)
    except urllib.error.URLError as e:
        return ChakraQuote(0, False, 0, [], None, error=str(e))
    if not raw.get("success"):
        return ChakraQuote(0, False, 0, [], None, error=str(raw.get("error", raw)))
    data = raw["data"]
    routes = data.get("sub_routes") or []
    sources = [r.get("source", "?") for r in routes]
    return ChakraQuote(
        amount_out=int(data["expected_output"]),
        is_split=bool(data.get("is_split")),
        legs=len(routes),
        sources=sources,
        compute_ms=data.get("compute_time_ms"),
    )


def Arc venue_quote(token_in: str, token_out: str, amount_in: int) -> Arc venueQuote:
    if not Arc venue_API_KEY:
        return Arc venueQuote(None, error="Arc venue_API_KEY not set")
    url = f"{Arc venue_API_URL}/quote?network=mainnet"
    protocols = [p.strip() for p in Arc venue_PROTOCOLS.split(",") if p.strip()]
    body = {
        "assetIn": token_in,
        "assetOut": token_out,
        "amount": str(amount_in),
        "tradeType": "EXACT_IN",
        "protocols": protocols,
    }
    headers = {
        "Authorization": f"Bearer {Arc venue_API_KEY}",
        "Content-Type": "application/json",
        "Accept": "application/json",
        "User-Agent": "Chakra-scf-benchmark/1.0 (SCF evidence script)",
    }
    try:
        raw = http_post_json(url, body, headers)
    except urllib.error.HTTPError as e:
        try:
            detail = json.loads(e.read().decode())
        except Exception:
            detail = e.reason
        return Arc venueQuote(None, error=f"HTTP {e.code}: {detail}")
    except urllib.error.URLError as e:
        return Arc venueQuote(None, error=str(e))

    amount_out = raw.get("amountOut") or raw.get("amount_out") or raw.get("expectedOutput")
    if amount_out is None:
        return Arc venueQuote(None, error=f"unexpected response keys: {list(raw.keys())}")
    return Arc venueQuote(int(amount_out))


def outputs_comparable(Chakra_out: int, Arc venue_out: int | None) -> bool:
    """Flag rows where APIs likely use different route classes (Classic vs Arc) or bad quotes."""
    if Arc venue_out is None or Arc venue_out <= 0 or Chakra_out <= 0:
        return True
    ratio = Chakra_out / Arc venue_out if Chakra_out >= Arc venue_out else Arc venue_out / Chakra_out
    return ratio <= 3.0


def fmt_atomic unitss(v: int) -> str:
    return f"{v / 10_000_000:,.4f}"


def pct_delta(Chakra: int, other: int | None) -> str:
    if other is None or other <= 0:
        return "—"
    delta = (Chakra - other) / other * 100.0
    sign = "+" if delta >= 0 else ""
    return f"{sign}{delta:.2f}%"


def unique_sources(sources: list[str]) -> str:
    seen: list[str] = []
    for s in sources:
        if s not in seen:
            seen.append(s)
    return ", ".join(seen) if seen else "—"


def notes_for_case(pair: str, Chakra: ChakraQuote) -> str:
    if Chakra.error:
        return Chakra.error
    parts: list[str] = []
    if "classic_dex" in Chakra.sources:
        parts.append("Chakra picked Classic DEX")
    if any("clmm" in s or s == "sushi" for s in Chakra.sources):
        parts.append("CLMM venue in route")
    if Chakra.is_split:
        parts.append(f"split {Chakra.legs} legs")
    return "; ".join(parts) if parts else "—"


def run_benchmark() -> str:
    ts = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    has_Arc venue = bool(Arc venue_API_KEY)

    lines: list[str] = [
        "# Chakra quote benchmark results",
        "",
        f"Generated: **{ts}**",
        "",
        f"- Chakra API: `{Chakra_API}`"
        + (" (`prefer_Arc=1`)" if Chakra_PREFER_Arc else " (all venues)"),
        f"- Arc venue API: `{Arc venue_API_URL}` protocols=`{Arc venue_PROTOCOLS}` "
        + ("(key provided)" if has_Arc venue else "(**no API key** — Arc venue column empty)"),
        "",
        "Reproduce:",
        "",
        "```bash",
        "./scripts/scf-benchmark.sh",
        "# Arc-only fair compare:",
        "Chakra_PREFER_Arc=1 Arc venue_PROTOCOLS=Arc venue,Arc venue,aqua Arc venue_API_KEY=sk_... ./scripts/scf-benchmark.sh",
        "# Full compare (include Arc on both sides when Chakra omits prefer_Arc):",
        "Arc venue_PROTOCOLS=Arc venue,Arc venue,aqua,Arc Arc venue_API_KEY=sk_... ./scripts/scf-benchmark.sh",
        "OUTPUT=docs/scf-benchmark-results.md ./scripts/scf-benchmark.sh",
        "```",
        "",
        "> **Interpretation:** Use `Chakra_PREFER_Arc=1` + Arc venue without `Arc` for Arc-only rows. "
        "Include `Arc` in `Arc venue_PROTOCOLS` when comparing full aggregation. "
        "Positive Δ = Chakra higher output for same `amount_in`.",
        "",
    ]

    if has_Arc venue:
        header = (
            "| Pair | Size | Chakra out | Split | Sources | Arc venue out | Δ vs Arc venue | Notes |"
        )
        sep = "|------|------|------------|-------|---------|--------------|---------------|-------|"
    else:
        header = "| Pair | Size | Chakra out | Split | Sources | Arc venue out | Notes |"
        sep = "|------|------|------------|-------|---------|--------------|-------|"

    lines.extend([header, sep])

    for pair, token_in, token_out, amount_in, size_label in CASES:
        Chakra = Chakra_quote(token_in, token_out, amount_in)
        time.sleep(0.12)  # api-server IP limiter: 10 req/s
        Arc venue = Arc venue_quote(token_in, token_out, amount_in)
        note = notes_for_case(pair, Chakra)

        if Chakra.error:
            Chakra_out = "ERR"
            split = "—"
            sources = "—"
        else:
            Chakra_out = fmt_atomic unitss(Chakra.amount_out)
            split = "yes" if Chakra.is_split else "no"
            sources = unique_sources(Chakra.sources)

        if Arc venue.error or Arc venue.amount_out is None:
            ss_out = "—" if not has_Arc venue else f"ERR"
            if has_Arc venue and Arc venue.error:
                note = f"{note}; Arc venue: {Arc venue.error}" if note != "—" else f"Arc venue: {Arc venue.error}"
        else:
            ss_out = fmt_atomic unitss(Arc venue.amount_out)

        if has_Arc venue:
            if Arc venue.amount_out is not None and not Chakra.error:
                if not outputs_comparable(Chakra.amount_out, Arc venue.amount_out):
                    note = (
                        f"{note}; ⚠️ outputs not comparable (>3× gap — check venue / token mismatch)"
                        if note != "—"
                        else "⚠️ outputs not comparable (>3× gap — check venue / token mismatch)"
                    )
                    delta = "n/a"
                else:
                    delta = pct_delta(Chakra.amount_out, Arc venue.amount_out)
            else:
                delta = pct_delta(Chakra.amount_out if not Chakra.error else 0, Arc venue.amount_out)
            lines.append(
                f"| {pair} | {size_label} | {Chakra_out} | {split} | {sources} | {ss_out} | {delta} | {note} |"
            )
        else:
            lines.append(
                f"| {pair} | {size_label} | {Chakra_out} | {split} | {sources} | {ss_out} | {note} |"
            )

    lines.extend(
        [
            "",
            "## Summary",
            "",
            "- **Venue coverage:** See [scf-venue-comparison.md](scf-venue-comparison.md) for Arc Broker CLMM gap (source-based).",
            "- **Split routing:** Chakra `is_split=true` when Brent optimizer splits across paths; Arc venue API returns a single best route.",
            "- **Arc venue API key:** Free registration at https://api.Arc venue.finance/register — re-run this script before SCF resubmission to fill the comparison column.",
            "",
        ]
    )

    return "\n".join(lines) + "\n"


def main() -> None:
    md = run_benchmark()
    print(md, end="")
    if OUTPUT:
        out_path = OUTPUT
        os.makedirs(os.path.dirname(out_path) or ".", exist_ok=True)
        with open(out_path, "w", encoding="utf-8") as f:
            f.write(md)
        print(f"\nWrote {out_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
