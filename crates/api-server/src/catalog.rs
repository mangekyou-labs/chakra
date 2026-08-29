//! Chakra frozen catalog plumbing for the API (SC-12, SC-14).
//!
//! Native USDC (18 dp gas) is never a swap token; `/balances` reports it as a
//! separate `native_usdc` field and never sums the two encodings.
//! 2026-08-29: catalog is exactly USDC / EURC / canonical cirBTC (no env
//! address — cirBTC is a fixed canonical Arc token).

use market_snapshot::decimals;

/// Catalog swap tokens (USDC, EURC, cirBTC).
pub fn catalog_swap_tokens() -> Vec<decimals::CatalogToken> {
    decimals::v1_catalog()
}

/// True when `token` is a catalog ERC-20 swap token (never native encodings).
pub fn is_catalog_swap_token(token: &str) -> bool {
    decimals::is_catalog_swap_token(token)
}
