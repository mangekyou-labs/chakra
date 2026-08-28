//! DEX adapter trait and shared math for Chakra Arc (EVM).
//!
//! Stellar adapter modules (aquarius, soroswap, phoenix, sushi, comet,
//! classic_dex, rpc, batch_refresh, on_chain_quote, pool_index, utils,
//! dex_event_kinds, router_events, token_registry, etc.) have been
//! removed from this crate. They live in the parent `avax-dex-agg` repo.

pub mod pool_index;
pub mod clmm_math;
pub mod stable_math;
pub mod traits;
pub mod cache;
pub mod common_balance_tokens;
pub mod token_logo;
pub mod token_logo_lists;
pub mod token_metadata;

// EVM modules for Arc venues
pub mod evm_fetch;
pub mod evm_logs;
pub mod evm_quote_math;
pub mod evm_rpc;

// Re-export only what downstream crates need.
pub use traits::*;
pub use cache::{default_cache_path, PoolCache};
pub use common_balance_tokens::{is_common_balance_token, COMMON_BALANCE_TOKEN_IDS};
pub use token_metadata::{LogoKind, TokenMetadata, TokenMetadataStore};
