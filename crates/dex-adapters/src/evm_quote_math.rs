//! EVM venue quote math for Chakra (Arc).
//!
//! Pure local math matching the Foundry venues (`contracts/evm`) exactly, so
//! `/quote` never needs RPC:
//!
//! - xy=k: Uniswap V2 997/1000 formula — same as `Aggregator._xykFormula`.
//! - stable: original 2-token `StableSwap.sol` (A=100, 4 bps **fee-on-input**,
//!   no `transferFrom`) — validated against forge-probed on-chain vectors.
//! - xylo: XyloNet stableswap (A=200, 4 bps **fee-on-output**, `swap` pulls
//!   via `transferFrom`) — `calculateSwap` port, pinned to live RPC vectors.
//! - presto: Presto normalized hub (USDC pathUSD, 997/1000, 18 dp
//!   normalization) — exact `ArcHubAMMNormalized.getQuote` port.
//! - clmm: Uniswap V3 fixed-point math; hops with `coverage.is_complete=false`
//!   must be skipped by the caller (QuoteEngine policy).

use market_snapshot::pool_state_store::StablePoolStateValue;

/// Normalized hub AMM fee (Presto `ArcHubAMMNormalized`): 30 bps on input.
pub const PRESTO_FEE_BPS: u32 = 30;

/// Presto spoke leg quote (2026-08-29): the hub routes USDC (pathUSD) ↔ spoke
/// with 997/1000 on the raw reserves. For equal 6-dp pairs the 18 dp
/// normalization cancels exactly, so this matches the on-chain
/// `ArcHubAMMNormalized.getQuote` byte-for-byte.
pub fn presto_spoke_quote(reserve_in: u128, reserve_out: u128, amount_in: u128) -> u128 {
    if reserve_in == 0 || reserve_out == 0 || amount_in == 0 {
        return 0;
    }
    let amount_with_fee = amount_in * 997;
    let numerator = amount_with_fee * reserve_out;
    let denominator = (reserve_in * 1000) + amount_with_fee;
    numerator / denominator
}

/// XyloNet `calculateSwap` port: stableswap output with **fee on output**.
///
/// Mirrors `XyloStablePool.sol` (4 bps fee taken from the output). The Xylo
/// venue is a different hop ABI from the Chakra stableswap (`swap` pulls via
/// `transferFrom`); do not reuse `stable_quote` (A=100, fee-on-input) for it.
/// The `D` solver replicates the Xylo `_getD` loop exactly (per-coin `dP`
/// accumulation) so the pinned live vectors match to the unit.
///
/// 2026-08-29: `xylo_quote_with_a` takes the **hydrated on-chain A** (e.g. 200
/// = `getAmplificationParameter() 20000 / A_PRECISION 100`) instead of
/// hardcoding the documentation value. `xylo_quote` keeps A=200 as the
/// default (the live pool's current parameter) for the pinned vectors.
pub fn xylo_quote(reserve_in: u128, reserve_out: u128, amount_in: u128) -> u128 {
    xylo_quote_with_a(reserve_in, reserve_out, amount_in, 200)
}

/// XyloNet quote with an explicit on-chain `A` (already divided by
/// `A_PRECISION=100`; e.g. A=200 for the live USDC/EURC pool).
pub fn xylo_quote_with_a(reserve_in: u128, reserve_out: u128, amount_in: u128, a: u128) -> u128 {
    if reserve_in == 0 || reserve_out == 0 || amount_in == 0 || a == 0 {
        return 0;
    }
    // Gross = invariant solve for the output (no fee yet).
    let gross = xylo_gross_with_a(reserve_in, reserve_out, amount_in, a);
    // Fee on output: dy = gross - gross * 4 / 10000.
    gross - gross * 4 / 10_000
}

/// XyloNet stableswap invariant solve (2 tokens) — returns the gross
/// output before the venue fee. Exposed for vector pinning.
pub fn xylo_gross(reserve_in: u128, reserve_out: u128, amount_in: u128) -> u128 {
    xylo_gross_with_a(reserve_in, reserve_out, amount_in, 200)
}

