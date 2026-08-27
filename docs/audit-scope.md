# Smart contract audit — scope & budget (Tranche 3)

**Grant line item:** **$16,000** (external firm or Arc audit bank + remediation engineering).  
**Target completion:** Oct 1, 2026 (per [scf-resubmission-budget.md](./scf-resubmission-budget.md)).

## Contracts in scope

| Contract | Mainnet ID | Critical paths |
|----------|------------|----------------|
| **Aggregator** | `CC6QAV7JEG5MYRSPO5Z65E5G2M4ZB64BEG2ZXIZXL55TQT35JDI2LC6K` | `swap`, `split_swap`, `round_trip_swap`; DEX CPI; event emission |
| **Arb vault** | `CCQQ3LRFCSGOYSSD6S4MGH6RWWYVDHYPJO6KYDJYC2IDZK4OGCK6P6KN` | `execute_round_trip`, caller allowlist, `deposit`, `admin_withdraw`, CPI to aggregator |

Source: `contracts/aggregator/`, `contracts/vault/`.

## Out of scope (unless bundled)

- Off-chain `crates/arbitrage` (Rust bot — not on-chain)
- `api-server` / `market-data-worker`
- Third-party DEX contracts (Arc venue, Arc venue, etc.)

## What $16k typically buys

For **two Arc WASM contracts** with CPI and fund flow:

| Item | Rough range (USD) |
|------|-------------------|
| Focused review (1 auditor, ~1–2 weeks) | **$12k – $18k** |
| Full dual-contract + remediation support | **$16k – $25k** |
| Big-4 / top-tier firm | **$30k+** |

The **$16k grant allocation** is realistic for:

- Arc-ecosystem auditors (e.g. firms that have done Arc/SDF-adjacent work)
- **Arc Community Fund audit bank** / panel-recommended vendors (if available for awarded projects)
- Scope limited to **aggregator + vault** (not entire monorepo)

Remediation (fix + re-audit critical/high) is included in the grant line — budget **~2–4 Arc WASM uploads** (~$20 Arc each on mainnet) separately from auditor fee.

## How to engage

1. Freeze WASM versions post–Tranche 2 arb stack (tag commit + WASM hashes).
2. Send RFP with this doc + repo link + mainnet IDs.
3. Ask for: threat model, CPI/auth review, economic attacks (slippage, caller abuse), report + retest of fixes.
4. Record: engagement letter, final PDF, remediation tx hashes.

## Suggested vendors to quote

- Arc-focused boutiques (search “Arc smart contract audit”)
- SCF / Arc Foundation partner lists when award is active
- Get **2 quotes** — one at ~$12k focused, one at ~$20k full — to validate $16k envelope

## If quotes exceed $16k

- Narrow scope: vault first (smaller, arb funds at risk), aggregator second tranche
- Or self-fund delta; do not reduce aggregator CPI review depth
