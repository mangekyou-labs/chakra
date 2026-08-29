//! Catalog tokens and decimal helpers (SC-12).
//!
//! Native USDC (18 dp) is gas only — never a PathFinder node or swap amount.

use std::collections::HashSet;

/// Arc testnet ERC-20 USDC (6 decimals). Same economic balance as native gas.
pub const USDC_ERC20: &str = "0x3600000000000000000000000000000000000000";
/// Arc testnet EURC (6 decimals).
pub const EURC: &str = "0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a";
/// Arc testnet canonical cirBTC (8 decimals, Presto/App Kit canonical address).
pub const CIRBTC: &str = "0xf0C4a4CE82A5746AbAAd9425360Ab04fbBA432BF";
/// Native gas encoding — not a catalog swap token.
pub const NATIVE_USDC: &str = "native_usdc";

pub const USDC_DECIMALS: u8 = 6;
pub const EURC_DECIMALS: u8 = 6;
pub const CIRBTC_DECIMALS: u8 = 8;
pub const NATIVE_USDC_DECIMALS: u8 = 18;

/// 1 ERC-20 USDC atomic = 1e12 wei (6 vs 18 dp).
pub const WEI_PER_ERC20_ATOMIC: u128 = 1_000_000_000_000;
/// Floor on the USDC MAX gas buffer (0.10 USDC at 6 dp).
pub const USDC_MAX_BUFFER_FLOOR: u128 = 100_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogToken {
    pub symbol: &'static str,
    pub address: String,
    pub decimals: u8,
}

/// v1 PathFinder catalog: ERC-20 USDC, EURC, cirBTC (canonical curated,
/// 2026-08-29). Native USDC is excluded.
pub fn v1_catalog() -> Vec<CatalogToken> {
    vec![
        CatalogToken {
            symbol: "USDC",
            address: USDC_ERC20.to_string(),
            decimals: USDC_DECIMALS,
        },
        CatalogToken {
            symbol: "EURC",
            address: EURC.to_string(),
            decimals: EURC_DECIMALS,
        },
        CatalogToken {
            symbol: "cirBTC",
            address: CIRBTC.to_string(),
            decimals: CIRBTC_DECIMALS,
        },
    ]
}

pub fn is_catalog_swap_token(address: &str) -> bool {
    let a = address.to_ascii_lowercase();
    a == USDC_ERC20.to_ascii_lowercase()
        || a == EURC.to_ascii_lowercase()
        || a == CIRBTC.to_ascii_lowercase()
}

pub fn is_native_usdc_encoding(token: &str) -> bool {
    token.eq_ignore_ascii_case(NATIVE_USDC)
        || token.eq_ignore_ascii_case("eth")
        || token == "0x0000000000000000000000000000000000000000"
}

/// Native USDC must never be a graph node.
pub fn graph_nodes() -> HashSet<String> {
    v1_catalog()
        .into_iter()
        .map(|t| t.address.to_ascii_lowercase())
        .collect()
}

/// Parse atomic amount as a decimal integer string (no floats, no scientific notation).
pub fn parse_atomic(s: &str) -> Result<u128, &'static str> {
    if s.is_empty() || s.contains('.') || s.contains('e') || s.contains('E') || s.contains('+') {
        return Err("atomic amounts must be unsigned decimal integers");
    }
    s.parse::<u128>().map_err(|_| "invalid atomic amount")
}

pub fn format_atomic(amount: u128) -> String {
    amount.to_string()
}

/// USDC MAX chip: reserve gas so a swap cannot drain native USDC needed for the tx.
///
/// `raw = ceil(gas_cost_wei / 1e12)`; `buffer = max(ceil(raw * 1.25), 100_000)`.
pub fn usdc_max_atomic(erc20_balance_6dp: u128, gas_cost_wei: u128) -> u128 {
    let raw = gas_cost_wei.div_ceil(WEI_PER_ERC20_ATOMIC);
    let with_margin = raw.saturating_mul(125).div_ceil(100);
    let buffer = with_margin.max(USDC_MAX_BUFFER_FLOOR);
    erc20_balance_6dp.saturating_sub(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_three_canonical_tokens() {
        let cat = v1_catalog();
        assert_eq!(cat.len(), 3);
        assert_eq!(cat[0].decimals, 6);
        assert_eq!(cat[1].decimals, 6);
        assert_eq!(cat[2].decimals, 8);
        assert_eq!(cat[2].symbol, "cirBTC");
        assert!(is_catalog_swap_token(USDC_ERC20));
        assert!(is_catalog_swap_token(EURC));
        assert!(is_catalog_swap_token(CIRBTC));
        // mBTC is no longer in the catalog (SC-14).
        assert!(!is_catalog_swap_token("0x1111111111111111111111111111111111111111"));
    }

    #[test]
    fn native_usdc_is_not_a_graph_node() {
        let nodes = graph_nodes();
        assert!(!nodes.contains(&NATIVE_USDC.to_string()));
        assert!(!nodes.contains(&"0x0000000000000000000000000000000000000000".to_string()));
        assert!(is_native_usdc_encoding(NATIVE_USDC));
        assert!(!is_catalog_swap_token(NATIVE_USDC));
        assert!(!is_catalog_swap_token("0x0000000000000000000000000000000000000000"));
    }

    #[test]
    fn parse_atomic_rejects_float_and_scientific() {
        assert_eq!(parse_atomic("1000000").unwrap(), 1_000_000);
        assert!(parse_atomic("1.0").is_err());
        assert!(parse_atomic("1e6").is_err());
        assert!(parse_atomic("1E6").is_err());
        assert_eq!(format_atomic(42), "42");
    }

    #[test]
    fn usdc_max_uses_1e12_and_floor() {
        // 0 wei gas still leaves 0.10 USDC floor.
        assert_eq!(usdc_max_atomic(1_000_000, 0), 900_000);
        // 2e12 wei → raw=2, *1.25=3, floor still 100_000.
        assert_eq!(usdc_max_atomic(500_000, 2_000_000_000_000), 400_000);
        // exact ceil: 1 wei over a 1e12 boundary.
        assert_eq!(usdc_max_atomic(1_000_000, 1_000_000_000_001), 1_000_000 - 100_000);
        // 1.25x dominates floor: raw=100_000, *1.25=125_000.
        let gas = 100_000 * WEI_PER_ERC20_ATOMIC;
        assert_eq!(usdc_max_atomic(1_000_000, gas), 1_000_000 - 125_000);
        assert_eq!(usdc_max_atomic(50_000, 0), 0);
    }
}