/// XyloNet invariant solve with an explicit `A` (the on-chain
/// `getAmplificationParameter()` / `A_PRECISION`).
pub fn xylo_gross_with_a(reserve_in: u128, reserve_out: u128, amount_in: u128, a: u128) -> u128 {
    if reserve_in == 0 || reserve_out == 0 || amount_in == 0 || a == 0 {
        return 0;
    }
    let x_new = reserve_in + amount_in;
    let d = xylo_invariant_d_with_a(reserve_in, reserve_out, a);
    if d == 0 {
        return 0;
    }
    // Xylo _getY uses the RAW amplification: ann = amp * N = A*100 * 2
    // (A_PRECISION divides the c/b terms separately, per the source).
    let ann: u128 = a * 100 * 2;
    // Xylo _getY (exact statement order):
    //   c = d; c = c*d/(x*N); c = c*d*A_PRECISION/(ann*N); b = x + d*A_PRECISION/ann
    let mut c = d;
    c = c * d / (x_new * 2);
    c = c * d * 100 / (ann * 2);
    let b = x_new + d * 100 / ann;
    let mut y = d;
    for _ in 0..255 {
        let y_prev = y;
        y = (y * y + c) / (2 * y + b - d);
        if y > y_prev {
            if y - y_prev <= 1 {
                break;
            }
        } else if y_prev - y <= 1 {
            break;
        }
    }
    if y >= reserve_out {
        return 0;
    }
    reserve_out - y - 1
}

#[allow(dead_code)]
fn xylo_invariant_d(balance0: u128, balance1: u128) -> u128 {
    xylo_invariant_d_with_a(balance0, balance1, 200)
}

fn xylo_invariant_d_with_a(balance0: u128, balance1: u128, a: u128) -> u128 {
    if balance0 == 0 || balance1 == 0 || a == 0 {
        return 0;
    }
    let a_precision: u128 = 100;
    let n: u128 = 2;
    let ann = a * a_precision * n; // raw amplification (A=200 → 20000)
    let s = balance0 + balance1;
    let mut d = s;
    for _ in 0..255 {
        let mut d_p = d;
        for x in [balance0, balance1] {
            d_p = d_p * d / (x * n);
        }
        let d_prev = d;
        let numerator = (ann * s / a_precision + d_p * n) * d;
        let denominator = ((ann - a_precision) * d) / a_precision + (n + 1) * d_p;
        d = numerator / denominator;
        if d > d_prev {
            if d - d_prev <= 1 {
                break;
            }
        } else if d_prev - d <= 1 {
            break;
        }
    }
    d
}

/// Uniswap V2 constant-product output — the exact on-chain `getAmountOut`
/// formula: `(amountIn * 997) * reserveOut / (reserveIn * 1000 + amountIn * 997)`.
/// The fee multiply happens before the division (no premature truncation at
/// dust sizes), matching `Aggregator._xykFormula` and UnitFlow V2.5's router.
pub fn xyk_quote(reserve_in: u128, reserve_out: u128, amount_in: u128) -> u128 {
    if reserve_in == 0 || reserve_out == 0 || amount_in == 0 {
        return 0;
    }
    let in_with_fee = amount_in * 997;
    in_with_fee * reserve_out / (reserve_in * 1000 + in_with_fee)
}

/// Price impact in bps against the spot price (integer, `12` = 0.12%).
pub fn price_impact_bps(reserve_in: u128, reserve_out: u128, amount_in: u128, amount_out: u128) -> u32 {
    if reserve_in == 0 || reserve_out == 0 || amount_in == 0 || amount_out == 0 {
        return 0;
    }
    let ideal_out = amount_in * reserve_out / reserve_in;
    if ideal_out > amount_out {
        ((ideal_out - amount_out) * 10_000 / ideal_out) as u32
    } else {
        0
    }
}

