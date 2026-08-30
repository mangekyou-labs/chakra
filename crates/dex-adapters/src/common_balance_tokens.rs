//! Curated catalog tokens queried in `/api/v1/balances`.
//!
//! Includes the frozen Arc testnet catalog tokens (USDC, EURC, cirBTC).
pub const COMMON_BALANCE_TOKEN_IDS: &[&str] = &[
    "0x3600000000000000000000000000000000000000", // USDC
    "0x89B50855Aa8D51744b8062fa4173AC0d1c4CD72a", // EURC
    "0x6De6cAC86b864aA2B7b2d56125A0F5952Ac0e774", // cirBTC
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
        assert!(is_common_balance_token("0x89b50855aa8d51744b8062fa4173ac0d1c4cd72a"));
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
