//! EVM log decoder for Arc venue events.
//!
//! Topics are computed with keccak256 from the exact event signatures declared
//! by the venues:
//!
//! - Uniswap V2 (`PairCreated`, `Swap`, `Sync`, `Mint`, `Burn`)
//! - Uniswap V3 (`PoolCreated`, `Swap`, `Mint`, `Burn`)
//! - Chakra stableswap (`PoolCreated`, `Swapped`, `LiquidityAdded`,
//!   `LiquidityRemoved`)
//!
//! ERC-20 `Transfer` is **never** a pool touch (native USDC sends do not emit
//! ERC-20 `Transfer`, and token transfers are not pool state changes). The
//! Arc never-call table (CCTP V2 / Gateway / USYC / FxEscrow / Memo /
//! Multicall3From) is never subscribed and never routed.

use {
    crate::{
        evm_rpc::{decode_hex, EvmLog},
        pool_index::PoolRef,
    },
    anyhow::{Context, Result},
    std::collections::HashSet,
    tiny_keccak::{Hasher, Keccak},
};

// ─── ABI helpers ────────────────────────────────────────────────────────────

pub fn keccak256(bytes: &[u8]) -> [u8; 32] {
    let mut keccak = Keccak::v256();
    keccak.update(bytes);
    let mut out = [0u8; 32];
    keccak.finalize(&mut out);
    out
}

/// topic0 of an event: keccak256 of its signature, `0x`-prefixed.
pub fn event_topic0_hex(signature: &str) -> String {
    format!("0x{}", hex::encode(keccak256(signature.as_bytes())))
}

/// First 4 bytes of keccak256(function signature), `0x`-prefixed.
pub fn function_selector_hex(signature: &str) -> String {
    let hash = keccak256(signature.as_bytes());
    format!("0x{}", hex::encode(&hash[..4]))
}

// ─── Venue event signatures ─────────────────────────────────────────────────

pub const XYK_PAIR_CREATED_SIG: &str = "PairCreated(address,address,address,uint256)";
pub const XYK_SWAP_SIG: &str = "Swap(address,uint256,uint256,uint256,uint256,address)";
pub const XYK_SYNC_SIG: &str = "Sync(uint112,uint112)";
pub const XYK_MINT_SIG: &str = "Mint(address,uint256,uint256)";
pub const XYK_BURN_SIG: &str = "Burn(address,uint256,uint256,address)";

pub const V3_POOL_CREATED_SIG: &str = "PoolCreated(address,address,uint24,int24,address)";
pub const V3_SWAP_SIG: &str = "Swap(address,address,int256,int256,uint160,uint128,int24)";
pub const V3_MINT_SIG: &str = "Mint(address,address,int24,int24,uint128,uint256,uint256)";
pub const V3_BURN_SIG: &str = "Burn(address,int24,int24,uint128,uint256,uint256)";

pub const STABLE_POOL_CREATED_SIG: &str = "PoolCreated(address,address,address)";
pub const STABLE_SWAPPED_SIG: &str = "Swapped(address,address,uint256,uint256)";
pub const STABLE_LIQUIDITY_ADDED_SIG: &str = "LiquidityAdded(address,uint256,uint256,uint256)";
pub const STABLE_LIQUIDITY_REMOVED_SIG: &str = "LiquidityRemoved(address,uint256,uint256,uint256)";

pub const ERC20_TRANSFER_SIG: &str = "Transfer(address,address,uint256)";

/// All pool-touch event signatures (Swap / Sync / Mint / Burn across venues).
pub const TOUCH_EVENT_SIGS: &[&str] = &[
    XYK_SWAP_SIG,
    XYK_SYNC_SIG,
    XYK_MINT_SIG,
    XYK_BURN_SIG,
    V3_SWAP_SIG,
    V3_MINT_SIG,
    V3_BURN_SIG,
    STABLE_SWAPPED_SIG,
    STABLE_LIQUIDITY_ADDED_SIG,
    STABLE_LIQUIDITY_REMOVED_SIG,
];