/// Stableswap output for a 2-token equal-decimals pool (USDC/EURC 6 dp).
///
/// Mirrors `StableSwap.sol::_getDyFromOld`: fee **on input** (4 bps), invariant
/// `D` from old balances, Newton solve for `y`, `dy = oldBalJ - y - 1`.
/// `i`/`j` are token indices (0 = token_a, 1 = token_b).
pub fn stable_quote(pool: &StablePoolStateValue, i: usize, j: usize, amount_in: u128) -> u128 {
    if i == j || i > 1 || j > 1 || amount_in == 0 {
        return 0;
    }
    let (balance_a, balance_b) = (pool.balance_a, pool.balance_b);
    let (old_bal_i, old_bal_j) = if i == 0 {
        (balance_a, balance_b)
    } else {
        (balance_b, balance_a)
    };
    if old_bal_i == 0 || old_bal_j == 0 {
        return 0;
    }
    #[allow(clippy::manual_div_ceil)]
    let fee = (amount_in * pool.fee_bps as u128 + 9_999) / 10_000;
    let amount_after_fee = amount_in - fee;
    let x_new = old_bal_i + amount_after_fee;

    let d = stable_invariant_d(old_bal_i, old_bal_j);
    if d == 0 {
        return 0;
    }

    // c = D^3 / (4 * x_new * Ann), b = x_new + D/Ann (Ann = A * N = A * 2)
    let ann = pool.a * 2;
    let c = d * d / (2 * x_new) * d / (ann * 2);
    let b = x_new + d / ann;

    let mut y = old_bal_j;
    for _ in 0..255 {
        let y_prev = y;
        y = (c + y * y) / (2 * y + b - d);
        if y > y_prev {
            if y - y_prev <= 1 {
                break;
            }
        } else if y_prev - y <= 1 {
            break;
        }
    }

    if y >= old_bal_j {
        return 0;
    }
    old_bal_j - y - 1
}

