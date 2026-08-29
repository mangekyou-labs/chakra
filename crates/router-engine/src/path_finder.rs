//! Path finder: discovers and caches trading paths across all DEX sources.

use {
    crate::{
        graph::TokenGraph,
        types::{Path, TokenId, TradingPair},
    },
    market_snapshot::{decimals::USDC_ERC20, MarketSnapshot},
    std::{collections::HashMap, sync::Mutex},
    tracing::info,
};

/// Configuration for path finding.
#[derive(Debug, Clone)]
pub struct PathFinderConfig {
    /// Maximum hops per path (default: 3)
    pub max_hops: usize,
    /// Maximum indirect paths (2+ hops). Direct pools between token_in/out are
    /// enumerated separately.
    pub max_multi_hop_paths: usize,
    /// Cap on direct (1-hop) pools between the pair (`0` = include all direct
    /// pools in the graph).
    pub max_direct_paths: usize,
    /// Bridge tokens used to improve multi-hop discovery (kept for
    /// compatibility; the graph BFS explores all edges, not only bridges).
    pub bridge_tokens: Vec<TokenId>,
}

impl Default for PathFinderConfig {
    fn default() -> Self {
        Self {
            max_hops: 3,
            max_multi_hop_paths: 50,
            max_direct_paths: 0,
            bridge_tokens: vec![TokenId::Contract {
                // Arc ERC-20 USDC — never XLM Native / Classic USDC.
                address: USDC_ERC20.to_string(),
            }],
        }
    }
}

/// Build router `TradingPair`s from a Chakra topology snapshot, honoring the
/// v1 catalog freeze (`decimals::graph_nodes()`): pools whose tokens are
/// outside {USDC, EURC, cirBTC} are unused, and native USDC encodings are
/// never nodes.
pub fn pairs_from_chakra_snapshot(snapshot: &MarketSnapshot) -> Vec<TradingPair> {
    let nodes = market_snapshot::decimals::graph_nodes();
    let mut pairs = Vec::new();
    for source in &snapshot.sources {
        for pair in &source.pairs {
            let a = pair.token_a.to_ascii_lowercase();
            let b = pair.token_b.to_ascii_lowercase();
            if !nodes.contains(&a) || !nodes.contains(&b) {
                continue;
            }
            // T4.7: the snapshot's `dex_type` is the hop identity; fall back
            // to the source-derived type for legacy snapshots.
            let dex_type = if pair.dex_type.is_empty() {
                dex_type_for_source(&source.source)
            } else {
                pair.dex_type.clone()
            };
            pairs.push(TradingPair {
                token_a: TokenId::Contract { address: a },
                token_b: TokenId::Contract { address: b },
                source: source.source.clone(),
                pool_address: pair.pool_address.clone(),
                fee_bps: pair.fee_bps,
                reserve_a: None,
                reserve_b: None,
                factory: pair.factory.clone(),
                dex_type,
            });
        }
    }
    for clmm in &snapshot.clmm_pool_refs {
        let a = clmm.token0.to_ascii_lowercase();
        let b = clmm.token1.to_ascii_lowercase();
        if !nodes.contains(&a) || !nodes.contains(&b) {
            continue;
        }
        pairs.push(TradingPair {
            token_a: TokenId::Contract { address: a },
            token_b: TokenId::Contract { address: b },
            source: clmm.source.clone(),
            pool_address: clmm.pool_address.clone(),
            fee_bps: clmm.fee_bps,
            reserve_a: None,
            reserve_b: None,
            factory: clmm.factory.clone(),
            dex_type: "clmm".to_string(),
        });
    }
    pairs
}

/// Map a Chakra source id to its DEX type (legacy snapshots without a
/// stamped `dex_type`). Unknown sources default to `xyk` — the pre-T3.1
/// default.
fn dex_type_for_source(source: &str) -> String {
    if source == "chakra-stable" || source == "xylo-stable" {
        "stable".to_string()
    } else if source == "chakra-clmm" {
        "clmm".to_string()
    } else if source == "presto-hub" {
        "presto".to_string()
    } else {
        "xyk".to_string()
    }
}

/// Path finder maintains the token graph and discovers paths.
pub struct PathFinder {
    graph: TokenGraph,
    config: PathFinderConfig,
    /// Path cache — separate mutex so `find_paths` only needs a read lock on
    /// the finder.
    cache: Mutex<HashMap<(String, String), CachedPaths>>,
}

struct CachedPaths {
    paths: Vec<Path>,
    cached_at_ms: u64,
}

/// Cache TTL: paths are valid for 30 seconds
const CACHE_TTL_MS: u64 = 30_000;

