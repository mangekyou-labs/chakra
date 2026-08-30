//! Token graph for path discovery.
//!
//! Maintains an adjacency list of all tradeable pairs across registered DEX
//! sources. Supports BFS-based path finding with configurable max hops.

use {
    crate::types::{Path, TokenId},
    std::collections::{HashMap, HashSet, VecDeque},
};

/// Edge in the token graph representing a tradeable pair on a specific DEX.
#[derive(Debug, Clone)]
pub struct Edge {
    pub target: TokenId,
    pub source: String,
    pub pool_address: String,
    pub fee_bps: u32,
    /// Per-edge DEX type (`xyk` | `stable` | `clmm` | …). T4.7 hop metadata.
    pub dex_type: String,
    /// Allowlisted venue factory address (empty when no stamp is available).
    pub factory: String,
    pub last_updated_ms: u64,
}

/// Token graph: adjacency list representation.
/// Each token maps to a list of edges (other tokens it can be swapped to).
#[derive(Debug, Default)]
pub struct TokenGraph {
    adjacency: HashMap<String, Vec<Edge>>,
    /// Set of all known tokens
    tokens: HashSet<String>,
}

impl TokenGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a bidirectional edge (trading pair) to the graph.
    pub fn add_pair(&mut self, token_a: &TokenId, token_b: &TokenId, source: &str, pool_address: &str, fee_bps: u32) {
        self.add_pair_meta(token_a, token_b, source, pool_address, fee_bps, "", "")
    }

    /// Add a bidirectional edge with T4.7 hop metadata (dex type + factory).
    #[allow(clippy::too_many_arguments)]
    pub fn add_pair_meta(
        &mut self,
        token_a: &TokenId,
        token_b: &TokenId,
        source: &str,
        pool_address: &str,
        fee_bps: u32,
        dex_type: &str,
        factory: &str,
    ) {
        let key_a = token_a.canonical();
        let key_b = token_b.canonical();
        let now = chrono::Utc::now().timestamp_millis() as u64;

        self.tokens.insert(key_a.clone());
        self.tokens.insert(key_b.clone());

        // A -> B
        self.adjacency.entry(key_a.clone()).or_default().push(Edge {
            target: token_b.clone(),
            source: source.to_string(),
            pool_address: pool_address.to_string(),
            fee_bps,
            dex_type: dex_type.to_string(),
            factory: factory.to_string(),
            last_updated_ms: now,
        });

        // B -> A
        self.adjacency.entry(key_b).or_default().push(Edge {
            target: token_a.clone(),
            source: source.to_string(),
            pool_address: pool_address.to_string(),
            fee_bps,
            dex_type: dex_type.to_string(),
            factory: factory.to_string(),
            last_updated_ms: now,
        });
    }

    /// Remove all edges from a specific source (e.g., when re-syncing a DEX
    /// adapter).
    pub fn remove_source(&mut self, source: &str) {
        for edges in self.adjacency.values_mut() {
            edges.retain(|e| e.source != source);
        }
    }

    /// Get all neighbors of a token.
    pub fn neighbors(&self, token: &TokenId) -> &[Edge] {
        self.adjacency
            .get(&token.canonical())
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Total number of unique tokens in the graph.
    pub fn token_count(&self) -> usize {
        self.tokens.len()
    }

    /// Total number of edges (directed) in the graph.
    pub fn edge_count(&self) -> usize {
        self.adjacency.values().map(|v| v.len()).sum()
    }

    /// Discover swap paths from `start` to `end`.
    ///
    /// - **Direct (1-hop)**: every pool edge between the pair (optional
    ///   `max_direct_paths` cap; `0` = no cap).
    /// - **Multi-hop (2+ hops)**: BFS up to `max_hops`, capped at
    ///   `max_multi_hop_paths`.
    ///
    /// Returns paths sorted by hop count (shortest first).
    #[allow(clippy::type_complexity)]
    pub fn find_paths(
        &self,
        start: &TokenId,
        end: &TokenId,
        max_hops: usize,
        max_multi_hop_paths: usize,
        max_direct_paths: usize,
    ) -> Vec<Path> {
        let start_key = start.canonical();
        let end_key = end.canonical();

        if start_key == end_key {
            return vec![];
        }

        let mut results: Vec<Path> = Vec::new();

        if let Some(edges) = self.adjacency.get(&start_key) {
            for edge in edges {
                if edge.target.canonical() != end_key {
                    continue;
                }
                results.push(Path {
                    hops: 1,
                    tokens: vec![start.clone(), edge.target.clone()],
                    sources: vec![edge.source.clone()],
                    pool_addresses: vec![edge.pool_address.clone()],
                    dex_types: vec![edge.dex_type.clone()],
                    fee_bps: vec![edge.fee_bps],
                    factories: vec![edge.factory.clone()],
                });
            }
        }

        if max_direct_paths > 0 && results.len() > max_direct_paths {
            results.truncate(max_direct_paths);
        }

        let mut multi_hop_count = 0usize;

        // BFS for indirect paths only (direct pools already in `results`).
        let mut queue: VecDeque<(
            String,
            Vec<TokenId>,
            Vec<String>,
            Vec<String>,
            Vec<String>,
            Vec<u32>,
            Vec<String>,
            HashSet<String>,
        )> = VecDeque::new();

        queue.push_back((
            start_key.clone(),
            vec![start.clone()],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            HashSet::new(),
        ));

        while let Some((
            current_key,
            token_path,
            source_path,
            pool_path,
            dex_path,
            fee_path,
            factory_path,
            visited_pools,
        )) = queue.pop_front()
        {
            if multi_hop_count >= max_multi_hop_paths {
                break;
            }

            if queue.len() > 10_000 {
                break;
            }

            if pool_path.len() >= max_hops {
                continue;
            }

            let edges = match self.adjacency.get(&current_key) {
                Some(e) => e,
                None => continue,
            };

            for edge in edges {
                let target_key = edge.target.canonical();

                if visited_pools.contains(&edge.pool_address) {
                    continue;
                }

                let next_hops = pool_path.len() + 1;

                if target_key == end_key {
                    if next_hops < 2 {
                        continue;
                    }
                    let mut new_tokens = token_path.clone();
                    new_tokens.push(edge.target.clone());

                    let mut new_sources = source_path.clone();
                    new_sources.push(edge.source.clone());

                    let mut new_pools = pool_path.clone();
                    new_pools.push(edge.pool_address.clone());

                    let mut new_dex = dex_path.clone();
                    new_dex.push(edge.dex_type.clone());

                    let mut new_fees = fee_path.clone();
                    new_fees.push(edge.fee_bps);

                    let mut new_factories = factory_path.clone();
                    new_factories.push(edge.factory.clone());

                    results.push(Path {
                        hops: new_sources.len(),
                        tokens: new_tokens,
                        sources: new_sources,
                        pool_addresses: new_pools,
                        dex_types: new_dex,
                        fee_bps: new_fees,
                        factories: new_factories,
                    });
                    multi_hop_count += 1;

                    if multi_hop_count >= max_multi_hop_paths {
                        break;
                    }
                    continue;
                }

                if token_path.iter().any(|t| t.canonical() == target_key) {
                    continue;
                }

                let mut new_tokens = token_path.clone();
                new_tokens.push(edge.target.clone());

                let mut new_sources = source_path.clone();
                new_sources.push(edge.source.clone());

                let mut new_pools = pool_path.clone();
                new_pools.push(edge.pool_address.clone());

                let mut new_dex = dex_path.clone();
                new_dex.push(edge.dex_type.clone());

                let mut new_fees = fee_path.clone();
                new_fees.push(edge.fee_bps);

                let mut new_factories = factory_path.clone();
                new_factories.push(edge.factory.clone());

                let mut new_visited = visited_pools.clone();
                new_visited.insert(edge.pool_address.clone());

                queue.push_back((
                    target_key,
                    new_tokens,
                    new_sources,
                    new_pools,
                    new_dex,
                    new_fees,
                    new_factories,
                    new_visited,
                ));
            }
        }

        results.sort_by_key(|p| p.hops);
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tok_a() -> TokenId {
        TokenId::Contract {
            address: "0x3600000000000000000000000000000000000000".into(),
        }
    }
    fn tok_b() -> TokenId {
        TokenId::Contract {
            address: "0x89B50855Aa8D51744b8062fa4173AC0d1c4CD72a".into(),
        }
    }
    fn tok_c() -> TokenId {
        TokenId::Contract {
            address: "0x6De6cAC86b864aA2B7b2d56125A0F5952Ac0e774".into(),
        }
    }

    #[test]
    fn test_direct_path() {
        let mut graph = TokenGraph::new();
        graph.add_pair(&tok_a(), &tok_b(), "chakra-xyk", "pool_1", 30);

        let paths = graph.find_paths(&tok_a(), &tok_b(), 4, 100, 0);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].hops, 1);
        assert_eq!(paths[0].tokens.len(), 2);
    }

    #[test]
    fn test_multi_hop_path() {
        let mut graph = TokenGraph::new();
        graph.add_pair(&tok_a(), &tok_b(), "chakra-xyk", "pool_1", 30);
        graph.add_pair(&tok_b(), &tok_c(), "chakra-stable", "pool_2", 30);

        let paths = graph.find_paths(&tok_a(), &tok_c(), 4, 100, 0);
        // Should find: tok_a -> tok_b -> tok_c (2 hops)
        assert!(paths.iter().any(|p| p.hops == 2));
    }

    #[test]
    fn test_multiple_paths() {
        let mut graph = TokenGraph::new();
        graph.add_pair(&tok_a(), &tok_b(), "chakra-xyk", "pool_1", 30);
        graph.add_pair(&tok_a(), &tok_b(), "chakra-stable", "pool_2", 20);
        graph.add_pair(&tok_a(), &tok_c(), "chakra-xyk", "pool_3", 30);
        graph.add_pair(&tok_c(), &tok_b(), "chakra-stable", "pool_4", 30);

        let paths = graph.find_paths(&tok_a(), &tok_b(), 4, 100, 0);
        // Direct: pool_1, pool_2; Indirect: tok_a->tok_c->tok_b
        assert!(paths.len() >= 3);
    }

    #[test]
    fn test_all_direct_paths_included_when_many_pools() {
        let mut graph = TokenGraph::new();
        for i in 0..15 {
            graph.add_pair(&tok_a(), &tok_b(), "chakra-stable", &format!("pool_{i}"), 30);
        }
        let paths = graph.find_paths(&tok_a(), &tok_b(), 3, 5, 0);
        assert_eq!(paths.len(), 15);
        assert!(paths.iter().all(|p| p.hops == 1));
    }

    #[test]
    fn test_multi_hop_capped_separately_from_direct() {
        let mut graph = TokenGraph::new();
        for i in 0..12 {
            graph.add_pair(&tok_a(), &tok_b(), "chakra-stable", &format!("direct_{i}"), 30);
        }
        graph.add_pair(&tok_a(), &tok_c(), "chakra-xyk", "bridge", 30);
        graph.add_pair(&tok_c(), &tok_b(), "chakra-stable", "indirect", 30);

        let paths = graph.find_paths(&tok_a(), &tok_b(), 3, 2, 0);
        assert_eq!(paths.iter().filter(|p| p.hops == 1).count(), 12);
        assert_eq!(paths.iter().filter(|p| p.hops == 2).count(), 1);
        assert_eq!(paths.len(), 13);
    }

    #[test]
    fn test_no_path() {
        let mut graph = TokenGraph::new();
        graph.add_pair(&tok_a(), &tok_b(), "chakra-xyk", "pool_1", 30);

        let paths = graph.find_paths(&tok_a(), &tok_c(), 4, 100, 0);
        assert!(paths.is_empty());
    }

    /// T4.7: per-hop dex_type / fee / factory metadata rides through path
    /// discovery (both direct and multi-hop).
    #[test]
    fn test_paths_carry_per_hop_dex_type_fee_factory() {
        let mut graph = TokenGraph::new();
        graph.add_pair_meta(
            &tok_a(),
            &tok_b(),
            "chakra-stable",
            "stable_pool",
            4,
            "stable",
            "0xSTABLE_FACTORY",
        );
        graph.add_pair_meta(&tok_b(), &tok_c(), "chakra-xyk", "xyk_pool", 30, "xyk", "0xXYK_FACTORY");

        // Direct path: tok_a → tok_b via chakra-stable.
        let direct = graph.find_paths(&tok_a(), &tok_b(), 4, 100, 0);
        assert_eq!(direct.len(), 1);
        assert_eq!(direct[0].dex_types, vec!["stable".to_string()]);
        assert_eq!(direct[0].fee_bps, vec![4]);
        assert_eq!(direct[0].factories, vec!["0xSTABLE_FACTORY".to_string()]);

        // Multi-hop: tok_a → tok_b → tok_c via stable then xyk.
        let multi = graph.find_paths(&tok_a(), &tok_c(), 4, 100, 0);
        assert!(multi.iter().any(|p| p.hops == 2));
        let two_hop = multi.iter().find(|p| p.hops == 2).unwrap();
        assert_eq!(two_hop.dex_types, vec!["stable".to_string(), "xyk".to_string()]);
        assert_eq!(two_hop.fee_bps, vec![4, 30]);
        assert_eq!(
            two_hop.factories,
            vec!["0xSTABLE_FACTORY".to_string(), "0xXYK_FACTORY".to_string()]
        );
    }

    /// Edges added without hop metadata yield empty dex_type/factory vectors.
    #[test]
    fn untyped_edges_yield_empty_hop_metadata() {
        let mut graph = TokenGraph::new();
        graph.add_pair(&tok_a(), &tok_b(), "chakra-xyk", "pool_1", 30);
        let paths = graph.find_paths(&tok_a(), &tok_b(), 4, 100, 0);
        assert_eq!(paths[0].dex_types, vec![String::new()]);
        assert_eq!(paths[0].fee_bps, vec![30]);
        assert_eq!(paths[0].factories, vec![String::new()]);
    }

    #[test]
    fn test_max_hops_limit() {
        let mut graph = TokenGraph::new();
        let a = TokenId::from_str_auto("0x0001");
        let b = TokenId::from_str_auto("0x0002");
        let c = TokenId::from_str_auto("0x0003");
        let d = TokenId::from_str_auto("0x0004");
        let e = TokenId::from_str_auto("0x0005");

        graph.add_pair(&a, &b, "dex", "p1", 30);
        graph.add_pair(&b, &c, "dex", "p2", 30);
        graph.add_pair(&c, &d, "dex", "p3", 30);
        graph.add_pair(&d, &e, "dex", "p4", 30);

        // max_hops=3 should NOT find A->B->C->D->E (4 hops)
        let paths = graph.find_paths(&a, &e, 3, 100, 0);
        assert!(paths.is_empty());

        // max_hops=4 should find it
        let paths = graph.find_paths(&a, &e, 4, 100, 0);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].hops, 4);
    }
}