/// Pool-creation event signatures (factory events).
pub const CREATED_EVENT_SIGS: &[&str] = &[XYK_PAIR_CREATED_SIG, V3_POOL_CREATED_SIG, STABLE_POOL_CREATED_SIG];

/// Every signature the watcher subscribes to.
pub fn watched_event_signatures() -> Vec<&'static str> {
    let mut sigs: Vec<&'static str> = TOUCH_EVENT_SIGS.to_vec();
    sigs.extend(CREATED_EVENT_SIGS);
    sigs
}

pub fn is_pool_touch_topic(topic0: &str) -> bool {
    let topic0 = normalize_hex_word(topic0);
    TOUCH_EVENT_SIGS.iter().any(|sig| event_topic0_hex(sig) == topic0)
}

pub fn is_created_event_topic(topic0: &str) -> bool {
    let topic0 = normalize_hex_word(topic0);
    CREATED_EVENT_SIGS.iter().any(|sig| event_topic0_hex(sig) == topic0)
}

// ─── Address helpers ────────────────────────────────────────────────────────

/// Canonical lowercase `0x`-padded 20-byte address.
pub fn normalize_evm_address(address: &str) -> String {
    let hex = address.strip_prefix("0x").unwrap_or(address).to_ascii_lowercase();
    let padded = format!("{hex:0>40}");
    format!("0x{}", &padded[padded.len() - 40..])
}

pub fn is_evm_address(address: &str) -> bool {
    let hex = address.strip_prefix("0x").unwrap_or(address);
    hex.len() == 40 && hex.chars().all(|c| c.is_ascii_hexdigit())
}

/// Decode the trailing 20 bytes of a 32-byte word into an address.
pub fn word_to_address(hex: &str) -> Result<String> {
    let bytes = decode_hex(hex).context("decode address word")?;
    let start = bytes.len().saturating_sub(20);
    Ok(format!("0x{}", hex::encode(&bytes[start..])))
}

fn normalize_hex_word(topic0: &str) -> String {
    let hex = topic0.strip_prefix("0x").unwrap_or(topic0).to_ascii_lowercase();
    format!("0x{hex:0>64}")
}

// ─── Never-call table (Arc contract-addresses.md) ───────────────────────────

/// Addresses that must never be subscribed, fetched, or allowlisted. Mirrors
/// `Aggregator.t.sol::neverCall`.
pub const NEVER_CALL_ADDRESSES: &[&str] = &[
    "0x8FE6B999Dc680CcFDD5Bf7EB0974218be2542DAA", // CCTP TokenMessengerV2
    "0xE737e5cEBEEBa77EFE34D4aa090756590b1CE275", // CCTP MessageTransmitterV2
    "0xb43db544E2c27092c107639Ad201b3dEfAbcF192", // CCTP TokenMinterV2
    "0xbaC0179bB358A8936169a63408C8481D582390C4", // CCTP MessageV2
    "0x0077777d7EBA4688BDeF3E311b846F25870A19B9", // GatewayWallet
    "0x0022222ABE238Cc2C7Bb1f21003F0a260052475B", // GatewayMinter
    "0x867650F5eAe8df91445971f14d89fd84F0C9a9f8", // StableFX FxEscrow
    "0xe9185F0c5F296Ed1797AaE4238D26CCaBEadb86C", // USYC
    "0xCC205224862C7641930c87679E98999d23C26113", // USYC Entitlements
    "0x9fdF14c5B14173D74C08Af27AebFf39240dC105A", // USYC Teller
    "0x9702466268ccF55eAB64cdf484d272Ac08d3b75b", // Memo
    "0xEb7cc06E3D3b5F9F9a5fA2B31B477ff72bB9c8b6", // Multicall3From
];

pub fn is_never_call_address(address: &str) -> bool {
    let normalized = normalize_evm_address(address);
    NEVER_CALL_ADDRESSES
        .iter()
        .any(|addr| normalize_evm_address(addr) == normalized)
}

