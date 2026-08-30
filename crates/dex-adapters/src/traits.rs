//! Core trait that DEX adapters implement.

use {
    anyhow::Result,
    async_trait::async_trait,
    serde::{Deserialize, Serialize},
};

/// Token identifier (ERC-20 contract address on Arc EVM).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TokenId {
    Contract { address: String },
}

impl TokenId {
    pub fn from_str_auto(s: &str) -> Self {
        Self::Contract { address: s.to_string() }
    }

    pub fn canonical(&self) -> String {
        match self {
            Self::Contract { address } => address.clone(),
        }
    }
}

impl std::fmt::Display for TokenId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.canonical())
    }
}

/// Protocol type classification for EVM venues.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolType {
    Xyk,
    Stable,
    Clmm,
    Xylo,
    Presto,
}

/// A trading pair available on a DEX.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterTradingPair {
    pub token_a: TokenId,
    pub token_b: TokenId,
    pub pool_address: String,
    pub fee_bps: u32,
    pub reserve_a: Option<u128>,
    pub reserve_b: Option<u128>,
}

/// Quote result from a single DEX hop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterQuote {
    pub amount_out: u128,
    pub fee_bps: u32,
    pub price_impact_bps: u32,
}

/// Swap operation parameters for transaction building.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapOperation {
    pub target: String,
    pub calldata: Vec<u8>,
    pub value: u128,
}

/// The core trait all DEX adapters implement.
#[async_trait]
pub trait DexAdapter: Send + Sync {
    /// Unique identifier for this adapter (e.g., "chakra-xyk", "xylo-stable").
    fn id(&self) -> &str;

    /// Human-readable name.
    fn name(&self) -> &str;

    /// Protocol type classification.
    fn protocol_type(&self) -> ProtocolType;

    /// Fetch all available trading pairs from this DEX.
    async fn get_trading_pairs(&self) -> Result<Vec<AdapterTradingPair>>;

    /// Get a quote for swapping `amount_in` of `token_in` to `token_out`
    /// through the specified pool.
    async fn get_quote(
        &self,
        token_in: &TokenId,
        token_out: &TokenId,
        amount_in: u128,
        pool_address: &str,
    ) -> Result<Option<AdapterQuote>>;

    /// Health check: verify the adapter can reach its data source.
    async fn health_check(&self) -> bool;
}
