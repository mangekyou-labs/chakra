//! DEX adapter trait and shared math for Chakra Arc (EVM).
pub mod cache;
pub mod clmm_math;
pub mod common_balance_tokens;
pub mod pool_index;
pub mod token_logo;
pub mod token_logo_lists;
pub mod token_metadata;
pub mod traits;

// EVM modules for Arc venues
pub mod evm_fetch;
pub mod evm_logs;
pub mod evm_quote_math;
pub mod evm_rpc;

// Re-export only what downstream crates need.
pub use cache::{default_cache_path, PoolCache};
pub use common_balance_tokens::{is_common_balance_token, COMMON_BALANCE_TOKEN_IDS};
pub use token_metadata::{LogoKind, TokenMetadata, TokenMetadataStore};
pub use traits::*;