// ─── Decoding ───────────────────────────────────────────────────────────────

/// A pool whose creation log was seen on a factory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodedCreated {
    Xyk {
        token0: String,
        token1: String,
        pool: String,
    },
    Stable {
        token0: String,
        token1: String,
        pool: String,
    },
    Clmm {
        token0: String,
        token1: String,
        fee: u32,
        tick_spacing: i32,
        pool: String,
    },
}

impl DecodedCreated {
    /// Sort tokens into EIP-address order (same as the venues' `token0 <
    /// token1` canonical sort) and normalize both to lowercase `0x` form.
    pub fn sorted(self) -> Self {
        let (token0, token1) = match &self {
            Self::Xyk { token0, token1, .. }
            | Self::Stable { token0, token1, .. }
            | Self::Clmm { token0, token1, .. } => (token0.clone(), token1.clone()),
        };
        let (token0, token1) = if normalize_evm_address(&token0) <= normalize_evm_address(&token1) {
            (token0, token1)
        } else {
            (token1, token0)
        };
        let (token0, token1) = (normalize_evm_address(&token0), normalize_evm_address(&token1));
        match self {
            Self::Xyk { pool, .. } => Self::Xyk { token0, token1, pool },
            Self::Stable { pool, .. } => Self::Stable { token0, token1, pool },
            Self::Clmm {
                fee,
                tick_spacing,
                pool,
                ..
            } => Self::Clmm {
                token0,
                token1,
                fee,
                tick_spacing,
                pool,
            },
        }
    }

    pub fn pool_address(&self) -> String {
        match self {
            Self::Xyk { pool, .. } | Self::Stable { pool, .. } | Self::Clmm { pool, .. } => pool.clone(),
        }
    }

    pub fn tokens(&self) -> (String, String) {
        match self {
            Self::Xyk { token0, token1, .. }
            | Self::Stable { token0, token1, .. }
            | Self::Clmm { token0, token1, .. } => (token0.clone(), token1.clone()),
        }
    }
}

/// Decode a factory creation log (requires `is_created_event_topic(topics[0])`).
pub fn decode_created_pool(log: &EvmLog) -> Option<DecodedCreated> {
    let topic0 = log.topics.first()?;
    let topic0 = normalize_hex_word(topic0);
    let data = decode_hex(&log.data).ok()?;
    let mut words = data.chunks(32);

    if topic0 == event_topic0_hex(XYK_PAIR_CREATED_SIG) {
        let token0 = word_to_address(log.topics.get(1)?).ok()?;
        let token1 = word_to_address(log.topics.get(2)?).ok()?;
        let pool_word = words.next()?;
        let pool = word_to_address(&format!("0x{}", hex::encode(pool_word))).ok()?;
        return Some(DecodedCreated::Xyk {
            token0: normalize_evm_address(&token0),
            token1: normalize_evm_address(&token1),
            pool: normalize_evm_address(&pool),
        });
    }
    if topic0 == event_topic0_hex(V3_POOL_CREATED_SIG) {
        let token0 = word_to_address(log.topics.get(1)?).ok()?;
        let token1 = word_to_address(log.topics.get(2)?).ok()?;
        let fee_word = words.next()?;
        let spacing_word = words.next()?;
        let pool_word = words.next()?;
        let fee = u32::from_be_bytes(fee_word[28..].try_into().ok()?);
        let spacing_raw = i32::from_be_bytes(spacing_word[28..].try_into().ok()?);
        let pool = word_to_address(&format!("0x{}", hex::encode(pool_word))).ok()?;
        return Some(DecodedCreated::Clmm {
            token0: normalize_evm_address(&token0),
            token1: normalize_evm_address(&token1),
            fee,
            tick_spacing: spacing_raw,
            pool: normalize_evm_address(&pool),
        });
    }
    if topic0 == event_topic0_hex(STABLE_POOL_CREATED_SIG) {
        let token0 = word_to_address(log.topics.get(1)?).ok()?;
        let token1 = word_to_address(log.topics.get(2)?).ok()?;
        let pool_word = words.next()?;
        let pool = word_to_address(&format!("0x{}", hex::encode(pool_word))).ok()?;
        return Some(DecodedCreated::Stable {
            token0: normalize_evm_address(&token0),
            token1: normalize_evm_address(&token1),
            pool: normalize_evm_address(&pool),
        });
    }
    None
}

