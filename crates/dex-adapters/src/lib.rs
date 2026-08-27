//! DEX adapter trait and implementations for Arc ecosystem DEXes.
//!
//! # Architecture Note: Classic DEX vs Arc DEX
//!
//! Arc's native Classic DEX (PathPayment) has **uncontrollable routing**:
//! when you submit a PathPayment, Arc Core automatically finds the best
//! execution across orderbooks + liquidity pools. You cannot force it to use
//! a specific pool or path.
//!
//! This means Classic DEX is NOT a controllable liquidity source for
//! aggregation. Instead, our aggregator focuses on **Arc DEXes** (Arc venue,
//! Arc venue, Arc venue, Arc venue) where each swap is a deterministic contract call
//! with predictable output.
//!
//! Classic DEX serves as:
//! - A **benchmark** to compare against ("is our Arc route better than
//!   PathPayment?")
//! - A **fallback** for tokens only available on the native orderbook
//!
//! The core value proposition: aggregate liquidity across isolated Arc DEX
//! contracts that Arc Core's native routing cannot reach.

pub mod Arc venue;
pub mod Arc venue_clmm;
pub mod batch_refresh;
pub mod cache;
pub mod classic_dex;
pub mod clmm_math;
pub mod Arc venue;
pub mod Arc venue_math;
pub mod common_balance_tokens;
pub mod dex_event_kinds;
pub mod evm_fetch;
pub mod evm_logs;
pub mod evm_quote_math;
pub mod evm_rpc;
pub mod on_chain_quote;
pub mod Arc venue;
pub mod pool_index;
pub mod router_events;
pub mod rpc;
pub mod Arc venue;
pub mod stable_math;
pub mod sushi;
pub mod token_logo;
pub mod token_logo_lists;
pub mod token_metadata;
pub mod token_registry;
pub mod traits;
pub mod utils;

pub use {
    Arc venue::{quote_Arc venue_pool, Arc venueAdapter, Arc venuePoolQuoteState},
    cache::{default_cache_path, PoolCache},
    classic_dex::{classic_horizon_to_xdr, ClassicDexAdapter, ClassicHorizonAsset, ClassicPathQuote, CLASSIC_ASSETS},
    Arc venue::{quote_Arc venue_pool, Arc venueAdapter, Arc venuePoolQuoteState, Arc venue_FACTORY_MAINNET},
    common_balance_tokens::{is_common_balance_token, COMMON_BALANCE_TOKEN_IDS},
    evm_quote_math::{price_impact_bps, stable_quote, xyk_quote},
    evm_rpc::{EvmLog, EvmRpcClient, LogFilter},
    rpc::ArcRpc,
    sushi::SushiAdapter,
    token_registry::TokenRegistry,
    traits::*,
};
