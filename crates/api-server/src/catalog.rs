//! Chakra frozen catalog plumbing for the API (SC-12).
//!
//! Native USDC (18 dp gas) is never a swap token; `/balances` reports it as a
//! separate `native_usdc` field and never sums the two encodings.

use market_snapshot::decimals;

/// mBTC address from env (`CHAKRA_MBTC_ADDRESS`), lowercased.
pub fn mbtc_address() -> String {
    std::env::var("CHAKRA_MBTC_ADDRESS")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

/// Catalog swap tokens (USDC, EURC, mBTC).
pub fn catalog_swap_tokens() -> Vec<decimals::CatalogToken> {
    catalog_swap_tokens_with(&mbtc_address())
}

/// Catalog swap tokens with an explicit mBTC address (empty mBTC excluded).
pub fn catalog_swap_tokens_with(mbtc: &str) -> Vec<decimals::CatalogToken> {
    decimals::v1_catalog(mbtc)
        .into_iter()
        .filter(|t| !(t.symbol == "mBTC" && mbtc.is_empty()))
        .collect()
}

/// True when `token` is a catalog ERC-20 swap token (never native encodings),
/// using the given mBTC address.
pub fn is_catalog_swap_token_with(token: &str, mbtc: &str) -> bool {
    decimals::is_catalog_swap_token(token, mbtc)
}

/// True when `token` is a catalog ERC-20 swap token (env mBTC address).
pub fn is_catalog_swap_token(token: &str) -> bool {
    is_catalog_swap_token_with(token, &mbtc_address())
}