/// Touched known pools from a batch of logs. ERC-20 `Transfer` (and any
/// non-venue event) is ignored; pool addresses not in the index are ignored.
pub fn touched_pools_from_evm_logs(logs: &[EvmLog], index: &crate::pool_index::KnownPoolIndex) -> HashSet<PoolRef> {
    let mut touched = HashSet::new();
    for log in logs {
        let Some(topic0) = log.topics.first() else {
            continue;
        };
        if !is_pool_touch_topic(topic0) {
            continue;
        }
        if is_never_call_address(&log.address) {
            continue;
        }
        if let Some(pool) = index.lookup_contract(&normalize_evm_address(&log.address)) {
            touched.insert(pool.clone());
        }
    }
    touched
}

/// Pool-creation events seen in a log batch (factory addresses only).
pub fn created_pools_from_evm_logs(logs: &[EvmLog]) -> Vec<DecodedCreated> {
    logs.iter()
        .filter(|log| !is_never_call_address(&log.address))
        .filter_map(|log| {
            let topic0 = log.topics.first()?;
            is_created_event_topic(topic0)
                .then(|| decode_created_pool(log))
                .flatten()
        })
        .map(DecodedCreated::sorted)
        .collect()
}

/// Addresses to subscribe (filters `never_call` and keeps only valid `0x`).
pub fn filter_subscribe_addresses(addresses: &[String]) -> Vec<String> {
    addresses
        .iter()
        .filter(|address| !is_never_call_address(address) && is_evm_address(address))
        .map(|address| normalize_evm_address(address))
        .collect()
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::pool_index::KnownPoolIndex,
        market_snapshot::{SourceSnapshot, TradingPairSnapshot},
    };

    // Well-known UniswapV2/V3 topic0 hashes (memory pins; failure means the
    // signature string above drifted from the real contract).
    const V2_SWAP_TOPIC0: &str = "0xd78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822";
    const V2_PAIR_CREATED_TOPIC0: &str = "0x0d3648bd0f6ba80134a33ba9275ac585d9d315f0ad8355cddefde31afa28d0e9";
    const V2_SYNC_TOPIC0: &str = "0x1c411e9a96e071241c2f21f7726b17ae89e3cab4c78be50e062b03a9fffbbad1";
    const V3_POOL_CREATED_TOPIC0: &str = "0x783cca1c0412dd0d695e784568c96da2e9c22ff989357a2e8b1d9b2b4e6b7118";
    const V3_SWAP_TOPIC0: &str = "0xc42079f94a6350d7e6235f29174924f928cc2ac818eb64fed8004e115fbcca67";
    const TRANSFER_TOPIC0: &str = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

    const USDC: &str = "0x3600000000000000000000000000000000000000";
    const EURC: &str = "0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a";
    const POOL: &str = "0x0000000000000000000000000000000000000001";

    fn evm_data(values: &[u128]) -> String {
        let mut data = "0x".to_string();
        for value in values {
            data.push_str(&format!("{value:0>64x}"));
        }
        data
    }

    fn log(address: &str, topic0: &str, topics_rest: &[&str], data: &str) -> EvmLog {
        let mut topics = vec![topic0.to_string()];
        topics.extend(topics_rest.iter().map(|t| t.to_string()));
        EvmLog {
            address: address.to_string(),
            topics,
            data: data.to_string(),
            block_number: Some(1),
            tx_hash: None,
            log_index: None,
        }
    }

    fn sample_index() -> KnownPoolIndex {
        KnownPoolIndex::rebuild(
            &[SourceSnapshot {
                source: "chakra-xyk".to_string(),
                pairs: vec![TradingPairSnapshot {
                    token_a: USDC.to_string(),
                    token_b: EURC.to_string(),
                    pool_address: POOL.to_string(),
                    fee_bps: 30,
                    dex_type: "xyk".to_string(),
                    factory: "0xfactory".to_string(),
                }],
            }],
            &[],
        )
    }

    #[test]
    fn topic0_hashes_match_well_known_uniswap_values() {
        assert_eq!(event_topic0_hex(XYK_SWAP_SIG), V2_SWAP_TOPIC0);
        assert_eq!(event_topic0_hex(XYK_PAIR_CREATED_SIG), V2_PAIR_CREATED_TOPIC0);
        assert_eq!(event_topic0_hex(XYK_SYNC_SIG), V2_SYNC_TOPIC0);
        assert_eq!(event_topic0_hex(V3_POOL_CREATED_SIG), V3_POOL_CREATED_TOPIC0);
        assert_eq!(event_topic0_hex(V3_SWAP_SIG), V3_SWAP_TOPIC0);
        assert_eq!(event_topic0_hex(ERC20_TRANSFER_SIG), TRANSFER_TOPIC0);
    }

    #[test]
    fn v2_swap_log_touches_known_pool() {
        let index = sample_index();
        let swap = log(
            POOL,
            event_topic0_hex(XYK_SWAP_SIG).as_str(),
            &[
                "0x00000000000000000000000000000000000000000000000000000000000000aa",
                "0xbb",
            ],
            &evm_data(&[1, 2, 3, 4]),
        );
        let touched = touched_pools_from_evm_logs(&[swap], &index);
        assert_eq!(touched.len(), 1);
        assert_eq!(touched.iter().next().unwrap().source, "chakra-xyk");
        assert_eq!(touched.iter().next().unwrap().pool_address, POOL);
    }

    #[test]
    fn transfer_log_never_touches_a_pool() {
        let index = sample_index();
        // ERC-20 Transfer emitted by a *token* contract that happens to be a
        // known pool-shaped address is still not a pool touch.
        let transfer = log(POOL, TRANSFER_TOPIC0, &["0xaa", "0xbb"], &evm_data(&[100, 1]));
        assert!(touched_pools_from_evm_logs(&[transfer], &index).is_empty());
    }

    #[test]
    fn sync_mint_and_burn_also_touch() {
        let index = sample_index();
        for sig in [XYK_SYNC_SIG, XYK_MINT_SIG, XYK_BURN_SIG] {
            let l = log(POOL, event_topic0_hex(sig).as_str(), &["0xaa"], &evm_data(&[1, 2]));
            assert!(
                !touched_pools_from_evm_logs(&[l], &index).is_empty(),
                "{sig} should touch"
            );
        }
    }

    #[test]
    fn v2_pair_created_decodes_pool_from_data() {
        let token0 = USDC;
        let token1 = EURC;
        let pair = "0x00000000000000000000000000000000000000ab";
        let pair_value = u128::from_str_radix(&pair[2..], 16).unwrap();
        let created = log(
            "0x00000000000000000000000000000000000000ca", // factory
            event_topic0_hex(XYK_PAIR_CREATED_SIG).as_str(),
            &[&format!("0x{:0>64}", &token0[2..]), &format!("0x{:0>64}", &token1[2..])],
            &evm_data(&[pair_value, 3]),
        );
        let decoded = decode_created_pool(&created).unwrap();
        match decoded {
            DecodedCreated::Xyk { token0, token1, pool } => {
                assert_eq!(token0, USDC.to_ascii_lowercase());
                assert_eq!(token1, EURC.to_ascii_lowercase());
                assert_eq!(pool, pair);
            }
            other => panic!("expected xyk: {other:?}"),
        }
    }

    #[test]
    fn v3_pool_created_decodes_fee_spacing_and_pool() {
        let token0 = EURC;
        let token1 = "0x1111111111111111111111111111111111111111";
        let pool = "0x00000000000000000000000000000000000000cd";
        let pool_value = u128::from_str_radix(&pool[2..], 16).unwrap();
        let spacing = (-24i32) as u32 as u128;
        let created = log(
            "0x00000000000000000000000000000000000000ce",
            event_topic0_hex(V3_POOL_CREATED_SIG).as_str(),
            &[&format!("0x{:0>64}", &token0[2..]), &format!("0x{:0>64}", &token1[2..])],
            &evm_data(&[3000, spacing, pool_value]),
        );
        match decode_created_pool(&created).unwrap() {
            DecodedCreated::Clmm {
                fee,
                tick_spacing,
                pool,
                ..
            } => {
                assert_eq!(fee, 3000);
                assert_eq!(tick_spacing, -24);
                assert_eq!(pool, pool);
            }
            other => panic!("expected clmm: {other:?}"),
        }
    }

    #[test]
    fn stable_pool_created_decodes_tokens_from_topics() {
        let token0 = USDC;
        let token1 = EURC;
        let pool = "0x00000000000000000000000000000000000000dd";
        let pool_value = u128::from_str_radix(&pool[2..], 16).unwrap();
        let created = log(
            "0x00000000000000000000000000000000000000df",
            event_topic0_hex(STABLE_POOL_CREATED_SIG).as_str(),
            &[&format!("0x{:0>64}", &token0[2..]), &format!("0x{:0>64}", &token1[2..])],
            &evm_data(&[pool_value]),
        );
        match decode_created_pool(&created).unwrap() {
            DecodedCreated::Stable { token0, token1, pool } => {
                assert_eq!(token0, USDC.to_ascii_lowercase());
                assert_eq!(token1, EURC.to_ascii_lowercase());
                assert_eq!(pool, pool);
            }
            other => panic!("expected stable: {other:?}"),
        }
    }

    #[test]
    fn created_pools_are_sorted_canonically() {
        let decoded = DecodedCreated::Xyk {
            token0: EURC.to_string(),
            token1: USDC.to_string(),
            pool: "0x00000000000000000000000000000000000000ee".to_string(),
        }
        .sorted();
        let (token0, token1) = decoded.tokens();
        // 0x36... (USDC) < 0x89... (EURC) as lowercase hex.
        assert_eq!(token0, USDC.to_ascii_lowercase());
        assert_eq!(token1, EURC.to_ascii_lowercase());
    }

    #[test]
    fn never_call_addresses_rejected_everywhere() {
        assert_eq!(NEVER_CALL_ADDRESSES.len(), 12);
        for address in NEVER_CALL_ADDRESSES {
            assert!(is_never_call_address(address), "{address} should be never-call");
        }
        assert!(!is_never_call_address(USDC));
        let filtered = filter_subscribe_addresses(&[
            NEVER_CALL_ADDRESSES[0].to_string(),
            USDC.to_string(),
            "garbage".to_string(),
        ]);
        assert_eq!(filtered, vec![USDC.to_string().to_ascii_lowercase()]);
    }

    #[test]
    fn normalize_pads_and_lowercases() {
        assert_eq!(
            normalize_evm_address("0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a"),
            "0x89b50855aa3be2f677cd6303cec089b5f319d72a"
        );
        assert_eq!(
            normalize_evm_address("0x1"),
            "0x0000000000000000000000000000000000000001"
        );
        assert!(is_evm_address("0x89b50855aa3be2f677cd6303cec089b5f319d72a"));
        assert!(!is_evm_address(
            "CA4HEQTL2WPEUYKYKCDOHCDNIV4QHNJ7EL4J4NQ6VADP7SYHVRYZ7AW2"
        ));
    }

    #[test]
    fn function_selector_matches_known_v2_pair_getter() {
        assert_eq!(function_selector_hex("getReserves()"), "0x0902f1ac");
        assert_eq!(function_selector_hex("balanceOf(address)"), "0x70a08231");
        assert_eq!(function_selector_hex("token0()"), "0x0dfe1681");
        assert_eq!(function_selector_hex("getPair(address,address)"), "0xe6a43905");
    }
}