impl PathFinder {
    pub fn new(config: PathFinderConfig) -> Self {
        Self {
            graph: TokenGraph::new(),
            config,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Update the graph with trading pairs from a DEX source.
    /// Replaces all existing edges from that source.
    pub fn update_from_source(&mut self, source: &str, pairs: &[TradingPair]) {
        // Remove old edges from this source
        self.graph.remove_source(source);

        // Add new edges
        for pair in pairs {
            self.graph.add_pair_meta(
                &pair.token_a,
                &pair.token_b,
                source,
                &pair.pool_address,
                pair.fee_bps,
                &pair.dex_type,
                &pair.factory,
            );
        }

        // Invalidate all cached paths (source changed)
        self.cache.lock().unwrap().clear();

        info!(
            source = source,
            pairs = pairs.len(),
            total_tokens = self.graph.token_count(),
            total_edges = self.graph.edge_count(),
            "Token graph updated"
        );
    }

    /// Update the graph from a Chakra topology snapshot (`MarketSnapshot`),
    /// replacing every source's edges. Pairs outside the v1 catalog are
    /// dropped (`pairs_from_chakra_snapshot`).
    pub fn update_from_chakra_snapshot(&mut self, snapshot: &MarketSnapshot) {
        let mut by_source: std::collections::BTreeMap<String, Vec<TradingPair>> = Default::default();
        for pair in pairs_from_chakra_snapshot(snapshot) {
            by_source.entry(pair.source.clone()).or_default().push(pair);
        }
        for (source, pairs) in by_source {
            self.update_from_source(&source, &pairs);
        }
    }

    /// Find all valid paths from token_in to token_out.
    pub fn find_paths(&self, token_in: &TokenId, token_out: &TokenId) -> Vec<Path> {
        self.find_paths_with_limits(
            token_in,
            token_out,
            self.config.max_hops,
            self.config.max_multi_hop_paths,
            self.config.max_direct_paths,
        )
    }

    pub fn default_max_hops(&self) -> usize {
        self.config.max_hops
    }

    pub fn default_max_multi_hop_paths(&self) -> usize {
        self.config.max_multi_hop_paths
    }

    pub fn default_max_direct_paths(&self) -> usize {
        self.config.max_direct_paths
    }

    pub fn find_paths_with_limits(
        &self,
        token_in: &TokenId,
        token_out: &TokenId,
        max_hops: usize,
        max_multi_hop_paths: usize,
        max_direct_paths: usize,
    ) -> Vec<Path> {
        let cache_key = (token_in.canonical(), token_out.canonical());
        let now = chrono::Utc::now().timestamp_millis() as u64;

        let use_cache = max_hops == self.config.max_hops
            && max_multi_hop_paths == self.config.max_multi_hop_paths
            && max_direct_paths == self.config.max_direct_paths;
        if use_cache {
            if let Ok(cache) = self.cache.lock() {
                if let Some(cached) = cache.get(&cache_key) {
                    if now - cached.cached_at_ms < CACHE_TTL_MS {
                        return cached.paths.clone();
                    }
                }
            }
        }

        let paths = self
            .graph
            .find_paths(token_in, token_out, max_hops, max_multi_hop_paths, max_direct_paths);

        if use_cache {
            if let Ok(mut cache) = self.cache.lock() {
                cache.insert(
                    cache_key,
                    CachedPaths {
                        paths: paths.clone(),
                        cached_at_ms: now,
                    },
                );
            }
        }

        paths
    }

    /// Invalidate cached paths involving a specific token pair.
    pub fn invalidate(&mut self, token_a: &TokenId, token_b: &TokenId) {
        let key_a = token_a.canonical();
        let key_b = token_b.canonical();
        if let Ok(mut cache) = self.cache.lock() {
            cache.retain(|k, _| k.0 != key_a && k.0 != key_b && k.1 != key_a && k.1 != key_b);
        }
    }

    /// Clear all caches.
    pub fn clear_cache(&mut self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }

    /// Get graph stats.
    pub fn stats(&self) -> (usize, usize) {
        (self.graph.token_count(), self.graph.edge_count())
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::types::TokenId};

    use market_snapshot::{
        decimals::{CIRBTC, EURC, USDC_ERC20},
        ClmmPoolRefSnapshot, MarketSnapshot, SourceSnapshot, TradingPairSnapshot,
    };

    const OTHER: &str = "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const XYK_POOL_UE: &str = "0x0000000000000000000000000000000000000001";
    const STABLE_POOL_UE: &str = "0x0000000000000000000000000000000000000002";
    const XYK_POOL_UM: &str = "0x0000000000000000000000000000000000000003";
    const XYK_POOL_EM: &str = "0x0000000000000000000000000000000000000004";
    const CLMM_POOL_UM: &str = "0x0000000000000000000000000000000000000005";

    fn token(address: &str) -> TokenId {
        TokenId::Contract {
            address: address.to_ascii_lowercase(),
        }
    }

    fn pair(token_a: &str, token_b: &str, pool_address: &str, dex_type: &str, source: &str) -> TradingPairSnapshot {
        TradingPairSnapshot {
            token_a: token_a.to_string(),
            token_b: token_b.to_string(),
            pool_address: pool_address.to_string(),
            fee_bps: if dex_type == "stable" { 4 } else { 30 },
            dex_type: dex_type.to_string(),
            factory: "0xCAFE".to_string(),
        }
    }

    /// The seeded Arc topology: thin + deep USDC/EURC, mBTC direct + CLMM,
    /// EURC→mBTC hop through USDC.
    fn chakra_snapshot() -> MarketSnapshot {
        MarketSnapshot::from_sources(
            "chakra-1",
            1_700_000_000_000,
            "arc-testnet",
            vec![
                SourceSnapshot {
                    source: "chakra-xyk".to_string(),
                    pairs: vec![
                        pair(USDC_ERC20, EURC, XYK_POOL_UE, "xyk", "chakra-xyk"),
                        pair(USDC_ERC20, CIRBTC, XYK_POOL_UM, "xyk", "chakra-xyk"),
                        pair(EURC, CIRBTC, XYK_POOL_EM, "xyk", "chakra-xyk"),
                    ],
                },
                SourceSnapshot {
                    source: "chakra-stable".to_string(),
                    pairs: vec![pair(USDC_ERC20, EURC, STABLE_POOL_UE, "stable", "chakra-stable")],
                },
            ],
        )
    }

    fn finder_with(snapshot: &MarketSnapshot) -> PathFinder {
        let mut finder = PathFinder::new(PathFinderConfig::default());
        finder.update_from_chakra_snapshot(snapshot);
        finder
    }

    #[test]
    fn usdc_to_eurc_finds_both_seeded_xyk_and_stable_pools() {
        let finder = finder_with(&chakra_snapshot());
        let paths = finder.find_paths(&token(USDC_ERC20), &token(EURC));
        let direct: Vec<_> = paths.iter().filter(|p| p.hops == 1).collect();
        assert_eq!(direct.len(), 2, "USDC→EURC should have xyk + stable direct pools");
        let mut sources: Vec<&str> = direct.iter().map(|p| p.sources[0].as_str()).collect();
        sources.sort();
        assert_eq!(sources, vec!["chakra-stable", "chakra-xyk"]);
        let mut pools: Vec<&str> = direct.iter().map(|p| p.pool_addresses[0].as_str()).collect();
        pools.sort();
        assert_eq!(pools, vec![XYK_POOL_UE, STABLE_POOL_UE]);
    }

    #[test]
    fn usdc_to_mbtc_finds_xyk_and_clmm() {
        let snapshot = chakra_snapshot().with_clmm_pool_refs(vec![ClmmPoolRefSnapshot {
            source: "chakra-clmm".to_string(),
            pool_address: CLMM_POOL_UM.to_string(),
            token0: USDC_ERC20.to_string(),
            token1: CIRBTC.to_string(),
            fee_bps: 30,
            tick_spacing: 60,
            factory: "factory-clmm".to_string(),
        }]);
        let finder = finder_with(&snapshot);
        let paths = finder.find_paths(&token(USDC_ERC20), &token(CIRBTC));
        let direct: Vec<_> = paths.iter().filter(|p| p.hops == 1).collect();
        assert_eq!(direct.len(), 2, "USDC→mBTC should have xyk + clmm direct pools");
        let mut sources: Vec<&str> = direct.iter().map(|p| p.sources[0].as_str()).collect();
        sources.sort();
        assert_eq!(sources, vec!["chakra-clmm", "chakra-xyk"]);
    }

    #[test]
    fn eurc_to_mbtc_finds_direct_and_two_hop_via_usdc() {
        let finder = finder_with(&chakra_snapshot());
        let paths = finder.find_paths(&token(EURC), &token(CIRBTC));
        assert!(paths.iter().any(|p| p.hops == 1), "direct EURC→mBTC xyk pool");
        assert!(paths.iter().any(|p| p.hops == 2), "2-hop EURC→USDC→mBTC");
        let two_hop = paths.iter().find(|p| p.hops == 2).unwrap();
        assert_eq!(two_hop.tokens[1].canonical(), USDC_ERC20.to_ascii_lowercase());
    }

    #[test]
    fn max_hops_one_excludes_multi_hop() {
        let finder = finder_with(&chakra_snapshot());
        let paths = finder.find_paths_with_limits(&token(EURC), &token(CIRBTC), 1, 50, 0);
        assert!(!paths.is_empty());
        assert!(
            paths.iter().all(|p| p.hops == 1),
            "max_hops=1 must not return the 2-hop route"
        );
    }

    #[test]
    fn unknown_token_or_same_in_out_yields_empty_candidates() {
        let finder = finder_with(&chakra_snapshot());
        assert!(finder.find_paths(&token(OTHER), &token(EURC)).is_empty());
        assert!(finder.find_paths(&token(USDC_ERC20), &token(USDC_ERC20)).is_empty());
    }

    #[test]
    fn non_catalog_pool_is_unused() {
        let snapshot = MarketSnapshot::from_sources(
            "other-1",
            1_700_000_000_000,
            "arc-testnet",
            vec![SourceSnapshot {
                source: "chakra-xyk".to_string(),
                pairs: vec![pair(
                    USDC_ERC20,
                    OTHER,
                    "0x00000000000000000000000000000000000000bb",
                    "xyk",
                    "chakra-xyk",
                )],
            }],
        );
        let finder = finder_with(&snapshot);
        assert_eq!(finder.stats().0, 0, "OTHER must be filtered by the catalog freeze");
        assert!(finder.find_paths(&token(USDC_ERC20), &token(OTHER)).is_empty());
    }

    #[test]
    fn native_usdc_encoding_is_not_a_graph_node() {
        let snapshot = MarketSnapshot::from_sources(
            "native-1",
            1_700_000_000_000,
            "arc-testnet",
            vec![SourceSnapshot {
                source: "chakra-xyk".to_string(),
                pairs: vec![
                    pair(
                        "native_usdc",
                        USDC_ERC20,
                        "0x00000000000000000000000000000000000000cc",
                        "xyk",
                        "chakra-xyk",
                    ),
                    pair(
                        "0x0000000000000000000000000000000000000000",
                        EURC,
                        "0x00000000000000000000000000000000000000dd",
                        "xyk",
                        "chakra-xyk",
                    ),
                ],
            }],
        );
        let finder = finder_with(&snapshot);
        assert_eq!(finder.stats().0, 0, "native USDC encodings are never graph nodes");
        assert!(finder.find_paths(&token("native_usdc"), &token(USDC_ERC20)).is_empty());
        assert!(finder
            .find_paths(&token("0x0000000000000000000000000000000000000000"), &token(EURC))
            .is_empty());
    }

    #[test]
    fn default_config_is_chakra_arc_three_hops_with_erc20_usdc_bridge() {
        let config = PathFinderConfig::default();
        assert_eq!(config.max_hops, 3);
        assert_eq!(
            config.bridge_tokens,
            vec![TokenId::Contract {
                address: USDC_ERC20.to_string()
            }]
        );
        assert!(
            config
                .bridge_tokens
                .iter()
                .all(|b| b.canonical() != "native" && !b.canonical().contains(':')),
            "bridge must never be XLM Native / Classic USDC for Chakra"
        );
    }

    fn contract_token(address: &str) -> TokenId {
        TokenId::Contract {
            address: address.to_string(),
        }
    }

    #[test]
    fn includes_comet_edges_in_routing_graph() {
        let mut finder = PathFinder::new(PathFinderConfig {
            max_hops: 1,
            max_multi_hop_paths: 10,
            max_direct_paths: 0,
            bridge_tokens: vec![],
        });
        let blnd = contract_token("CDTKPWPLOURQA2SGTKTUQOWRCBZEORB4BWBOMJ3D3ZTQQSGE5F6JBQLV");
        let usdc = contract_token("CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75");
        finder.update_from_source(
            "comet",
            &[TradingPair {
                token_a: blnd.clone(),
                token_b: usdc.clone(),
                source: "comet".to_string(),
                pool_address: "comet-pool".to_string(),
                fee_bps: 30,
                reserve_a: Some(1_000_000),
                reserve_b: Some(2_000_000),
                factory: String::new(),
                dex_type: String::new(),
            }],
        );

        let paths = finder.find_paths(&blnd, &usdc);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].sources, vec!["comet".to_string()]);
        assert_eq!(paths[0].pool_addresses, vec!["comet-pool".to_string()]);
    }
}
