//! EVM venue quote math for Chakra (Arc).
//!
//! Pure local math matching the Foundry venues (`contracts/evm`) exactly, so
//! `/quote` never needs RPC:
//!
//! - xy=k: Uniswap V2 997/1000 formula — same as `Aggregator._xykFormula`.
//! - stable: original 2-token `StableSwap.sol` (A=100, 4 bps **fee-on-input**,
//!   no `transferFrom`) — validated against forge-probed on-chain vectors.
//! - clmm: Uniswap V3 fixed-point math; hops with `coverage.is_complete=false`
//!   must be skipped by the caller (QuoteEngine policy).

use market_snapshot::pool_state_store::StablePoolStateValue;

/// Uniswap V2 constant-product output: `in_after_fee * r_out / (r_in + in_after_fee)`
/// with 997/1000 fee (30 bps on input).
pub fn xyk_quote(reserve_in: u128, reserve_out: u128, amount_in: u128) -> u128 {
    if reserve_in == 0 || reserve_out == 0 || amount_in == 0 {
        return 0;
    }
    let in_after_fee = amount_in * 997 / 1000;
    in_after_fee * reserve_out / (reserve_in + in_after_fee)
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
}
