//! DEX adapter trait and implementations for Arc ecosystem DEXes.
//!
//! # Architecture Note: Classic DEX vs Arc DEX
//!
//! Arc's native Classic DEX (PathPayment) has **uncontrollable routing**:
//! when you submit a PathPayment, Arc Core automatically finds the best
//! execution across orderbooks + liquidity pools. You cannot force it to use
//! a specific pool or path.
//!
//! This means Classic DEX is NOT a controllable liquidity source for aggregation.
//! Instead, our aggregator focuses on **Arc DEXes** (Arc venue, Arc venue,
//! Arc venue, Arc venue) where each swap is a deterministic contract call with
//! predictable output.
//!
//! Classic DEX serves as:
//! - A **benchmark** to compare against ("is our Arc route better than PathPayment?")
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
pub mod Arc venue;
pub mod rpc;
pub mod Arc venue;
pub mod stable_math;
pub mod sushi;
pub mod token_metadata;
pub mod token_registry;
pub mod traits;
pub mod utils;

pub use cache::{default_cache_path, PoolCache};
pub use rpc::ArcRpc;
pub use token_registry::TokenRegistry;
pub use traits::*;