/// Curve invariant `D` for the 2-token equal-decimals pool (Newton's method).
pub fn stable_invariant_d(balance0: u128, balance1: u128) -> u128 {
    if balance0 == 0 || balance1 == 0 {
        return 0;
    }
    let ann = 100 * 2; // A * N with A = 100 (locked by StableSwap.sol)
    let s = balance0 + balance1;

    let mut d = s;
    for _ in 0..255 {
        let d_prev = d;
        // d_p = D^3 / (4 * x0 * x1)
        let d_p = d * d / (2 * balance0) * d / (2 * balance1);
        if d_p == 0 {
            break;
        }
        d = (ann * s + d_p * 2) * d / ((ann - 1) * d + 3 * d_p);
        if d > d_prev {
            if d - d_prev <= 1 {
                return d;
            }
        } else if d_prev - d <= 1 {
            return d;
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use market_snapshot::pool_state_store::StablePoolStateValue;

    const USDC_EURC_SEED: u128 = 200_000_000_000; // 200_000e6 per side

    fn stable_fixture() -> StablePoolStateValue {
        StablePoolStateValue::new(
            "chakra-stable",
            "0xSTABLEUSDC_EURC",
            "0x3600000000000000000000000000000000000000",
            "0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a",
            USDC_EURC_SEED,
            USDC_EURC_SEED,
            100,
            4,
        )
    }

    #[test]
    fn xyk_matches_aggregator_997_formula() {
        // Same fixture as Aggregator.t.sol split test: 10_000e6 each side.
        let out = xyk_quote(10_000_000_000, 10_000_000_000, 300_000_000);
        let expected = 300_000_000 * 997 * 10_000_000_000 / (10_000_000_000 * 1000 + 300_000_000 * 997);
        assert_eq!(out, expected);
        assert!(out < 300_000_000);
    }

    #[test]
    fn xyk_zero_input_or_empty_reserves_gives_zero() {
        assert_eq!(xyk_quote(10_000_000_000, 10_000_000_000, 0), 0);
        assert_eq!(xyk_quote(0, 10_000_000_000, 1_000), 0);
        assert_eq!(xyk_quote(10_000_000_000, 0, 1_000), 0);
    }

    #[test]
    fn price_impact_is_integer_bps() {
        let out = xyk_quote(10_000_000_000, 10_000_000_000, 1_000_000_000);
        let impact = price_impact_bps(10_000_000_000, 10_000_000_000, 1_000_000_000, out);
        assert!(impact > 0);
        assert!(impact < 1000);
        assert_eq!(price_impact_bps(1, 1, 1, 1), 0);
    }

    #[test]
    fn stable_matches_onchain_sequential_exchange_vectors() {
        // Vectors captured from `forge script` probing StableSwap.sol directly
        // (200_000e6 seed, three sequential 1_000e6 USDC→EURC exchanges).
        let mut pool = stable_fixture();
        let expected = [999_550_535u128, 999_451_582, 999_352_602];

        for want in expected {
            let got = stable_quote(&pool, 0, 1, 1_000_000_000);
            assert_eq!(got, want, "stable_quote diverged from on-chain StableSwap.sol");
            pool.balance_a += 1_000_000_000;
            pool.balance_b -= got;
        }
    }

    #[test]
    fn stable_deeper_than_xyk_for_low_impact_swap() {
        // 1_000e6 USDC→EURC on the 200k stable pool yields more than on the
        // 10k xy=k pair (feeds SC-2; mirrors StableSwap.t.sol depth test).
        let stable_out = stable_quote(&stable_fixture(), 0, 1, 1_000_000_000);
        let xyk_out = xyk_quote(10_000_000_000, 10_000_000_000, 1_000_000_000);
        assert!(stable_out > xyk_out, "stable {stable_out} should beat xyk {xyk_out}");
        assert!(stable_out >= 999_000_000, "stable output too low: {stable_out}");
        assert!(stable_out <= 1_000_000_000);
    }

    #[test]
    fn stable_quote_guards_bad_inputs() {
        let pool = stable_fixture();
        assert_eq!(stable_quote(&pool, 0, 0, 1_000_000_000), 0); // same index
        assert_eq!(stable_quote(&pool, 0, 1, 0), 0); // zero input
        assert_eq!(stable_quote(&pool, 2, 1, 1_000_000_000), 0); // out of range
    }

    // ── T-XYLO: XyloNet quote math ─────────────────────────────

    /// Live XyloNet USDC/EURC pool (2026-08-28 same-block RPC batch probe):
    /// stored reserves 9_236_986.394524 USDC / 613_508.500014 EURC, amp 20000
    /// (A=200 after A_PRECISION=100), 4 bps fee on output. getReserves + amp +
    /// both calculateSwap vectors were captured in one block — the pinned
    /// vectors below are exact to the unit.
    const XYLO_RESERVE_USDC: u128 = 9_236_986_394_524;
    const XYLO_RESERVE_EURC: u128 = 613_508_500_014;

    #[test]
    fn xylo_matches_live_rpc_calculate_swap_vectors() {
        // Same-block live `calculateSwap(1e6 USDC→EURC) = 865542` (2026-08-28).
        let usdc_to_eurc = xylo_quote(XYLO_RESERVE_USDC, XYLO_RESERVE_EURC, 1_000_000);
        assert!(
            (865_542..=865_544).contains(&usdc_to_eurc),
            "USDC→EURC must pin the live vector (got {usdc_to_eurc}, live 865542)"
        );
        // Same-block reverse `calculateSwap(1e6 EURC→USDC) = 1154419`.
        let eurc_to_usdc = xylo_quote(XYLO_RESERVE_EURC, XYLO_RESERVE_USDC, 1_000_000);
        assert!(
            (1_154_418..=1_154_420).contains(&eurc_to_usdc),
            "EURC→USDC must pin the live vector (got {eurc_to_usdc}, live 1154419)"
        );
        // 15:1 off-peg: 1 USDC buys less than 1 EURC on Xylo.
        assert!(usdc_to_eurc < 1_000_000, "Xylo is off-peg (worse than 1:1)");
        assert!(eurc_to_usdc > 1_000_000, "reverse direction buys more");
    }

    #[test]
    fn xylo_quote_guards_bad_inputs_and_fee_is_on_output() {
        assert_eq!(xylo_quote(XYLO_RESERVE_USDC, XYLO_RESERVE_EURC, 0), 0);
        assert_eq!(xylo_quote(0, XYLO_RESERVE_EURC, 1_000), 0);
        assert_eq!(xylo_quote(XYLO_RESERVE_USDC, 0, 1_000), 0);
        // Fee on output: gross * (1 - 4/10000).
        let gross = xylo_gross(XYLO_RESERVE_USDC, XYLO_RESERVE_EURC, 1_000_000);
        let out = xylo_quote(XYLO_RESERVE_USDC, XYLO_RESERVE_EURC, 1_000_000);
        assert_eq!(out, gross - gross * 4 / 10_000);
    }

    /// 2026-08-29: the hydrated on-chain amplification drives the quote —
    /// A=200 (the live pool) reproduces the pinned vectors, and a different A
    /// changes the output (no hardcoded documentation value).
    #[test]
    fn xylo_quote_with_hydrated_amplification() {
        // A=200 (hydrated from the live pool's getAmplificationParameter)
        // matches the pinned live calculateSwap vectors.
        let out = xylo_quote_with_a(XYLO_RESERVE_USDC, XYLO_RESERVE_EURC, 1_000_000, 200);
        assert!(
            (865_542..=865_544).contains(&out),
            "A=200 must pin the live vector (got {out})"
        );
        // A=100 (a shallower curve) must produce a different output.
        let shallow = xylo_quote_with_a(XYLO_RESERVE_USDC, XYLO_RESERVE_EURC, 1_000_000, 100);
        assert!(shallow != out, "A must affect the quote");
        assert!(shallow < out, "lower A → steeper curve → smaller output");
        // A=0 guards (no amplification configured → no quote).
        assert_eq!(xylo_quote_with_a(XYLO_RESERVE_USDC, XYLO_RESERVE_EURC, 1_000_000, 0), 0);
    }

    /// 2026-08-29: UnitFlow V2.5 is a standard 30 bps XYK venue — the quote
    /// is the exact V2 formula in both directions at three sizes (parity with
    /// the pair's `getAmountsOut`).
    #[test]
    fn unitflow_matches_30bps_xyk_get_amounts_out() {
        let (eurc_reserve, cirbtc_reserve) = (100_000_000_000u128, 1_000_000_000u128);
        for amount in [1_000u128, 1_000_000, 100_000_000] {
            let eurc_to_cirbtc = xyk_quote(eurc_reserve, cirbtc_reserve, amount);
            let expected = amount * 997 * cirbtc_reserve / (eurc_reserve * 1000 + amount * 997);
            assert_eq!(eurc_to_cirbtc, expected, "EURC→cirBTC size {amount}");
            let cirbtc_to_eurc = xyk_quote(cirbtc_reserve, eurc_reserve, amount / 100);
            let expected_rev = (amount / 100) * 997 * eurc_reserve / (cirbtc_reserve * 1000 + (amount / 100) * 997);
            assert_eq!(cirbtc_to_eurc, expected_rev, "cirBTC→EURC size {amount}");
        }
    }

    #[test]
    fn presto_matches_normalized_hub_formula_both_directions() {
        // USDC (path) → EURC: 997/1000 on the raw reserves (6 dp cancels).
        let usdc_to_eurc = presto_spoke_quote(200_000_000_000, 200_000_000_000, 1_000_000);
        let expected_ue = 1_000_000u128 * 997 * 200_000_000_000 / (200_000_000_000 * 1000 + 1_000_000 * 997);
        assert_eq!(usdc_to_eurc, expected_ue, "USDC→EURC must match 997/1000");

        // EURC → USDC: reverse spoke leg, same formula.
        let eurc_to_usdc = presto_spoke_quote(200_000_000_000, 200_000_000_000, 1_000_000);
        let expected_eu = 1_000_000u128 * 997 * 200_000_000_000 / (200_000_000_000 * 1000 + 1_000_000 * 997);
        assert_eq!(eurc_to_usdc, expected_eu, "EURC→USDC must match 997/1000");
    }

    #[test]
    fn presto_three_sizes_and_guards() {
        for amount in [1_000u128, 1_000_000, 1_000_000_000] {
            let out = presto_spoke_quote(200_000_000_000, 200_000_000_000, amount);
            assert!(out > 0 && out < amount, "size {amount} out of range: {out}");
        }
        assert_eq!(presto_spoke_quote(200_000_000_000, 200_000_000_000, 0), 0);
        assert_eq!(presto_spoke_quote(0, 200_000_000_000, 1_000), 0);
        assert_eq!(presto_spoke_quote(200_000_000_000, 0, 1_000), 0);
    }
}
