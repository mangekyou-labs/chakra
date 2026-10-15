//! Curated catalog tokens queried in `/api/v1/balances`.
//!
//! Includes the frozen Arc mainnet catalog tokens (USDC, EURC, cirBTC).
pub const COMMON_BALANCE_TOKEN_IDS: &[&str] = &[
    "0x3600000000000000000000000000000000000000", // USDC
    "0xbEf5f6d51CB62b58e6A8f77868681825C6fe21c1", // EURC
    "0x171A4217b86A807A64eB94757Db6849fb4bDbAA0", // cirBTC
];

pub fn is_common_balance_token(contract_id: &str) -> bool {
    COMMON_BALANCE_TOKEN_IDS
        .iter()
        .any(|id| id.eq_ignore_ascii_case(contract_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_core_swap_tokens() {
        assert!(is_common_balance_token("0x3600000000000000000000000000000000000000"));
        assert!(is_common_balance_token("0xbef5f6d51cb62b58e6a8f77868681825c6fe21c1"));
    }

    #[test]
    fn list_is_small_and_unique() {
        let mut seen = std::collections::HashSet::new();
        for id in COMMON_BALANCE_TOKEN_IDS {
            assert!(id.starts_with("0x") && id.len() == 42);
            assert!(seen.insert(id.to_ascii_lowercase()), "duplicate token: {id}");
        }
        assert!(COMMON_BALANCE_TOKEN_IDS.len() <= 32);
    }
}
